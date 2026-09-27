//! Bookmarks store.

use rusqlite::{params, Connection};

pub struct Bookmark {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub folder: String,
}

pub fn add(conn: &Connection, url: &str, title: &str, folder: &str) -> bool {
    conn.execute(
        "INSERT OR IGNORE INTO bookmarks (url, title, folder, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![url, title, folder, super::db::now()],
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

pub fn remove(conn: &Connection, url: &str) {
    let _ = conn.execute("DELETE FROM bookmarks WHERE url = ?1", params![url]);
}

pub fn has(conn: &Connection, url: &str) -> bool {
    conn.query_row("SELECT 1 FROM bookmarks WHERE url = ?1", params![url], |_| Ok(()))
        .is_ok()
}

pub fn list(conn: &Connection) -> Vec<Bookmark> {
    let mut stmt = match conn
        .prepare_cached("SELECT id, url, COALESCE(title,''), COALESCE(folder,'') FROM bookmarks ORDER BY created_at DESC")
    {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([], |r| {
        Ok(Bookmark { id: r.get(0)?, url: r.get(1)?, title: r.get(2)?, folder: r.get(3)? })
    });
    match rows {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    }
}

pub fn search(conn: &Connection, q: &str, limit: i64) -> Vec<Bookmark> {
    let pattern = format!("%{}%", q.replace('%', ""));
    let mut stmt = match conn.prepare_cached(
        "SELECT id, url, COALESCE(title,''), COALESCE(folder,'') FROM bookmarks
         WHERE url LIKE ?1 OR title LIKE ?1 ORDER BY created_at DESC LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map(params![pattern, limit], |r| {
        Ok(Bookmark { id: r.get(0)?, url: r.get(1)?, title: r.get(2)?, folder: r.get(3)? })
    });
    match rows {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_has_remove() {
        let dir = std::env::temp_dir().join(format!("kestrel-bm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = super::super::db::open(&dir.join("t.db")).unwrap();
        assert!(add(&conn, "https://rust-lang.org", "Rust", ""));
        assert!(!add(&conn, "https://rust-lang.org", "Rust dup", ""));
        assert!(has(&conn, "https://rust-lang.org"));
        assert_eq!(list(&conn).len(), 1);
        remove(&conn, "https://rust-lang.org");
        assert!(!has(&conn, "https://rust-lang.org"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
