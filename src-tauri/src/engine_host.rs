//! Privacy engine lifecycle: a thread-confined actor owns the adblock
//! engine (it holds `Rc` internally and cannot cross threads). The shell
//! talks to it through a channel-based client that is cheap to clone and
//! fully `Send + Sync`.

use crate::state::{include_gz_lines, AppState};
use kestrel_privacy::engine::CosmeticResult;
use kestrel_privacy::PrivacyEngine;
use serde_json::json;
use std::collections::HashSet;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// One job for the engine actor.
pub enum Job {
    Block {
        url: String,
        source: String,
        rtype: &'static str,
        resp: Sender<bool>,
    },
    Cosmetic {
        url: String,
        resp: Sender<CosmeticResult>,
    },
}

/// Cloneable handle to the engine actor. `Sender` is Send + Sync as long as
/// `Job` is Send.
#[derive(Clone)]
pub struct PrivacyClient {
    tx: Sender<Job>,
}

impl PrivacyClient {
    pub fn should_block(&self, url: &str, source: &str, rtype: &'static str) -> bool {
        let (tx, rx) = channel();
        let job = Job::Block {
            url: url.to_string(),
            source: source.to_string(),
            rtype,
            resp: tx,
        };
        if self.tx.send(job).is_err() {
            return false;
        }
        // bounded wait: fail-open if the engine is momentarily busy
        rx.recv_timeout(Duration::from_millis(250)).unwrap_or(false)
    }

    pub fn cosmetic(&self, url: &str) -> CosmeticResult {
        let (tx, rx) = channel();
        let job = Job::Cosmetic {
            url: url.to_string(),
            resp: tx,
        };
        if self.tx.send(job).is_ok() {
            if let Ok(r) = rx.recv_timeout(Duration::from_millis(250)) {
                return r;
            }
        }
        CosmeticResult::default()
    }
}

/// Bundled offline seed lists (so the very first launch is fully protected).
struct Seed {
    id: &'static str,
    gz: &'static [u8],
    enabled_key: &'static str,
    tracker_list: bool,
}

const SEEDS: &[Seed] = &[
    Seed { id: "easylist", gz: include_bytes!("../assets/filters/easylist.txt.gz"), enabled_key: "easylist", tracker_list: false },
    Seed { id: "easyprivacy", gz: include_bytes!("../assets/filters/easyprivacy.txt.gz"), enabled_key: "easyprivacy", tracker_list: true },
    Seed { id: "fanboy-annoyance", gz: include_bytes!("../assets/filters/fanboy-annoyance.txt.gz"), enabled_key: "fanboy_annoyance", tracker_list: false },
    Seed { id: "peter-lowe", gz: include_bytes!("../assets/filters/peterlowe.txt.gz"), enabled_key: "peter_lowe", tracker_list: false },
];

const LIST_URLS: &[(&str, &str)] = &[
    ("easylist", "https://easylist.to/easylist/easylist.txt"),
    ("easyprivacy", "https://easylist.to/easylist/easyprivacy.txt"),
    ("fanboy-annoyance", "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/annoyances.txt"),
    ("peter-lowe", "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=nohtml&mimetype=plaintext"),
    ("urlhaus", "https://urlhaus.abuse.ch/downloads/hostfile/"),
];

/// Compose the rule text for the current settings: cached remote lists when
/// present, otherwise bundled seeds; plus the user's custom rules.
fn compose_rules(state: &AppState) -> (Vec<String>, HashSet<String>) {
    let settings = state.settings.read().unwrap().clone();
    let mut rules = Vec::new();
    let mut trackers = HashSet::new();

    for seed in SEEDS {
        let enabled = match seed.enabled_key {
            "easylist" => settings.filter_lists.easylist,
            "easyprivacy" => settings.filter_lists.easyprivacy,
            "fanboy_annoyance" => settings.filter_lists.fanboy_annoyance,
            "peter_lowe" => settings.filter_lists.peter_lowe,
            _ => true,
        };
        if !enabled {
            continue;
        }
        let cached = state.data_dir.join("filters").join(format!("{}.txt", seed.id));
        let text = match std::fs::read_to_string(&cached) {
            Ok(t) if !t.is_empty() => t,
            _ => include_gz_lines(seed.gz).join("\n"),
        };
        if seed.tracker_list {
            trackers.extend(extract_hosts(&text));
        }
        rules.push(text);
    }

    // urlhaus safe-browsing feed: keep a plain-text copy for the interstitial
    {
        let cached = state.data_dir.join("filters").join("urlhaus.txt");
        if !cached.exists() {
            let lines = include_gz_lines(include_bytes!("../assets/filters/urlhaus.txt.gz"));
            std::fs::write(&cached, lines.join("\n")).ok();
        }
    }

    for f in &settings.custom_filters {
        if !f.trim().is_empty() && !f.starts_with('!') {
            rules.push(f.clone());
        }
    }

    (rules, trackers)
}

