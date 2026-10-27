//! Server-side traffic accounting, ported from monitor-probe/monitor's
//! `accumulate` / `period_start` / `month_used` / `set_traffic` model.
//!
//! The client only reports lifetime kernel counters (`network_in` /
//! `network_out`). This module books *only the increase this server has
//! observed* into day / billing-period / lifetime totals, so a reboot, a
//! counter reset, or a server restart can never inflate the totals, and a
//! reboot that happened while the server was not watching is recovered once
//! (see `unseen_boot` below) instead of being booked as the machine's whole
//! history.
//!
//! Persistence is `traffic.json` next to the server's `stats.json`. It holds
//! accounting state only; operator settings live in `admin.json` (see
//! `admin.rs`). The file format is version-tolerant: unknown fields are
//! ignored and missing fields default to zero, so an old file still loads.

use chrono::{DateTime, Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::{LazyLock, Mutex};

const TRAFFIC_FILE: &str = "traffic.json";

/// Unlimited traffic is spelled 0: the admin form's limit field uses
/// 0 for unlimited (its placeholder says so), and every consumer
/// (percent, alerts, displays) must use is_unlimited instead of
/// comparing against 0 directly. A literal 9999 GiB is a real quota,
/// not unlimited.
pub fn is_unlimited(limit_bytes: u64) -> bool {
    limit_bytes == 0
}

/// One host's accounting row. All byte counters are u64 and only ever grow
/// via `saturating_add`, mirroring monitor's saturating i64 arithmetic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrafficState {
    #[serde(default)]
    pub boot_id: String,
    #[serde(default)]
    pub last_rx: u64,
    #[serde(default)]
    pub last_tx: u64,
    #[serde(default)]
    pub total_rx: u64,
    #[serde(default)]
    pub total_tx: u64,
    #[serde(default)]
    pub month_rx: u64,
    #[serde(default)]
    pub month_tx: u64,
    #[serde(default)]
    pub month_start: String,
    #[serde(default)]
    pub day_rx: u64,
    #[serde(default)]
    pub day_tx: u64,
    #[serde(default)]
    pub day_start: String,
    /// Traffic-alert step already announced for `alerted_period` (0, the
    /// configured percent, or 100). Stored so a restart does not re-announce
    /// the same crossing, and a manual correction that lowers usage lets the
    /// same crossing announce again.
    #[serde(default)]
    pub alerted_step: u32,
    #[serde(default)]
    pub alerted_period: String,
}

/// This period's usage as the host's plan meters it.
pub fn month_used(month_rx: u64, month_tx: u64, traffic_mode: &str) -> u64 {
    match traffic_mode {
        "up" => month_tx,
        "down" => month_rx,
        "max" => month_rx.max(month_tx),
        _ => month_rx.saturating_add(month_tx),
    }
}

/// Split a single "month used" correction value into (month_rx, month_tx)
/// so that `month_used` of the result equals the corrected value under the
/// host's metering mode. Direction names follow the counters: "up" is
/// upload = tx, "down" is download = rx. (An earlier version had up/down
/// backwards — up put everything into rx, so the billed figure collapsed
/// to ~0 and corrections looked like they "didn't take effect".)
/// Superseded by `align_used_to` for corrections; kept (with its test)
/// as the documented split rule, so it is allowed dead in normal builds.
#[allow(dead_code)]
pub fn split_month_used(v: u64, traffic_mode: &str) -> (u64, u64) {
    match traffic_mode {
        "up" => (0, v),
        "down" => (v, 0),
        "max" => (v, v),
        _ => (v / 2 + v % 2, v / 2),
    }
}

/// Align the metered ("used") figure to `v` without destroying the
/// unmetered direction's real history. Unlike `split_month_used` (which
/// replaces both directions), this keeps the side the mode does not bill:
/// up keeps rx and sets tx, down keeps rx's counterpart (sets rx, keeps
/// tx), max sets the larger side to `v` (capping the smaller at `v` so the
/// max is exactly `v`). Sum has no unmetered side, so it keeps the
/// established half/half split. In every case `month_used` of the result
/// equals `v` exactly.
pub fn align_used_to(v: u64, traffic_mode: &str, cur_rx: u64, cur_tx: u64) -> (u64, u64) {
    match traffic_mode {
        "up" => (cur_rx, v),
        "down" => (v, cur_tx),
        "max" => {
            if cur_rx >= cur_tx {
                (v, cur_tx.min(v))
            } else {
                (cur_rx.min(v), v)
            }
        }
        _ => (v / 2 + v % 2, v / 2),
    }
}

