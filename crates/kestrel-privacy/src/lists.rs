//! Filter-list management: bundled seeds, runtime cache, remote updates,
//! tracker-host extraction for the real-time shield audit.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ListId {
    EasyList,
    EasyPrivacy,
    Annoyances,
    Custom,
}

impl ListId {
    pub const ALL: [ListId; 3] = [ListId::EasyList, ListId::EasyPrivacy, ListId::Annoyances];
    pub fn key(&self) -> &'static str {
        match self {
            ListId::EasyList => "easylist",
            ListId::EasyPrivacy => "easyprivacy",
            ListId::Annoyances => "annoyances",
            ListId::Custom => "custom",
        }
    }
    pub fn title(&self) -> &'static str {
        match self {
            ListId::EasyList => "EasyList (ads)",
            ListId::EasyPrivacy => "EasyPrivacy (tracking)",
            ListId::Annoyances => "uBlock Annoyances",
            ListId::Custom => "Custom filters",
        }
    }
    pub fn url(&self) -> &'static str {
        match self {
            ListId::EasyList => "https://easylist.to/easylist/easylist.txt",
            ListId::EasyPrivacy => "https://easylist.to/easylist/easyprivacy.txt",
            ListId::Annoyances => {
                "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/annoyances.txt"
            }
            ListId::Custom => "",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ListStatus {
    pub key: String,
    pub title: String,
    pub enabled: bool,
    pub cached: bool,
    pub rules: usize,
    pub stale: bool,
}

pub struct ListManager {
    dir: PathBuf,
}

/// Minimal bundled seeds so first-run protection works even offline
/// (full lists are fetched on first launch with network access).
pub const SEED_EASYLIST: &str = include_str!("../assets/filters/seed-easylist.txt");
pub const SEED_EASYPRIVACY: &str = include_str!("../assets/filters/seed-easyprivacy.txt");

impl ListManager {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref().to_path_buf();
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn path_for(&self, id: ListId) -> PathBuf {
        self.dir.join(format!("{}.txt", id.key()))
    }

    fn stamp_for(&self, id: ListId) -> PathBuf {
        self.dir.join(format!("{}.stamp", id.key()))
    }

    pub fn cached_rules(&self, id: ListId) -> Option<String> {
        std::fs::read_to_string(self.path_for(id)).ok()
    }

    fn file_age_days(&self, id: ListId) -> f64 {
        let stamp = self.stamp_for(id);
        let Ok(meta) = std::fs::metadata(&stamp) else { return f64::INFINITY };
        let Ok(modified) = meta.modified() else { return f64::INFINITY };
        let Ok(elapsed) = modified.elapsed() else { return f64::INFINITY };
        elapsed.as_secs_f64() / 86400.0
    }

    /// Load rules for all lists (cached + seeds fallback + custom filters).
    /// `enabled` maps list key -> on/off.
    pub fn load_all(
        &self,
        enabled: &HashMap<String, bool>,
        custom_filters: &[String],
    ) -> (Vec<String>, Vec<ListStatus>) {
        let mut rules = Vec::new();
        let mut statuses = Vec::new();
        for id in ListId::ALL {
            let enabled = *enabled.get(id.key()).unwrap_or(&true);
            let cached = self.cached_rules(id);
            let stale = self.file_age_days(id) > 3.0;
            let text = match cached {
                Some(t) if !t.trim().is_empty() => Some(t),
                _ => None,
            };
            let source_text = text.unwrap_or_else(|| {
                match id {
                    ListId::EasyList => Some(SEED_EASYLIST.to_string()),
                    ListId::EasyPrivacy => Some(SEED_EASYPRIVACY.to_string()),
                    _ => None,
                }
                .unwrap_or_default()
            });
            let count = source_text.lines().filter(|l| !l.trim().is_empty()).count();
            if enabled {
                rules.extend(source_text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()));
            }
            statuses.push(ListStatus {
                key: id.key().to_string(),
                title: id.title().to_string(),
                enabled,
                cached: self.cached_rules(id).is_some(),
                rules: count,
                stale,
            });
        }
        rules.extend(custom_filters.iter().cloned());
        (rules, statuses)
    }

    /// Fetch one list over the network (blocking — call from a worker thread).
    pub fn fetch(&self, id: ListId) -> Result<usize, String> {
        let url = id.url();
        if url.is_empty() {
            return Err("no url".into());
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(crate::KESTREL_UA)
            .build();
        let resp = agent.get(url).call().map_err(|e| e.to_string())?;
        let mut text = String::new();
        use std::io::Read;
        resp.into_reader()
            .take(24 * 1024 * 1024)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() < 256 {
            return Err("suspiciously small list".into());
        }
        let count = text.lines().count();
        std::fs::write(self.path_for(id), text).map_err(|e| e.to_string())?;
        std::fs::write(self.stamp_for(id), b"ok").map_err(|e| e.to_string())?;
        Ok(count)
    }

    pub fn fetch_all(&self, ids: &[ListId]) -> HashMap<String, Result<usize, String>> {
        let mut out = HashMap::new();
        for id in ids {
            out.insert(id.key().to_string(), self.fetch(*id));
        }
        out
    }
}

/// Compact host list from `||host^` style rules — used by the UI-process
/// shield audit to count real tracker/ads requests observed per page.
pub fn extract_tracker_hosts(rules: &[String]) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    for r in rules {
        let r = r.trim();
        if r.starts_with("@@") || r.is_empty() {
            continue;
        }
        let Some(rest) = r.strip_prefix("||") else { continue };
        // Reject rules with options or separators we don't understand.
        let (base, opts) = split_opts(rest);
        if opts.is_some() {
            // Allow simple script/image/stylesheet options as tracker rules.
            let ok = opts.unwrap().split(',').all(|o| {
                matches!(o, "script" | "image" | "stylesheet" | "third-party" | "subdocument" | "xmlhttprequest")
            });
            if !ok {
                continue;
            }
        }
        let host = base.trim_end_matches('^').trim_start_matches('.');
        if host.is_empty() || host.len() < 4 || host.contains('*') || host.contains('/') {
            continue;
        }
        set.insert(host.to_ascii_lowercase());
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

fn split_opts(rule: &str) -> (&str, Option<&str>) {
    if let Some(i) = rule.rfind('$') {
        let (b, o) = (&rule[..i], &rule[i + 1..]);
        if !b.is_empty() && !o.is_empty() && o.chars().all(|c| c.is_ascii_alphanumeric() || "-_,=".contains(c)) {
            return (b, Some(o));
        }
    }
    (rule, None)
}
