//! New-tab page shortcuts (the quick tiles on the start page).

use crate::store::JsonStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Shortcut {
    pub title: String,
    pub url: String,
}

pub struct ShortcutsStore {
    store: JsonStore,
    items: Vec<Shortcut>,
}

impl Default for &ShortcutsStore {
    fn default() -> Self {
        unreachable!()
    }
}

impl ShortcutsStore {
    pub fn new(store: JsonStore) -> Self {
        let items: Vec<Shortcut> = store.load_or(Self::defaults);
        Self { store, items }
    }

    pub fn defaults() -> Vec<Shortcut> {
        vec![
            Shortcut {
                title: "Google".into(),
                url: "https://www.google.com/".into(),
            },
            Shortcut {
                title: "YouTube".into(),
                url: "https://www.youtube.com/".into(),
            },
            Shortcut {
                title: "Gmail".into(),
                url: "https://mail.google.com/".into(),
            },
            Shortcut {
                title: "Drive".into(),
                url: "https://drive.google.com/".into(),
            },
        ]
    }

    pub fn list(&self) -> &[Shortcut] {
        &self.items
    }

    pub fn add(&mut self, title: &str, url: &str) -> bool {
        if self.items.iter().any(|s| s.url == url) {
            return false;
        }
        self.items.push(Shortcut {
            title: title.to_string(),
            url: url.to_string(),
        });
        self.persist();
        true
    }

    pub fn remove(&mut self, url: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|s| s.url != url);
        let changed = self.items.len() != before;
        if changed {
            self.persist();
        }
        changed
    }

    fn persist(&self) {
        self.store.save(&self.items).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::JsonStore;

    #[test]
    fn defaults_and_editing() {
        let dir = std::env::temp_dir().join(format!("kestrel-sc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = ShortcutsStore::new(JsonStore::new(dir.join("shortcuts.json")));
        assert_eq!(s.list().len(), 4);
        assert_eq!(s.list()[0].title, "Google");
        assert!(s.add("Rust", "https://rust-lang.org"));
        assert!(!s.add("Dup", "https://rust-lang.org"));
        assert!(s.remove("https://rust-lang.org"));
        assert_eq!(s.list().len(), 4);
    }
}
