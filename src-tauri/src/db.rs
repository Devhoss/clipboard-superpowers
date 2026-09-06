use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClipboardItem {
    pub id: i64,
    pub content: String,      // text content, or absolute PNG path for kind="image"
    pub content_hash: String, // sha256 hex
    pub category: String,     // plain|link|code|color|email|image
    pub kind: String,         // text|image
    pub pinned: bool,
    pub created_at: String, // RFC3339
}

#[derive(Debug)]
pub struct InsertOutcome {
    pub id: i64,
    /// true when the row is new, false when an existing row was bumped to top
    pub inserted: bool,
}

pub fn open_db(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn open_in_memory_db() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS clipboard_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            content_hash TEXT NOT NULL UNIQUE,
            category TEXT NOT NULL,
            kind TEXT NOT NULL,
            pinned INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_created_at ON clipboard_history(created_at DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_category ON clipboard_history(category)",
        [],
    )?;
    Ok(())
}

/// Insert a new item, or bump an existing duplicate to the top of the
/// history by refreshing its timestamp. MAX_ITEMS caps unpinned history.
pub fn insert_item(conn: &Connection, item: &ClipboardItem, max_items: i64) -> Result<InsertOutcome> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM clipboard_history WHERE content_hash = ?1",
            params![item.content_hash],
            |row| row.get(0),
        )
        .optional()?;
    conn.execute(
        "INSERT INTO clipboard_history (content, content_hash, category, kind, pinned, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(content_hash) DO UPDATE SET created_at = excluded.created_at",
        params![
            item.content,
            item.content_hash,
            item.category,
            item.kind,
            item.pinned as i32,
            item.created_at
        ],
    )?;
    // last_insert_rowid() is unreliable after ON CONFLICT DO UPDATE —
    // it returns the previous insert's rowid — so reuse the queried id.
    let id = existing.unwrap_or_else(|| conn.last_insert_rowid());
    prune_old_items(conn, max_items)?;
    Ok(InsertOutcome {
        id,
        inserted: existing.is_none(),
    })
}

/// Delete oldest unpinned items beyond the cap.
fn prune_old_items(conn: &Connection, max_items: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM clipboard_history
         WHERE pinned = 0 AND id NOT IN (
             SELECT id FROM clipboard_history WHERE pinned = 0
             ORDER BY created_at DESC LIMIT ?1
         )",
        params![max_items],
    )?;
    Ok(())
}

const ITEM_COLUMNS: &str = "id, content, content_hash, category, kind, pinned, created_at";

fn row_to_item(row: &rusqlite::Row) -> Result<ClipboardItem> {
    Ok(ClipboardItem {
        id: row.get(0)?,
        content: row.get(1)?,
        content_hash: row.get(2)?,
        category: row.get(3)?,
        kind: row.get(4)?,
        pinned: row.get::<_, i32>(5)? != 0,
        created_at: row.get(6)?,
    })
}

pub fn get_all_items(conn: &Connection, limit: i64) -> Result<Vec<ClipboardItem>> {
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM clipboard_history ORDER BY pinned DESC, created_at DESC LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], row_to_item)?;
    rows.collect()
}

pub fn search_items(conn: &Connection, query: &str, limit: i64) -> Result<Vec<ClipboardItem>> {
    // Escape LIKE wildcards so searching "100%" doesn't match "100X".
    let escaped = query.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM clipboard_history
         WHERE content LIKE ?1 ESCAPE '\\' ORDER BY pinned DESC, created_at DESC LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![pattern, limit], row_to_item)?;
    rows.collect()
}

pub fn delete_item(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])
}

