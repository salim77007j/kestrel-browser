//! Session persistence + URL normalization heuristics.

use kestrel_data::session::{SessionState, TabState};
use kestrel_privacy::shield::hostname;

pub fn normalize_uri(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return "kestrel://newtab".to_string();
    }
    if t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("kestrel://")
        || t.starts_with("file://")
        || t.starts_with("about:")
    {
        return t.to_string();
    }
    // "localhost:8080" -> host
    if let Some(rest) = t.strip_prefix("localhost") {
        if rest.starts_with(':') || rest.is_empty() {
            return format!("http://{t}");
        }
    }
    // Domain-like: has a dot + no spaces, or IPv6-ish
    let domain_like = (t.contains('.') && !t.contains(' ')) || t.starts_with('[');
    if domain_like {
        format!("https://{t}")
    } else {
        // Fallback search (address bar uses the configured engine instead).
        format!("https://duckduckgo.com/?q={}", kestrel_data::settings::url_encode(t))
    }
}

pub fn save_session(state: &std::rc::Rc<crate::state::AppState>) {
    let Some(w) = state.main_window() else { return };
    let tabs = w.tab_states();
    let s = SessionState { active: w.active_index(), tabs };
    let _ = kestrel_data::session::save(&s);
}

pub fn restore_session(state: &std::rc::Rc<crate::state::AppState>) -> Option<(bool, SessionState)> {
    let crashed = kestrel_data::session::previous_run_crashed();
    let st = kestrel_data::session::load()?;
    if st.tabs.is_empty() {
        return None;
    }
    if !state.settings.borrow().restore_session {
        return None;
    }
    Some((crashed, st))
}

pub fn tab_state_of(uri: &str, title: &str, pinned: bool, muted: bool, private: bool) -> TabState {
    TabState {
        uri: uri.to_string(),
        title: title.to_string(),
        pinned,
        muted,
        private,
    }
}

/// Hostname of a tab URI, if any (used for vault match).
pub fn host_of(uri: &str) -> Option<String> {
    hostname(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(normalize_uri("https://a.io/x"), "https://a.io/x");
        assert_eq!(normalize_uri("a.io"), "https://a.io");
        assert_eq!(normalize_uri("localhost:8080"), "http://localhost:8080");
        assert_eq!(normalize_uri("kestrel://settings"), "kestrel://settings");
        // search fallback — hits settings default
        let q = normalize_uri("rust programming");
        assert!(q.starts_with("https://duckduckgo.com") || q.contains("search"));
    }
}