/// First day of the billing period containing `today`, anchored on
/// `reset_day` (1..=31, clamped to the month's length, e.g. 31 in February
/// becomes the 28th/29th). Ported from monitor's `period_start`.
pub fn period_start(today: NaiveDate, reset_day: u32) -> NaiveDate {
    let day = reset_day.clamp(1, 31);
    let clamped = |y: i32, m: u32| {
        let last = NaiveDate::from_ymd_opt(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1)
            .unwrap()
            .pred_opt()
            .unwrap()
            .day();
        NaiveDate::from_ymd_opt(y, m, day.min(last)).unwrap()
    };
    let this = clamped(today.year(), today.month());
    if today >= this {
        this
    } else if today.month() == 1 {
        clamped(today.year() - 1, 12)
    } else {
        clamped(today.year(), today.month() - 1)
    }
}

impl TrafficState {
    /// Book one reading of a host's raw counters. Returns the delta actually
    /// booked. `booted` is the host's boot time as a unix timestamp (now
    /// minus reported uptime), `last_seen` is the previous report's timestamp.
    pub fn accumulate(
        &mut self,
        boot_id: &str,
        rx: u64,
        tx: u64,
        reset_day: u32,
        at: DateTime<Local>,
        booted: Option<i64>,
        last_seen: i64,
    ) -> (u64, u64) {
        // Only bytes this server observed a counter climb through are booked.
        // Without a baseline under this exact boot there is nothing to
        // subtract from, and a bare reading represents the machine's entire
        // history. Except for a machine that booted after the server last
        // heard from it: its counters started from zero while unobserved, so
        // the reading is exactly the bytes since boot.
        let unseen_boot = !self.boot_id.is_empty()
            && self.boot_id.split('/').next() != boot_id.split('/').next()
            && booted.is_some_and(|b| last_seen > 0 && b > last_seen);
        let (d_rx, d_tx) = if unseen_boot {
            (rx, tx)
        } else if self.boot_id.is_empty() || self.boot_id != boot_id {
            (0, 0)
        } else {
            (rx.saturating_sub(self.last_rx), tx.saturating_sub(self.last_tx))
        };

        self.total_rx = self.total_rx.saturating_add(d_rx);
        self.total_tx = self.total_tx.saturating_add(d_tx);
        self.month_rx = self.month_rx.saturating_add(d_rx);
        self.month_tx = self.month_tx.saturating_add(d_tx);
        self.day_rx = self.day_rx.saturating_add(d_rx);
        self.day_tx = self.day_tx.saturating_add(d_tx);

        // Both boundaries are calendar dates, so both follow the server's
        // local timezone rather than UTC. Never date a row before a period it
        // already carries: the admin panel stamps the current period with a
        // correction, which a reading held back from before the boundary
        // would otherwise read as a new period and discard.
        let (date, today) = (at.date_naive(), Local::now().date_naive());
        let dated = |stored: &str| {
            stored
                .parse::<NaiveDate>()
                .ok()
                .filter(|stamp| *stamp <= today)
                .map_or(date, |stamp| stamp.max(date))
        };
        let period = period_start(dated(&self.month_start), reset_day).to_string();
        if self.month_start != period {
            self.month_rx = d_rx;
            self.month_tx = d_tx;
        }
        self.month_start = period;
        let day = dated(&self.day_start).to_string();
        if self.day_start != day {
            self.day_rx = d_rx;
            self.day_tx = d_tx;
        }
        self.day_start = day;

        self.boot_id = boot_id.to_string();
        self.last_rx = rx;
        self.last_tx = tx;
        (d_rx, d_tx)
    }

    /// All-time counters must cover the current period: traffic counted
    /// in this period really happened, so it belongs in the all-time total
    /// too. A month figure seeded or corrected above the accumulate
    /// baseline (e.g. pre-panel history) would otherwise leave total below
    /// month, and the admin form would show total < month. Returns true
    /// when either total was raised. Never lowers a total.
    pub fn cover_month_with_total(&mut self) -> bool {
        let mut raised = false;
        if self.total_rx < self.month_rx {
            self.total_rx = self.month_rx;
            raised = true;
        }
        if self.total_tx < self.month_tx {
            self.total_tx = self.month_tx;
            raised = true;
        }
        raised
    }

