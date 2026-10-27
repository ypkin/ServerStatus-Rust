//! Server-side probe latency history (replaces browser sampling).
//!
//! Before this module the latency chart was sampled by the browser
//! itself: every 30 s stats refresh appended one live value per
//! configured probe/stack into in-memory minute buckets, so closing
//! the browser discarded all history and a freshly opened chart had
//! to wait for points to accumulate. The server already receives
//! every probe reading on every client report, so it now buckets
//! those readings itself and serves them back to the chart.
//!
//! Model (mirrors the old browser sampler's bucket rules, so the
//! chart code barely changes):
//! - One series per host per probe slot: `cu`/`ct`/`cm` x `v4`/`v6`.
//!   Slots (not display names) are the keys, so renaming a probe in
//!   the admin panel keeps one continuous series; the chart resolves
//!   the current display name from stats `ping_probes` (which now
//!   carries the slot) at draw time.
//! - Every report merges its reading into the current minute bucket
//!   and the current hour bucket (running mean of the samples in
//!   that bucket). Minute buckets feed the 1h/6h/24h ranges, hour
//!   buckets feed 7d. Unlike the browser sampler, hour buckets are
//!   live-updated too, so 7d needs no current-hour partial.
//! - Sample classification is exactly the browser sampler's `latVal`:
//!   time >= 1000 ms or loss >= 100% is a timeout (an explicit null
//!   bucket, so the chart breaks the line instead of bridging it);
//!   time <= 0 is unmeasured (no point at all); anything else is a
//!   valid reading. A valid reading later in the same minute wins
//!   over an earlier timeout null in that minute.
//! - Persistence is `ping_history.json` next to `traffic.json`
//!   (same style: version-tolerant JSON, write-then-rename, loaded
//!   once at startup, saved on the stats 60 s tick). History only
//!   exists from the deploy that introduces this file: nothing
//!   before that is backfilled or fabricated.
//! - Retention: minute buckets a bit past 24 h (25 h, 1500 points),
//!   hour buckets a bit past 7 d (8 d, 192 points). A crash loses at
//!   most the 60 s since the last tick save, same as traffic.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::sync::{LazyLock, Mutex};

const PING_HIST_FILE: &str = "ping_history.json";
/// Minute buckets are read by the 24 h range; keep one extra hour so
/// a slow tick never clips the window's left edge.
const MIN_KEEP_SECS: i64 = 25 * 3600;
const MAX_MIN_POINTS: usize = 1500;
/// Hour buckets are read by the 7 d range; keep one extra day.
const HOUR_KEEP_SECS: i64 = 8 * 24 * 3600;
const MAX_HOUR_POINTS: usize = 192;

/// One averaged bucket. `ms`/`loss` are `None` for a timeout-only
/// bucket (explicit gap); `n` counts the valid samples averaged
/// (0 for a timeout-only bucket).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bucket {
    #[serde(default)]
    pub t: i64,
    #[serde(default)]
    pub ms: Option<f64>,
    #[serde(default)]
    pub loss: Option<f64>,
    #[serde(default)]
    pub n: u32,
}

/// One probe slot's series (minute + hour tiers).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Series {
    #[serde(default)]
    pub min: VecDeque<Bucket>,
    #[serde(default)]
    pub hour: VecDeque<Bucket>,
}

/// Whole file: host name -> slot key (`cu:v4`) -> series.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct PingHistFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    hosts: HashMap<String, HashMap<String, Series>>,
}

/// Live state, shared by the report thread (record), the timer
/// thread (periodic save) and the HTTP handler (query). This lock
/// never nests with the registry or ADMIN locks: record/query/save
/// only ever take this one lock.
pub static PING_HIST: LazyLock<Mutex<PingHistFile>> = LazyLock::new(Default::default);

