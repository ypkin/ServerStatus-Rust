#![allow(unused)]
use anyhow::Result;
use chrono::{Datelike, Local, Timelike};
use once_cell::sync::OnceCell;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::sync::mpsc::sync_channel;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Host;
use crate::notifier::{Event, Notifier};
use crate::payload::{HostStat, StatsResp};

const SAVE_INTERVAL: u64 = 60;
const OS_LIST: [&str; 10] = [
    "centos", "debian", "ubuntu", "arch", "windows", "macos", "pi", "android", "linux", "freebsd",
];

static STAT_SENDER: OnceCell<SyncSender<Cow<HostStat>>> = OnceCell::new();

use crate::registry::{GROUPS_MAP, HOSTS_MAP, STAT_MAP};

pub struct StatsMgr {
    resp_json: Arc<Mutex<String>>,
    stats_data: Arc<Mutex<StatsResp>>,
}

impl StatsMgr {
    pub fn new() -> Self {
        Self {
            resp_json: Arc::new(Mutex::new("{}".to_string())),
            stats_data: Arc::new(Mutex::new(StatsResp::new())),
        }
    }

    fn load_last_network(hosts_map: &mut HashMap<String, Host>) {
        let contents = fs::read_to_string("stats.json").unwrap_or_default();
        if contents.is_empty() {
            return;
        }

        if let Ok(stats_json) = serde_json::from_str::<serde_json::Value>(contents.as_str()) {
            if let Some(servers) = stats_json["servers"].as_array() {
                for v in servers {
                    if let (Some(name), Some(last_network_in), Some(last_network_out)) = (
                        v["name"].as_str(),
                        v["last_network_in"].as_u64(),
                        v["last_network_out"].as_u64(),
                    ) {
                        if let Some(srv) = hosts_map.get_mut(name) {
                            srv.last_network_in = last_network_in;
                            srv.last_network_out = last_network_out;

                            trace!("{} => last in/out ({}/{}))", &name, last_network_in, last_network_out);
                        }
                    } else {
                        error!("invalid json => {v:?}");
                    }
                }
                trace!("load stats.json succ!");
            }
        } else {
            warn!("ignore invalid stats.json");
        }
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::unnecessary_wraps)]
    pub fn init(
        &mut self,
        cfg: &'static crate::config::Config,
        notifies: Arc<Mutex<Vec<Box<dyn Notifier + Send>>>>,
    ) -> Result<()> {
        // Seed the live registries: config hosts (minus backend-deleted
        // ones), overlaid by admin-managed hosts from admin.json.
        // Fetch admin state first (ADMIN lock), then take the registries:
        // lock order everywhere is registry -> ADMIN, never the reverse, so
        // no admin handler may hold ADMIN while taking a registry lock.
        let admin_hosts = crate::admin::admin_hosts();
        let deleted_hosts = crate::admin::deleted_hosts();
        let admin_groups = crate::admin::admin_groups();
        let deleted_groups = crate::admin::deleted_groups();
        {
            let mut groups = GROUPS_MAP.lock().unwrap();
            *groups = cfg.hosts_group_map.clone();
            groups.retain(|gid, _| !deleted_groups.contains(gid));
            for mut g in admin_groups {
                g.traffic_mode = crate::config::normalize_traffic_mode(&g.traffic_mode);
                groups.insert(g.gid.clone(), g);
            }
        }
        {
            let mut map = HOSTS_MAP.lock().unwrap();
            *map = cfg.hosts_map.clone();
            map.retain(|name, _| !deleted_hosts.contains(name));
            for mut h in admin_hosts {
                h.pos = map.values().map(|o| o.pos).max().unwrap_or(0) + 1;
                // Weight persists once set via the admin form; only derive
                // it for records that predate editable sort (weight 0).
                if h.weight == 0 {
                    h.weight = 10000_u64.saturating_sub(h.pos as u64);
                }
                if h.alias.is_empty() {
                    h.alias = h.name.clone();
                }
                if h.monthstart < 1 || h.monthstart > 31 {
                    h.monthstart = 1;
                }
                h.traffic_mode = crate::config::normalize_traffic_mode(&h.traffic_mode);
                map.insert(h.name.clone(), h);
            }
            // load last_network_in/out
            Self::load_last_network(&mut map);
        }

        let (stat_tx, stat_rx) = sync_channel(512);
        STAT_SENDER.set(stat_tx).unwrap();
        let (notifier_tx, notifier_rx) = sync_channel(512);

        // stat_rx thread
        thread::spawn({
            let notifier_tx = notifier_tx.clone();

            move || loop {
                while let Ok(mut stat) = stat_rx.recv() {
                    trace!("recv stat `{stat:?}");

                    let mut stat_t = stat.to_mut();

                    // group mode
                    if !stat_t.gid.is_empty() {
                        if stat_t.alias.is_empty() {
                            stat_t.alias = stat_t.name.clone();
                        }

                        if let Ok(mut hosts_map) = HOSTS_MAP.lock() {
                            let host = hosts_map.get(&stat_t.name);
                            if host.is_none() || !host.unwrap().gid.eq(&stat_t.gid) {
                                let group = GROUPS_MAP.lock().unwrap().get(&stat_t.gid).cloned();
                                if let Some(group) = group.as_ref() {
                                    // 名称不变，换组了，更新组配置 & last in/out
                                    let mut inst = group.inst_host(&stat_t.name);
                                    if let Some(o) = host {
                                        inst.last_network_in = o.last_network_in;
                                        inst.last_network_out = o.last_network_out;
                                    }
                                    hosts_map.insert(stat_t.name.clone(), inst);
                                } else {
                                    continue;
                                }
                            }
                        }
                    }

                    //
                    if let Ok(mut hosts_map) = HOSTS_MAP.lock() {
                        let host_info = hosts_map.get_mut(&stat_t.name);
                        if host_info.is_none() {
                            error!("invalid stat `{stat_t:?}");
                            continue;
                        }
                        let info = host_info.unwrap();

                        if info.disabled {
                            continue;
                        }

                        // 补齐 precedence: explicit client value >
                        // backend host value > client auto-detection.
                        // (Detected values arrive in separate fields so
                        // they can never shadow an explicit setting.)
                        if stat_t.location.is_empty() {
                            stat_t.location = info.location.clone();
                        }
                        if stat_t.location.is_empty() {
                            stat_t.location = stat_t.location_detected.clone();
                        }
                        if stat_t.host_type.is_empty() {
                            stat_t.host_type = info.r#type.clone();
                        }
                        if stat_t.host_type.is_empty() {
                            stat_t.host_type = stat_t.type_detected.clone();
                        }
                        stat_t.notify = info.notify && stat_t.notify;
                        stat_t.pos = info.pos;
                        stat_t.disabled = info.disabled;
                        stat_t.weight += info.weight;
                        stat_t.labels = info.labels.clone();

                        // !group
                        if !info.alias.is_empty() {
                            stat_t.alias = info.alias.clone();
                        }

                        // Previous report time, before it is overwritten below:
                        // accumulate() uses it to recognise a reboot that
                        // happened while this server was not watching.
                        let prev_latest_ts = info.latest_ts;
                        info.latest_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                        stat_t.latest_ts = info.latest_ts;

                        // Fork: server-side probe latency history (see
                        // ping_hist.rs). Every report's six probe readings
                        // are bucketed here, so the latency chart no
                        // longer depends on the browser staying open to
                        // sample. Slot keys, not display names, so a
                        // probe rename keeps one continuous series.
                        crate::ping_hist::record(
                            &stat_t.name,
                            stat_t.latest_ts as i64,
                            &[
                                crate::ping_hist::ProbeReading { slot: "cu:v4", time_ms: stat_t.time_10010, loss: stat_t.ping_10010 },
                                crate::ping_hist::ProbeReading { slot: "cu:v6", time_ms: stat_t.time_10010_v6, loss: stat_t.ping_10010_v6 },
                                crate::ping_hist::ProbeReading { slot: "ct:v4", time_ms: stat_t.time_189, loss: stat_t.ping_189 },
                                crate::ping_hist::ProbeReading { slot: "ct:v6", time_ms: stat_t.time_189_v6, loss: stat_t.ping_189_v6 },
                                crate::ping_hist::ProbeReading { slot: "cm:v4", time_ms: stat_t.time_10086, loss: stat_t.ping_10086 },
                                crate::ping_hist::ProbeReading { slot: "cm:v6", time_ms: stat_t.time_10086_v6, loss: stat_t.ping_10086_v6 },
                            ],
                        );

                        // Fork traffic accounting (see traffic.rs). Effective
                        // settings are base config merged with admin overrides.
                        let (traffic_limit, traffic_mode, monthstart) =
                            crate::admin::effective_host_settings(&stat_t.name, info);
                        stat_t.traffic_limit = traffic_limit;
                        // Fork plan display (see web/plan-overlay.js): the
                        // registered Host record already carries the
                        // effective plan (panel edits update it, group
                        // members inherit the template), so copy it onto
                        // the outgoing stat for stats.json.
                        stat_t.plan_start_date = info.plan_start_date.clone();
                        stat_t.plan_end_date = info.plan_end_date.clone();
                        stat_t.annual_fee = info.annual_fee;
                        stat_t.currency = if info.currency.is_empty() {
                            "$".to_string()
                        } else {
                            info.currency.clone()
                        };
                        stat_t.bandwidth_mbps = info.bandwidth_mbps;
                        stat_t.tag = info.tag.clone();
                        stat_t.public_note = info.public_note.clone();
                        stat_t.traffic_mode = traffic_mode.clone();
                        let mut traffic_notify = false;
                        if stat_t.vnstat {
                            // vnstat already split the month client-side:
                            // last_network_* = lifetime - this month.
                            stat_t.month_rx = stat_t.network_in.saturating_sub(stat_t.last_network_in);
                            stat_t.month_tx = stat_t.network_out.saturating_sub(stat_t.last_network_out);
                            stat_t.total_rx = stat_t.network_in;
                            stat_t.total_tx = stat_t.network_out;
                            stat_t.month_start =
                                crate::traffic::period_start(Local::now().date_naive(), monthstart).to_string();
                            stat_t.day_rx = 0;
                            stat_t.day_tx = 0;
                        } else if stat_t.boot_id.is_empty() {
                            // Legacy / python clients without boot_id: keep the
                            // old baseline behaviour (month = lifetime -
                            // baseline), folded through the configured mode.
                            let local_now = Local::now();
                            if info.last_network_in == 0
                                || (stat_t.network_in != 0 && info.last_network_in > stat_t.network_in)
                                || (local_now.day() == monthstart && local_now.hour() == 0 && local_now.minute() < 5)
                            {
                                info.last_network_in = stat_t.network_in;
                                info.last_network_out = stat_t.network_out;
                            } else {
                                stat_t.last_network_in = info.last_network_in;
                                stat_t.last_network_out = info.last_network_out;
                            }
                            stat_t.month_rx = stat_t.network_in.saturating_sub(stat_t.last_network_in);
                            stat_t.month_tx = stat_t.network_out.saturating_sub(stat_t.last_network_out);
                            stat_t.total_rx = stat_t.network_in;
                            stat_t.total_tx = stat_t.network_out;
                            stat_t.month_start =
                                crate::traffic::period_start(Local::now().date_naive(), monthstart).to_string();
                            stat_t.day_rx = 0;
                            stat_t.day_tx = 0;
                        } else {
                            // Server-side accumulate: only observed increases.
                            let now = Local::now();
                            let now_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                            let booted = if stat_t.uptime > 0 && (stat_t.uptime as i64) <= now_ts {
                                Some(now_ts - stat_t.uptime as i64)
                            } else {
                                None
                            };
                            let mut traffic_map = crate::traffic::TRAFFIC.lock().unwrap();
                            let st = traffic_map.entry(stat_t.name.clone()).or_default();
                            st.accumulate(
                                &stat_t.boot_id.clone(),
                                stat_t.network_in,
                                stat_t.network_out,
                                monthstart,
                                now,
                                booted,
                                prev_latest_ts as i64,
                            );
                            stat_t.month_rx = st.month_rx;
                            stat_t.month_tx = st.month_tx;
                            stat_t.day_rx = st.day_rx;
                            stat_t.day_tx = st.day_tx;
                            stat_t.total_rx = st.total_rx;
                            stat_t.total_tx = st.total_tx;
                            stat_t.month_start = st.month_start.clone();
                            // Compatibility shim for old themes, which compute
                            // the month as network_in - last_network_in.
                            stat_t.last_network_in = stat_t.network_in.saturating_sub(st.month_rx);
                            stat_t.last_network_out = stat_t.network_out.saturating_sub(st.month_tx);
                            // Deduped allowance alert (monitor model).
                            // Unlimited nodes (0, or 9999 GiB from the
                            // admin form) never alert: there is no quota
                            // to cross.
                            if !crate::traffic::is_unlimited(traffic_limit) {
                                let used = crate::traffic::month_used(st.month_rx, st.month_tx, &traffic_mode);
                                let step = crate::traffic::TrafficState::alert_step(
                                    used,
                                    traffic_limit,
                                    cfg.traffic_alert_percent,
                                );
                                if step > 0 && (st.alerted_period != stat_t.month_start || st.alerted_step < step) {
                                    st.alerted_step = step;
                                    st.alerted_period = stat_t.month_start.clone();
                                    traffic_notify = true;
                                } else if step < st.alerted_step && st.alerted_period == stat_t.month_start {
                                    st.alerted_step = step;
                                }
                            }
                        }
                        stat_t.month_used = crate::traffic::month_used(stat_t.month_rx, stat_t.month_tx, &traffic_mode);
                        stat_t.traffic_percent = if !crate::traffic::is_unlimited(traffic_limit) {
                            (stat_t.month_used as u128 * 100 / traffic_limit as u128) as u64
                        } else {
                            0
                        };

                        // Fork: keep the raw seconds alongside the
                        // formatted string (see payload uptime_sec), so
                        // the overlay can compute a real boot time.
                        stat_t.uptime_sec = stat_t.uptime;
                        // uptime str
                        let day = stat_t.uptime / (3600 * 24);
                        if day > 0 {
                            stat_t.uptime_str = format!("{day} 天");
                        } else {
                            stat_t.uptime_str = format!(
                                "{:02}:{:02}:{:02}",
                                stat_t.uptime / 3600,
                                (stat_t.uptime / 60) % 60,
                                stat_t.uptime % 60
                            );
                        }

                        info!("update stat `{stat_t:?}");
                        if let Ok(mut host_stat_map) = STAT_MAP.lock() {
                            let mut notify_up = false;
                            if let Some(pre_stat) = host_stat_map.get(&stat_t.name) {
                                if stat_t.ip_info.is_none() {
                                    stat_t.ip_info = pre_stat.ip_info.clone();
                                }

                                if stat_t.notify && (pre_stat.latest_ts + cfg.offline_threshold < stat_t.latest_ts) {
                                    notify_up = true;
                                }
                            }
                            let arc_stat = Arc::new(stat.into_owned());
                            if notify_up {
                                // node up notify
                                notifier_tx.send((Event::NodeUp, Arc::clone(&arc_stat)));
                            }
                            if traffic_notify {
                                // traffic allowance step notify (deduped)
                                let _ = notifier_tx.send((Event::Traffic, Arc::clone(&arc_stat)));
                            }
                            host_stat_map.insert(arc_stat.name.clone(), arc_stat);
                            //trace!("{:?}", host_stat_map);
                        }
                    }
                }
            }
        });

        // timer thread
        thread::spawn({
            let resp_json = self.resp_json.clone();
            let stats_data = self.stats_data.clone();
            let notifier_tx = notifier_tx.clone();
            let mut latest_notify_ts = 0_u64;
            let mut latest_save_ts = 0_u64;
            let mut latest_group_gc = 0_u64;
            let mut latest_alert_check_ts = 0_u64;
            move || loop {
                thread::sleep(Duration::from_millis(500));

                let mut resp = StatsResp::new();
                let now = resp.updated;
                let mut any_notified = false;

                // group gc
                if latest_group_gc + cfg.group_gc < now {
                    latest_group_gc = now;
                    //
                    if let Ok(mut hm) = HOSTS_MAP.lock() {
                        hm.retain(|_, o| o.gid.is_empty() || o.latest_ts + cfg.group_gc >= now);
                    }
                    //
                    if let Ok(mut sm) = STAT_MAP.lock() {
                        sm.retain(|_, o| o.gid.is_empty() || o.latest_ts + cfg.group_gc >= now);
                    }
                }

                if let Ok(mut host_stat_map) = STAT_MAP.lock() {
                    for (_, stat) in host_stat_map.iter_mut() {
                        if stat.disabled {
                            resp.servers.push(Arc::clone(stat));
                            continue;
                        }
                        let notify_event = {
                            let o = Arc::make_mut(stat);
                            // 30s 下线
                            if o.latest_ts + cfg.offline_threshold < now {
                                o.online4 = false;
                                o.online6 = false;
                            }

                            // labels
                            if !o.labels.contains("os=") {
                                if let Some(sys_info) = &o.sys_info {
                                    let os_r = sys_info.os_release.to_lowercase();
                                    for s in &OS_LIST {
                                        if os_r.contains(s) {
                                            if o.labels.is_empty() {
                                                write!(o.labels, "os={s}");
                                            } else {
                                                write!(o.labels, ";os={s}");
                                            }
                                            break;
                                        }
                                    }
                                }
                            }

                            // determine notify event (o is dropped after this block)
                            if o.notify && latest_notify_ts + cfg.notify_interval < now {
                                if o.online4 || o.online6 {
                                    Some(Event::Custom)
                                } else {
                                    o.disabled = true;
                                    Some(Event::NodeDown)
                                }
                            } else {
                                None
                            }
                        };

                        // client notify — Arc::clone is O(1), no HostStat copy
                        if let Some(event) = notify_event {
                            notifier_tx.send((event, Arc::clone(stat)));
                            any_notified = true;
                        }

                        resp.servers.push(Arc::clone(stat));
                    }
                    if any_notified {
                        latest_notify_ts = now;
                    }
                }

                resp.servers.sort_by(|a, b| {
                    if a.weight != b.weight {
                        return a.weight.cmp(&b.weight).reverse();
                    }
                    if a.pos != b.pos {
                        return a.pos.cmp(&b.pos);
                    }
                    // same group
                    a.alias.cmp(&b.alias)
                });

                // last_network_in/out save /60s (plus traffic accounting)
                if latest_save_ts + SAVE_INTERVAL < now {
                    latest_save_ts = now;
                    crate::traffic::save_traffic();
                    crate::ping_hist::save();
                    if !resp.servers.is_empty() {
                        if let Ok(mut file) = File::create("stats.json") {
                            file.write_all(serde_json::to_string(&resp).unwrap().as_bytes());
                            file.flush();
                            trace!("save stats.json succ!");
                        } else {
                            error!("save stats.json fail!");
                        }
                    }
                }
                //
                if let Ok(mut o) = resp_json.lock() {
                    *o = serde_json::to_string(&resp).unwrap();
                }
                if let Ok(mut o) = stats_data.lock() {
                    *o = resp;
                }
            }
        });

        // notify thread
        thread::spawn(move || loop {
            while let Ok(msg) = notifier_rx.recv() {
                let (e, stat) = msg;
                let notify_list = &*notifies.lock().unwrap();
                trace!("recv notify => {e:?}, {stat:?}");
                for n in notify_list {
                    trace!("{} notify {:?} => {:?}", n.kind(), e, stat);
                    n.notify(&e, &stat);
                }
            }
        });

        Ok(())
    }

    pub fn get_stats(&self) -> Arc<Mutex<StatsResp>> {
        self.stats_data.clone()
    }

    pub fn get_stats_json(&self) -> String {
        // Serve the enriched view (ip_info, sys_info, ping_probes) rather
        // than the timer thread's cached plain serialization, which drops
        // every skip_serializing field. The overlay (plan/spec/probe
        // lines) and the React detail view read those fields from here.
        // Falls back to the cached string if enrichment ever fails.
        match self.get_all_info() {
            Ok(v) => serde_json::to_string(&v)
                .unwrap_or_else(|_| self.resp_json.lock().unwrap().to_string()),
            Err(_) => self.resp_json.lock().unwrap().to_string(),
        }
    }

    #[allow(clippy::unused_self)]
    #[allow(clippy::unnecessary_wraps)]
    pub fn report(&self, data: serde_json::Value) -> Result<()> {
        static SENDER: LazyLock<SyncSender<Cow<'static, HostStat>>> =
            LazyLock::new(|| STAT_SENDER.get().unwrap().clone());

        match serde_json::from_value(data) {
            Ok(stat) => {
                trace!("send stat => {stat:?} ");
                SENDER.send(Cow::Owned(stat));
            }
            Err(err) => {
                error!("report error => {err:?}");
            }
        }
        Ok(())
    }

    pub fn get_all_info(&self) -> Result<serde_json::Value> {
        let data = self.stats_data.lock().unwrap();
        let mut resp_json = serde_json::to_value(&*data)?;
        // for skip_serializing
        let ping = crate::admin::effective_ping();
        if let Some(srv_list) = resp_json["servers"].as_array_mut() {
            for (idx, stat) in data.servers.iter().enumerate() {
                if let Some(srv) = srv_list[idx].as_object_mut() {
                    srv.insert("ip_info".into(), serde_json::to_value(stat.ip_info.as_ref())?);
                    srv.insert("sys_info".into(), serde_json::to_value(stat.sys_info.as_ref())?);
                    srv.insert("ping_probes".into(), ping_probes_json(&ping, stat));
                    // Fork (batch 3, probe consistency): the precompiled
                    // React panel reads the legacy time_*/ping_* fields
                    // directly, so a stale value there contradicts an
                    // empty probe config. Zero every stack the user has
                    // not actually configured (or all stacks when probing
                    // is disabled) in this enriched view only — the
                    // stored HostStat is untouched, and a configured
                    // stack keeps its live measurement.
                    zero_unconfigured_probe_fields(srv, &ping);
                    // Fork: the proto IpInfo carries a country *name* only,
                    // so derive the 2-letter code here for the admin flag
                    // columns. Unknown names yield "" (text only, never a
                    // guessed flag); the admin's own name mapping stays as
                    // a second fallback for hand-filled locations.
                    let cc = stat
                        .ip_info
                        .as_ref()
                        .map(|i| country_code_for_country(&i.country))
                        .unwrap_or("");
                    srv.insert("country_code".into(), serde_json::json!(cc));
                }
            }
        } else {
            // todo!()
        }

        Ok(resp_json)
    }
}

