//! Crash-safe session journal.
//!
//! A `session.json` file records open tabs. A `session.clean` marker file is
//! written on orderly shutdown and removed on launch; if tabs exist without
//! the marker we know the previous session crashed (or was killed) and can
//! tell the user their session was recovered.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabState {
    pub uri: String,
    pub title: String,
    pub pinned: bool,
    pub muted: bool,
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub active: usize,
    pub tabs: Vec<TabState>,
}

fn session_file() -> PathBuf {
    crate::dirs::base_data().join("session.json")
}

fn clean_marker() -> PathBuf {
    crate::dirs::base_data().join("session.clean")
}

pub fn save(state: &SessionState) -> Result<(), String> {
    let path = session_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string(state).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load() -> Option<SessionState> {
    let text = std::fs::read_to_string(session_file()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn mark_clean() {
    let _ = std::fs::write(clean_marker(), b"ok");
}

/// true when the previous run did NOT shut down cleanly but had open tabs.
pub fn previous_run_crashed() -> bool {
    !clean_marker().exists() && session_file().exists()
}

pub fn clear_marker() {
    let _ = std::fs::remove_file(clean_marker());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let state = SessionState {
            active: 0,
            tabs: vec![TabState {
                uri: "https://example.com".into(),
                title: "Example".into(),
                pinned: false,
                muted: true,
                private: false,
            }],
        };
        let text = serde_json::to_string(&state).unwrap();
        let back: SessionState = serde_json::from_str(&text).unwrap();
        assert_eq!(back.tabs.len(), 1);
        assert!(back.tabs[0].muted);
    }
}
