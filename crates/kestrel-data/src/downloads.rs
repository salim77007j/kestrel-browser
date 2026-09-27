//! Downloads store.

use rusqlite::{params, Connection};

pub struct DownloadRow {
    pub id: i64,
    pub uri: String,
    pub destination: String,
    pub filename: String,
    pub state: String,
    pub received_bytes: i64,
    pub total_bytes: i64,
    pub started_at: i64,
}

pub fn insert(conn: &Connection, uri: &str, filename: &str) -> i64 {
    let _ = conn.execute(
        "INSERT INTO downloads (uri, filename, state, started_at) VALUES (?1, ?2, 'active', ?3)",
        params![uri, filename, super::db::now()],
    );
    conn.last_insert_rowid()
}

pub fn update(
    conn: &Connection,
    id: i64,
    destination: &str,
    state: &str,
    received: i64,
    total: i64,
) {
    let finished = if state == "done" || state == "failed" || state == "cancelled" {
        Some(super::db::now())
    } else {
        None
    };
    let _ = conn.execute(
        "UPDATE downloads SET destination=?2, state=?3, received_bytes=?4, total_bytes=?5, finished_at=?6 WHERE id=?1",
        params![id, destination, state, received, total, finished],
    );
}

pub fn list(conn: &Connection, limit: i64) -> Vec<DownloadRow> {
    let mut stmt = match conn.prepare_cached(
        "SELECT id, uri, COALESCE(destination,''), COALESCE(filename,''), state,
                received_bytes, total_bytes, started_at
         FROM downloads ORDER BY started_at DESC LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map(params![limit], |r| {
        Ok(DownloadRow {
            id: r.get(0)?,
            uri: r.get(1)?,
            destination: r.get(2)?,
            filename: r.get(3)?,
            state: r.get(4)?,
            received_bytes: r.get(5)?,
            total_bytes: r.get(6)?,
            started_at: r.get(7)?,
        })
    });
    match rows {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    }
}

pub fn clear(conn: &Connection) {
    let _ = conn.execute("DELETE FROM downloads WHERE state != 'active'", []);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_update_list() {
        let dir = std::env::temp_dir().join(format!("kestrel-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = super::super::db::open(&dir.join("t.db")).unwrap();
        let id = insert(&conn, "https://a.io/f.zip", "f.zip");
        update(&conn, id, "/tmp/f.zip", "done", 100, 100);
        let rows = list(&conn, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, "done");
        clear(&conn);
        assert!(list(&conn, 10).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
