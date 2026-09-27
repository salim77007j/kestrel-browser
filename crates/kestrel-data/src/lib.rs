//! Kestrel data layer.
//!
//! JSON-file-backed stores with atomic writes. Pure Rust: paths are
//! injected by the shell so the crate has no platform dependencies.

pub mod bookmarks;
pub mod downloads;
pub mod history;
pub mod permissions;
pub mod session;
pub mod settings;
pub mod shortcuts;
pub mod store;

pub use bookmarks::{BookmarkNode, BookmarksStore};
pub use downloads::{DownloadRecord, DownloadState, DownloadsStore};
pub use history::{HistoryEntry, HistoryStore};
pub use permissions::{PermissionDecision, PermissionStore};
pub use session::{ClosedTab, SessionState, TabState};
pub use settings::Settings;
pub use shortcuts::ShortcutsStore;
pub use store::JsonStore;
