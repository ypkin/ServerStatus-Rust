#![deny(warnings)]
#![allow(clippy::struct_excessive_bools)]
use serde::{Deserialize, Serialize};
use stat_common::server_status::{DiskInfo, IpInfo, SysInfo};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn default_as_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostStat {
    pub name: String,
    #[serde(default = "Default::default")]
    pub alias: String,
    #[serde(rename = "type", default = "Default::default")]
    pub host_type: String,
    #[serde(default = "Default::default")]
    pub location: String,
    #[serde(default = "default_as_true")]
    pub notify: bool,
    #[serde(default = "bool::default")]
    pub vnstat: bool,

    #[serde(default = "default_as_true")]
    pub online4: bool,
    #[serde(default = "default_as_true")]
    pub online6: bool,

    #[serde(rename(deserialize = "uptime"), skip_serializing)]
    pub uptime: u64,
    #[serde(rename(serialize = "uptime"), skip_deserializing)]
    pub uptime_str: String,

    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,

    pub ping_10010: f64,
    pub ping_189: f64,
    pub ping_10086: f64,
    pub time_10010: f64,
    pub time_189: f64,
    pub time_10086: f64,

    // Fork: IPv6 three-carrier probe results (loss % / latency ms).
    // Zero means "not probed" (no v6 address configured, or an old client).
    // Defaults keep old clients' reports deserializing.
    #[serde(default)]
    pub ping_10010_v6: f64,
    #[serde(default)]
    pub ping_189_v6: f64,
    #[serde(default)]
    pub ping_10086_v6: f64,
    #[serde(default)]
    pub time_10010_v6: f64,
    #[serde(default)]
    pub time_189_v6: f64,
    #[serde(default)]
    pub time_10086_v6: f64,

    #[serde(rename(deserialize = "tcp"))]
    pub tcp_count: u32,
    #[serde(rename(deserialize = "udp"))]
    pub udp_count: u32,
    #[serde(rename(deserialize = "process"))]
    pub process_count: u32,
    #[serde(rename(deserialize = "thread"))]
    pub thread_count: u32,

    pub network_rx: u64,
    pub network_tx: u64,
    pub network_in: u64,
    pub network_out: u64,

    #[serde(default)]
    pub last_network_in: u64,
    #[serde(default)]
    pub last_network_out: u64,

    // Fork: client-reported boot identifier (never serialized out: it is an
    // internal accounting key, not public data).
    #[serde(default, skip_serializing)]
    pub boot_id: String,

    // Fork: client auto-detection (see stats.rs precedence). Detected
    // values only fill in when neither an explicit client value nor a
    // backend host value exists; they are resolved into location/type
    // before stats are served, so they never appear in stats.json.
    #[serde(default, skip_serializing)]
    pub location_detected: String,
    #[serde(default, skip_serializing)]
    pub type_detected: String,

    // Fork: server-side traffic accounting (see traffic.rs). Set by the
    // server on every report; all default to zero for old clients/themes.
    #[serde(default)]
    pub month_rx: u64,
    #[serde(default)]
    pub month_tx: u64,
    #[serde(default)]
    pub day_rx: u64,
    #[serde(default)]
    pub day_tx: u64,
    #[serde(default)]
    pub total_rx: u64,
    #[serde(default)]
    pub total_tx: u64,
    /// This period's usage under the host's traffic_mode.
    #[serde(default)]
    pub month_used: u64,
    /// Configured allowance in bytes, 0 = unlimited.
    #[serde(default)]
    pub traffic_limit: u64,
    #[serde(default)]
    pub traffic_mode: String,
    /// month_used as a percentage of traffic_limit, 0 when unlimited.
    #[serde(default)]
    pub traffic_percent: u64,
    #[serde(default)]
    pub month_start: String,

    // Fork: plan display (see web/plan-overlay.js). Copied from the
    // registered Host record on every report; empty/zero = unset, never
    // fabricated (in particular no expiry date is derived here).
    #[serde(default)]
    pub plan_start_date: String,
    #[serde(default)]
    pub annual_fee: f64,
    #[serde(default)]
    pub bandwidth_mbps: u32,

    // Fork: nezha-style display group tag (free text, backend-managed).
    // Distinct from gid (auth/template membership): tag never affects
    // auth, it only drives admin filtering and main-panel grouping.
    #[serde(default)]
    pub tag: String,
    // Fork: public note, shown on main panel cards via the overlay.
    #[serde(default)]
    pub public_note: String,
    // Fork: client-reported version (StatRequest.version, e.g. 1.8.2).
    // Flows through the serde report conversion; empty for clients that
    // do not report one, never fabricated.
    #[serde(default)]
    pub version: String,

    pub cpu: f64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub hdd_total: u64,
    pub hdd_used: u64,

    #[serde(skip_deserializing)]
    pub labels: String,
    #[serde(skip_deserializing)]
    pub custom: String,

    #[serde(skip_serializing)]
    pub ip_info: Option<IpInfo>,
    #[serde(skip_serializing)]
    pub sys_info: Option<SysInfo>,

    // group
    #[serde(default = "Default::default")]
    pub gid: String,
    #[serde(default = "Default::default")]
    pub weight: u64,

    // user data
    #[serde(skip_deserializing)]
    pub latest_ts: u64,

    #[serde(skip_serializing, skip_deserializing)]
    pub pos: usize,
    #[serde(skip_serializing, skip_deserializing)]
    pub disabled: bool,

    // false: KiB (1024), true: KB (1000)
    #[serde(default = "Default::default")]
    pub si: bool,
    #[serde(default = "Default::default", skip_serializing)]
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatsResp {
    pub updated: u64,
    pub servers: Vec<Arc<HostStat>>,
}
impl StatsResp {
    pub fn new() -> Self {
        Self {
            updated: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            servers: Vec::new(),
        }
    }
}