pub fn toggle_pin(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clipboard_history SET pinned = CASE WHEN pinned = 1 THEN 0 ELSE 1 END WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Refresh an existing row's timestamp by content hash (re-copy = most
/// recent). Returns the updated row, or None when the hash isn't stored.
pub fn touch_by_hash(conn: &Connection, hash: &str, now: &str) -> Result<Option<ClipboardItem>> {
    let updated = conn.execute(
        "UPDATE clipboard_history SET created_at = ?1 WHERE content_hash = ?2",
        params![now, hash],
    )?;
    if updated == 0 {
        return Ok(None);
    }
    let sql = format!("SELECT {ITEM_COLUMNS} FROM clipboard_history WHERE content_hash = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let row = stmt.query_row(params![hash], row_to_item)?;
    Ok(Some(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(content: &str, created_at: &str) -> ClipboardItem {
        ClipboardItem {
            id: 0,
            content: content.into(),
            content_hash: format!("hash-{content}"),
            category: "plain".into(),
            kind: "text".into(),
            pinned: false,
            created_at: created_at.into(),
        }
    }

    #[test]
    fn creates_table_and_inserts_item() {
        let conn = open_in_memory_db().unwrap();
        let out = insert_item(&conn, &item("hello world", "2026-01-01T00:00:00Z"), 1000).unwrap();
        assert!(out.inserted);
        let items = get_all_items(&conn, 1000).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "hello world");
    }

    #[test]
    fn duplicate_content_bumps_existing_row_instead_of_inserting() {
        let conn = open_in_memory_db().unwrap();
        let first = insert_item(&conn, &item("dup", "2026-01-01T00:00:00Z"), 1000).unwrap();
        insert_item(&conn, &item("other", "2026-01-02T00:00:00Z"), 1000).unwrap();
        let second = insert_item(&conn, &item("dup", "2026-01-03T00:00:00Z"), 1000).unwrap();
        assert!(!second.inserted, "re-copy should update, not insert");
        assert_eq!(first.id, second.id);
        let items = get_all_items(&conn, 1000).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].content, "dup", "bumped item must sort to top");
        assert_eq!(items[0].created_at, "2026-01-03T00:00:00Z");
    }

    #[test]
    fn prunes_oldest_unpinned_beyond_limit_but_keeps_pinned() {
        let conn = open_in_memory_db().unwrap();
        for i in 0..5 {
            insert_item(&conn, &item(&format!("n{i}"), &format!("2026-01-0{}T00:00:0{i}Z", i + 1)), 3)
                .unwrap();
        }
        // pin the oldest, it must survive pruning
        let items = get_all_items(&conn, 1000).unwrap();
        let oldest_id = items.iter().min_by_key(|i| i.created_at.clone()).unwrap().id;
        toggle_pin(&conn, oldest_id).unwrap();
        insert_item(&conn, &item("extra", "2026-01-09T00:00:00Z"), 3).unwrap();

        let items = get_all_items(&conn, 1000).unwrap();
        let unpinned: Vec<_> = items.iter().filter(|i| !i.pinned).collect();
        assert_eq!(unpinned.len(), 3, "unpinned history capped at 3");
        assert!(items.iter().any(|i| i.id == oldest_id), "pinned survives");
    }

    #[test]
    fn search_filters_by_content() {
        let conn = open_in_memory_db().unwrap();
        insert_item(&conn, &item("https://example.com", "2026-01-01T00:00:00Z"), 1000).unwrap();
        insert_item(&conn, &item("hello world", "2026-01-02T00:00:00Z"), 1000).unwrap();
        let hits = search_items(&conn, "example", 200).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "https://example.com");
    }

    #[test]
    fn delete_and_toggle_pin_work() {
        let conn = open_in_memory_db().unwrap();
        let out = insert_item(&conn, &item("pin me", "2026-01-01T00:00:00Z"), 1000).unwrap();
        toggle_pin(&conn, out.id).unwrap();
        assert!(get_all_items(&conn, 1000).unwrap()[0].pinned);
        assert_eq!(delete_item(&conn, out.id).unwrap(), 1);
        assert!(get_all_items(&conn, 1000).unwrap().is_empty());
    }

    #[test]
    fn search_treats_percent_and_underscore_literally() {
        let conn = open_in_memory_db().unwrap();
        insert_item(&conn, &item("100% legit", "2026-01-01T00:00:00Z"), 1000).unwrap();
        insert_item(&conn, &item("100X legit", "2026-01-02T00:00:00Z"), 1000).unwrap();
        let hits = search_items(&conn, "100%", 200).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "100% legit");
    }

    #[test]
    fn touch_by_hash_bumps_timestamp_and_returns_row() {
        let conn = open_in_memory_db().unwrap();
        insert_item(&conn, &item("bump me", "2026-01-01T00:00:00Z"), 1000).unwrap();
        insert_item(&conn, &item("other", "2026-01-02T00:00:00Z"), 1000).unwrap();
        let touched = touch_by_hash(&conn, "hash-bump me", "2026-01-03T00:00:00Z").unwrap();
        assert!(touched.is_some());
        assert_eq!(touched.unwrap().created_at, "2026-01-03T00:00:00Z");
        let items = get_all_items(&conn, 1000).unwrap();
        assert_eq!(items[0].content, "bump me");
    }

    #[test]
    fn touch_by_hash_returns_none_for_unknown_hash() {
        let conn = open_in_memory_db().unwrap();
        assert!(touch_by_hash(&conn, "nope", "2026-01-03T00:00:00Z").unwrap().is_none());
    }
}
