//! Settings: JSON document with defaults, atomic save.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub theme: String,                 // "dark" | "light"
    pub search_engine: String,         // duckduckgo | google | bing | brave | custom
    pub custom_search_url: String,     // must contain {q}
    pub fingerprint_protection: String, // standard | strict | off
    pub lists: BTreeMap<String, bool>,
    pub custom_filters: Vec<String>,
    pub webrtc_enabled: bool,
    pub popups: String,                // block | ask
    pub ask_download_location: bool,
    pub downloads_dir: String,
    pub restore_session: bool,
    pub do_not_track: bool,
    pub gestures: bool,
    pub discard_inactive_tabs: bool,
    pub hardware_accel: bool,
    pub default_zoom: f64,
    pub block_third_party_cookies: bool,
    pub permissions: BTreeMap<String, bool>, // "origin|kind" -> allowed
    pub lifetime_blocked: u64,
    pub vault_enabled: bool,
    pub window_width: i32,
    pub window_height: i32,
}

impl Default for Settings {
    fn default() -> Self {
        let mut lists = BTreeMap::new();
        lists.insert("easylist".to_string(), true);
        lists.insert("easyprivacy".to_string(), true);
        lists.insert("annoyances".to_string(), false);
        Self {
            theme: "dark".into(),
            search_engine: "duckduckgo".into(),
            custom_search_url: String::new(),
            fingerprint_protection: "standard".into(),
            lists,
            custom_filters: Vec::new(),
            webrtc_enabled: false,
            popups: "block".into(),
            ask_download_location: false,
            downloads_dir: crate::dirs::default_downloads().to_string_lossy().to_string(),
            restore_session: true,
            do_not_track: true,
            gestures: true,
            discard_inactive_tabs: true,
            hardware_accel: true,
            default_zoom: 1.0,
            block_third_party_cookies: true,
            permissions: BTreeMap::new(),
            lifetime_blocked: 0,
            vault_enabled: true,
            window_width: 1280,
            window_height: 820,
        }
    }
}

pub fn settings_path() -> PathBuf {
    crate::dirs::base_config().join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        match std::fs::read_to_string(settings_path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Atomic save: write tmp file, then rename over the target.
    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn search_url(&self, query: &str) -> String {
        let q = url_encode(query);
        match self.search_engine.as_str() {
            "google" => format!("https://www.google.com/search?q={q}"),
            "bing" => format!("https://www.bing.com/search?q={q}"),
            "brave" => format!("https://search.brave.com/search?q={q}"),
            "custom" => self
                .custom_search_url
                .replace("{q}", &q)
                .replace("%s", &q),
            _ => format!("https://duckduckgo.com/?q={q}"),
        }
    }
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip() {
        let s = Settings::default();
        let text = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(s, s2);
        // Unknown/missing fields fall back to defaults
        let s3: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s3.search_engine, "duckduckgo");
    }

    #[test]
    fn search_urls() {
        let mut s = Settings::default();
        assert!(s.search_url("rust lang").starts_with("https://duckduckgo.com/?q=rust%20lang"));
        s.search_engine = "custom".into();
        s.custom_search_url = "https://searx.example/search?q={q}".into();
        assert_eq!(s.search_url("a b"), "https://searx.example/search?q=a%20b");
    }

    #[test]
    fn encoding() {
        assert_eq!(url_encode("a&b=c d"), "a%26b%3Dc%20d");
    }
}
