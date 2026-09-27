//! Generic JSON file store with atomic writes (tmp + rename).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load<T: serde::de::DeserializeOwned + Default>(&self) -> T {
        self.load_or(T::default)
    }

    pub fn load_or<T: serde::de::DeserializeOwned>(&self, default: impl FnOnce() -> T) -> T {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| default()),
            Err(_) => default(),
        }
    }

    /// Atomic write: write to `<path>.tmp` then rename over the target.
    pub fn save<T: serde::Serialize>(&self, value: &T) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            let bytes = serde_json::to_vec_pretty(value).unwrap_or_else(|_| b"{}".to_vec());
            f.write_all(&bytes)?;
            f.sync_all().ok();
        }
        fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_save_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kestrel-test-{}", std::process::id()));
        let store = JsonStore::new(dir.join("t.json"));
        store.save(&serde_json::json!({"a": 1, "b": "x"})).unwrap();
        let v: serde_json::Value = store.load();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], "x");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn load_missing_file_gives_default() {
        let store = JsonStore::new("/nonexistent/kestrel/x.json");
        let v: serde_json::Value = store.load_or(|| serde_json::json!({"def": true}));
        assert_eq!(v["def"], true);
    }
}
