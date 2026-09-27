//! SQLite handle: WAL mode, schema migrations in one place.

use rusqlite::Connection;
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "synchronous", "NORMAL").ok();
    conn.pragma_update(None, "foreign_keys", "ON").ok();
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            visited_at INTEGER NOT NULL,
            visit_count INTEGER NOT NULL DEFAULT 1
        );
        CREATE INDEX IF NOT EXISTS idx_history_url ON history(url);
        CREATE INDEX IF NOT EXISTS idx_history_time ON history(visited_at DESC);

        CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL DEFAULT '',
            folder TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS downloads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            uri TEXT NOT NULL,
            destination TEXT NOT NULL DEFAULT '',
            filename TEXT NOT NULL DEFAULT '',
            state TEXT NOT NULL DEFAULT 'active',  -- active|done|failed|cancelled
            received_bytes INTEGER NOT NULL DEFAULT 0,
            total_bytes INTEGER NOT NULL DEFAULT 0,
            started_at INTEGER NOT NULL,
            finished_at INTEGER DEFAULT NULL
        );

        CREATE TABLE IF NOT EXISTS vault_index (
            origin TEXT PRIMARY KEY,
            username TEXT NOT NULL DEFAULT '',
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS permissions (
            key TEXT PRIMARY KEY,
            granted INTEGER NOT NULL DEFAULT 0,
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .map_err(|e| e.to_string())
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_migrates() {
        let dir = std::env::temp_dir().join(format!("kestrel-db-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = open(&dir.join("t.db")).unwrap();
        conn.execute("INSERT INTO bookmarks (url, title, created_at) VALUES ('https://a.io','A',1)", [])
            .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
