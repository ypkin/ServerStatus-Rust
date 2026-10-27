//! Live registries (fork).
//!
//! Upstream kept hosts and live stats in maps owned by `StatsMgr::init`.
//! The admin backend needs the same maps: nodes and groups added or deleted
//! in the panel must take effect without a restart, and a deleted node must
//! disappear from the public page immediately. These globals are therefore
//! the single source of truth, seeded at startup from `config.toml` plus
//! `admin.json` (see `admin.rs`) and mutated only through the admin API
//! and the report threads.
//!
//! Lock order (to avoid deadlocks): HOSTS_MAP / GROUPS_MAP / STAT_MAP may
//! be held while taking the ADMIN lock (via `admin::effective_*`), but no
//! code may hold ADMIN while taking any registry lock.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use crate::config::{Host, HostGroup};
use crate::payload::HostStat;

/// Live hosts: config hosts + admin-managed hosts + group-dynamic hosts.
pub static HOSTS_MAP: LazyLock<Mutex<HashMap<String, Host>>> = LazyLock::new(Default::default);
/// Live stats by host name, feeding `/json/stats.json` and the admin page.
pub static STAT_MAP: LazyLock<Mutex<HashMap<String, Arc<HostStat>>>> = LazyLock::new(Default::default);
/// Live groups (dynamic-registration templates): config groups +
/// admin-managed groups.
pub static GROUPS_MAP: LazyLock<Mutex<HashMap<String, HostGroup>>> = LazyLock::new(Default::default);
