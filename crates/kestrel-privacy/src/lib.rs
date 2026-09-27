//! Kestrel privacy core.
//!
//! Pure-Rust, UI-agnostic: the same engine code is unit-tested locally and
//! compiled into the Tauri shell on every platform.

pub mod engine;
pub mod fingerprint;
pub mod headers;
pub mod safebrowsing;

pub use engine::PrivacyEngine;
pub use safebrowsing::{SafeBrowsing, Threat};
