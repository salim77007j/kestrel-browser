//! Browser settings: the single source of truth for every toggle in the
//! settings UI. Serialized to settings.json; every field has a sane default
//! and partial updates are applied as JSON patches.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use kestrel_privacy::fingerprint::FingerprintSettings;
use kestrel_privacy::headers::CookiePolicy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    // ---- Appearance ----
    pub theme: Theme,
    pub show_bookmarks_bar: bool,

    // ---- Startup ----
    pub startup: StartupMode,
    pub startup_pages: Vec<String>,

    // ---- Search ----
    pub search_engine: SearchEngineId,
    pub search_suggestions: bool,

    // ---- Privacy ----
    pub adblock_enabled: bool,
    pub filter_lists: FilterLists,
    pub custom_filters: Vec<String>,
    pub fingerprint: FingerprintSettings,
    pub cookie_policy: CookiePolicy,
    pub https_only: bool,
    pub safe_browsing: bool,
    pub do_not_track: bool,
    pub global_privacy_control: bool,

    // ---- Tabs ----
    pub confirm_close_multiple: bool,
    pub new_tab_position: NewTabPosition,
    pub gestures_enabled: bool,

    // ---- Downloads ----
    pub download_dir: String,
    pub download_ask_each_time: bool,

    // ---- Session ----
    pub restore_on_crash: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            show_bookmarks_bar: true,
            startup: StartupMode::Continue,
            startup_pages: Vec::new(),
            search_engine: SearchEngineId::DuckDuckGo,
            search_suggestions: true,
            adblock_enabled: true,
            filter_lists: FilterLists::default(),
            custom_filters: Vec::new(),
            fingerprint: FingerprintSettings::default(),
            cookie_policy: CookiePolicy::BlockThirdParty,
            https_only: true,
            safe_browsing: true,
            do_not_track: true,
            global_privacy_control: true,
            confirm_close_multiple: true,
            new_tab_position: NewTabPosition::End,
            gestures_enabled: true,
            download_dir: String::new(), // shell fills with the OS Downloads dir
            download_ask_each_time: false,
            restore_on_crash: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StartupMode {
    Continue,
    NewTab,
    Pages,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NewTabPosition {
    End,
    AfterCurrent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngineId {
    DuckDuckGo,
    Google,
    Bing,
    Brave,
}

impl SearchEngineId {
    pub fn search_url(&self, query: &str) -> String {
        let q = urlencode(query);
        match self {
            Self::DuckDuckGo => format!("https://duckduckgo.com/?q={q}"),
            Self::Google => format!("https://www.google.com/search?q={q}"),
            Self::Bing => format!("https://www.bing.com/search?q={q}"),
            Self::Brave => format!("https://search.brave.com/search?q={q}"),
        }
    }

    pub fn suggest_url(&self, query: &str) -> Option<String> {
        let q = urlencode(query);
        match self {
            Self::DuckDuckGo => Some(format!("https://duckduckgo.com/ac/?q={q}&type=list")),
            Self::Google => Some(format!(
                "https://suggestqueries.google.com/complete/search?client=firefox&q={q}"
            )),
            Self::Bing => Some(format!("https://api.bing.com/osjson.aspx?query={q}")),
            Self::Brave => Some(format!(
                "https://search.brave.com/api/suggest?q={q}&rich=false"
            )),
        }
    }
}

/// Which community filter lists are subscribed (EasyList/EasyPrivacy by
/// default; all bundled as offline seeds so first launch is fully protected).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct FilterLists {
    pub easylist: bool,
    pub easyprivacy: bool,
    pub fanboy_annoyance: bool,
    pub peter_lowe: bool,
}

impl Default for FilterLists {
    fn default() -> Self {
        Self {
            easylist: true,
            easyprivacy: true,
            fanboy_annoyance: true,
            peter_lowe: true,
        }
    }
}

impl Settings {
    /// Load settings.json from the app data dir, or defaults.
    pub fn load_from(dir: &std::path::Path) -> Settings {
        let path = dir.join("settings.json");
        match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Settings::default(),
        }
    }

    /// Save settings.json to the app data dir.
    pub fn save_to(&self, dir: &std::path::Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            std::fs::write(dir.join("settings.json"), json).ok();
        }
    }

    /// Apply a partial JSON patch (deep-merge objects, replace everything else).
    pub fn apply_patch(&mut self, patch: &Value) {
        let mut v = serde_json::to_value(&*self).unwrap_or(Value::Null);
        merge_json(&mut v, patch);
        if let Ok(s) = serde_json::from_value::<Settings>(v) {
            *self = s;
        }
    }
}

fn merge_json(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                merge_json(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (t, p) => *t = p.clone(),
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_private() {
        let s = Settings::default();
        assert!(s.adblock_enabled);
        assert_eq!(s.cookie_policy, CookiePolicy::BlockThirdParty);
        assert!(s.fingerprint.enabled && s.fingerprint.canvas);
        assert!(s.https_only);
        assert!(s.safe_browsing);
        assert_eq!(s.theme, Theme::System);
    }

    #[test]
    fn patches_apply_deeply() {
        let mut s = Settings::default();
        let patch: Value =
            serde_json::from_str(r#"{"fingerprint": {"canvas": false}, "theme": "dark"}"#)
                .unwrap();
        s.apply_patch(&patch);
        assert!(!s.fingerprint.canvas);
        assert!(s.fingerprint.enabled);
        assert_eq!(s.theme, Theme::Dark);
        // untouched field
        assert!(s.adblock_enabled);
    }

    #[test]
    fn search_urls() {
        assert_eq!(
            SearchEngineId::DuckDuckGo.search_url("rust browser"),
            "https://duckduckgo.com/?q=rust+browser"
        );
        assert!(SearchEngineId::Google
            .search_url("a&b")
            .contains("a%26b"));
    }
}