/// 2-letter country code for a geoip country *name* (fork). The proto
/// IpInfo carries no code, so the admin flag columns derive it here
/// instead of changing the proto (which would force a client rollout).
/// Unknown or empty names yield "" — the admin then falls back to its
/// own name mapping for hand-filled locations, and shows text only.
pub fn country_code_for_country(name: &str) -> &'static str {
    match name.trim().to_ascii_lowercase().as_str() {
        "united states" | "united states of america" | "usa" | "us" => "US",
        "canada" => "CA",
        "mexico" => "MX",
        "guatemala" => "GT",
        "belize" => "BZ",
        "honduras" => "HN",
        "el salvador" => "SV",
        "nicaragua" => "NI",
        "costa rica" => "CR",
        "panama" => "PA",
        "cuba" => "CU",
        "jamaica" => "JM",
        "haiti" => "HT",
        "dominican republic" => "DO",
        "puerto rico" => "PR",
        "trinidad and tobago" => "TT",
        "barbados" => "BB",
        "bahamas" | "the bahamas" => "BS",
        "bermuda" => "BM",
        "cayman islands" => "KY",
        "british virgin islands" | "virgin islands, british" => "VG",
        "us virgin islands" | "virgin islands, u.s." => "VI",
        "anguilla" => "AI",
        "antigua and barbuda" => "AG",
        "dominica" => "DM",
        "grenada" => "GD",
        "saint lucia" => "LC",
        "saint vincent and the grenadines" => "VC",
        "saint kitts and nevis" => "KN",
        "montserrat" => "MS",
        "aruba" => "AW",
        "curacao" | "curaçao" => "CW",
        "guadeloupe" => "GP",
        "martinique" => "MQ",
        "saint martin" => "MF",
        "turks and caicos islands" => "TC",
        "greenland" => "GL",
        "brazil" => "BR",
        "argentina" => "AR",
        "chile" => "CL",
        "colombia" => "CO",
        "peru" => "PE",
        "venezuela" => "VE",
        "ecuador" => "EC",
        "bolivia" => "BO",
        "paraguay" => "PY",
        "uruguay" => "UY",
        "guyana" => "GY",
        "suriname" => "SR",
        "french guiana" => "GF",
        "united kingdom" | "great britain" | "uk" | "gb" => "GB",
        "ireland" => "IE",
        "france" => "FR",
        "germany" => "DE",
        "netherlands" | "the netherlands" => "NL",
        "belgium" => "BE",
        "luxembourg" => "LU",
        "switzerland" => "CH",
        "austria" => "AT",
        "italy" => "IT",
        "spain" => "ES",
        "portugal" => "PT",
        "sweden" => "SE",
        "norway" => "NO",
        "denmark" => "DK",
        "finland" => "FI",
        "iceland" => "IS",
        "poland" => "PL",
        "czechia" | "czech republic" | "czech" => "CZ",
        "slovakia" => "SK",
        "slovenia" => "SI",
        "hungary" => "HU",
        "croatia" => "HR",
        "bosnia and herzegovina" => "BA",
        "serbia" => "RS",
        "montenegro" => "ME",
        "north macedonia" | "macedonia" => "MK",
        "albania" => "AL",
        "greece" => "GR",
        "romania" => "RO",
        "bulgaria" => "BG",
        "moldova" => "MD",
        "ukraine" => "UA",
        "belarus" => "BY",
        "russia" | "russian federation" => "RU",
        "lithuania" => "LT",
        "latvia" => "LV",
        "estonia" => "EE",
        "malta" => "MT",
        "cyprus" => "CY",
        "andorra" => "AD",
        "monaco" => "MC",
        "liechtenstein" => "LI",
        "san marino" => "SM",
        "vatican" | "vatican city" | "holy see" => "VA",
        "gibraltar" => "GI",
        "isle of man" => "IM",
        "jersey" => "JE",
        "guernsey" => "GG",
        "faroe islands" => "FO",
        "aland islands" | "åland islands" => "AX",
        "svalbard" => "SJ",
        "turkey" | "turkiye" => "TR",
        "georgia" => "GE",
        "armenia" => "AM",
        "azerbaijan" => "AZ",
        "kazakhstan" => "KZ",
        "kyrgyzstan" => "KG",
        "uzbekistan" => "UZ",
        "tajikistan" => "TJ",
        "turkmenistan" => "TM",
        "mongolia" => "MN",
        "china" => "CN",
        "hong kong" => "HK",
        "macau" | "macao" => "MO",
        "taiwan" => "TW",
        "japan" => "JP",
        "south korea" | "korea" | "korea, south" => "KR",
        "north korea" | "korea, north" => "KP",
        "india" => "IN",
        "pakistan" => "PK",
        "bangladesh" => "BD",
        "sri lanka" => "LK",
        "nepal" => "NP",
        "bhutan" => "BT",
        "myanmar" | "burma" => "MM",
        "thailand" => "TH",
        "laos" => "LA",
        "vietnam" | "viet nam" => "VN",
        "cambodia" => "KH",
        "malaysia" => "MY",
        "singapore" => "SG",
        "indonesia" => "ID",
        "philippines" => "PH",
        "brunei" => "BN",
        "timor-leste" | "east timor" => "TL",
        "papua new guinea" => "PG",
        "fiji" => "FJ",
        "tonga" => "TO",
        "samoa" => "WS",
        "vanuatu" => "VU",
        "solomon islands" => "SB",
        "new caledonia" => "NC",
        "french polynesia" => "PF",
        "guam" => "GU",
        "american samoa" => "AS",
        "northern mariana islands" => "MP",
        "australia" => "AU",
        "new zealand" => "NZ",
        "israel" => "IL",
        "palestine" => "PS",
        "jordan" => "JO",
        "lebanon" => "LB",
        "syria" => "SY",
        "iraq" => "IQ",
        "iran" => "IR",
        "afghanistan" => "AF",
        "saudi arabia" => "SA",
        "yemen" => "YE",
        "oman" => "OM",
        "united arab emirates" | "uae" => "AE",
        "qatar" => "QA",
        "bahrain" => "BH",
        "kuwait" => "KW",
        "egypt" => "EG",
        "libya" => "LY",
        "tunisia" => "TN",
        "algeria" => "DZ",
        "morocco" => "MA",
        "western sahara" => "EH",
        "mauritania" => "MR",
        "mali" => "ML",
        "niger" => "NE",
        "chad" => "TD",
        "sudan" => "SD",
        "south sudan" => "SS",
        "ethiopia" => "ET",
        "eritrea" => "ER",
        "djibouti" => "DJ",
        "somalia" => "SO",
        "kenya" => "KE",
        "uganda" => "UG",
        "tanzania" => "TZ",
        "rwanda" => "RW",
        "burundi" => "BI",
        "congo" | "congo, rep." => "CG",
        "democratic republic of the congo" | "congo, dem. rep." | "congo, democratic republic of the" => "CD",
        "gabon" => "GA",
        "equatorial guinea" => "GQ",
        "cameroon" => "CM",
        "central african republic" => "CF",
        "nigeria" => "NG",
        "benin" => "BJ",
        "togo" => "TG",
        "ghana" => "GH",
        "ivory coast" | "côte d'ivoire" | "cote d'ivoire" => "CI",
        "liberia" => "LR",
        "sierra leone" => "SL",
        "guinea" => "GN",
        "guinea-bissau" => "GW",
        "senegal" => "SN",
        "gambia" | "the gambia" => "GM",
        "burkina faso" => "BF",
        "cape verde" | "cabo verde" => "CV",
        "sao tome and principe" => "ST",
        "angola" => "AO",
        "zambia" => "ZM",
        "zimbabwe" => "ZW",
        "malawi" => "MW",
        "mozambique" => "MZ",
        "botswana" => "BW",
        "namibia" => "NA",
        "south africa" => "ZA",
        "lesotho" => "LS",
        "eswatini" | "swaziland" => "SZ",
        "madagascar" => "MG",
        "mauritius" => "MU",
        "seychelles" => "SC",
        "comoros" => "KM",
        "mayotte" => "YT",
        "reunion" => "RE",
        _ => "",
    }
}

