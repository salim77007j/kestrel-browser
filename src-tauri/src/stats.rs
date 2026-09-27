//! Blocking statistics: today + all-time counters, per-domain breakdown.

use crate::state::{chrono_day, AppState, Stats};
use std::path::PathBuf;

pub fn load_stats(dir: &PathBuf) -> Stats {
    let today = chrono_day();
    let path = dir.join("stats.json");
    let stored: Option<StoredStats> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    match stored {
        Some(s) if s.day == today => Stats {
            day: s.day,
            blocked_today: s.blocked_today,
            trackers_today: s.trackers_today,
            requests_today: s.requests_today,
            blocked_all: s.blocked_all,
            trackers_all: s.trackers_all,
            per_domain: s.per_domain,
        },
        Some(s) => Stats {
            day: today,
            blocked_today: 0,
            trackers_today: 0,
            requests_today: 0,
            blocked_all: s.blocked_all,
            trackers_all: s.trackers_all,
            per_domain: s.per_domain,
        },
        None => Stats::new_today(),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredStats {
    day: String,
    blocked_today: u64,
    trackers_today: u64,
    requests_today: u64,
    blocked_all: u64,
    trackers_all: u64,
    per_domain: std::collections::BTreeMap<String, u64>,
}

pub fn save_stats(state: &AppState) {
    let s = state.stats.lock().unwrap();
    let st = StoredStats {
        day: s.day.clone(),
        blocked_today: s.blocked_today,
        trackers_today: s.trackers_today,
        requests_today: s.requests_today,
        blocked_all: s.blocked_all,
        trackers_all: s.trackers_all,
        per_domain: s.per_domain.clone(),
    };
    drop(s);
    std::fs::write(
        state.data_dir.join("stats.json"),
        serde_json::to_string(&st).unwrap_or_default(),
    )
    .ok();
}

/// Record one observed request; `blocked` says whether the engine stopped it.
/// `is_tracker` is matched against the EasyPrivacy host set.
pub fn record(state: &AppState, host: &str, blocked: bool, is_tracker: bool) {
    let mut s = state.stats.lock().unwrap();
    let today = chrono_day();
    if s.day != today {
        s.day = today;
        s.blocked_today = 0;
        s.trackers_today = 0;
        s.requests_today = 0;
        s.per_domain.clear();
    }
    s.requests_today += 1;
    if blocked {
        s.blocked_today += 1;
        s.blocked_all += 1;
        if is_tracker {
            s.trackers_today += 1;
            s.trackers_all += 1;
        }
        *s.per_domain.entry(host.to_string()).or_insert(0) += 1;
        if s.per_domain.len() > 500 {
            // keep the heaviest offenders
            let mut v: Vec<(String, u64)> = s.per_domain.iter().map(|(k, v)| (k.clone(), *v)).collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v.truncate(200);
            s.per_domain = v.into_iter().collect();
        }
    }
}
