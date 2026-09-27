//! Per-origin permission decisions remembered across sessions
//! (camera, microphone, geolocation, notifications, clipboard, midi, ...).

use crate::store::JsonStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Deny,
}

pub type PermissionMap = HashMap<String, PermissionDecision>;

pub struct PermissionStore {
    store: JsonStore,
    /// origin ("https://example.com") -> kind ("camera") -> decision
    map: HashMap<String, PermissionMap>,
}

impl PermissionStore {
    pub fn new(store: JsonStore) -> Self {
        Self {
            map: store.load(),
            store,
        }
    }

    pub fn get(&self, origin: &str, kind: &str) -> Option<PermissionDecision> {
        self.map.get(origin)?.get(kind).copied()
    }

    pub fn set(&mut self, origin: &str, kind: &str, decision: PermissionDecision) {
        self.map
            .entry(origin.to_string())
            .or_default()
            .insert(kind.to_string(), decision);
        self.persist();
    }

    pub fn clear_origin(&mut self, origin: &str) -> bool {
        let removed = self.map.remove(origin).is_some();
        if removed {
            self.persist();
        }
        removed
    }

    pub fn clear_all(&mut self) {
        self.map.clear();
        self.persist();
    }

    /// Snapshot for the settings page permissions manager.
    pub fn snapshot(&self) -> &HashMap<String, PermissionMap> {
        &self.map
    }

    fn persist(&self) {
        self.store.save(&self.map).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::JsonStore;

    #[test]
    fn set_get_clear() {
        let dir = std::env::temp_dir().join(format!("kestrel-perm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut p = PermissionStore::new(JsonStore::new(dir.join("perm.json")));
        p.set("https://meet.test", "camera", PermissionDecision::Allow);
        p.set("https://meet.test", "mic", PermissionDecision::Deny);
        assert_eq!(
            p.get("https://meet.test", "camera"),
            Some(PermissionDecision::Allow)
        );
        assert_eq!(p.get("https://other.test", "camera"), None);
        assert_eq!(p.snapshot().len(), 1);
        assert!(p.clear_origin("https://meet.test"));
        assert!(p.snapshot().is_empty());
    }
}