pub fn load() {
    let contents = fs::read_to_string(PING_HIST_FILE).unwrap_or_default();
    if contents.is_empty() {
        return;
    }
    match serde_json::from_str::<PingHistFile>(&contents) {
        Ok(file) => {
            *PING_HIST.lock().unwrap() = file;
        }
        Err(e) => {
            error!("ignore invalid {PING_HIST_FILE}: {e}");
        }
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Merge one reading into a tier's current bucket. A valid reading
/// merges into (or replaces a timeout null in) its bucket; a
/// timeout only creates a null bucket when the bucket has no point
/// yet, and never overwrites a valid average.
fn merge_point(tier: &mut VecDeque<Bucket>, bucket: i64, ms: Option<f64>, loss: Option<f64>) {
    if let Some(last) = tier.back_mut() {
        if last.t == bucket {
            match (last.ms, ms) {
                (Some(prev), Some(m)) => {
                    let n = last.n as f64;
                    last.ms = Some(round1((prev * n + m) / (n + 1.0)));
                    let prev_loss = last.loss.unwrap_or(0.0);
                    last.loss = Some(round1((prev_loss * n + loss.unwrap_or(0.0)) / (n + 1.0)));
                    last.n += 1;
                }
                (None, Some(m)) => {
                    last.ms = Some(round1(m));
                    last.loss = loss.map(round1);
                    last.n = 1;
                }
                _ => {}
            }
            return;
        }
    }
    tier.push_back(Bucket { t: bucket, ms: ms.map(round1), loss: loss.map(round1), n: if ms.is_some() { 1 } else { 0 } });
}

fn prune(tier: &mut VecDeque<Bucket>, cutoff: i64, cap: usize) {
    while tier.front().is_some_and(|b| b.t < cutoff) {
        tier.pop_front();
    }
    while tier.len() > cap {
        tier.pop_front();
    }
}

fn prune_all(file: &mut PingHistFile, now: i64) {
    for series in file.hosts.values_mut().flat_map(|slots| slots.values_mut()) {
        prune(&mut series.min, now - MIN_KEEP_SECS, MAX_MIN_POINTS);
        prune(&mut series.hour, now - HOUR_KEEP_SECS, MAX_HOUR_POINTS);
    }
}

/// One probe reading from a client report: slot key plus the raw
/// time (ms) / loss (%) pair, exactly as the client reported it.
pub struct ProbeReading {
    pub slot: &'static str,
    pub time_ms: f64,
    pub loss: f64,
}

/// Book one report's probe readings into their minute + hour
/// buckets. `now` is the server receive time (unix seconds).
/// Classification mirrors the overlay's old `latVal`: timeout ->
/// null bucket, unmeasured (time <= 0, loss < 100) -> no point.
pub fn record(host: &str, now: i64, readings: &[ProbeReading]) {
    let mut file = PING_HIST.lock().unwrap();
    record_in(&mut file, host, now, readings);
}

/// Pure core of [`record`], kept lock-free so tests can run on a
/// local file value without sharing the global mutex.
fn record_in(file: &mut PingHistFile, host: &str, now: i64, readings: &[ProbeReading]) {
    let slots = file.hosts.entry(host.to_string()).or_default();
    let minute = now / 60 * 60;
    let hour = now / 3600 * 3600;
    for r in readings {
        let (ms, loss) = if r.time_ms >= 1000.0 || r.loss >= 100.0 {
            (None, None)
        } else if r.time_ms > 0.0 {
            (Some(r.time_ms), Some(r.loss))
        } else {
            continue;
        };
        let series = slots.entry(r.slot.to_string()).or_default();
        merge_point(&mut series.min, minute, ms, loss);
        merge_point(&mut series.hour, hour, ms, loss);
        prune(&mut series.min, now - MIN_KEEP_SECS, MAX_MIN_POINTS);
        prune(&mut series.hour, now - HOUR_KEEP_SECS, MAX_HOUR_POINTS);
    }
}

/// One series for the API response: slot key plus its points in
/// the requested window, as `[timestamp, ms, loss]` triples with
/// JSON nulls for timeout buckets.
#[derive(Debug, Clone, Serialize)]
pub struct SeriesOut {
    pub slot: String,
    pub points: Vec<(i64, Option<f64>, Option<f64>)>,
}

/// Read a host's history. `week` selects hour buckets (7 d),
/// otherwise minute buckets for the given window in seconds.
/// Only series with at least one point in the window are returned;
/// the chart joins them against the configured probes from stats,
/// so an unconfigured slot's stale series is never drawn.
pub fn query(host: &str, week: bool, window_secs: i64, now: i64) -> Vec<SeriesOut> {
    let file = PING_HIST.lock().unwrap();
    query_in(&file, host, week, window_secs, now)
}

/// Pure core of [`query`], kept lock-free for tests.
fn query_in(file: &PingHistFile, host: &str, week: bool, window_secs: i64, now: i64) -> Vec<SeriesOut> {
    let Some(slots) = file.hosts.get(host) else {
        return Vec::new();
    };
    let from = now - window_secs;
    let mut out = Vec::new();
    for (slot, series) in slots {
        let tier = if week { &series.hour } else { &series.min };
        let points: Vec<(i64, Option<f64>, Option<f64>)> = tier
            .iter()
            .filter(|b| b.t >= from)
            .map(|b| (b.t, b.ms, b.loss))
            .collect();
        if !points.is_empty() {
            out.push(SeriesOut { slot: slot.clone(), points });
        }
    }
    out.sort_by(|a, b| a.slot.cmp(&b.slot));
    out
}

/// Persist the file (write-then-rename, same as traffic.json).
/// Prunes first so the file never grows past retention.
pub fn save() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let data = {
        let mut file = PING_HIST.lock().unwrap();
        prune_all(&mut file, now);
        serde_json::to_string(&*file).unwrap_or_default()
    };
    if data.is_empty() {
        return;
    }
    // Write-then-rename so a crash mid-write never leaves a torn file.
    if fs::write(format!("{PING_HIST_FILE}.tmp"), &data).is_ok() {
        if let Err(e) = fs::rename(format!("{PING_HIST_FILE}.tmp"), PING_HIST_FILE) {
            error!("save {PING_HIST_FILE} fail: {e}");
        }
    } else {
        error!("save {PING_HIST_FILE} fail");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(slot: &'static str, time_ms: f64, loss: f64) -> ProbeReading {
        ProbeReading { slot, time_ms, loss }
    }

    fn min_points(file: &PingHistFile, host: &str, slot: &str) -> Vec<(i64, Option<f64>, u32)> {
        file.hosts
            .get(host)
            .and_then(|slots| slots.get(slot))
            .map(|s| s.min.iter().map(|b| (b.t, b.ms, b.n)).collect())
            .unwrap_or_default()
    }

    #[test]
    fn minute_buckets_average_samples_in_the_same_minute() {
        let mut file = PingHistFile::default();
        // 1_700_000_000 sits 20 s into its minute, so +0 s and +30 s
        // share one minute bucket and +70 s starts the next.
        record_in(&mut file, "h1", 1_700_000_000, &[reading("cu:v4", 40.0, 0.0)]);
        record_in(&mut file, "h1", 1_700_000_030, &[reading("cu:v4", 44.0, 0.0)]);
        record_in(&mut file, "h1", 1_700_000_070, &[reading("cu:v4", 50.0, 0.0)]);
        let pts = min_points(&file, "h1", "cu:v4");
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].0, 1_700_000_000 / 60 * 60);
        assert_eq!(pts[0].1, Some(42.0));
        assert_eq!(pts[0].2, 2);
        assert_eq!(pts[1].1, Some(50.0));
    }

    #[test]
    fn timeout_is_a_null_gap_and_valid_wins_within_the_minute() {
        let mut file = PingHistFile::default();
        record_in(&mut file, "h1", 1_700_000_000, &[reading("cu:v4", 1000.0, 0.0)]);
        let pts = min_points(&file, "h1", "cu:v4");
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].1, None);
        assert_eq!(pts[0].2, 0);
        // A valid reading later in the same minute replaces the null.
        record_in(&mut file, "h1", 1_700_000_030, &[reading("cu:v4", 42.0, 0.0)]);
        let pts = min_points(&file, "h1", "cu:v4");
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].1, Some(42.0));
        assert_eq!(pts[0].2, 1);
        // And a later timeout in that minute must not clobber it.
        record_in(&mut file, "h1", 1_700_000_035, &[reading("cu:v4", 1000.0, 100.0)]);
        let pts = min_points(&file, "h1", "cu:v4");
        assert_eq!(pts[0].1, Some(42.0));
    }

    #[test]
    fn unmeasured_zero_reports_leave_no_point() {
        let mut file = PingHistFile::default();
        record_in(&mut file, "h1", 1_700_000_000, &[reading("cu:v4", 0.0, 0.0)]);
        let pts = min_points(&file, "h1", "cu:v4");
        assert!(pts.is_empty());
    }

    #[test]
    fn retention_prunes_old_buckets() {
        let mut file = PingHistFile::default();
        let t0 = 1_700_000_000;
        record_in(&mut file, "h1", t0, &[reading("cu:v4", 40.0, 0.0)]);
        // 26 h later the minute point is past 25 h retention, but
        // the hour point (8 d retention) survives.
        record_in(&mut file, "h1", t0 + 26 * 3600, &[reading("cu:v4", 41.0, 0.0)]);
        assert_eq!(min_points(&file, "h1", "cu:v4").len(), 1);
        assert_eq!(file.hosts["h1"]["cu:v4"].hour.len(), 2);
        // 9 d later the old hour points are past 8 d retention.
        record_in(&mut file, "h1", t0 + 9 * 24 * 3600, &[reading("cu:v4", 42.0, 0.0)]);
        let hours: Vec<i64> = file.hosts["h1"]["cu:v4"].hour.iter().map(|b| b.t).collect();
        assert_eq!(hours.len(), 2); // t0+26h hour + t0+9d hour
        assert!(hours.iter().all(|t| *t >= t0 + 9 * 24 * 3600 - HOUR_KEEP_SECS));
        assert_eq!(min_points(&file, "h1", "cu:v4").len(), 1);
    }

    #[test]
    fn query_filters_window_and_week_uses_hour_buckets() {
        let mut file = PingHistFile::default();
        // now sits 800 s into its hour, so -600 s / -60 s / 0 s share
        // one hour bucket but three distinct minute buckets.
        let now = 1_700_000_000;
        record_in(&mut file, "h1", now - 600, &[reading("cu:v4", 40.0, 0.0)]);
        record_in(&mut file, "h1", now - 60, &[reading("cu:v4", 42.0, 0.0)]);
        record_in(&mut file, "h1", now, &[reading("cu:v4", 44.0, 0.0)]);
        // 1 h window keeps all three minute points.
        let out = query_in(&file, "h1", false, 3600, now);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].slot, "cu:v4");
        assert_eq!(out[0].points.len(), 3);
        assert_eq!(out[0].points[0].1, Some(40.0));
        assert_eq!(out[0].points[2].1, Some(44.0));
        // 7 d window reads hour buckets: all three samples share one
        // hour bucket, averaged.
        let out = query_in(&file, "h1", true, 7 * 24 * 3600, now);
        assert_eq!(out[0].points.len(), 1);
        assert_eq!(out[0].points[0].1, Some(42.0)); // (40+42+44)/3
    }

    #[test]
    fn query_unknown_host_is_empty_and_file_format_is_tolerant() {
        let file = PingHistFile::default();
        assert!(query_in(&file, "nope", false, 3600, 1_700_000_000).is_empty());
        let parsed: Result<PingHistFile, _> = serde_json::from_str("not json");
        assert!(parsed.is_err());
        let parsed: PingHistFile = serde_json::from_str("{}").unwrap();
        assert!(parsed.hosts.is_empty());
        // A saved file round-trips (the on-disk shape).
        let mut file = PingHistFile::default();
        record_in(&mut file, "h1", 1_700_000_010, &[reading("cu:v4", 42.5, 0.0)]);
        let json = serde_json::to_string(&file).unwrap();
        let back: PingHistFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hosts["h1"]["cu:v4"].min[0].ms, Some(42.5));
    }
}
