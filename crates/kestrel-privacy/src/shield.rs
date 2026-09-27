//! Shield: per-site decisions and heuristics that live in the UI process.

use crate::fingerprint::FingerprintMode;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Shield {
    tracker_hosts: HashSet<String>,
}

impl Shield {
    pub fn new(tracker_hosts: &[String]) -> Self {
        Self { tracker_hosts: tracker_hosts.iter().cloned().collect() }
    }

    pub fn update_hosts(&mut self, hosts: &[String]) {
        self.tracker_hosts = hosts.iter().cloned().collect();
    }

    /// Does this URL match the compact tracker-host set?
    pub fn is_tracker(&self, url: &str) -> bool {
        match hostname(url) {
            Some(h) => self.tracker_hosts.iter().any(|t| h == *t || h.ends_with(&format!(".{t}"))),
            None => false,
        }
    }

    /// Homoglyph / punycode lookalike detection (no third-party calls).
    pub fn is_lookalike(&self, url: &str) -> bool {
        match hostname(url) {
            Some(h) => h.split('.').any(|label| label.starts_with("xn--")),
            None => false,
        }
    }

    pub fn tracker_host_count(&self) -> usize {
        self.tracker_hosts.len()
    }
}

pub fn hostname(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next()?;
    let host = host.split('@').next_back().unwrap_or(host);
    let host = host.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

pub fn fingerprint_mode(key: &str) -> FingerprintMode {
    FingerprintMode::from_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_matching() {
        let s = Shield::new(&["tracker.io".into(), "ads.test".into()]);
        assert!(s.is_tracker("https://cdn.tracker.io/pixel.js"));
        assert!(s.is_tracker("https://ads.test/x"));
        assert!(!s.is_tracker("https://notads.test/x"));
        assert!(!s.is_tracker("https://example.com/"));
    }

    #[test]
    fn lookalike() {
        let s = Shield::new(&[]);
        assert!(s.is_lookalike("https://xn--pypal-4ve.com/login"));
        assert!(!s.is_lookalike("https://example.com/"));
    }

    #[test]
    fn hostname_extraction() {
        assert_eq!(hostname("https://Example.com:443/path?q=1").as_deref(), Some("example.com"));
        assert_eq!(hostname("http://user:pw@Host.io/"), Some("host.io".to_string()));
    }
}