/// Zero the legacy probe fields of every stack the user has not
/// configured (see the get_all_info call site). `ping` is the normalized
/// effective config, so cu_addr already equals the effective v4 address.
fn zero_unconfigured_probe_fields(
    srv: &mut serde_json::Map<String, serde_json::Value>,
    ping: &crate::config::PingConfig,
) {
    let mut zero = |time_key: &str, loss_key: &str| {
        srv.insert(time_key.to_string(), serde_json::json!(0.0));
        srv.insert(loss_key.to_string(), serde_json::json!(0.0));
    };
    if !ping.enabled || ping.cu_addr.is_empty() {
        zero("time_10010", "ping_10010");
    }
    if !ping.enabled || ping.ct_addr.is_empty() {
        zero("time_189", "ping_189");
    }
    if !ping.enabled || ping.cm_addr.is_empty() {
        zero("time_10086", "ping_10086");
    }
    if !ping.enabled || ping.cu_addr_v6.is_empty() {
        zero("time_10010_v6", "ping_10010_v6");
    }
    if !ping.enabled || ping.ct_addr_v6.is_empty() {
        zero("time_189_v6", "ping_189_v6");
    }
    if !ping.enabled || ping.cm_addr_v6.is_empty() {
        zero("time_10086_v6", "ping_10086_v6");
    }
}

