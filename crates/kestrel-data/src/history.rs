//! Browsing history (capped, local-only).

use crate::store::JsonStore;
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub url: String,
    pub title: String,
    pub visited_at: u64, // unix ms
    pub visit_count: u64,
}

pub struct HistoryStore {
    store: JsonStore,
    entries: Vec<HistoryEntry>,
    next_id: u64,
}

impl HistoryStore {
    pub fn new(store: JsonStore) -> Self {
        let entries: Vec<HistoryEntry> = store.load();
        let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        Self {
            store,
            entries,
            next_id,
        }
    }

    /// Record a visit. Same URL within the same day updates the existing
    /// entry and moves it to the top (entries are ordered by recency).
    pub fn visit(&mut self, url: &str, title: &str, now_ms: u64) {
        if url.starts_with("kestrel-internal:") || url.starts_with("data:") {
            return;
        }
        let day = now_ms / 86_400_000;
        if let Some(pos) = self
            .entries
            .iter()
            .position(|e| e.url == url && e.visited_at / 86_400_000 == day)
        {
            let mut e = self.entries.remove(pos);
            if !title.is_empty() {
                e.title = title.to_string();
            }
            e.visited_at = now_ms;
            e.visit_count += 1;
            self.entries.insert(0, e);
            self.persist();
            return;
        }
        self.entries.insert(
            0,
            HistoryEntry {
                id: self.next_id,
                url: url.to_string(),
                title: title.to_string(),
                visited_at: now_ms,
                visit_count: 1,
            },
        );
        self.next_id += 1;
        if self.entries.len() > MAX_ENTRIES {
            self.entries.truncate(MAX_ENTRIES);
        }
        self.persist();
    }

    /// Local search over url + title, most recent first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<HistoryEntry> {
        let q = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                q.is_empty() || e.url.to_lowercase().contains(&q) || e.title.to_lowercase().contains(&q)
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn delete(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
        self.persist();
    }

    pub fn forget_site(&mut self, host: &str) {
        self.entries
            .retain(|e| !url_host(&e.url).eq_ignore_ascii_case(host));
        self.persist();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.persist();
    }

    pub fn all(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Top visited hosts across all history (for the privacy dashboard /
    /// stats page).
    pub fn top_hosts(&self, n: usize) -> Vec<(String, u64)> {
        let mut counts: std::collections::HashMap<String, u64> = Default::default();
        for e in &self.entries {
            *counts.entry(url_host(&e.url)).or_insert(0) += 1;
        }
        let mut v: Vec<(String, u64)> = counts.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    fn persist(&self) {
        self.store.save(&self.entries).ok();
    }
}

fn url_host(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let end = rest.find(['/', '?', ':', '#']).unwrap_or(rest.len());
    rest[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store(tag: &str) -> HistoryStore {
        let dir = std::env::temp_dir().join(format!("kestrel-hist-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        HistoryStore::new(JsonStore::new(dir.join("history.json")))
    }

    #[test]
    fn visit_search_delete() {
        let mut h = tmp_store("1");
        h.visit("https://example.com/a", "Example A", 1_000);
        h.visit("https://example.com/a", "Example A2", 2_000);
        h.visit("https://rust-lang.org/", "Rust", 3_000);
        assert_eq!(h.all().len(), 2);
        // most recent visit first
        assert_eq!(h.all()[0].title, "Rust");
        assert_eq!(h.all()[1].title, "Example A2");
        let res = h.search("rust", 10);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].url, "https://rust-lang.org/");
        h.delete(res[0].id);
        assert_eq!(h.search("rust", 10).len(), 0);
        assert_eq!(h.top_hosts(5)[0], ("example.com".to_string(), 1));
    }

    #[test]
    fn internal_pages_not_recorded() {
        let mut h = tmp_store("2");
        h.visit("kestrel-internal:newtab", "New Tab", 1);
        assert!(h.all().is_empty());
    }
}
