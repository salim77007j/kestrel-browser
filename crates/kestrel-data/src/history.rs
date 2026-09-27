//! History store.

use rusqlite::{params, Connection};
use std::time::{SystemTime, UNIX_EPOCH};

fn day_start() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    secs - (secs % 86400)
}

pub fn add(conn: &Connection, url: &str, title: &str) {
    let now = super::db::now();
    let _ = conn.execute(
        "INSERT INTO history (url, title, visited_at) VALUES (?1, ?2, ?3)",
        params![url, title, now],
    );
}

pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visited_at: i64,
}

pub fn search(conn: &Connection, query: &str, limit: i64) -> Vec<HistoryEntry> {
    let pattern = format!("%{}%", query.replace('%', ""));
    let mut stmt = match conn.prepare_cached(
        "SELECT id, url, COALESCE(title,''), visited_at FROM history
         WHERE url LIKE ?1 OR title LIKE ?1 ORDER BY visited_at DESC LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map(params![pattern, limit], |r| {
        Ok(HistoryEntry {
            id: r.get(0)?,
            url: r.get(1)?,
            title: r.get(2)?,
            visited_at: r.get(3)?,
        })
    });
    match rows {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    }
}

/// Top sites (most visited unique hosts+paths) for the new-tab speed dial.
pub fn top_sites(conn: &Connection, limit: i64) -> Vec<HistoryEntry> {
    let mut stmt = match conn.prepare_cached(
        "SELECT MIN(id), url, MAX(COALESCE(title,'')), MAX(visited_at) FROM history
         WHERE url LIKE 'http%' GROUP BY url ORDER BY COUNT(*) DESC, MAX(visited_at) DESC LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map(params![limit], |r| {
        Ok(HistoryEntry {
            id: r.get(0)?,
            url: r.get(1)?,
            title: r.get(2)?,
            visited_at: r.get(3)?,
        })
    });
    match rows {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    }
}

pub fn remove(conn: &Connection, id: i64) {
    let _ = conn.execute("DELETE FROM history WHERE id = ?1", params![id]);
}

pub fn clear_all(conn: &Connection) {
    let _ = conn.execute("DELETE FROM history", []);
}

pub fn clear_today(conn: &Connection) {
    let _ = conn.execute("DELETE FROM history WHERE visited_at >= ?1", params![day_start()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_search_clear() {
        let dir = std::env::temp_dir().join(format!("kestrel-hist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = super::super::db::open(&dir.join("t.db")).unwrap();
        add(&conn, "https://example.com/a", "Example A");
        add(&conn, "https://example.com/b", "Example B");
        add(&conn, "https://other.org/", "Other");
        let hits = search(&conn, "example", 10);
        assert_eq!(hits.len(), 2);
        assert_eq!(top_sites(&conn, 2).len(), 2);
        assert_eq!(top_sites(&conn, 10).len(), 3);
        clear_all(&conn);
        assert!(search(&conn, "", 10).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
