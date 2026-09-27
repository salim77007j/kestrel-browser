//! Download records (actual transfers run in the shell layer).

use crate::store::JsonStore;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadState {
    Active,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: u64,
    pub url: String,
    pub path: String,
    pub filename: String,
    pub total_bytes: u64,
    pub received_bytes: u64,
    pub state: DownloadState,
    pub started_at: u64,
    pub finished_at: Option<u64>,
}

pub struct DownloadsStore {
    store: JsonStore,
    items: Vec<DownloadRecord>,
    next_id: u64,
}

impl DownloadsStore {
    pub fn new(store: JsonStore) -> Self {
        let items: Vec<DownloadRecord> = store.load();
        let next_id = items.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        Self {
            store,
            items,
            next_id,
        }
    }

    pub fn begin(&mut self, url: &str, path: &str, now_ms: u64) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let filename = path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("download")
            .to_string();
        self.items.insert(
            0,
            DownloadRecord {
                id,
                url: url.to_string(),
                path: path.to_string(),
                filename,
                total_bytes: 0,
                received_bytes: 0,
                state: DownloadState::Active,
                started_at: now_ms,
                finished_at: None,
            },
        );
        self.persist();
        id
    }

    pub fn progress(&mut self, id: u64, received: u64, total: u64) {
        if let Some(d) = self.items.iter_mut().find(|d| d.id == id) {
            d.received_bytes = received;
            if total > 0 {
                d.total_bytes = total;
            }
        }
        self.persist();
    }

    pub fn finish(&mut self, id: u64, success: bool, final_size: u64) {
        if let Some(d) = self.items.iter_mut().find(|d| d.id == id) {
            d.state = if success {
                DownloadState::Completed
            } else {
                DownloadState::Failed
            };
            d.finished_at = Some(d.started_at + 1); // shell overrides with real time
            if final_size > 0 {
                d.received_bytes = final_size;
                d.total_bytes = final_size;
            }
        }
        self.persist();
    }

    pub fn cancel(&mut self, id: u64) {
        if let Some(d) = self.items.iter_mut().find(|d| d.id == id) {
            d.state = DownloadState::Cancelled;
        }
        self.persist();
    }

    pub fn list(&self) -> &[DownloadRecord] {
        &self.items
    }

    pub fn get(&self, id: u64) -> Option<&DownloadRecord> {
        self.items.iter().find(|d| d.id == id)
    }

    pub fn clear_finished(&mut self) {
        self.items
            .retain(|d| matches!(d.state, DownloadState::Active));
        self.persist();
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
    fn download_lifecycle() {
        let dir = std::env::temp_dir().join(format!("kestrel-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut d = DownloadsStore::new(JsonStore::new(dir.join("dl.json")));
        let id = d.begin("https://x.test/f/setup.exe", "C:/Users/a/setup.exe", 100);
        d.progress(id, 500, 1000);
        assert_eq!(d.get(id).unwrap().state, DownloadState::Active);
        assert_eq!(d.get(id).unwrap().filename, "setup.exe");
        d.finish(id, true, 1000);
        assert_eq!(d.get(id).unwrap().state, DownloadState::Completed);
        assert_eq!(d.get(id).unwrap().total_bytes, 1000);
        d.clear_finished();
        assert!(d.list().is_empty());
    }
}
