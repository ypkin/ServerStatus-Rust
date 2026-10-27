#![deny(warnings)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use uuid::Uuid;

use crate::notifier;

fn default_as_true() -> bool {
    true
}
fn default_grpc_addr() -> String {
    "0.0.0.0:9394".to_string()
}
fn default_http_addr() -> String {
    "0.0.0.0:8080".to_string()
}
fn default_workspace() -> String {
    "/opt/ServerStatus".to_string()
}
fn default_tls_dir() -> String {
    "tls".to_string()
}
fn default_ping_cu() -> String {
    // No prefilled probe addresses: empty means "user has not set this
    // probe", and unset probes are skipped, never probed with a default.
    String::new()
}
fn default_ping_ct() -> String {
    String::new()
}
fn default_ping_cm() -> String {
    String::new()
}
fn default_ping_cu_name() -> String {
    "联通".to_string()
}
fn default_ping_ct_name() -> String {
    "电信".to_string()
}
fn default_ping_cm_name() -> String {
    "移动".to_string()
}
fn default_traffic_alert_percent() -> u32 {
    80
}

/// Server-managed three-carrier probe settings (fork). These are the
/// defaults; the admin panel may override them at runtime and the override
/// is persisted in `admin.json` (see `admin.rs`). Clients fetch the
/// effective values on every report instead of needing per-host CLI flags.
///
/// Each carrier has a user-editable display name (default 联通/电信/移动)
/// plus an IPv4 and an IPv6 probe address. The `cu_addr` / `ct_addr` /
/// `cm_addr` keys are the legacy spelling and stay the canonical IPv4
/// storage; `*_addr_v4` are explicit aliases that win when non-empty, and
/// an empty explicit v4 falls back to the legacy key (so configs written
/// before the v4/v6 split keep working unchanged).
/// An empty address means "user has not set this probe/stack": it is
/// skipped, never probed with a prefilled default. There are no default
/// probe addresses anymore.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PingConfig {
    #[serde(default = "default_as_true")]
    pub enabled: bool,
    #[serde(default = "default_ping_cu_name")]
    pub cu_name: String,
    #[serde(default = "default_ping_ct_name")]
    pub ct_name: String,
    #[serde(default = "default_ping_cm_name")]
    pub cm_name: String,
    #[serde(default = "default_ping_cu")]
    pub cu_addr: String,
    #[serde(default = "default_ping_ct")]
    pub ct_addr: String,
    #[serde(default = "default_ping_cm")]
    pub cm_addr: String,
    #[serde(default)]
    pub cu_addr_v4: String,
    #[serde(default)]
    pub ct_addr_v4: String,
    #[serde(default)]
    pub cm_addr_v4: String,
    #[serde(default)]
    pub cu_addr_v6: String,
    #[serde(default)]
    pub ct_addr_v6: String,
    #[serde(default)]
    pub cm_addr_v6: String,
}

impl PingConfig {
    pub fn v4_cu(&self) -> &str {
        if self.cu_addr_v4.is_empty() {
            &self.cu_addr
        } else {
            &self.cu_addr_v4
        }
    }

    pub fn v4_ct(&self) -> &str {
        if self.ct_addr_v4.is_empty() {
            &self.ct_addr
        } else {
            &self.ct_addr_v4
        }
    }

    pub fn v4_cm(&self) -> &str {
        if self.cm_addr_v4.is_empty() {
            &self.cm_addr
        } else {
            &self.cm_addr_v4
        }
    }

