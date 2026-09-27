//! Kestrel data layer — persistence for everything the browser remembers.

pub mod bookmarks;
pub mod db;
pub mod dirs;
pub mod downloads;
pub mod history;
pub mod permissions;
pub mod session;
pub mod settings;
pub mod vault;

pub use settings::Settings;