/// Probe latency rows for one server in stats.json (see plan-overlay.js).
/// Only probes/stacks the user actually configured are included; when
/// probing is disabled the list is empty. The legacy time_*/ping_* fields
/// are untouched, so the precompiled React panel keeps working.
fn ping_probes_json(ping: &crate::config::PingConfig, stat: &HostStat) -> serde_json::Value {
    if !ping.enabled {
        return serde_json::json!([]);
    }
    let mut out = Vec::new();
    let mut push = |slot: &str,
                    name: &str,
                    v4_set: bool,
                    time_v4: f64,
                    loss_v4: f64,
                    v6_set: bool,
                    time_v6: f64,
                    loss_v6: f64| {
        if !v4_set && !v6_set {
            return;
        }
        // `slot` is the stable probe key (cu/ct/cm): the latency
        // chart joins server-side history (keyed by slot) against
        // these items, so a probe rename keeps one continuous series.
        let mut item = serde_json::json!({ "name": name, "slot": slot });
        if v4_set {
            item["v4"] = serde_json::json!({ "time": time_v4, "loss": loss_v4 });
        }
        if v6_set {
            item["v6"] = serde_json::json!({ "time": time_v6, "loss": loss_v6 });
        }
        out.push(item);
    };
    push(
        "cu",
        &ping.cu_name,
        !ping.cu_addr.is_empty(),
        stat.time_10010,
        stat.ping_10010,
        !ping.cu_addr_v6.is_empty(),
        stat.time_10010_v6,
        stat.ping_10010_v6,
    );
    push(
        "ct",
        &ping.ct_name,
        !ping.ct_addr.is_empty(),
        stat.time_189,
        stat.ping_189,
        !ping.ct_addr_v6.is_empty(),
        stat.time_189_v6,
        stat.ping_189_v6,
    );
    push(
        "cm",
        &ping.cm_name,
        !ping.cm_addr.is_empty(),
        stat.time_10086,
        stat.ping_10086,
        !ping.cm_addr_v6.is_empty(),
        stat.time_10086_v6,
        stat.ping_10086_v6,
    );
    serde_json::Value::Array(out)
}