    /// Clone with every IPv4 key (legacy + explicit) set to the effective
    /// IPv4 address, so API responses never disagree between the two
    /// spellings, and with empty display names reset to the carrier
    /// defaults. Every address is additionally stripped to its bare host
    /// (port and v6 brackets removed): ICMP probes ping hosts, and this
    /// is what makes old TCP-era saved values keep working without
    /// re-entry, both at save time (`put_ping`) and at send time
    /// (`effective_ping` feeds gRPC and HTTP /report).
    pub fn normalized(&self) -> Self {
        let mut p = self.clone();
        p.cu_addr = self.v4_cu().to_string();
        p.cu_addr_v4 = p.cu_addr.clone();
        p.ct_addr = self.v4_ct().to_string();
        p.ct_addr_v4 = p.ct_addr.clone();
        p.cm_addr = self.v4_cm().to_string();
        p.cm_addr_v4 = p.cm_addr.clone();
        for addr in [
            &mut p.cu_addr,
            &mut p.cu_addr_v4,
            &mut p.ct_addr,
            &mut p.ct_addr_v4,
            &mut p.cm_addr,
            &mut p.cm_addr_v4,
            &mut p.cu_addr_v6,
            &mut p.ct_addr_v6,
            &mut p.cm_addr_v6,
        ] {
            *addr = stat_common::probe::normalize_probe_target(addr);
        }
        if p.cu_name.is_empty() {
            p.cu_name = default_ping_cu_name();
        }
        if p.ct_name.is_empty() {
            p.ct_name = default_ping_ct_name();
        }
        if p.cm_name.is_empty() {
            p.cm_name = default_ping_cm_name();
        }
        p
    }
}

impl Default for PingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cu_name: default_ping_cu_name(),
            ct_name: default_ping_ct_name(),
            cm_name: default_ping_cm_name(),
            cu_addr: default_ping_cu(),
            ct_addr: default_ping_ct(),
            cm_addr: default_ping_cm(),
            cu_addr_v4: String::new(),
            ct_addr_v4: String::new(),
            cm_addr_v4: String::new(),
            cu_addr_v6: String::new(),
            ct_addr_v6: String::new(),
            cm_addr_v6: String::new(),
        }
    }
}

#[cfg(test)]
mod ping_tests {
    use super::*;

