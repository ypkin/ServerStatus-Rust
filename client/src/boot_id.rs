//! Stable boot identifier for server-side traffic accounting (fork).
//!
//! Format: "<boot_epoch>/<iface_digest>", mirroring monitor's agent
//! `boot_id`. The epoch changes only on reboot; the digest changes when
//! the interface set changes, so the server can tell "same machine,
//! counters restarted" apart from "counters shrank because an interface
//! left the sum". A client restart alone does not change it, so traffic
//! booked across client restarts is not double-counted.
//!
//! Linux reads the kernel boot time from /proc/stat (btime). Elsewhere it
//! falls back to now minus sysinfo uptime. The digest is a FNV-1a hash of
//! the sorted /proc/net/dev interface names (empty elsewhere).

use std::time::{SystemTime, UNIX_EPOCH};

fn boot_epoch() -> i64 {
    if let Ok(stat) = std::fs::read_to_string("/proc/stat") {
        for line in stat.lines() {
            if let Some(v) = line.strip_prefix("btime ") {
                if let Ok(n) = v.trim().parse::<i64>() {
                    return n;
                }
            }
        }
    }
    let uptime = sysinfo::System::uptime();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 - uptime as i64)
        .unwrap_or(0)
}

fn iface_digest() -> String {
    let mut names: Vec<String> = Vec::new();
    if let Ok(dev) = std::fs::read_to_string("/proc/net/dev") {
        for line in dev.lines().skip(2) {
            if let Some((name, _)) = line.split_once(':') {
                names.push(name.trim().to_string());
            }
        }
    }
    names.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for name in &names {
        for b in name.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn boot_id() -> String {
    format!("{}/{}", boot_epoch(), iface_digest())
}
