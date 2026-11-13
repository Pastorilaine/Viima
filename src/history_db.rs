//! Browsing history in SQLite: one row per URL, FTS5 full-text search, no size cap.
use crate::storage::{HistoryItem, is_internal_url};
use rusqlite::{Connection, OpenFlags, params};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    visited_at INTEGER NOT NULL,
    visit_count INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_history_visited ON history(visited_at DESC);
CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(
    title, url, content='history', content_rowid='id', tokenize='unicode61 remove_diacritics 2'
);
CREATE TRIGGER IF NOT EXISTS history_ai AFTER INSERT ON history BEGIN
    INSERT INTO history_fts(rowid, title, url) VALUES (new.id, new.title, new.url);
END;
CREATE TRIGGER IF NOT EXISTS history_ad AFTER DELETE ON history BEGIN
    INSERT INTO history_fts(history_fts, rowid, title, url) VALUES ('delete', old.id, old.title, old.url);
END;
CREATE TRIGGER IF NOT EXISTS history_au AFTER UPDATE OF title, url ON history BEGIN
    INSERT INTO history_fts(history_fts, rowid, title, url) VALUES ('delete', old.id, old.title, old.url);
    INSERT INTO history_fts(rowid, title, url) VALUES (new.id, new.title, new.url);
END;
";

