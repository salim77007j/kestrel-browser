//! Per-origin permission store: key is "origin|kind".

use rusqlite::{params, Connection};

pub fn get(conn: &Connection, origin: &str, kind: &str) -> Option<bool> {
    let key = format!("{origin}|{kind}");
    conn.query_row(
        "SELECT granted FROM permissions WHERE key = ?1",
        params![key],
        |r| r.get::<_, i64>(0),
    )
    .ok()
    .map(|v| v != 0)
}

pub fn set(conn: &Connection, origin: &str, kind: &str, granted: bool) {
    let key = format!("{origin}|{kind}");
    let _ = conn.execute(
        "INSERT INTO permissions (key, granted, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET granted=?2, updated_at=?3",
        params![key, granted as i64, super::db::now()],
    );
}

pub fn clear(conn: &Connection) {
    let _ = conn.execute("DELETE FROM permissions", []);
}
