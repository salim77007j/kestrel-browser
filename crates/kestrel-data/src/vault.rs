//! Password vault.
//!
//! Design: credentials are stored in the OS secret service (libsecret /
//! GNOME Keyring / KDE Wallet) — the OS encrypts at rest, we never write
//! plaintext secrets to disk. Only a non-secret index (origin + username)
//! lives in SQLite so the settings UI can list entries.
//!
//! If no secret service is available the vault is *disabled* (never fallback
//! to plaintext). Building without the `vault` feature compiles a stub that
//! reports the same, keeping the API stable.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub origin: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct VaultEntry {
    pub origin: String,
    pub username: String,
}

#[cfg(feature = "vault")]
mod backend {
    use super::*;
    use rusqlite::params;

    const SERVICE: &str = "io.kestrel.Browser";

    fn entry(origin: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, &format!("pass:{origin}")).map_err(|e| e.to_string())
    }

    pub fn save(conn: &Connection, cred: &Credential) -> Result<(), String> {
        let e = entry(&cred.origin)?;
        let payload = serde_json::to_string(cred).map_err(|e| e.to_string())?;
        e.set_password(&payload).map_err(|e| e.to_string())?;
        let _ = conn.execute(
            "INSERT INTO vault_index (origin, username, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(origin) DO UPDATE SET username=?2, updated_at=?3",
            params![cred.origin, cred.username, super::super::db::now()],
        );
        Ok(())
    }

    pub fn get(conn: &Connection, origin: &str) -> Result<Option<Credential>, String> {
        if conn
            .query_row("SELECT 1 FROM vault_index WHERE origin=?1", params![origin], |_| Ok(()))
            .is_err()
        {
            return Ok(None);
        }
        let e = entry(origin)?;
        match e.get_password() {
            Ok(payload) => serde_json::from_str(&payload).map(Some).map_err(|e| e.to_string()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn delete(conn: &Connection, origin: &str) -> Result<(), String> {
        if let Ok(e) = entry(origin) {
            let _ = e.delete_credential();
        }
        let _ = conn.execute("DELETE FROM vault_index WHERE origin=?1", params![origin]);
        Ok(())
    }

    pub fn list(conn: &Connection) -> Vec<VaultEntry> {
        let mut stmt = match conn.prepare_cached("SELECT origin, username FROM vault_index ORDER BY origin") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt.query_map([], |r| {
            Ok(VaultEntry { origin: r.get(0)?, username: r.get(1)? })
        });
        match rows {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => vec![],
        }
    }
}

#[cfg(not(feature = "vault"))]
mod backend {
    use super::*;
    use rusqlite::params;

    const DISABLED: &str = "vault disabled: built without keyring support";

    pub fn save(_conn: &Connection, _cred: &Credential) -> Result<(), String> {
        Err(DISABLED.into())
    }
    pub fn get(_conn: &Connection, _origin: &str) -> Result<Option<Credential>, String> {
        Ok(None)
    }
    pub fn delete(conn: &Connection, origin: &str) -> Result<(), String> {
        let _ = conn.execute("DELETE FROM vault_index WHERE origin=?1", params![origin]);
        Ok(())
    }
    pub fn list(_conn: &Connection) -> Vec<VaultEntry> {
        vec![]
    }
}

pub use backend::{delete, get, list, save};