#[cfg(test)]
mod country_tests {
    use super::*;

    #[test]
    fn country_code_mapping_covers_live_fleet() {
        // Names actually seen in the fleet's ip_info.country values.
        assert_eq!(country_code_for_country("Hong Kong"), "HK");
        assert_eq!(country_code_for_country("United States"), "US");
        assert_eq!(country_code_for_country("Japan"), "JP");
        assert_eq!(country_code_for_country("Germany"), "DE");
        assert_eq!(country_code_for_country("Netherlands"), "NL");
        assert_eq!(country_code_for_country("United Kingdom"), "GB");
        assert_eq!(country_code_for_country("Singapore"), "SG");
        assert_eq!(country_code_for_country("France"), "FR");
        assert_eq!(country_code_for_country("The Netherlands"), "NL");
        assert_eq!(country_code_for_country("  japan  "), "JP");
    }

    #[test]
    fn unknown_country_yields_empty_not_a_guess() {
        assert_eq!(country_code_for_country(""), "");
        assert_eq!(country_code_for_country("Somewhere New"), "");
    }

    #[test]
    fn unconfigured_probe_stacks_zero_out_legacy_fields() {
        // Batch 3 probe consistency: only the configured stack keeps its
        // live numbers; every unconfigured stack (and every stack when
        // probing is disabled) must read 0, never a stale measurement
        // from before the config was emptied.
        let mut srv = serde_json::json!({
            "time_10010": 42.0, "ping_10010": 0.0,
            "time_189": 39.0, "ping_189": 0.0,
            "time_10086": 344.0, "ping_10086": 0.0,
            "time_10010_v6": 55.0, "ping_10010_v6": 0.0,
        })
        .as_object()
        .unwrap()
        .clone();
        let mut ping = crate::config::PingConfig::default();
        ping.enabled = true;
        ping.cu_addr = "1.1.1.1:80".to_string();
        ping.ct_addr = String::new();
        ping.cm_addr = String::new();
        ping.cu_addr_v6 = String::new();
        zero_unconfigured_probe_fields(&mut srv, &ping);
        assert_eq!(srv["time_10010"], 42.0);
        assert_eq!(srv["time_189"], 0.0);
        assert_eq!(srv["time_10086"], 0.0);
        assert_eq!(srv["time_10010_v6"], 0.0);
        ping.enabled = false;
        zero_unconfigured_probe_fields(&mut srv, &ping);
        assert_eq!(srv["time_10010"], 0.0);
    }
}
