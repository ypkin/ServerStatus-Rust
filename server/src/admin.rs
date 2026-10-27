//! Admin backend (fork): operator settings that monitor keeps in its
//! database, adapted to this server's file-based model.
//!
//! Two files sit next to `stats.json`:
//! - `admin.json`: operator settings (ping overrides, per-host traffic
//!   settings). Written only by the admin API.
//! - `traffic.json`: accounting state, owned by `traffic.rs`.
//!
//! Effective settings are always base (`config.toml`) merged with the
//! admin override, computed on every report, so an admin change takes
//! effect on the next report without a restart and without rewriting
//! `config.toml` (which would lose comments and formatting).

use axum::{
    extract::{Json, Path},
    http::{header::HeaderMap, StatusCode, Uri},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::sync::{LazyLock, Mutex};

use crate::config::{normalize_traffic_mode, Host, HostGroup, PingConfig};
use crate::jwt;
use crate::traffic::{month_used, TrafficState, TRAFFIC};
use crate::G_CONFIG;

const ADMIN_FILE: &str = "admin.json";

/// Per-host traffic settings as stored in `admin.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostTrafficOverride {
    pub traffic_limit: u64,
    pub traffic_mode: String,
    pub monthstart: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminStore {
    #[serde(default)]
    pub ping: Option<PingConfig>,
    #[serde(default)]
    pub hosts: HashMap<String, HostTrafficOverride>,
    /// Nodes created or adopted in the panel (full Host records).
    /// These overlay config.toml hosts of the same name at startup and at
    /// runtime, so panel edits survive restarts without rewriting config.
    #[serde(default)]
    pub admin_hosts: Vec<Host>,
    /// Config hosts deleted in the panel: kept as tombstones so a restart
    /// does not resurrect them from config.toml.
    #[serde(default)]
    pub deleted_hosts: Vec<String>,
    /// PushPlus runtime override (see notifier::pushplus). None = use
    /// config.toml [pushplus]. The token lives here, never in stats.json.
    #[serde(default)]
    pub pushplus: Option<crate::notifier::pushplus::Config>,
    /// Groups created or adopted in the panel (full HostGroup records).
    #[serde(default)]
    pub admin_groups: Vec<crate::config::HostGroup>,
    /// Config groups deleted in the panel (tombstones, like deleted_hosts).
    #[serde(default)]
    pub deleted_groups: Vec<String>,
}

static ADMIN: LazyLock<Mutex<AdminStore>> = LazyLock::new(Default::default);

/// Load `admin.json` if present. Call once at startup, after `G_CONFIG` is
/// set; before that, effective settings fall back to config defaults.
///
/// Invariant (user rule, 2026-10-27): `plan_end_date` is never derived.
/// An empty end stays empty through load/save round-trips; a filled end
/// is kept verbatim. Nothing in this file computes an end date from the
/// start date, the fee, or anything else.
///
/// Footgun, proven 2026-10-27: `ADMIN` is loaded once here and then held
/// in memory. Editing `admin.json` on disk while the server runs does NOT
/// update memory, and the next admin API save rewrites the whole file
/// from stale memory — silently resurrecting values removed on disk
/// (that is how six emptied `plan_end_date` values came back on
/// 2026-10-27). External edits to `admin.json` therefore require a
/// restart (stop → edit → start) so memory reloads the edited file.
pub fn init_admin_store() {
    let mut store = AdminStore::default();
    if let Ok(contents) = fs::read_to_string(ADMIN_FILE) {
        if let Ok(parsed) = serde_json::from_str::<AdminStore>(&contents) {
            store = parsed;
        } else {
            error!("ignore invalid {ADMIN_FILE}");
        }
    }
    *ADMIN.lock().unwrap() = store;
}

fn save_admin_store() -> Result<(), String> {
    let data = serde_json::to_string(&*ADMIN.lock().unwrap()).map_err(|e| e.to_string())?;
    fs::write(format!("{ADMIN_FILE}.tmp"), &data).map_err(|e| e.to_string())?;
    fs::rename(format!("{ADMIN_FILE}.tmp"), ADMIN_FILE).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(ADMIN_FILE, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Effective ping settings: admin override wins, otherwise config defaults.
/// The result is normalized, so the legacy v4 keys always equal the
/// effective v4 addresses even when the stored config only used one
/// spelling (pre-split files, older cached admin pages).
pub fn effective_ping() -> PingConfig {
    if let Some(p) = ADMIN.lock().unwrap().ping.clone() {
        return p.normalized();
    }
    match G_CONFIG.get() {
        Some(cfg) => cfg.ping.clone().normalized(),
        None => PingConfig::default(),
    }
}

/// Effective per-host traffic settings: admin override wins, otherwise the
/// host's own config (explicit host or group template).
pub fn effective_host_settings(name: &str, base: &Host) -> (u64, String, u32) {
    if let Some(o) = ADMIN.lock().unwrap().hosts.get(name) {
        return (o.traffic_limit, normalize_traffic_mode(&o.traffic_mode), o.monthstart);
    }
    (base.traffic_limit, normalize_traffic_mode(&base.traffic_mode), base.monthstart)
}

/// Admin-managed hosts (panel-created or adopted), cloned.
pub fn admin_hosts() -> Vec<Host> {
    ADMIN.lock().unwrap().admin_hosts.clone()
}

pub fn admin_host(name: &str) -> Option<Host> {
    ADMIN.lock().unwrap().admin_hosts.iter().find(|h| h.name == name).cloned()
}

pub fn deleted_hosts() -> Vec<String> {
    ADMIN.lock().unwrap().deleted_hosts.clone()
}

pub fn is_deleted_host(name: &str) -> bool {
    ADMIN.lock().unwrap().deleted_hosts.iter().any(|n| n == name)
}

/// Effective single-host authentication, in precedence order:
/// tombstoned -> deny; admin-managed host -> its password only;
/// otherwise config.toml. Group auth is handled by the caller.
pub fn check_host_auth(user: &str, pass: &str) -> bool {
    if is_deleted_host(user) {
        return false;
    }
    if let Some(h) = admin_host(user) {
        return pass == h.password;
    }
    match G_CONFIG.get() {
        Some(cfg) => cfg.auth(user, pass),
        None => false,
    }
}

pub fn admin_groups() -> Vec<crate::config::HostGroup> {
    ADMIN.lock().unwrap().admin_groups.clone()
}

pub fn admin_group(gid: &str) -> Option<crate::config::HostGroup> {
    ADMIN.lock().unwrap().admin_groups.iter().find(|g| g.gid == gid).cloned()
}

pub fn deleted_groups() -> Vec<String> {
    ADMIN.lock().unwrap().deleted_groups.clone()
}

pub fn is_deleted_group(gid: &str) -> bool {
    ADMIN.lock().unwrap().deleted_groups.iter().any(|g| g == gid)
}

/// Effective group authentication, same precedence as hosts:
/// tombstoned -> deny; admin-managed group -> its password only;
/// otherwise config.toml.
pub fn check_group_auth(gid: &str, pass: &str) -> bool {
    if is_deleted_group(gid) {
        return false;
    }
    if let Some(g) = admin_group(gid) {
        return pass == g.password;
    }
    match G_CONFIG.get() {
        Some(cfg) => cfg.group_auth(gid, pass),
        None => false,
    }
}

/// Effective PushPlus config: admin override wins, else config.toml.
pub fn effective_pushplus() -> crate::notifier::pushplus::Config {
    if let Some(p) = ADMIN.lock().unwrap().pushplus.clone() {
        return p;
    }
    match G_CONFIG.get() {
        Some(cfg) => cfg.pushplus.clone(),
        None => crate::notifier::pushplus::Config::default(),
    }
}

fn traffic_summary(name: &str, alias: &str, limit: u64, mode: &str, monthstart: u32, st: &TrafficState) -> Value {
    let used = month_used(st.month_rx, st.month_tx, mode);
    let percent = if !crate::traffic::is_unlimited(limit) { (used as u128 * 100 / limit as u128) as u64 } else { 0 };
    json!({
        "name": name,
        "alias": alias,
        "traffic_limit": limit,
        "traffic_mode": mode,
        "monthstart": monthstart,
        "month_rx": st.month_rx,
        "month_tx": st.month_tx,
        "day_rx": st.day_rx,
        "day_tx": st.day_tx,
        "total_rx": st.total_rx,
        "total_tx": st.total_tx,
        "month_used": used,
        "traffic_percent": percent,
        "month_start": st.month_start,
    })
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, axum::Json<Value>) {
    (status, axum::Json(json!({ "error": msg })))
}

pub async fn get_traffic(_claims: jwt::Claims) -> impl IntoResponse {
    let traffic = TRAFFIC.lock().unwrap().clone();
    let mut names: Vec<(String, String)> = Vec::new();
    {
        let hosts = crate::registry::HOSTS_MAP.lock().unwrap();
        for h in hosts.values() {
            names.push((h.name.clone(), h.alias.clone()));
        }
    }
    for name in traffic.keys() {
        if !names.iter().any(|(n, _)| n == name) {
            names.push((name.clone(), name.clone()));
        }
    }
    let mut out = Vec::new();
    for (name, alias) in names {
        let (limit, mode, monthstart) = {
            let hosts = crate::registry::HOSTS_MAP.lock().unwrap();
            match hosts.get(&name) {
                Some(base) => effective_host_settings(&name, base),
                None => {
                    let base = Host::default();
                    effective_host_settings(&name, &base)
                }
            }
        };
        let st = traffic.get(&name).cloned().unwrap_or_default();
        out.push(traffic_summary(&name, &alias, limit, &mode, monthstart, &st));
    }
    (StatusCode::OK, axum::Json(json!(out)))
}

pub async fn get_hosts(_claims: jwt::Claims) -> impl IntoResponse {
    let admin_names: Vec<String> = ADMIN.lock().unwrap().admin_hosts.iter().map(|h| h.name.clone()).collect();
    let mut out = Vec::new();
    // Collect host records first, then overlay live values after the
    // HOSTS_MAP guard is dropped (never nest registry locks).
    let list: Vec<Host> = {
        let hosts = crate::registry::HOSTS_MAP.lock().unwrap();
        let mut list: Vec<Host> = hosts.values().cloned().collect();
        // Same order as the main panel (stats.rs sorts servers by weight
        // desc, pos asc, alias asc): the admin table, drag-sort and the
        // panel must never disagree about which node is "first".
        list.sort_by(|a, b| {
            if a.weight != b.weight {
                return b.weight.cmp(&a.weight);
            }
            if a.pos != b.pos {
                return a.pos.cmp(&b.pos);
            }
            a.alias.cmp(&b.alias)
        });
        list
    };
    // Live location/type from the latest report (already resolved with
    // explicit > backend > detected precedence in stats.rs). Shown when
    // the host record itself has nothing, so auto-detected values appear
    // in the backend node table without ever being written back into the
    // record (a later re-detection can still replace them).
    let live: HashMap<String, (String, String)> = {
        let stats = crate::registry::STAT_MAP.lock().unwrap();
        stats
            .iter()
            .map(|(n, s)| (n.clone(), (s.location.clone(), s.host_type.clone())))
            .collect()
    };
    for h in &list {
        let (limit, mode, monthstart) = effective_host_settings(&h.name, h);
        let source = if admin_names.contains(&h.name) {
            "admin"
        } else if !h.gid.is_empty() {
            "group"
        } else {
            "config"
        };
        let (mut location, mut host_type) = (h.location.clone(), h.r#type.clone());
        if let Some((live_loc, live_type)) = live.get(&h.name) {
            if location.is_empty() {
                location = live_loc.clone();
            }
            if host_type.is_empty() {
                host_type = live_type.clone();
            }
        }
        out.push(json!({
            "name": h.name, "alias": h.alias, "location": location,
            "type": host_type, "notify": h.notify, "disabled": h.disabled,
            "labels": h.labels, "gid": h.gid, "source": source,
            "traffic_limit": limit, "traffic_mode": mode, "monthstart": monthstart,
            "plan_start_date": h.plan_start_date, "plan_end_date": h.plan_end_date,
            "annual_fee": h.annual_fee,
            "currency": crate::config::normalize_currency(&h.currency),
            "bandwidth_mbps": h.bandwidth_mbps,
            "tag": h.tag, "note": h.note, "public_note": h.public_note,
            "weight": h.weight,
        }));
    }
    (StatusCode::OK, axum::Json(json!(out)))
}

#[derive(Debug, Deserialize)]
pub struct HostTrafficBody {
    pub traffic_limit: u64,
    pub traffic_mode: String,
    pub monthstart: u32,
}

pub async fn put_host_traffic(
    _claims: jwt::Claims,
    Path(name): Path<String>,
    Json(body): Json<HostTrafficBody>,
) -> impl IntoResponse {
    if name.is_empty() {
        return err(StatusCode::BAD_REQUEST, "empty host name");
    }
    if body.monthstart < 1 || body.monthstart > 31 {
        return err(StatusCode::BAD_REQUEST, "monthstart must be 1..=31");
    }
    let ov = HostTrafficOverride {
        traffic_limit: body.traffic_limit,
        traffic_mode: normalize_traffic_mode(&body.traffic_mode),
        monthstart: body.monthstart,
    };
    ADMIN.lock().unwrap().hosts.insert(name.clone(), ov);
    match save_admin_store() {
        Ok(()) => (StatusCode::OK, axum::Json(json!({ "ok": true, "name": name }))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

#[derive(Debug, Deserialize)]
pub struct TrafficCorrectBody {
    pub month_rx: Option<u64>,
    pub month_tx: Option<u64>,
    pub total_rx: Option<u64>,
    pub total_tx: Option<u64>,
    /// Fork: a single "month used" value, aligned to the host's metered
    /// ("used") figure under its billing mode (up: tx, down: rx, max: the
    /// larger side, sum: the sum). Unlike a plain split, alignment keeps
    /// the unmetered direction's real history (up keeps rx, down keeps
    /// tx); see traffic::align_used_to. Later reports accumulate on top
    /// of the aligned baseline.
    pub month_used: Option<u64>,
}

pub async fn put_host_traffic_correct(
    _claims: jwt::Claims,
    Path(name): Path<String>,
    Json(body): Json<TrafficCorrectBody>,
) -> impl IntoResponse {
    if body.month_rx.is_none() && body.month_tx.is_none() && body.total_rx.is_none() && body.total_tx.is_none() && body.month_used.is_none() {
        return err(StatusCode::BAD_REQUEST, "at least one field is required");
    }
    let (limit, mode, monthstart) = match crate::registry::HOSTS_MAP.lock().unwrap().get(&name).cloned() {
        Some(base) => effective_host_settings(&name, &base),
        None => {
            let base = Host::default();
            effective_host_settings(&name, &base)
        }
    };
    // The form's month_used is the USED baseline to align to, not a month
    // sum to split: align it under the host's mode while preserving the
    // unmetered direction (traffic::align_used_to). Do not go back to a
    // plain split here: the previous split version replaced both
    // directions (up wiped rx to 0), destroying real history, and an even
    // earlier inline version had up/down backwards (up put everything
    // into rx), so under up/down modes a correction collapsed the billed
    // figure to ~0 and looked like it "didn't take effect".
    let (month_rx, month_tx) = match body.month_used {
        Some(v) => {
            let (cur_rx, cur_tx) = {
                let traffic = TRAFFIC.lock().unwrap();
                match traffic.get(&name) {
                    Some(st) => (st.month_rx, st.month_tx),
                    None => (0, 0),
                }
            };
            let (rx, tx) = crate::traffic::align_used_to(v, mode.as_str(), cur_rx, cur_tx);
            (Some(rx), Some(tx))
        }
        None => (body.month_rx, body.month_tx),
    };
    let today = chrono::Local::now().date_naive();
    let mut traffic = TRAFFIC.lock().unwrap();
    let st = traffic.entry(name.clone()).or_default();
    st.correct(month_rx, month_tx, body.total_rx, body.total_tx, monthstart, today);
    let summary = traffic_summary(&name, &name, limit, &mode, monthstart, st);
    drop(traffic);
    crate::traffic::save_traffic();
    (StatusCode::OK, axum::Json(summary))
}

pub async fn get_ping(_claims: jwt::Claims) -> impl IntoResponse {
    (StatusCode::OK, axum::Json(json!(effective_ping())))
}

pub async fn put_ping(_claims: jwt::Claims, Json(body): Json<PingConfig>) -> impl IntoResponse {
    // Normalize first: an explicit v4 wins, an empty explicit v4 falls back
    // to the legacy key, empty display names reset to carrier defaults.
    // Empty addresses are valid: they mean "this probe/stack is not set"
    // and the client skips it. No address is ever required.
    let body = body.normalized();
    ADMIN.lock().unwrap().ping = Some(body);
    match save_admin_store() {
        Ok(()) => (StatusCode::OK, axum::Json(json!({ "ok": true }))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

// ---------- node CRUD + install command ----------

fn valid_plan_date(d: &str) -> bool {
    d.is_empty() || chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Panel origin for install links: config server_url wins (minus its
/// trailing /report), otherwise the request's own scheme/host, honouring
/// x-forwarded-* so a panel behind nginx links to its public address.
fn panel_origin(headers: &HeaderMap, uri: &Uri) -> String {
    if let Some(cfg) = G_CONFIG.get() {
        if !cfg.server_url.is_empty() {
            let base = cfg.server_url.trim_end_matches("/report").trim_end_matches('/');
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| uri.scheme_str().map(str::to_string))
        .unwrap_or_else(|| "http".to_string());
    let host = headers
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| headers.get("host").and_then(|v| v.to_str().ok()).map(str::to_string))
        .unwrap_or_else(|| "localhost".to_string());
    format!("{scheme}://{host}")
}

/// One-click install artefacts for a host. Group members get the group
/// join form (-g gid), standalone hosts the single form (-u name).
fn install_for_host(name: &str, host: &Host, headers: &HeaderMap, uri: &Uri) -> Value {
    let origin = panel_origin(headers, uri);
    let alias = if host.alias.is_empty() { name.to_string() } else { host.alias.clone() };
    if !host.gid.is_empty() {
        if let Some(group) = crate::registry::GROUPS_MAP.lock().unwrap().get(&host.gid).cloned() {
            let url = format!(
                "{origin}/i?gid={}&pass={}&alias={}",
                percent_encode(&host.gid),
                percent_encode(&group.password),
                percent_encode(&alias)
            );
            return json!({
                "name": name,
                "kind": "group",
                "gid": host.gid,
                "install_url": url,
                "install_command": format!("bash -c \"$(curl -fsSL '{url}')\""),
                "client_command": format!(
                    "stat_client -a \"{origin}/report\" -g {} -p {} --alias {}",
                    shell_quote(&host.gid),
                    shell_quote(&group.password),
                    shell_quote(&alias)
                ),
            });
        }
    }
    let url = format!(
        "{origin}/i?uid={}&pass={}&alias={}",
        percent_encode(name),
        percent_encode(&host.password),
        percent_encode(&alias)
    );
    json!({
        "name": name,
        "kind": "single",
        "install_url": url,
        "install_command": format!("bash -c \"$(curl -fsSL '{url}')\""),
        "client_command": format!(
            "stat_client -a \"{origin}/report\" -u {} -p {}",
            shell_quote(name),
            shell_quote(&host.password)
        ),
    })
}

fn host_json(h: &Host) -> Value {
    let (limit, mode, monthstart) = effective_host_settings(&h.name, h);
    json!({
        "name": h.name, "alias": h.alias, "location": h.location,
        "type": h.r#type, "gid": h.gid, "notify": h.notify,
        "disabled": h.disabled, "labels": h.labels,
        "traffic_limit": limit, "traffic_mode": mode, "monthstart": monthstart,
        "plan_start_date": h.plan_start_date, "plan_end_date": h.plan_end_date,
        "annual_fee": h.annual_fee,
        "currency": crate::config::normalize_currency(&h.currency),
        "bandwidth_mbps": h.bandwidth_mbps,
        "tag": h.tag, "note": h.note, "public_note": h.public_note,
        "weight": h.weight,
    })
}

#[derive(Debug, Deserialize)]
pub struct HostCreateBody {
    pub name: String,
    pub password: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub location: String,
    #[serde(default, rename = "type")]
    pub host_type: String,
    #[serde(default)]
    pub gid: String,
    #[serde(default)]
    pub monthstart: Option<u32>,
    #[serde(default)]
    pub traffic_limit: Option<u64>,
    #[serde(default)]
    pub traffic_mode: Option<String>,
    #[serde(default)]
    pub notify: Option<bool>,
    #[serde(default)]
    pub labels: String,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub plan_start_date: String,
    #[serde(default)]
    pub plan_end_date: String,
    #[serde(default)]
    pub annual_fee: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub bandwidth_mbps: Option<u32>,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub public_note: String,
    #[serde(default)]
    pub weight: Option<u64>,
}

pub async fn post_host(
    _claims: jwt::Claims,
    headers: HeaderMap,
    uri: Uri,
    Json(body): Json<HostCreateBody>,
) -> impl IntoResponse {
    if !valid_name(&body.name) {
        return err(StatusCode::BAD_REQUEST, "invalid name: use letters, digits, - or _");
    }
    if body.password.is_empty() {
        return err(StatusCode::BAD_REQUEST, "password must not be empty");
    }
    if !body.gid.is_empty() && !crate::registry::GROUPS_MAP.lock().unwrap().contains_key(&body.gid) {
        return err(StatusCode::BAD_REQUEST, "unknown gid: create the group first");
    }
    let monthstart = body.monthstart.unwrap_or(1);
    if !(1..=31).contains(&monthstart) {
        return err(StatusCode::BAD_REQUEST, "monthstart must be 1..=31");
    }
    if !valid_plan_date(&body.plan_start_date) {
        return err(StatusCode::BAD_REQUEST, "plan_start_date must be YYYY-MM-DD");
    }
    if !valid_plan_date(&body.plan_end_date) {
        return err(StatusCode::BAD_REQUEST, "plan_end_date must be YYYY-MM-DD");
    }
    if body.annual_fee.map(|f| f < 0.0 || !f.is_finite()).unwrap_or(false) {
        return err(StatusCode::BAD_REQUEST, "annual_fee must be >= 0");
    }
    let host = {
        let mut hosts = crate::registry::HOSTS_MAP.lock().unwrap();
        if hosts.contains_key(&body.name) {
            return err(StatusCode::CONFLICT, "host already exists");
        }
        let pos = hosts.values().map(|o| o.pos).max().unwrap_or(0) + 1;
        let host = Host {
            name: body.name.clone(),
            password: body.password.clone(),
            alias: if body.alias.is_empty() { body.name.clone() } else { body.alias.clone() },
            location: body.location.clone(),
            r#type: body.host_type.clone(),
            gid: body.gid.clone(),
            monthstart,
            notify: body.notify.unwrap_or(true),
            disabled: body.disabled.unwrap_or(false),
            labels: body.labels.clone(),
            traffic_limit: body.traffic_limit.unwrap_or(0),
            traffic_mode: normalize_traffic_mode(body.traffic_mode.as_deref().unwrap_or("sum")),
            plan_start_date: body.plan_start_date.clone(),
            plan_end_date: body.plan_end_date.clone(),
            annual_fee: body.annual_fee.unwrap_or(0.0),
            currency: body
                .currency
                .as_deref()
                .map(crate::config::normalize_currency)
                .unwrap_or_else(|| "$".to_string()),
            bandwidth_mbps: body.bandwidth_mbps.unwrap_or(0),
            tag: body.tag.clone(),
            note: body.note.clone(),
            public_note: body.public_note.clone(),
            pos,
            weight: body.weight.unwrap_or_else(|| 10000_u64.saturating_sub(pos as u64)),
            ..Default::default()
        };
        hosts.insert(host.name.clone(), host.clone());
        host
    };
    {
        let mut store = ADMIN.lock().unwrap();
        store.deleted_hosts.retain(|n| n != &host.name);
        store.admin_hosts.retain(|h| h.name != host.name);
        store.admin_hosts.push(host.clone());
    }
    if let Err(e) = save_admin_store() {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    let mut out = host_json(&host);
    out["install"] = install_for_host(&host.name, &host, &headers, &uri);
    (StatusCode::CREATED, axum::Json(out))
}

#[derive(Debug, Deserialize, Default)]
pub struct HostUpdateBody {
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default, rename = "type")]
    pub host_type: Option<String>,
    #[serde(default)]
    pub gid: Option<String>,
    #[serde(default)]
    pub monthstart: Option<u32>,
    #[serde(default)]
    pub traffic_limit: Option<u64>,
    #[serde(default)]
    pub traffic_mode: Option<String>,
    #[serde(default)]
    pub notify: Option<bool>,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub plan_start_date: Option<String>,
    #[serde(default)]
    pub plan_end_date: Option<String>,
    #[serde(default)]
    pub annual_fee: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub bandwidth_mbps: Option<u32>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub public_note: Option<String>,
    #[serde(default)]
    pub weight: Option<u64>,
}

pub async fn put_host(
    _claims: jwt::Claims,
    Path(name): Path<String>,
    Json(body): Json<HostUpdateBody>,
) -> impl IntoResponse {
    if let Some(gid) = body.gid.as_ref() {
        if !gid.is_empty() && !crate::registry::GROUPS_MAP.lock().unwrap().contains_key(gid) {
            return err(StatusCode::BAD_REQUEST, "unknown gid: create the group first");
        }
    }
    if let Some(m) = body.monthstart {
        if !(1..=31).contains(&m) {
            return err(StatusCode::BAD_REQUEST, "monthstart must be 1..=31");
        }
    }
    if let Some(d) = body.plan_start_date.as_ref() {
        if !valid_plan_date(d) {
            return err(StatusCode::BAD_REQUEST, "plan_start_date must be YYYY-MM-DD");
        }
    }
    if let Some(d) = body.plan_end_date.as_ref() {
        if !valid_plan_date(d) {
            return err(StatusCode::BAD_REQUEST, "plan_end_date must be YYYY-MM-DD");
        }
    }
    if body.annual_fee.map(|f| f < 0.0 || !f.is_finite()).unwrap_or(false) {
        return err(StatusCode::BAD_REQUEST, "annual_fee must be >= 0");
    }
    let host = {
        let mut hosts = crate::registry::HOSTS_MAP.lock().unwrap();
        let Some(h) = hosts.get_mut(&name) else {
            return err(StatusCode::NOT_FOUND, "host not found");
        };
        if let Some(p) = body.password.as_ref() {
            if !p.is_empty() {
                h.password = p.clone();
            }
        }
        if let Some(v) = body.alias { h.alias = v; }
        if let Some(v) = body.location { h.location = v; }
        if let Some(v) = body.host_type { h.r#type = v; }
        if let Some(v) = body.gid { h.gid = v; }
        if let Some(v) = body.monthstart { h.monthstart = v; }
        if let Some(v) = body.traffic_limit { h.traffic_limit = v; }
        if let Some(v) = body.traffic_mode { h.traffic_mode = normalize_traffic_mode(&v); }
        if let Some(v) = body.notify { h.notify = v; }
        if let Some(v) = body.labels { h.labels = v; }
        if let Some(v) = body.disabled { h.disabled = v; }
        if let Some(v) = body.plan_start_date { h.plan_start_date = v; }
        if let Some(v) = body.plan_end_date { h.plan_end_date = v; }
        if let Some(v) = body.annual_fee { h.annual_fee = v; }
        if let Some(v) = body.currency { h.currency = crate::config::normalize_currency(&v); }
        if let Some(v) = body.bandwidth_mbps { h.bandwidth_mbps = v; }
        if let Some(v) = body.tag { h.tag = v; }
        if let Some(v) = body.note { h.note = v; }
        if let Some(v) = body.public_note { h.public_note = v; }
        if let Some(v) = body.weight { h.weight = v; }
        h.clone()
    };
    {
        let mut store = ADMIN.lock().unwrap();
        store.admin_hosts.retain(|h| h.name != host.name);
        store.admin_hosts.push(host.clone());
    }
    match save_admin_store() {
        Ok(()) => (StatusCode::OK, axum::Json(host_json(&host))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn delete_host(_claims: jwt::Claims, Path(name): Path<String>) -> impl IntoResponse {
    let removed = {
        let mut hosts = crate::registry::HOSTS_MAP.lock().unwrap();
        hosts.remove(&name)
    };
    let Some(_host) = removed else {
        return err(StatusCode::NOT_FOUND, "host not found");
    };
    crate::registry::STAT_MAP.lock().unwrap().remove(&name);
    TRAFFIC.lock().unwrap().remove(&name);
    let was_dynamic = {
        let in_cfg = G_CONFIG.get().map(|c| c.hosts_map.contains_key(&name)).unwrap_or(false);
        let in_admin = ADMIN.lock().unwrap().admin_hosts.iter().any(|h| h.name == name);
        !in_cfg && !in_admin
    };
    {
        let mut store = ADMIN.lock().unwrap();
        store.hosts.remove(&name);
        store.admin_hosts.retain(|h| h.name != name);
        if G_CONFIG.get().map(|c| c.hosts_map.contains_key(&name)).unwrap_or(false)
            && !store.deleted_hosts.contains(&name)
        {
            store.deleted_hosts.push(name.clone());
        }
    }
    if let Err(e) = save_admin_store() {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    crate::traffic::save_traffic();
    let mut out = json!({ "ok": true, "name": name });
    if was_dynamic {
        out["note"] = json!("group-dynamic host: it will reappear on its next report unless its group is also removed or the client is stopped");
    }
    (StatusCode::OK, axum::Json(out))
}

pub async fn get_host_install(
    _claims: jwt::Claims,
    headers: HeaderMap,
    uri: Uri,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(host) = crate::registry::HOSTS_MAP.lock().unwrap().get(&name).cloned() else {
        return err(StatusCode::NOT_FOUND, "host not found");
    };
    (StatusCode::OK, axum::Json(install_for_host(&name, &host, &headers, &uri)))
}

// ---------- groups ----------

fn group_json(g: &HostGroup, member_count: usize, source: &str) -> Value {
    json!({
        "gid": g.gid, "location": g.location, "type": g.r#type,
        "notify": g.notify, "labels": g.labels,
        "traffic_limit": g.traffic_limit, "traffic_mode": normalize_traffic_mode(&g.traffic_mode),
        "plan_start_date": g.plan_start_date, "annual_fee": g.annual_fee,
        "bandwidth_mbps": g.bandwidth_mbps,
        "member_count": member_count, "source": source,
    })
}

pub async fn get_groups(_claims: jwt::Claims) -> impl IntoResponse {
    let admin_gids: Vec<String> = ADMIN.lock().unwrap().admin_groups.iter().map(|g| g.gid.clone()).collect();
    let mut out = Vec::new();
    {
        let groups = crate::registry::GROUPS_MAP.lock().unwrap();
        let hosts = crate::registry::HOSTS_MAP.lock().unwrap();
        let mut list: Vec<&HostGroup> = groups.values().collect();
        list.sort_by_key(|g| g.pos);
        for g in list {
            let count = hosts.values().filter(|h| h.gid == g.gid).count();
            out.push(group_json(g, count, if admin_gids.contains(&g.gid) { "admin" } else { "config" }));
        }
    }
    (StatusCode::OK, axum::Json(json!(out)))
}

#[derive(Debug, Deserialize)]
pub struct GroupCreateBody {
    pub gid: String,
    pub password: String,
    #[serde(default)]
    pub location: String,
    #[serde(default, rename = "type")]
    pub group_type: String,
    #[serde(default)]
    pub notify: Option<bool>,
    #[serde(default)]
    pub labels: String,
    #[serde(default)]
    pub traffic_limit: Option<u64>,
    #[serde(default)]
    pub traffic_mode: Option<String>,
    #[serde(default)]
    pub plan_start_date: String,
    #[serde(default)]
    pub annual_fee: Option<f64>,
    #[serde(default)]
    pub bandwidth_mbps: Option<u32>,
}

pub async fn post_group(_claims: jwt::Claims, Json(body): Json<GroupCreateBody>) -> impl IntoResponse {
    if !valid_name(&body.gid) {
        return err(StatusCode::BAD_REQUEST, "invalid gid: use letters, digits, - or _");
    }
    if body.password.is_empty() {
        return err(StatusCode::BAD_REQUEST, "password must not be empty");
    }
    if !valid_plan_date(&body.plan_start_date) {
        return err(StatusCode::BAD_REQUEST, "plan_start_date must be YYYY-MM-DD");
    }
    if body.annual_fee.map(|f| f < 0.0 || !f.is_finite()).unwrap_or(false) {
        return err(StatusCode::BAD_REQUEST, "annual_fee must be >= 0");
    }
    let group = {
        let mut groups = crate::registry::GROUPS_MAP.lock().unwrap();
        if groups.contains_key(&body.gid) {
            return err(StatusCode::CONFLICT, "group already exists");
        }
        let pos = groups.values().map(|o| o.pos).max().unwrap_or(0) + 1;
        let group = HostGroup {
            gid: body.gid.clone(),
            password: body.password.clone(),
            location: body.location.clone(),
            r#type: body.group_type.clone(),
            notify: body.notify.unwrap_or(true),
            labels: body.labels.clone(),
            traffic_limit: body.traffic_limit.unwrap_or(0),
            traffic_mode: normalize_traffic_mode(body.traffic_mode.as_deref().unwrap_or("sum")),
            plan_start_date: body.plan_start_date.clone(),
            annual_fee: body.annual_fee.unwrap_or(0.0),
            bandwidth_mbps: body.bandwidth_mbps.unwrap_or(0),
            pos,
            weight: 10000_u64.saturating_sub(pos as u64),
        };
        groups.insert(group.gid.clone(), group.clone());
        group
    };
    {
        let mut store = ADMIN.lock().unwrap();
        store.deleted_groups.retain(|g| g != &group.gid);
        store.admin_groups.retain(|g| g.gid != group.gid);
        store.admin_groups.push(group.clone());
    }
    match save_admin_store() {
        Ok(()) => (StatusCode::CREATED, axum::Json(group_json(&group, 0, "admin"))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct GroupUpdateBody {
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default, rename = "type")]
    pub group_type: Option<String>,
    #[serde(default)]
    pub notify: Option<bool>,
    #[serde(default)]
    pub labels: Option<String>,
    #[serde(default)]
    pub traffic_limit: Option<u64>,
    #[serde(default)]
    pub traffic_mode: Option<String>,
    #[serde(default)]
    pub plan_start_date: Option<String>,
    #[serde(default)]
    pub annual_fee: Option<f64>,
    #[serde(default)]
    pub bandwidth_mbps: Option<u32>,
}

pub async fn put_group(
    _claims: jwt::Claims,
    Path(gid): Path<String>,
    Json(body): Json<GroupUpdateBody>,
) -> impl IntoResponse {
    let group = {
        let mut groups = crate::registry::GROUPS_MAP.lock().unwrap();
        let Some(g) = groups.get_mut(&gid) else {
            return err(StatusCode::NOT_FOUND, "group not found");
        };
        if let Some(p) = body.password.as_ref() {
            if !p.is_empty() {
                g.password = p.clone();
            }
        }
        if let Some(v) = body.location { g.location = v; }
        if let Some(v) = body.group_type { g.r#type = v; }
        if let Some(v) = body.notify { g.notify = v; }
        if let Some(v) = body.labels { g.labels = v; }
        if let Some(v) = body.traffic_limit { g.traffic_limit = v; }
        if let Some(v) = body.traffic_mode { g.traffic_mode = normalize_traffic_mode(&v); }
        if let Some(v) = body.plan_start_date {
            if !valid_plan_date(&v) {
                return err(StatusCode::BAD_REQUEST, "plan_start_date must be YYYY-MM-DD");
            }
            g.plan_start_date = v;
        }
        if let Some(v) = body.annual_fee {
            if v < 0.0 || !v.is_finite() {
                return err(StatusCode::BAD_REQUEST, "annual_fee must be >= 0");
            }
            g.annual_fee = v;
        }
        if let Some(v) = body.bandwidth_mbps { g.bandwidth_mbps = v; }
        g.clone()
    };
    {
        let mut store = ADMIN.lock().unwrap();
        store.admin_groups.retain(|g| g.gid != group.gid);
        store.admin_groups.push(group.clone());
    }
    let count = crate::registry::HOSTS_MAP.lock().unwrap().values().filter(|h| h.gid == group.gid).count();
    let mut out = group_json(&group, count, "admin");
    out["note"] = json!("already-registered members keep their old template until they re-register");
    match save_admin_store() {
        Ok(()) => (StatusCode::OK, axum::Json(out)),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

pub async fn delete_group(_claims: jwt::Claims, Path(gid): Path<String>) -> impl IntoResponse {
    let removed = {
        let mut groups = crate::registry::GROUPS_MAP.lock().unwrap();
        groups.remove(&gid)
    };
    if removed.is_none() {
        return err(StatusCode::NOT_FOUND, "group not found");
    }
    {
        let mut store = ADMIN.lock().unwrap();
        store.admin_groups.retain(|g| g.gid != gid);
        if G_CONFIG.get().map(|c| c.hosts_group_map.contains_key(&gid)).unwrap_or(false)
            && !store.deleted_groups.contains(&gid)
        {
            store.deleted_groups.push(gid.clone());
        }
    }
    match save_admin_store() {
        Ok(()) => (StatusCode::OK, axum::Json(json!({ "ok": true, "gid": gid, "note": "already-registered members are NOT removed; delete them individually if needed" }))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

// ---------- pushplus notify settings ----------

pub async fn get_pushplus(_claims: jwt::Claims) -> impl IntoResponse {
    let p = effective_pushplus();
    (StatusCode::OK, axum::Json(json!({
        "enabled": p.enabled, "title": p.title, "channel": p.channel,
        "token_set": !p.token.is_empty(),
        "online_tpl": p.online_tpl, "offline_tpl": p.offline_tpl,
        "custom_tpl": p.custom_tpl, "traffic_tpl": p.traffic_tpl,
    })))
}

#[derive(Debug, Deserialize, Default)]
pub struct PushPlusPutBody {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub online_tpl: Option<String>,
    #[serde(default)]
    pub offline_tpl: Option<String>,
    #[serde(default)]
    pub custom_tpl: Option<String>,
    #[serde(default)]
    pub traffic_tpl: Option<String>,
}

pub async fn put_pushplus(_claims: jwt::Claims, Json(body): Json<PushPlusPutBody>) -> impl IntoResponse {
    let mut p = effective_pushplus();
    if let Some(v) = body.enabled { p.enabled = v; }
    if let Some(t) = body.token.as_ref() {
        if !t.is_empty() {
            p.token = t.clone();
        }
    }
    if let Some(v) = body.title { p.title = v; }
    if let Some(v) = body.channel { p.channel = v; }
    if let Some(v) = body.online_tpl { p.online_tpl = v; }
    if let Some(v) = body.offline_tpl { p.offline_tpl = v; }
    if let Some(v) = body.custom_tpl { p.custom_tpl = v; }
    if let Some(v) = body.traffic_tpl { p.traffic_tpl = v; }
    ADMIN.lock().unwrap().pushplus = Some(p.clone());
    if let Err(e) = save_admin_store() {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    crate::notifier::pushplus::apply_pushplus_config(p);
    (StatusCode::OK, axum::Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host_with_dates(name: &str, start: &str, end: &str) -> Host {
        Host {
            name: name.to_string(),
            plan_start_date: start.to_string(),
            plan_end_date: end.to_string(),
            ..Default::default()
        }
    }

    /// Save/load round-trip (serde_json::to_string / from_str, exactly what
    /// save_admin_store / init_admin_store do) must never fill an empty
    /// plan_end_date. Regression test for the 2026-10-27 incident, where
    /// six emptied end dates reappeared in admin.json.
    #[test]
    fn empty_end_date_survives_save_load_round_trip() {
        let mut store = AdminStore::default();
        store.admin_hosts.push(host_with_dates("kuroit", "2025-12-01", ""));
        store.admin_hosts.push(host_with_dates("exact", "2026-06-15", ""));
        store.admin_hosts.push(host_with_dates("cheap-jp", "2026-10-01", ""));
        let saved = serde_json::to_string(&store).unwrap();
        let reloaded: AdminStore = serde_json::from_str(&saved).unwrap();
        for h in &reloaded.admin_hosts {
            assert_eq!(h.plan_end_date, "", "end date filled for {}", h.name);
        }
        // Start dates ride along untouched.
        assert_eq!(reloaded.admin_hosts[0].plan_start_date, "2025-12-01");
        assert_eq!(reloaded.admin_hosts[1].plan_start_date, "2026-06-15");
    }

    /// A filled end date survives the same round-trip verbatim.
    #[test]
    fn filled_end_date_survives_save_load_round_trip() {
        let mut store = AdminStore::default();
        store.admin_hosts.push(host_with_dates("sailor", "2025-12-12", "2026-12-12"));
        let saved = serde_json::to_string(&store).unwrap();
        let reloaded: AdminStore = serde_json::from_str(&saved).unwrap();
        assert_eq!(reloaded.admin_hosts[0].plan_end_date, "2026-12-12");
    }

    /// The incident file shape: six hosts with starts set and ends empty,
    /// plus a traffic override, must round-trip with ends still empty
    /// and the override intact.
    #[test]
    fn incident_shape_round_trip_keeps_ends_empty() {
        let mut store = AdminStore::default();
        for (name, start) in [
            ("56idc", "2026-05-24"),
            ("cheap-jp", "2026-10-01"),
            ("cheap-sg", "2026-10-01"),
            ("exact", "2026-06-15"),
            ("kuroit", "2025-12-01"),
            ("sailor", "2025-12-12"),
        ] {
            store.admin_hosts.push(host_with_dates(name, start, ""));
        }
        store.hosts.insert(
            "cdt".to_string(),
            HostTrafficOverride {
                traffic_limit: 200 * 1024 * 1024 * 1024,
                traffic_mode: "up".to_string(),
                monthstart: 20,
            },
        );
        let saved = serde_json::to_string(&store).unwrap();
        let reloaded: AdminStore = serde_json::from_str(&saved).unwrap();
        assert_eq!(reloaded.admin_hosts.len(), 6);
        for h in &reloaded.admin_hosts {
            assert_eq!(h.plan_end_date, "", "end date filled for {}", h.name);
            assert!(!h.plan_start_date.is_empty());
        }
        assert_eq!(
            reloaded.hosts["cdt"].traffic_limit,
            200 * 1024 * 1024 * 1024
        );
    }
}
