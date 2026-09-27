//! Session persistence: open tabs, closed-tab stack for Ctrl+Shift+T,
//! crash-safe restore.

use crate::store::JsonStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabState {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub muted: bool,
    pub group: Option<GroupInfo>,
    pub zoom: f64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub color: String,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub tabs: Vec<TabState>,
    pub closed: Vec<ClosedTab>,
    /// false while the app is running; set true on clean exit. If false at
    /// startup, the last session ended unexpectedly and gets restored.
    #[serde(default)]
    pub clean_exit: bool,
}

impl SessionState {
    pub fn load(store: &JsonStore) -> Self {
        store.load_or(Self::default)
    }

    pub fn save(&self, store: &JsonStore) {
        store.save(self).ok();
    }

    pub fn crashed_last_time(&self) -> bool {
        !self.clean_exit
    }

    pub fn push_closed(&mut self, url: String, title: String) {
        self.closed.insert(0, ClosedTab { url, title });
        if self.closed.len() > 25 {
            self.closed.truncate(25);
        }
    }

    pub fn pop_closed(&mut self) -> Option<ClosedTab> {
        if self.closed.is_empty() {
            None
        } else {
            Some(self.closed.remove(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::JsonStore;

    #[test]
    fn closed_stack_and_crash_flag() {
        let dir = std::env::temp_dir().join(format!("kestrel-sess-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = JsonStore::new(dir.join("session.json"));

        let mut s = SessionState::load(&store);
        assert!(s.crashed_last_time()); // never exited cleanly
        s.push_closed("https://a.test".into(), "A".into());
        s.push_closed("https://b.test".into(), "B".into());
        s.tabs.push(TabState {
            url: "https://c.test".into(),
            title: "C".into(),
            pinned: false,
            muted: false,
            group: Some(GroupInfo {
                id: "g1".into(),
                name: "Work".into(),
                color: "blue".into(),
                collapsed: false,
            }),
            zoom: 1.25,
            active: true,
        });
        s.clean_exit = true;
        s.save(&store);

        let mut s2 = SessionState::load(&store);
        assert!(!s2.crashed_last_time());
        assert_eq!(s2.pop_closed().unwrap().url, "https://b.test");
        assert_eq!(s2.tabs[0].group.as_ref().unwrap().name, "Work");
        assert_eq!(s2.tabs[0].zoom, 1.25);
    }
}