    #[test]
    fn v4_falls_back_to_legacy_key() {
        // No prefilled defaults anymore: a fresh config probes nothing.
        let p = PingConfig::default();
        assert!(p.v4_cu().is_empty());
        assert!(p.v4_ct().is_empty());
        assert!(p.v4_cm().is_empty());
        assert_eq!(p.cu_name, "联通");
        // Pre-split admin.json has no v4/v6/name keys at all.
        let legacy: PingConfig =
            serde_json::from_str(r#"{"enabled":true,"cu_addr":"a:80","ct_addr":"b:80","cm_addr":"c:80"}"#)
                .unwrap();
        assert_eq!(legacy.v4_cu(), "a:80");
        assert!(legacy.cu_addr_v6.is_empty());
        let n = legacy.normalized();
        // ICMP era: the TCP-era port is stripped at normalization.
        assert_eq!(n.cu_addr, "a");
        assert_eq!(n.cu_addr_v4, "a");
        assert_eq!(n.cu_name, "联通");
    }

    #[test]
    fn explicit_v4_wins_and_normalized_agrees() {
        let mut p = PingConfig::default();
        p.cu_addr_v4 = "10.0.0.1:80".to_string();
        p.cu_addr_v6 = "[2001:db8::1]:80".to_string();
        assert_eq!(p.v4_cu(), "10.0.0.1:80");
        let n = p.normalized();
        assert_eq!(n.cu_addr, "10.0.0.1");
        assert_eq!(n.cu_addr_v4, "10.0.0.1");
        assert_eq!(n.cu_addr_v6, "2001:db8::1");
    }

    #[test]
    fn normalized_strips_ports_on_all_six_targets() {
        let mut p = PingConfig::default();
        p.cu_addr = "cuccv4.passall.us:8443".to_string();
        p.ct_addr = "ct.example.com:80".to_string();
        p.cm_addr = "cm.tz.cloudcpp.com:80".to_string();
        p.cu_addr_v6 = "cuccv6.passall.us:8443".to_string();
        p.ct_addr_v6 = String::new();
        p.cm_addr_v6 = "op2.passall.us:8443".to_string();
        let n = p.normalized();
        assert_eq!(n.cu_addr, "cuccv4.passall.us");
        assert_eq!(n.cu_addr_v4, "cuccv4.passall.us");
        assert_eq!(n.ct_addr, "ct.example.com");
        assert_eq!(n.cm_addr, "cm.tz.cloudcpp.com");
        assert_eq!(n.cu_addr_v6, "cuccv6.passall.us");
        assert_eq!(n.ct_addr_v6, "");
        assert_eq!(n.cm_addr_v6, "op2.passall.us");
    }

    #[test]
    fn custom_probe_names_survive_round_trip() {
        // Rename path (batch-3 check): a user-edited probe name must
        // survive deserialize + normalized() and reach ping_probes_json.
        // Only an *empty* name falls back to the default.
        let p: PingConfig = serde_json::from_str(
            r#"{"enabled":true,"cu_name":"自家探针","ct_name":"","cm_name":"移动"}"#,
        )
        .unwrap();
        let n = p.normalized();
        assert_eq!(n.cu_name, "自家探针");
        assert_eq!(n.ct_name, "电信");
        assert_eq!(n.cm_name, "移动");
    }

    #[test]
    fn currency_normalization() {
        assert_eq!(normalize_currency(""), "$");
        assert_eq!(normalize_currency("  ¥  "), "¥");
        assert_eq!(normalize_currency("HK$"), "HK$");
        // Long pasted text is capped, never stored verbatim.
        assert_eq!(normalize_currency("0123456789ABCDEF").len(), 8);
    }

    #[test]
    fn host_without_currency_defaults_to_dollar() {
        // Old admin.json records predate the field: value and meaning
        // both stay "$" (existing fees are not re-denominated).
        let h: Host = serde_json::from_str(r#"{"name":"x","password":"p"}"#).unwrap();
        assert_eq!(h.currency, "$");
    }
}

pub fn normalize_traffic_mode(mode: &str) -> String {
    match mode {
        "up" | "down" | "max" => mode.to_string(),
        _ => "sum".to_string(),
    }
}

/// Annual fee currency symbol (fork, per-host). "$" when unset; a custom
/// symbol is stored verbatim (trimmed, capped at 8 chars so a pasted
/// sentence cannot blow up the table cell).
pub fn normalize_currency(s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return "$".to_string();
    }
    t.chars().take(8).collect()
}

fn default_currency() -> String {
    "$".to_string()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Host {
    pub name: String,
    pub password: String,
    #[serde(default = "Default::default")]
    pub alias: String,
    #[serde(default = "Default::default")]
    pub location: String,
    #[serde(default = "Default::default")]
    pub r#type: String,
    #[serde(default = "u32::default")]
    pub monthstart: u32,
    #[serde(default = "default_as_true")]
    pub notify: bool,
    #[serde(default = "bool::default")]
    pub disabled: bool,
    #[serde(default = "Default::default")]
    pub labels: String,
    /// Traffic allowance in bytes, 0 = unlimited (fork, monitor model);
    /// a literal 9999 GiB is a real quota, not unlimited
    /// (see traffic::is_unlimited).
    #[serde(default = "Default::default")]
    pub traffic_limit: u64,
    /// Metering mode: sum | up | down | max (fork, default sum).
    #[serde(default = "Default::default")]
    pub traffic_mode: String,
    /// Plan start date, YYYY-MM-DD, empty = unset (fork, backend-managed).
    #[serde(default = "Default::default")]
    pub plan_start_date: String,
    /// Plan end date, YYYY-MM-DD, empty = unset (fork, batch 3). It is
    /// only ever what the user typed in the admin form: never derived
    /// from the annual fee or the start date, never fabricated.
    #[serde(default = "Default::default")]
    pub plan_end_date: String,
    /// Annual fee in the panel's currency (e.g. USD), 0 = unset (fork).
    #[serde(default = "Default::default")]
    pub annual_fee: f64,
    /// Currency symbol for annual_fee (fork, per-host). Records written
    /// before this field existed deserialize to "$", so existing fees
    /// keep their value and their dollar meaning.
    #[serde(default = "default_currency")]
    pub currency: String,
    /// Advertised bandwidth in Mbps, 0 = unset (fork).
    #[serde(default = "Default::default")]
    pub bandwidth_mbps: u32,
    /// Display group tag, nezha-style free text (fork). Empty = Default
    /// group. Unlike gid (auth/template membership), tag never affects
    /// auth; it only drives admin filtering and main-panel grouping.
    #[serde(default = "Default::default")]
    pub tag: String,
    /// Admin-only note, never sent to the main panel (fork).
    #[serde(default = "Default::default")]
    pub note: String,
    /// Public note, shown on main panel cards via the overlay (fork).
    #[serde(default = "Default::default")]
    pub public_note: String,

    #[serde(skip_deserializing)]
    pub last_network_in: u64,
    #[serde(skip_deserializing)]
    pub last_network_out: u64,

    // user data
    #[serde(skip_serializing, skip_deserializing)]
    pub pos: usize,
    #[serde(default = "Default::default")]
    pub weight: u64,
    #[serde(default = "Default::default")]
    pub gid: String,
    #[serde(default = "Default::default")]
    pub latest_ts: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostGroup {
    pub gid: String,
    pub password: String,
    #[serde(default = "Default::default")]
    pub location: String,
    #[serde(default = "Default::default")]
    pub r#type: String,
    #[serde(default = "default_as_true")]
    pub notify: bool,
    #[serde(default = "Default::default")]
    pub traffic_limit: u64,
    #[serde(default = "Default::default")]
    pub traffic_mode: String,
    /// Plan defaults for members (fork, see Host).
    #[serde(default = "Default::default")]
    pub plan_start_date: String,
    #[serde(default = "Default::default")]
    pub annual_fee: f64,
    #[serde(default = "Default::default")]
    pub bandwidth_mbps: u32,
    // user data
    #[serde(skip_serializing, skip_deserializing)]
    pub pos: usize,
    #[serde(default = "Default::default", skip_serializing)]
    pub weight: u64,
    #[serde(default = "Default::default")]
    pub labels: String,
}

impl HostGroup {
    pub fn inst_host(&self, name: &str) -> Host {
        Host {
            name: name.to_owned(),
            gid: self.gid.clone(),
            password: self.password.clone(),
            location: self.location.clone(),
            r#type: self.r#type.clone(),
            monthstart: 1,
            notify: self.notify,
            traffic_limit: self.traffic_limit,
            traffic_mode: self.traffic_mode.clone(),
            plan_start_date: self.plan_start_date.clone(),
            annual_fee: self.annual_fee,
            bandwidth_mbps: self.bandwidth_mbps,
            pos: self.pos,
            weight: self.weight,
            labels: self.labels.clone(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_http_addr")]
    pub http_addr: String,
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
    #[serde(default = "Default::default")]
    pub notify_interval: u64,
    #[serde(default = "Default::default")]
    pub offline_threshold: u64,
    #[serde(default = "Default::default")]
    pub grpc_tls: u32,
    #[serde(default = "default_tls_dir")]
    pub tls_dir: String,
    // admin user & pass
    pub admin_user: Option<String>,
    pub admin_pass: Option<String>,
    pub jwt_secret: Option<String>,

    #[serde(default = "Default::default")]
    pub tgbot: notifier::tgbot::Config,
    #[serde(default = "Default::default")]
    pub wechat: notifier::wechat::Config,
    #[serde(default = "Default::default")]
    pub email: notifier::email::Config,
    #[serde(default = "Default::default")]
    pub log: notifier::log::Config,
    #[serde(default = "Default::default")]
    pub webhook: notifier::webhook::Config,
    #[serde(default = "Default::default")]
    pub pushplus: notifier::pushplus::Config,

    /// Three-carrier probe defaults (fork, overridable from the admin panel).
    #[serde(default = "Default::default")]
    pub ping: PingConfig,
    /// Traffic-alert step in percent, e.g. 80 alerts at 80% and at 100%
    /// (fork). Clamped to 1..=100 at load.
    #[serde(default = "default_traffic_alert_percent")]
    pub traffic_alert_percent: u32,

    #[serde(default = "Default::default")]
    pub hosts: Vec<Host>,
    #[serde(default = "Default::default")]
    pub hosts_group: Vec<HostGroup>,
    #[serde(default = "Default::default")]
    pub group_gc: u64,

    // deploy
    #[serde(default = "Default::default")]
    pub server_url: String,
    #[serde(default = "default_workspace")]
    pub workspace: String,

    #[serde(skip_deserializing)]
    pub hosts_map: HashMap<String, Host>,

    #[serde(skip_deserializing)]
    pub hosts_group_map: HashMap<String, HostGroup>,
}

impl Config {
    pub fn auth(&self, user: &str, pass: &str) -> bool {
        if let Some(o) = self.hosts_map.get(user) {
            return pass.eq(o.password.as_str());
        }
        false
    }
    pub fn group_auth(&self, gid: &str, pass: &str) -> bool {
        if let Some(o) = self.hosts_group_map.get(gid) {
            return pass.eq(o.password.as_str());
        }
        false
    }
    pub fn admin_auth(&self, user: &str, pass: &str) -> bool {
        if let (Some(u), Some(p)) = (self.admin_user.as_ref(), self.admin_pass.as_ref()) {
            return user.eq(u.as_str()) && pass.eq(p.as_str());
        }
        false
    }

    pub fn to_json_value(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(anyhow::Error::new)
    }
}

pub fn from_str(content: &str) -> Option<Config> {
    let mut o = toml::from_str::<Config>(content).unwrap();
    o.hosts_map = HashMap::new();

    for (idx, host) in o.hosts.iter_mut().enumerate() {
        host.pos = idx;
        if host.alias.is_empty() {
            host.alias = host.name.clone();
        }
        if host.monthstart < 1 || host.monthstart > 31 {
            host.monthstart = 1;
        }
        host.traffic_mode = normalize_traffic_mode(&host.traffic_mode);
        host.weight = 10000_u64 - idx as u64;
        o.hosts_map.insert(host.name.clone(), host.clone());
    }

    for group in o.hosts_group.iter_mut() {
        group.traffic_mode = normalize_traffic_mode(&group.traffic_mode);
    }

    if o.traffic_alert_percent < 1 || o.traffic_alert_percent > 100 {
        o.traffic_alert_percent = 80;
    }

    for (idx, group) in o.hosts_group.iter_mut().enumerate() {
        group.pos = idx;
        group.weight = (10000 - (1 + idx) * 100) as u64;
        o.hosts_group_map.insert(group.gid.clone(), group.clone());
    }

    if o.offline_threshold < 30 {
        o.offline_threshold = 30;
    }
    if o.notify_interval < 30 {
        o.notify_interval = 30;
    }
    if o.group_gc < 30 {
        o.group_gc = 30;
    }

    if o.admin_user.is_none() || o.admin_user.as_ref()?.is_empty() {
        o.admin_user = Some("admin".to_string());
    }
    if o.admin_pass.is_none() || o.admin_pass.as_ref()?.is_empty() {
        o.admin_pass = Some(Uuid::new_v4().to_string());
    }
    if o.jwt_secret.is_none() || o.jwt_secret.as_ref()?.is_empty() {
        o.jwt_secret = Some(Uuid::new_v4().to_string());
    }

    Some(o)
}

pub fn from_env() -> Option<Config> {
    from_str(
        env::var("SRV_CONF")
            .expect("can't load config from env `SRV_CONF")
            .as_str(),
    )
}

pub fn from_file(cfg: &str) -> Option<Config> {
    fs::read_to_string(cfg)
        .map(|contents| from_str(contents.as_str()))
        .ok()?
}

pub fn test_from_file(cfg: &str) -> Result<Config> {
    fs::read_to_string(cfg)
        .map(|contents| toml::from_str::<Config>(&contents))
        .unwrap()
        .map_err(anyhow::Error::new)
}