pub struct HistoryDb {
    conn: Option<Connection>,
    read_only: bool,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Turns user text into an FTS5 prefix query: `rust lang` -> `"rust"* "lang"*`.
fn fts_query(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split_whitespace()
        .take(8)
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryItem> {
    Ok(HistoryItem {
        id: format!("hist-{}", row.get::<_, i64>(0)?),
        url: row.get(1)?,
        title: row.get(2)?,
        visited_at: row.get::<_, i64>(3)? as u64,
        visit_count: row.get::<_, i64>(4)? as u32,
    })
}

impl HistoryDb {
    /// `read_only` (private window): reads the normal history but never records anything.
    pub fn open(path: &Path, read_only: bool) -> Self {
        let conn = if read_only {
            Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
        } else {
            Connection::open(path)
                .ok()
                .filter(|c| c.execute_batch(SCHEMA).is_ok())
        };
        Self { conn, read_only }
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        Self {
            conn: Some(conn),
            read_only: false,
        }
    }

    fn writer(&self) -> Option<&Connection> {
        if self.read_only { None } else { self.conn.as_ref() }
    }

    pub fn is_available(&self) -> bool {
        self.conn.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.conn
            .as_ref()
            .and_then(|c| {
                c.query_row("SELECT NOT EXISTS(SELECT 1 FROM history)", [], |r| r.get(0))
                    .ok()
            })
            .unwrap_or(true)
    }

    /// One-time import of the old JSON history (newest first).
    pub fn import(&mut self, items: &[HistoryItem]) {
        let Some(conn) = self.conn.as_mut().filter(|_| !self.read_only) else {
            return;
        };
        let Ok(tx) = conn.transaction() else { return };
        for item in items.iter().rev() {
            let _ = tx.execute(
                "INSERT INTO history(url, title, visited_at, visit_count) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(url) DO UPDATE SET visit_count = visit_count + excluded.visit_count,
                   visited_at = MAX(visited_at, excluded.visited_at), title = excluded.title",
                params![item.url, item.title, item.visited_at as i64, item.visit_count as i64],
            );
        }
        let _ = tx.commit();
    }

    pub fn add_visit(&self, title: &str, url: &str) {
        if url.is_empty() || url == "about:blank" || is_internal_url(url) {
            return;
        }
        let Some(conn) = self.writer() else { return };
        let title = if title.trim().is_empty() { url } else { title.trim() };
        let _ = conn.execute(
            "INSERT INTO history(url, title, visited_at, visit_count) VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(url) DO UPDATE SET visit_count = visit_count + 1, visited_at = excluded.visited_at,
               title = CASE WHEN excluded.title = excluded.url THEN title ELSE excluded.title END",
            params![url, title, now()],
        );
    }

    /// Page titles usually arrive after the load-finished event that recorded the visit.
    pub fn update_title(&self, url: &str, title: &str) {
        let title = title.trim();
        let Some(conn) = self.writer() else { return };
        if title.is_empty() {
            return;
        }
        let _ = conn.execute(
            "UPDATE history SET title = ?2 WHERE url = ?1 AND title != ?2",
            params![url, title],
        );
    }

    /// Newest first; with a query, full-text matches on title and URL.
    pub fn search(&self, query: &str, limit: usize) -> Vec<HistoryItem> {
        let Some(conn) = &self.conn else {
            return Vec::new();
        };
        let result = match fts_query(query) {
            Some(q) => conn
                .prepare_cached(
                    "SELECT h.id, h.url, h.title, h.visited_at, h.visit_count FROM history_fts f
                     JOIN history h ON h.id = f.rowid WHERE history_fts MATCH ?1
                     ORDER BY h.visited_at DESC LIMIT ?2",
                )
                .and_then(|mut s| {
                    s.query_map(params![q, limit as i64], row_to_item)?
                        .collect::<rusqlite::Result<Vec<_>>>()
                }),
            None => conn
                .prepare_cached(
                    "SELECT id, url, title, visited_at, visit_count FROM history
                     ORDER BY visited_at DESC LIMIT ?1",
                )
                .and_then(|mut s| {
                    s.query_map(params![limit as i64], row_to_item)?
                        .collect::<rusqlite::Result<Vec<_>>>()
                }),
        };
        result.unwrap_or_default()
    }

    /// (url, visit_count) of the most visited URLs containing `typed`, for omnibox completion.
    pub fn completion_candidates(&self, typed: &str) -> Vec<(String, u32)> {
        let Some(conn) = &self.conn else {
            return Vec::new();
        };
        let pattern = format!(
            "%{}%",
            typed.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
        );
        conn.prepare_cached(
            "SELECT url, visit_count FROM history WHERE url LIKE ?1 ESCAPE '\\'
             ORDER BY visit_count DESC LIMIT 200",
        )
        .and_then(|mut s| {
            s.query_map(params![pattern], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default()
    }

    pub fn delete_url(&self, url: &str) {
        if let Some(conn) = self.writer() {
            let _ = conn.execute("DELETE FROM history WHERE url = ?1", params![url]);
        }
    }

    pub fn clear(&self) {
        if let Some(conn) = self.writer() {
            let _ = conn.execute_batch("DELETE FROM history; INSERT INTO history_fts(history_fts) VALUES('rebuild');");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visits_search_and_delete() {
        let db = HistoryDb::in_memory();
        db.add_visit("", "https://yle.fi/uutiset");
        db.update_title("https://yle.fi/uutiset", "Yle Uutiset");
        db.add_visit("Yle Uutiset", "https://yle.fi/uutiset");
        db.add_visit("Rust-ohjelmointikieli", "https://www.rust-lang.org/");
        db.add_visit("Uusi välilehti", "http://browser.newtab/");

        let all = db.search("", 10);
        assert_eq!(all.len(), 2, "internal pages are never recorded");
        let yle = all.iter().find(|h| h.url.contains("yle")).unwrap();
        assert_eq!(yle.visit_count, 2);
        assert_eq!(yle.title, "Yle Uutiset");

        // FTS prefix + diacritics-insensitive search
        assert_eq!(db.search("uuti", 10).len(), 1);
        assert_eq!(db.search("ohjelmointi", 10).len(), 1);
        assert_eq!(db.search("rust \"lang", 10).len(), 1, "quotes are escaped");
        assert!(db.search("nothinghere", 10).is_empty());

        assert_eq!(db.completion_candidates("yle.f")[0].1, 2);
        db.delete_url("https://yle.fi/uutiset");
        assert!(db.search("uutiset", 10).is_empty());
        db.clear();
        assert!(db.is_empty());
    }
}