    /// Admin correction (`set_traffic` semantics): the corrected month
    /// figures are stamped with the current period, otherwise the read side
    /// would answer them as zero and the next report would discard them.
    /// Month fields left as `None` reset to zero when the period changed.
    pub fn correct(
        &mut self,
        month_rx: Option<u64>,
        month_tx: Option<u64>,
        total_rx: Option<u64>,
        total_tx: Option<u64>,
        reset_day: u32,
        today: NaiveDate,
    ) {
        let period = period_start(today, reset_day).to_string();
        if let Some(v) = total_rx {
            self.total_rx = v;
        }
        if let Some(v) = total_tx {
            self.total_tx = v;
        }
        self.month_rx = month_rx.unwrap_or(if self.month_start == period { self.month_rx } else { 0 });
        self.month_tx = month_tx.unwrap_or(if self.month_start == period { self.month_tx } else { 0 });
        self.month_start = period;
        // Keep total >= month even when the month was aligned upward
        // past everything accumulate has seen (see cover_month_with_total).
        self.cover_month_with_total();
        // A correction lowers (or raises) usage: let the same alert crossing
        // announce again instead of staying silenced by the old step.
        self.alerted_step = 0;
    }

    /// Alert step reached for `used` against `limit`: 0, `alert_percent`,
    /// or 100. Uses u128 so 100x a near-u64::MAX limit cannot overflow.
    pub fn alert_step(used: u64, limit: u64, alert_percent: u32) -> u32 {
        if limit == 0 {
            return 0;
        }
        let percent = alert_percent.clamp(1, 100);
        if used as u128 * 100 >= limit as u128 * 100 {
            100
        } else if used as u128 * 100 >= limit as u128 * percent as u128 {
            percent
        } else {
            0
        }
    }
}

/// Live accounting state, shared by the report thread, the timer thread
/// (periodic save) and the admin handlers.
pub static TRAFFIC: LazyLock<Mutex<HashMap<String, TrafficState>>> = LazyLock::new(Default::default);

pub fn load_traffic() {
    let contents = fs::read_to_string(TRAFFIC_FILE).unwrap_or_default();
    if contents.is_empty() {
        return;
    }
    match serde_json::from_str::<HashMap<String, TrafficState>>(&contents) {
        Ok(map) => {
            let mut guard = TRAFFIC.lock().unwrap();
            *guard = map;
            // Repair rows whose month was seeded or corrected above
            // everything accumulate has seen (e.g. pre-panel history):
            // without this, total stays below month until the next
            // correction. Only raises totals, never lowers them.
            let mut changed = false;
            for st in guard.values_mut() {
                changed |= st.cover_month_with_total();
            }
            drop(guard);
            if changed {
                save_traffic();
            }
        }
        Err(e) => {
            error!("ignore invalid {TRAFFIC_FILE}: {e}");
        }
    }
}

