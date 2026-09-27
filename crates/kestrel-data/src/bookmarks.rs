//! Bookmarks: a bookmarks-bar list and an "Other bookmarks" folder.

use crate::store::JsonStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookmarkNode {
    pub id: u64,
    pub title: String,
    pub url: String,
}

pub struct BookmarksStore {
    store: JsonStore,
    bar: Vec<BookmarkNode>,
    other: Vec<BookmarkNode>,
    next_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarksFile {
    pub bar: Vec<BookmarkNode>,
    pub other: Vec<BookmarkNode>,
    pub next_id: u64,
}

impl Default for BookmarksFile {
    fn default() -> Self {
        // The bookmarks bar from the wed1 design: Apps shortcut is rendered
        // by the UI itself; the default sites live here.
        Self {
            bar: vec![
                node(1, "Google", "https://www.google.com/"),
                node(2, "YouTube", "https://www.youtube.com/"),
                node(3, "Gmail", "https://mail.google.com/"),
                node(4, "Maps", "https://maps.google.com/"),
                node(5, "Drive", "https://drive.google.com/"),
            ],
            other: Vec::new(),
            next_id: 6,
        }
    }
}

fn node(id: u64, title: &str, url: &str) -> BookmarkNode {
    BookmarkNode {
        id,
        title: title.to_string(),
        url: url.to_string(),
    }
}

impl BookmarksStore {
    pub fn new(store: JsonStore) -> Self {
        let file: BookmarksFile = store.load();
        let next_id = file.next_id.max(1);
        Self {
            store,
            bar: file.bar,
            other: file.other,
            next_id,
        }
    }

    pub fn bar(&self) -> &[BookmarkNode] {
        &self.bar
    }

    pub fn other(&self) -> &[BookmarkNode] {
        &self.other
    }

    pub fn toggle(&mut self, title: &str, url: &str) -> bool {
        // returns true when the bookmark was added
        if let Some(pos) = self.bar.iter().position(|b| b.url == url) {
            self.bar.remove(pos);
            self.persist();
            return false;
        }
        if let Some(pos) = self.other.iter().position(|b| b.url == url) {
            self.other.remove(pos);
            self.persist();
            return false;
        }
        self.add(title, url, false)
    }

    pub fn add(&mut self, title: &str, url: &str, to_other: bool) -> bool {
        let b = node(self.next_id, title, url);
        self.next_id += 1;
        if to_other {
            self.other.push(b);
        } else {
            self.bar.push(b);
        }
        self.persist();
        true
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.bar.len() + self.other.len();
        self.bar.retain(|b| b.id != id);
        self.other.retain(|b| b.id != id);
        let changed = self.bar.len() + self.other.len() != before;
        if changed {
            self.persist();
        }
        changed
    }

    pub fn rename(&mut self, id: u64, title: &str) {
        for b in self.bar.iter_mut().chain(self.other.iter_mut()) {
            if b.id == id {
                b.title = title.to_string();
            }
        }
        self.persist();
    }

    pub fn move_to_bar(&mut self, id: u64) {
        if let Some(pos) = self.other.iter().position(|b| b.id == id) {
            let b = self.other.remove(pos);
            self.bar.push(b);
            self.persist();
        }
    }

    pub fn reorder(&mut self, ids: &[u64]) {
        let mut new_bar = Vec::with_capacity(self.bar.len());
        for id in ids {
            if let Some(pos) = self.bar.iter().position(|b| b.id == *id) {
                new_bar.push(self.bar.remove(pos));
            }
        }
        new_bar.extend(self.bar.drain(..));
        self.bar = new_bar;
        self.persist();
    }

    pub fn find_by_url(&self, url: &str) -> Option<&BookmarkNode> {
        self.bar
            .iter()
            .chain(self.other.iter())
            .find(|b| b.url == url)
    }

    fn persist(&self) {
        self.store
            .save(&BookmarksFile {
                bar: self.bar.clone(),
                other: self.other.clone(),
                next_id: self.next_id,
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::JsonStore;

    fn tmp(tag: &str) -> BookmarksStore {
        let dir = std::env::temp_dir().join(format!("kestrel-bm-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        BookmarksStore::new(JsonStore::new(dir.join("bookmarks.json")))
    }

    #[test]
    fn defaults_match_design_bar() {
        let b = tmp("1");
        let titles: Vec<&str> = b.bar().iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["Google", "YouTube", "Gmail", "Maps", "Drive"]);
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut b = tmp("2");
        assert!(b.toggle("Rust", "https://rust-lang.org"));
        assert!(b.find_by_url("https://rust-lang.org").is_some());
        assert!(!b.toggle("Rust", "https://rust-lang.org"));
        assert!(b.find_by_url("https://rust-lang.org").is_none());
    }

    #[test]
    fn remove_and_move() {
        let mut b = tmp("3");
        b.add("X", "https://x.test", true);
        let id = b.other()[0].id;
        b.move_to_bar(id);
        assert!(b.other().is_empty());
        assert_eq!(b.bar().last().unwrap().title, "X");
        assert!(b.remove(id));
    }
}