/// Pull `||host^` hosts out of a filter list (used for tracker stats).
fn extract_hosts(text: &str) -> HashSet<String> {
    let mut hosts = HashSet::new();
    for line in text.lines().take(120_000) {
        if let Some(rest) = line.strip_prefix("||") {
            let end = rest.find(['^', '/', '$', ':', '?', '*']).unwrap_or(rest.len());
            let host = &rest[..end];
            if !host.is_empty() && !host.contains('*') && host.contains('.') && !host.starts_with('-') {
                hosts.insert(host.to_ascii_lowercase());
            }
        }
    }
    hosts
}

/// Build the engine in a background thread and swap the client in when ready.
pub fn spawn_engine_build(app: AppHandle) {
    std::thread::spawn(move || {
        build_and_swap(&app);
    });
}

pub fn build_and_swap(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (rules, trackers) = compose_rules(&state);
    match PrivacyEngine::from_rules(&rules) {
        Ok(engine) => {
            let count = engine.rule_count();
            let (tx, rx) = channel::<Job>();
            std::thread::spawn(move || {
                // the engine lives on THIS thread only
                while let Ok(job) = rx.recv() {
                    match job {
                        Job::Block { url, source, rtype, resp } => {
                            let _ = resp.send(engine.should_block(&url, &source, rtype));
                        }
                        Job::Cosmetic { url, resp } => {
                            let _ = resp.send(engine.cosmetic(&url));
                        }
                    }
                }
            });
            *state.engine.write().unwrap() = Some(PrivacyClient { tx });
            *state.tracker_hosts.write().unwrap() = Arc::new(trackers);
            state.engine_ready.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = app.emit_to(
                crate::state::CHROME_LABEL,
                "engine-ready",
                json!({"rules": count}),
            );
        }
        Err(e) => {
            let _ = app.emit_to(
                crate::state::CHROME_LABEL,
                "toast",
                json!({"level": "error", "text": format!("Ad-block engine failed to build: {e}")}),
            );
        }
    }
}

/// Fetch the latest community lists and rebuild the engine.
pub fn update_lists_blocking(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let mut report = String::new();
    for (id, url) in LIST_URLS {
        match fetch_text(url) {
            Ok(text) if text.len() > 512 => {
                let path = state.data_dir.join("filters").join(format!("{id}.txt"));
                if std::fs::write(&path, &text).is_ok() {
                    report.push_str(&format!("{id}: updated ({} KB)\n", text.len() / 1024));
                }
            }
            Ok(_) => report.push_str(&format!("{id}: server returned an empty list, kept current\n")),
            Err(e) => report.push_str(&format!("{id}: failed ({e}), kept current\n")),
        }
    }
    build_and_swap(app);
    let rules = state.engine_rules.load(std::sync::atomic::Ordering::SeqCst);
    if rules > 0 {
        report.push_str(&format!("engine rebuilt: {rules} rules active\n"));
    } else {
        report.push_str("engine rebuild FAILED\n");
    }
    report
}

fn fetch_text(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout(std::time::Duration::from_secs(45))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Kestrel/0.2")
        .build();
    agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_tracker_hosts() {
        let hosts = extract_hosts("||doubleclick.net^\n||google-analytics.com/$script\n! comment\n||tracker.example.com^$third-party");
        assert!(hosts.contains("doubleclick.net"));
        assert!(hosts.contains("google-analytics.com"));
        assert!(hosts.contains("tracker.example.com"));
        assert_eq!(hosts.len(), 3);
    }

    #[test]
    fn seeds_decode() {
        let lines = include_gz_lines(include_bytes!("../assets/filters/easylist.txt.gz"));
        assert!(lines.len() > 50_000);
        assert!(lines.iter().any(|l| l.starts_with("||")));
    }
}