pub fn save_traffic() {
    let data = serde_json::to_string(&*TRAFFIC.lock().unwrap()).unwrap_or_default();
    if data.is_empty() {
        return;
    }
    // Write-then-rename so a crash mid-write never leaves a torn file.
    if fs::write(format!("{TRAFFIC_FILE}.tmp"), &data).is_ok() {
        if let Err(e) = fs::rename(format!("{TRAFFIC_FILE}.tmp"), TRAFFIC_FILE) {
            error!("save {TRAFFIC_FILE} fail: {e}");
        }
    } else {
        error!("save {TRAFFIC_FILE} fail");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    #[test]
    fn period_start_handles_short_months_and_wraparound() {
        let d = |y: i32, m: u32, day: u32| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(period_start(d(2026, 3, 20), 15), d(2026, 3, 15));
        assert_eq!(period_start(d(2026, 3, 15), 15), d(2026, 3, 15));
        assert_eq!(period_start(d(2026, 3, 10), 15), d(2026, 2, 15));
        assert_eq!(period_start(d(2026, 1, 10), 15), d(2025, 12, 15));
        assert_eq!(period_start(d(2026, 2, 28), 31), d(2026, 2, 28));
        assert_eq!(period_start(d(2028, 2, 29), 31), d(2028, 2, 29));
    }

    #[test]
    fn accumulate_books_only_observed_increases() {
        let mut s = TrafficState::default();
        // First reading under a new boot is only a baseline, nothing booked.
        assert_eq!(s.accumulate("b1/x", 1000, 2000, 1, at(2026, 10, 26), Some(1_700_000_000), 0), (0, 0));
        assert_eq!(s.accumulate("b1/x", 1500, 2600, 1, at(2026, 10, 26), Some(1_700_000_000), 1_700_000_100), (500, 600));
        assert_eq!(s.month_rx, 500);
        assert_eq!(s.total_tx, 600);
        // Counter moved backwards under the same boot: book nothing, re-align.
        assert_eq!(s.accumulate("b1/x", 100, 100, 1, at(2026, 10, 26), Some(1_700_000_000), 1_700_000_200), (0, 0));
        assert_eq!(s.accumulate("b1/x", 300, 400, 1, at(2026, 10, 26), Some(1_700_000_000), 1_700_000_300), (200, 300));
    }

    #[test]
    fn accumulate_recovers_unseen_reboot_and_resets_period() {
        let mut s = TrafficState::default();
        s.accumulate("b1/x", 1000, 1000, 15, at(2026, 10, 20), Some(1_700_000_000), 0);
        s.accumulate("b1/x", 2000, 2000, 15, at(2026, 10, 20), Some(1_700_000_000), 1_700_001_000);
        // Host rebooted after the server last heard from it: its counters
        // since boot are booked once, not discarded as a re-alignment.
        let (dr, dt) = s.accumulate("b2/x", 700, 800, 15, at(2026, 10, 21), Some(1_700_002_000), 1_700_001_000);
        assert_eq!((dr, dt), (700, 800));
        // Crossing into a new billing period restarts the month counter.
        s.accumulate("b2/x", 900, 1000, 15, at(2026, 11, 16), Some(1_700_002_000), 1_700_003_000);
        assert_eq!(s.month_rx, 200);
        assert_eq!(s.month_start, "2026-11-15");
        assert_eq!(s.total_rx, 1000 + 700 + 200);
    }

    #[test]
    fn unlimited_is_zero_only() {
        // Only 0 means unlimited; 9999 GiB is a real quota.
        assert!(is_unlimited(0));
        assert!(!is_unlimited(9999 * 1024 * 1024 * 1024));
        assert!(!is_unlimited(9998 * 1024 * 1024 * 1024));
        assert!(!is_unlimited(1000 * 1024 * 1024 * 1024));
        assert!(!is_unlimited(1));
    }

    #[test]
    fn split_month_used_matches_metering_mode() {
        // "up" meters tx, "down" meters rx: the split must land the value
        // on the metered direction, otherwise month_used collapses to 0.
        assert_eq!(split_month_used(100, "up"), (0, 100));
        assert_eq!(split_month_used(100, "down"), (100, 0));
        assert_eq!(split_month_used(100, "max"), (100, 100));
        assert_eq!(split_month_used(101, "sum"), (51, 50));
        for mode in ["up", "down", "max", "sum"] {
            let (rx, tx) = split_month_used(100, mode);
            assert_eq!(month_used(rx, tx, mode), 100, "mode {mode}");
        }
    }

    #[test]
    fn align_used_to_matches_metering_mode_and_preserves_other_side() {
        // up: tx becomes v, rx untouched; down: mirror image.
        assert_eq!(align_used_to(50_000, "up", 300, 700), (300, 50_000));
        assert_eq!(align_used_to(50_000, "down", 300, 700), (50_000, 700));
        // max: the larger side becomes v, the smaller is capped at v.
        assert_eq!(align_used_to(50_000, "max", 300, 700), (300, 50_000));
        assert_eq!(align_used_to(200, "max", 300, 700), (200, 200));
        // sum: no unmetered side, keeps the half/half split.
        assert_eq!(align_used_to(50_001, "sum", 300, 700), (25_001, 25_000));
        // In every case the metered figure of the result equals v exactly.
        for mode in ["up", "down", "max", "sum"] {
            let (rx, tx) = align_used_to(50_000, mode, 300, 700);
            assert_eq!(month_used(rx, tx, mode), 50_000, "mode {mode}");
        }
    }

    #[test]
    fn correct_raises_total_to_cover_month_and_never_lowers_it() {
        // Mirrors cdt's live row: month seeded above everything
        // accumulate has seen, so total < month before the fix.
        let mut s = TrafficState::default();
        s.month_rx = 27_718_162_800;
        s.month_tx = 660_846_860;
        s.total_rx = 998_911_716;
        s.total_tx = 1_029_586_087;
        s.month_start = "2026-10-20".to_string();
        s.correct(None, None, None, None, 20, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(s.total_rx, 27_718_162_800);
        assert_eq!(s.total_tx, 1_029_586_087);
        assert!(s.total_rx + s.total_tx >= s.month_rx + s.month_tx);
        // A later downward month correction must not shrink total.
        s.correct(Some(100), Some(200), None, None, 20, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(s.total_rx, 27_718_162_800);
        assert_eq!(s.total_tx, 1_029_586_087);
    }

    #[test]
    fn align_used_survives_later_reports_and_keeps_total_above_month() {
        // Mirrors the admin endpoint (align by mode, then correct)
        // followed by three client reports (accumulate): the billed
        // figure must stay at the aligned value plus only the metered
        // direction's deltas, the unmetered direction must be preserved,
        // and total must stay >= month throughout.
        let mut s = TrafficState::default();
        s.accumulate("b1/x", 1000, 2000, 14, at(2026, 10, 20), Some(1_700_000_000), 0);
        s.accumulate("b1/x", 5000, 9000, 14, at(2026, 10, 20), Some(1_700_000_000), 1_700_001_000);
        assert_eq!(month_used(s.month_rx, s.month_tx, "up"), 7000);
        // Admin aligns used to 50_000 under "up": tx becomes 50_000,
        // the real rx history (4000) is preserved, not wiped to 0.
        let (rx, tx) = align_used_to(50_000, "up", s.month_rx, s.month_tx);
        s.correct(Some(rx), Some(tx), None, None, 14, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(month_used(s.month_rx, s.month_tx, "up"), 50_000);
        assert_eq!(s.month_rx, 4000);
        assert!(s.total_rx >= s.month_rx && s.total_tx >= s.month_tx);
        // Three later reports, each adding (100 rx, 300 tx).
        let mut last_rx = 5000u64;
        let mut last_tx = 9000u64;
        for _ in 0..3 {
            last_rx += 100;
            last_tx += 300;
            s.accumulate("b1/x", last_rx, last_tx, 14, at(2026, 10, 26), Some(1_700_000_000), 1_700_002_000);
        }
        assert_eq!(month_used(s.month_rx, s.month_tx, "up"), 50_000 + 900);
        assert_eq!(s.month_rx, 4000 + 300);
        assert!(s.total_rx >= s.month_rx && s.total_tx >= s.month_tx);
    }

    #[test]
    fn correct_month_used_survives_later_reports() {
        // Mirrors the admin endpoint (split by mode, then correct) followed
        // by three client reports (accumulate): the billed figure must stay
        // at the corrected value plus only the metered direction's deltas,
        // never fall back to ~0 or to the pre-correction value.
        let mut s = TrafficState::default();
        s.accumulate("b1/x", 1000, 2000, 14, at(2026, 10, 20), Some(1_700_000_000), 0);
        s.accumulate("b1/x", 5000, 9000, 14, at(2026, 10, 20), Some(1_700_000_000), 1_700_001_000);
        assert_eq!(month_used(s.month_rx, s.month_tx, "up"), 7000);
        // Admin corrects month used to 50 GiB-equivalent units under "up".
        let (rx, tx) = split_month_used(50_000, "up");
        s.correct(Some(rx), Some(tx), None, None, 14, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(month_used(s.month_rx, s.month_tx, "up"), 50_000);
        // Three later reports, each adding (100 rx, 300 tx).
        let mut last_rx = 5000u64;
        let mut last_tx = 9000u64;
        for _ in 0..3 {
            last_rx += 100;
            last_tx += 300;
            s.accumulate("b1/x", last_rx, last_tx, 14, at(2026, 10, 26), Some(1_700_000_000), 1_700_002_000);
        }
        assert_eq!(month_used(s.month_rx, s.month_tx, "up"), 50_000 + 900);
        assert_eq!(s.month_rx, 300);
    }

    #[test]
    fn month_used_and_alert_steps() {
        assert_eq!(month_used(3, 5, "sum"), 8);
        assert_eq!(month_used(3, 5, "up"), 5);
        assert_eq!(month_used(3, 5, "down"), 3);
        assert_eq!(month_used(3, 5, "max"), 5);
        assert_eq!(TrafficState::alert_step(79, 100, 80), 0);
        assert_eq!(TrafficState::alert_step(80, 100, 80), 80);
        assert_eq!(TrafficState::alert_step(100, 100, 80), 100);
        assert_eq!(TrafficState::alert_step(50, 0, 80), 0);
    }
}
