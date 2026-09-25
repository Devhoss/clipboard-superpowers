use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClipboardItem {
    pub id: i64,
    pub content: String, // text content, or absolute PNG path for kind="image"
    pub content_hash: String, // sha256 hex
    pub category: String, // plain|link|code|color|email|image|file|secret
    pub kind: String,    // text|image|file
    pub pinned: bool,
    pub created_at: String,   // RFC3339
    pub html: Option<String>, // sanitized HTML flavor (PR4), None = plain text
    /// Friendly name of the app that wrote the clip (detail pane). None for
    /// rows captured before this existed or when the owner was unavailable.
    pub source_app: Option<String>,
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
    // Retained secrets outlive the renderer, so a plain DELETE is not enough:
    // without this, deleted password bytes stay in freed pages (and in the WAL)
    // and remain recoverable from the file. Zero freed content on delete.
    conn.pragma_update(None, "secure_delete", "ON")?;
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
            created_at TEXT NOT NULL,
            source_app TEXT
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
    // source_app migration: friendly name of the copying app. Guarded like
    // the html migration below — pre-existing databases migrate in place.
    let has_source: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('clipboard_history') WHERE name = 'source_app'")?
        .exists([])?;
    if !has_source {
        conn.execute(
            "ALTER TABLE clipboard_history ADD COLUMN source_app TEXT",
            [],
        )?;
    }
    // PR4 migration: formatted-HTML flavor alongside plain text. Guarded so
    // existing databases (created before this column) migrate in place —
    // ADD COLUMN on a table that already has it is an error, hence the check.
    let has_html: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('clipboard_history') WHERE name = 'html'")?
        .exists([])?;
    if !has_html {
        conn.execute("ALTER TABLE clipboard_history ADD COLUMN html TEXT", [])?;
    }
    Ok(())
}

/// Insert a new item, or bump an existing duplicate to the top of the
/// history by refreshing its timestamp. MAX_ITEMS caps unpinned history.
pub fn insert_item(
    conn: &Connection,
    item: &ClipboardItem,
    max_items: i64,
) -> Result<InsertOutcome> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM clipboard_history WHERE content_hash = ?1",
            params![item.content_hash],
            |row| row.get(0),
        )
        .optional()?;
    conn.execute(
        "INSERT INTO clipboard_history (content, content_hash, category, kind, pinned, created_at, html, source_app)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(content_hash) DO UPDATE SET
           created_at = excluded.created_at,
           -- Re-capture re-categorizes: category rules evolve (e.g. the rgb()
           -- anchoring fix), and a stale row keeps its wrong tab forever if
           -- the bump doesn't rewrite it. Pinned is never written here at all
           -- (only toggle_pin changes it) so bumps can't unpin.
           category = CASE
               WHEN clipboard_history.category = 'secret' THEN 'secret'
               ELSE excluded.category
           END,
           -- A re-capture with a failed flavor read (None) must not erase
           -- known formatting.
           html = COALESCE(excluded.html, clipboard_history.html)",
        params![
            item.content,
            item.content_hash,
            item.category,
            item.kind,
            item.pinned as i32,
            item.created_at,
            item.html,
            item.source_app
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

/// Delete oldest unpinned items beyond the cap. Retained secrets are ordinary
/// visible rows now, so the cap applies to them too — otherwise the list could
/// grow past `max_items` with rows the user can actually see.
pub fn prune_old_items(conn: &Connection, max_items: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM clipboard_history
         WHERE pinned = 0 AND id NOT IN (
             SELECT id FROM clipboard_history
             WHERE pinned = 0
             ORDER BY created_at DESC LIMIT ?1
         )",
        params![max_items],
    )?;
    Ok(())
}

/// Count of rows a [`purge_secrets`] call would destroy right now. Read-only,
/// so the UI can state the number before the user commits to the delete.
pub fn count_secrets(conn: &Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM clipboard_history WHERE category = 'secret'",
        [],
        |r| r.get(0),
    )
    .map_err(Into::into)
}

/// Remove every retained secret row. Used when the user turns skipping on;
/// secrets are not otherwise visible to the renderer or the history cap.
///
/// `secure_delete` (set in [`open_db`]) zeroes freed page content, but the
/// original INSERT images still sit in un-checkpointed WAL frames. Fold those
/// back and truncate the WAL too, so the bytes are not left recoverable on
/// disk. Best-effort: another connection mid-read can block a TRUNCATE
/// checkpoint, and the secure-delete pass has already done the important part.
pub fn purge_secrets(conn: &Connection) -> Result<usize> {
    let removed = conn.execute(
        "DELETE FROM clipboard_history WHERE category = 'secret'",
        [],
    )?;
    match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(0) => {}
        Ok(_) | Err(_) => {
            eprintln!("clipboard-superpowers: WAL truncate after secret purge failed")
        }
    }
    Ok(removed)
}

/// Count rows shown in the history list. Retained secrets are listed now
/// (redacted), so they are counted too.
pub fn get_visible_counts(conn: &Connection) -> Result<(i64, i64)> {
    Ok((
        conn.query_row("SELECT COUNT(*) FROM clipboard_history", [], |row| {
            row.get(0)
        })?,
        conn.query_row(
            "SELECT COUNT(*) FROM clipboard_history WHERE pinned = 1",
            [],
            |row| row.get(0),
        )?,
    ))
}

const ITEM_COLUMNS: &str =
    "id, content, content_hash, category, kind, pinned, created_at, html, source_app";

fn row_to_item(row: &rusqlite::Row) -> Result<ClipboardItem> {
    Ok(ClipboardItem {
        id: row.get(0)?,
        content: row.get(1)?,
        content_hash: row.get(2)?,
        category: row.get(3)?,
        kind: row.get(4)?,
        pinned: row.get::<_, i32>(5)? != 0,
        created_at: row.get(6)?,
        html: row.get(7)?,
        source_app: row.get(8)?,
    })
}

/// Internal/raw read: includes retained secret rows. NOT for renderer-facing
/// code — every command must go through [`get_history_previews`],
/// [`search_history_previews`], or [`get_item_by_id`], which all exclude
/// secrets. Exists for tests, the perf benchmark, and the dev dump example;
/// a future command that reaches for this "convenient" helper would re-open
/// the leak the whole secret-hygiene lane exists to close.
pub fn get_all_items(conn: &Connection, limit: i64) -> Result<Vec<ClipboardItem>> {
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM clipboard_history
         ORDER BY pinned DESC, created_at DESC LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], row_to_item)?;
    rows.collect()
}

/// Preview rows for list/search responses. Text clips carry a 300-char
/// content slice and a 2400-char html slice — far beyond what the 3-line
/// card preview and `richPreviewHtml` (2000) need; the measured payload of
/// a full 1000-row read was 7.3 MB of JSON vs 0.26 MB here. Full text/html
/// lives only in the DB; copy/paste loads the complete row on demand via
/// [`get_item_for_copy`]. File rows keep full content (path lists must reach
/// FileCard and copy-back intact); image rows carry a path either way.
///
/// Retained secrets ARE listed (so the user can see that something was
/// captured) but carry no secret-derived data at all: content becomes a fixed
/// placeholder, html becomes NULL, and **content_hash is blanked**. The hash
/// is a SHA-256 of the secret, so shipping it turns the list into a
/// verification oracle — hash a guessed password locally and compare. That is
/// catastrophic for a low-entropy secret such as a 6-digit OTP, where the
/// whole keyspace is brute-forceable in seconds. The real bytes reach the
/// renderer only through [`reveal_secret`], one row at a time, on an explicit
/// user click.
pub fn get_history_previews(conn: &Connection, limit: i64) -> Result<Vec<ClipboardItem>> {
    let sql = String::from(
        "SELECT id,
                 CASE
                   WHEN category = 'secret' THEN '••••••••'
                   WHEN kind = 'text' THEN substr(content, 1, 300)
                   ELSE content
                 END,
                 CASE WHEN category = 'secret' THEN '' ELSE content_hash END,
                 category, kind, pinned, created_at,
                 CASE
                   WHEN category = 'secret' THEN NULL
                   WHEN kind = 'text' THEN substr(html, 1, 2400)
                   ELSE html
                 END, source_app
           FROM clipboard_history
           ORDER BY pinned DESC, created_at DESC LIMIT ?1",
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], row_to_item)?;
    rows.collect()
}

/// Search filters: residual text is optional and a date is optional, but
/// both are AND-combined when present.
#[derive(Debug, Default, PartialEq)]
struct SearchFilter {
    text: Option<String>,
    date: Option<NaiveDate>,
}

fn parse_date_token(token: &str) -> Option<NaiveDate> {
    let raw = token.strip_prefix("date:")?;
    if raw.len() != 10
        || raw.as_bytes().get(4) != Some(&b'-')
        || raw.as_bytes().get(7) != Some(&b'-')
    {
        return None;
    }
    let date = NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    Some(date)
}

/// Split a query into a text filter and an optional date filter.
///
/// A `date:YYYY-MM-DD` token becomes a local-calendar-day filter and is
/// removed from the text, so `report date:2026-09-24` means
/// "text LIKE %report% AND that local day". Invalid or incomplete tokens
/// (`date:2026-01`, `date:2026-99-99`) stay literal text, so a genuine clip
/// containing that string is still findable.
///
/// Only one date filter exists, so with two valid date tokens the **first
/// wins** and the second is kept as literal search text. That is a
/// degenerate query (it can only match clips whose content literally
/// contains "date:2026-09-25") rather than a silent wrong-day match.
fn parse_search_query(query: &str) -> SearchFilter {
    let mut text = Vec::new();
    let mut date = None;
    for token in query.split_whitespace() {
        if let Some(parsed) = parse_date_token(token) {
            if date.is_none() {
                date = Some(parsed);
                continue;
            }
        }
        text.push(token);
    }
    SearchFilter {
        text: (!text.is_empty()).then(|| text.join(" ")),
        date,
    }
}

/// Full content of ONE secret row, for the explicit reveal click.
///
/// This is the only renderer-facing path that ever returns secret bytes, and
/// it is deliberately narrow: a single id, no bulk read, no listing. The
/// caller must have a user gesture behind it. Callers must not cache the
/// result — the frontend drops it on blur/restart.
pub fn reveal_secret(conn: &Connection, id: i64) -> Result<Option<String>> {
    conn.query_row(
        "SELECT content FROM clipboard_history WHERE id = ?1 AND category = 'secret'",
        params![id],
        |row| row.get(0),
    )
    .optional()
}

/// Full row for copy/paste, INCLUDING retained secrets. Separate from
/// [`get_item_by_id`] on purpose: the detail-pane loader must never pull
/// secret content implicitly, but an explicit copy of a secret row is a
/// user action and is allowed. The frontend only offers that action on a row
/// the user can see.
pub fn get_item_for_copy(conn: &Connection, id: i64) -> Result<Option<ClipboardItem>> {
    let sql = format!("SELECT {ITEM_COLUMNS} FROM clipboard_history WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row(params![id], row_to_item).optional()
}

/// Full row by id for the detail pane — never returns secret content.
pub fn get_item_by_id(conn: &Connection, id: i64) -> Result<Option<ClipboardItem>> {
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM clipboard_history
         WHERE id = ?1 AND category <> 'secret'"
    );
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row(params![id], row_to_item).optional()
}

/// Search over previews, including redacted secret rows.
///
/// A **text** query never matches a secret's content — otherwise the search
/// box becomes an oracle that leaks a password one character at a time
/// (`password=hunter` narrowing the result set). So the text filter carries
/// `category <> 'secret'`. A **date-only** query has no text term and does
/// list secrets, which is the useful case: "what did I copy on Tuesday?"
/// answers with a redacted row you can reveal.
pub fn search_history_previews(
    conn: &Connection,
    query: &str,
    limit: i64,
) -> Result<Vec<ClipboardItem>> {
    let filter = parse_search_query(query);
    let mut sql = String::from(
        "SELECT id,
                 CASE
                   WHEN category = 'secret' THEN '••••••••'
                   WHEN kind = 'text' THEN substr(content, 1, 300)
                   ELSE content
                 END,
                 CASE WHEN category = 'secret' THEN '' ELSE content_hash END,
                 category, kind, pinned, created_at,
                 CASE
                   WHEN category = 'secret' THEN NULL
                   WHEN kind = 'text' THEN substr(html, 1, 2400)
                   ELSE html
                 END, source_app
           FROM clipboard_history
           WHERE 1=1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(text) = filter.text {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        // Excluding secrets here is what stops text search probing a secret's
        // content. The rows still appear for date-only queries.
        sql.push_str(" AND content LIKE ? ESCAPE '\\' AND category <> 'secret'");
        args.push(Box::new(format!("%{escaped}%")));
    }
    if let Some(date) = filter.date {
        sql.push_str(" AND date(created_at, 'localtime') = ?");
        args.push(Box::new(date.format("%Y-%m-%d").to_string()));
    }
    sql.push_str(" ORDER BY pinned DESC, created_at DESC LIMIT ?");
    args.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_item)?;
    rows.collect()
}

/// Internal full-row search used by tests and the perf benchmark. Keeps the
/// `category <> 'secret'` guard: unlike the preview search it is not a
/// renderer path, and returning full secret rows here would hand the whole
/// secret table to any caller that reaches for it.
pub fn search_items(conn: &Connection, query: &str, limit: i64) -> Result<Vec<ClipboardItem>> {
    let filter = parse_search_query(query);
    let mut sql = format!(
        "SELECT {ITEM_COLUMNS} FROM clipboard_history
         WHERE category <> 'secret'"
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(text) = filter.text {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        sql.push_str(" AND content LIKE ? ESCAPE '\\'");
        args.push(Box::new(format!("%{escaped}%")));
    }
    if let Some(date) = filter.date {
        sql.push_str(" AND date(created_at, 'localtime') = ?");
        args.push(Box::new(date.format("%Y-%m-%d").to_string()));
    }
    sql.push_str(" ORDER BY pinned DESC, created_at DESC LIMIT ?");
    args.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), row_to_item)?;
    rows.collect()
}

/// Delete visible history rows, returning the count and the image paths that
/// are now orphaned.
///
/// Retained secrets are ordinary visible rows now (redacted in the preview,
/// revealed on demand), so "Clear everything" clears them too — otherwise the
/// button would leave behind exactly the rows the user can see. Because
/// `secure_delete` is on, their bytes are zeroed rather than left in freed
/// pages. Shared with its test so the behavior is asserted against the SQL
/// that ships.
pub fn clear_visible_history(
    conn: &Connection,
    delete_pinned: bool,
) -> Result<(usize, Vec<String>)> {
    let flag = if delete_pinned { 1 } else { 0 };
    let paths: Vec<String> = conn
        .prepare(
            "SELECT content FROM clipboard_history
             WHERE kind = 'image' AND (?1 = 1 OR pinned = 0)",
        )?
        .query_map([flag], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let deleted = conn.execute(
        "DELETE FROM clipboard_history
         WHERE (?1 = 1 OR pinned = 0)",
        [flag],
    )?;
    Ok((deleted, paths))
}

/// Delete one row. Retained secrets are deletable — the row is visible in
/// history, so the user must be able to remove it. This is distinct from
/// re-enabling skip, which purges *all* secrets at once.
pub fn delete_item(conn: &Connection, id: i64) -> Result<usize> {
    conn.execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])
}

pub fn toggle_pin(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clipboard_history
         SET pinned = CASE WHEN pinned = 1 THEN 0 ELSE 1 END
         WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Refresh an existing row's timestamp by content hash (re-copy = most
/// recent). Returns the updated row, or None when the hash isn't stored.
pub fn touch_by_hash(conn: &Connection, hash: &str, now: &str) -> Result<Option<ClipboardItem>> {
    let updated = conn.execute(
        "UPDATE clipboard_history
         SET created_at = ?1
         WHERE content_hash = ?2 AND category <> 'secret'",
        params![now, hash],
    )?;
    if updated == 0 {
        return Ok(None);
    }
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM clipboard_history
         WHERE content_hash = ?1 AND category <> 'secret'"
    );
    let mut stmt = conn.prepare(&sql)?;
    let row = stmt.query_row(params![hash], row_to_item)?;
    Ok(Some(row))
}

/// Read one visible row by content hash. The poller emits this after
/// insert/bump so the event carries the true stored state (pinned, id) — never
/// the freshly built item with its `pinned: false` default, which visually
/// unpinned cards when our own copy was re-captured after the suppress window.
pub fn get_by_hash(conn: &Connection, hash: &str) -> Result<Option<ClipboardItem>> {
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM clipboard_history
         WHERE content_hash = ?1 AND category <> 'secret'"
    );
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row(params![hash], row_to_item).optional()
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
            html: None,
            source_app: None,
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
    fn previews_truncate_text_but_keep_files_full() {
        let conn = open_in_memory_db().unwrap();
        let big = "x".repeat(10_000);
        let mut rich = item(&big, "2026-01-01T00:00:00Z");
        rich.html = Some("<b>y</b>".repeat(4_000));
        insert_item(&conn, &rich, 1000).unwrap();
        let paths = (0..100)
            .map(|i| format!("C:\\dir\\file{i}.txt"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut file_item = item(&paths, "2026-01-02T00:00:00Z");
        file_item.kind = "file".into();
        file_item.category = "file".into();
        insert_item(&conn, &file_item, 1000).unwrap();

        let previews = get_history_previews(&conn, 100).unwrap();
        assert_eq!(previews.len(), 2);
        let text = previews.iter().find(|i| i.kind == "text").unwrap();
        assert_eq!(text.content.len(), 300, "text content must be capped");
        assert_eq!(
            text.html.as_ref().unwrap().len(),
            2400,
            "html must be capped"
        );
        let file = previews.iter().find(|i| i.kind == "file").unwrap();
        assert_eq!(file.content, paths, "file path lists must not be truncated");

        // Search previews truncate the same way.
        let hits = search_history_previews(&conn, "xxxx", 200).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content.len(), 300);

        // And the full row is still recoverable by id for copy/paste.
        let full = get_item_by_id(&conn, text.id).unwrap().unwrap();
        assert_eq!(full.content, big);
        assert_eq!(full.html.unwrap().len(), "<b>y</b>".len() * 4_000);
        assert!(get_item_by_id(&conn, 999_999).unwrap().is_none());
    }

    #[test]
    fn migration_adds_source_app_column_to_preexisting_tables() {
        let conn = Connection::open_in_memory().unwrap();
        // A table created before source_app existed (PR3-era schema).
        conn.execute(
            "CREATE TABLE clipboard_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL UNIQUE,
                category TEXT NOT NULL,
                kind TEXT NOT NULL,
                pinned INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            )",
            [],
        )
        .unwrap();
        init_schema(&conn).unwrap();
        let has: bool = conn
            .prepare(
                "SELECT 1 FROM pragma_table_info('clipboard_history') WHERE name = 'source_app'",
            )
            .unwrap()
            .exists([])
            .unwrap();
        assert!(has);
    }

    #[test]
    fn source_app_round_trips_and_survives_bump() {
        let conn = open_in_memory_db().unwrap();
        let mut app_clip = item(&"just text".repeat(1), "2026-01-01T00:00:00Z");
        app_clip.source_app = Some("VS Code".into());
        insert_item(&conn, &app_clip, 1000).unwrap();
        // Re-copy from elsewhere (html None) must keep the original app.
        let mut again = item("just text", "2026-01-02T00:00:00Z");
        again.source_app = None;
        insert_item(&conn, &again, 1000).unwrap();
        let row = get_by_hash(&conn, "hash-just text").unwrap().unwrap();
        assert_eq!(row.source_app.as_deref(), Some("VS Code"));
    }

    #[test]
    fn bump_recategorizes_stale_rows() {
        // Category rules evolve; a re-copy of the same content must refresh
        // the category instead of keeping the stale (wrong) one forever.
        let conn = open_in_memory_db().unwrap();
        let mut stale = item("mislabeled", "2026-01-01T00:00:00Z");
        stale.category = "color".into();
        insert_item(&conn, &stale, 1000).unwrap();
        let mut fresh = item("mislabeled", "2026-01-02T00:00:00Z");
        fresh.category = "code".into();
        insert_item(&conn, &fresh, 1000).unwrap();
        let items = get_all_items(&conn, 1000).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, "code");
    }

    #[test]
    fn html_flavor_round_trips_and_survives_htmlless_recapture() {
        let conn = open_in_memory_db().unwrap();
        let mut rich = item("hello", "2026-01-01T00:00:00Z");
        rich.html = Some("<b>hello</b>".into());
        insert_item(&conn, &rich, 1000).unwrap();
        let got = get_all_items(&conn, 1000).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].html.as_deref(), Some("<b>hello</b>"));
        // A flavorless re-capture (transient read failure, mid-tag slice)
        // must NOT erase known formatting — the poller re-reads the same
        // ambient content constantly, so "latest wins" visibly flaps cards
        // with no user action. Only a well-formed new fragment replaces.
        let mut plain = item("hello", "2026-01-02T00:00:00Z");
        plain.content_hash = "hash-hello".into();
        insert_item(&conn, &plain, 1000).unwrap();
        let got = get_all_items(&conn, 1000).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].html.as_deref(), Some("<b>hello</b>"));
        let mut richer = item("hello", "2026-01-03T00:00:00Z");
        richer.content_hash = "hash-hello".into();
        richer.html = Some("<i>hello</i>".into());
        insert_item(&conn, &richer, 1000).unwrap();
        let got = get_all_items(&conn, 1000).unwrap();
        assert_eq!(got[0].html.as_deref(), Some("<i>hello</i>"));
    }

    #[test]
    fn purge_secrets_removes_all_stored_secret_rows() {
        let conn = open_in_memory_db().unwrap();
        let mut old_secret = item("password=x", "2026-01-01T00:00:00Z");
        old_secret.category = "secret".into();
        insert_item(&conn, &old_secret, 1000).unwrap();
        let mut pinned_secret = item("password=z", "2026-01-01T00:00:00Z");
        pinned_secret.category = "secret".into();
        let pinned_id = insert_item(&conn, &pinned_secret, 1000).unwrap().id;
        toggle_pin(&conn, pinned_id).unwrap();
        insert_item(&conn, &item("hello", "2026-01-01T00:00:00Z"), 1000).unwrap();

        assert_eq!(purge_secrets(&conn).unwrap(), 2);
        let remaining: Vec<String> = get_all_items(&conn, 1000)
            .unwrap()
            .into_iter()
            .map(|i| i.content)
            .collect();
        assert_eq!(remaining, vec!["hello"]);
    }

    #[test]
    fn purge_secrets_zeroes_content_in_the_database_file() {
        // A logical DELETE is not enough for a retained password: without
        // secure_delete + a WAL checkpoint, the bytes stay in freed pages and
        // un-checkpointed frames. Scan the raw file for a unique sentinel.
        let dir = std::env::temp_dir().join(format!("clipsuper-purge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clipboard.db");
        let sentinel = "zzq-secret-sentinel-4f2a91c7";

        {
            let conn = open_db(&path.to_string_lossy()).unwrap();
            let mut secret = item(&format!("password={sentinel}"), "2026-01-01T00:00:00Z");
            secret.category = "secret".into();
            insert_item(&conn, &secret, 1000).unwrap();
            // Commit the INSERT to the WAL, then purge and checkpoint.
            assert!(!conn
                .query_row("SELECT 1 FROM clipboard_history", [], |r| r
                    .get::<_, i64>(0))
                .is_err());
            assert_eq!(purge_secrets(&conn).unwrap(), 1);
        }

        let mut raw = std::fs::read(&path).unwrap();
        // WAL sidecar can hold the original INSERT image if checkpointing was
        // blocked; on a single connection it is truncated to zero bytes.
        let wal = dir.join("clipboard.db-wal");
        if wal.exists() {
            raw.extend_from_slice(&std::fs::read(&wal).unwrap());
        }
        assert!(
            !raw.windows(sentinel.len())
                .any(|w| w == sentinel.as_bytes()),
            "secret bytes survived the purge on disk"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn date_search_token_combines_with_text_using_local_day() {
        use chrono::{Local, TimeZone};
        let conn = open_in_memory_db().unwrap();
        let local_day = Local::now().date_naive();
        let same_day = Local
            .from_local_datetime(&local_day.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .to_rfc3339();
        let other_day = Local
            .from_local_datetime(
                &(local_day - chrono::Days::new(1))
                    .and_hms_opt(12, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .to_rfc3339();
        insert_item(
            &conn,
            &item(&format!("needle {local_day}"), &same_day),
            1000,
        )
        .unwrap();
        insert_item(&conn, &item("needle", &other_day), 1000).unwrap();
        insert_item(&conn, &item("other", &same_day), 1000).unwrap();

        let hits =
            search_history_previews(&conn, &format!("needle date:{local_day}"), 200).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, format!("needle {local_day}"));

        let date_only = search_history_previews(&conn, &format!("date:{local_day}"), 200).unwrap();
        assert_eq!(date_only.len(), 2);
    }

    #[test]
    fn date_only_search_filters_by_local_day() {
        use chrono::{Local, TimeZone};
        let conn = open_in_memory_db().unwrap();
        let day = Local::now().date_naive();
        let today = Local
            .from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .to_rfc3339();
        let yesterday = Local
            .from_local_datetime(&(day - chrono::Days::new(1)).and_hms_opt(12, 0, 0).unwrap())
            .single()
            .unwrap()
            .to_rfc3339();
        insert_item(&conn, &item("today row", &today), 1000).unwrap();
        insert_item(&conn, &item("yesterday row", &yesterday), 1000).unwrap();

        let hits = search_history_previews(&conn, &format!("date:{day}"), 200).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "today row");
    }

    #[test]
    fn two_date_tokens_take_the_first_and_keep_the_second_as_text() {
        // Documented, not accidental: with one date filter, the first token
        // wins. The result is a degenerate query, not a wrong-day match.
        let parsed = parse_search_query("date:2026-01-01 date:2026-01-02");
        assert_eq!(
            parsed
                .date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .as_deref(),
            Some("2026-01-01")
        );
        assert_eq!(parsed.text.as_deref(), Some("date:2026-01-02"));

        let parsed = parse_search_query("report date:2026-01-01 date:2026-01-02 notes");
        assert_eq!(
            parsed
                .date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .as_deref(),
            Some("2026-01-01")
        );
        assert_eq!(parsed.text.as_deref(), Some("report date:2026-01-02 notes"));
    }

    #[test]
    fn invalid_or_incomplete_date_tokens_remain_literal_text() {
        let conn = open_in_memory_db().unwrap();
        insert_item(
            &conn,
            &item("date:2026-99-99", "2026-01-01T00:00:00Z"),
            1000,
        )
        .unwrap();
        insert_item(&conn, &item("date:2026-01", "2026-01-01T00:00:00Z"), 1000).unwrap();

        assert_eq!(
            search_history_previews(&conn, "date:2026-99-99", 200)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            search_history_previews(&conn, "date:2026-01", 200)
                .unwrap()
                .len(),
            1
        );
    }

    /// The placeholder must be a fixed string, never derived from the secret.
    /// If it echoed the real length/content, the list itself would leak the
    /// secret to anyone glancing at the row.
    const SECRET_PLACEHOLDER: &str = "••••••••";

    fn secret_item(content: &str) -> ClipboardItem {
        let mut s = item(content, "2026-01-01T00:00:00Z");
        s.category = "secret".into();
        s
    }

    #[test]
    fn history_lists_secrets_but_never_their_content() {
        let conn = open_in_memory_db().unwrap();
        let stored = secret_item("password=hunter2-and-then-some");
        let real_hash = stored.content_hash.clone();
        let real = stored.content.clone();
        insert_item(&conn, &stored, 1000).unwrap();
        insert_item(&conn, &item("ordinary", "2026-01-01T00:00:00Z"), 1000).unwrap();

        let rows = get_history_previews(&conn, 100).unwrap();
        let shown: Vec<&ClipboardItem> = rows.iter().filter(|r| r.category == "secret").collect();
        assert_eq!(shown.len(), 1, "the secret row is listed");
        // Fixed placeholder — a length- or content-derived one would leak the
        // secret to anyone glancing at the row.
        assert_eq!(shown[0].content, SECRET_PLACEHOLDER);
        assert_eq!(shown[0].content.len(), SECRET_PLACEHOLDER.len());
        assert!(shown[0].html.is_none(), "no rich payload for a secret");
        // The hash is SHA-256(secret): shipping it would let anyone confirm a
        // guessed password (or brute-force a 6-digit OTP) by hashing locally.
        assert_eq!(
            shown[0].content_hash, "",
            "a secret's hash must never reach the renderer"
        );
        // Nothing anywhere in the payload carries the real value.
        let json = serde_json::to_string(&rows).unwrap();
        assert!(
            !json.contains(real.as_str()),
            "secret bytes must not appear in the list payload"
        );
        assert!(
            !json.contains(real_hash.as_str()),
            "no secret hash may appear in the list payload"
        );
    }

    #[test]
    fn reveal_returns_one_secret_and_refuses_ordinary_rows() {
        let conn = open_in_memory_db().unwrap();
        let secret_id = insert_item(&conn, &secret_item("password=hunter2"), 1000)
            .unwrap()
            .id;
        let plain_id = insert_item(&conn, &item("just text", "2026-01-01T00:00:00Z"), 1000)
            .unwrap()
            .id;

        assert_eq!(
            reveal_secret(&conn, secret_id).unwrap().as_deref(),
            Some("password=hunter2")
        );
        // Refuses non-secret rows: this command must not become a way to read
        // full ordinary content and bypass the 300-char preview cap.
        assert!(reveal_secret(&conn, plain_id).unwrap().is_none());
        assert!(reveal_secret(&conn, 999_999).unwrap().is_none());
    }

    #[test]
    fn detail_pane_loader_refuses_secrets_but_copy_loader_allows_them() {
        let conn = open_in_memory_db().unwrap();
        let secret_id = insert_item(&conn, &secret_item("password=hunter2"), 1000)
            .unwrap()
            .id;

        // Implicit load (selection change) must never pull secret content.
        assert!(get_item_by_id(&conn, secret_id).unwrap().is_none());
        // Explicit copy is a user action and is allowed.
        let row = get_item_for_copy(&conn, secret_id).unwrap().unwrap();
        assert_eq!(row.content, "password=hunter2");
        assert_eq!(row.category, "secret");
    }

    #[test]
    fn text_search_cannot_probe_a_secret_but_a_date_query_lists_it() {
        let conn = open_in_memory_db().unwrap();
        insert_item(&conn, &secret_item("password=hunter2"), 1000).unwrap();

        // A text query must not match secret content — otherwise the search box
        // leaks the password one character at a time.
        for probe in ["hunter2", "password", "hunter", "hunter2 "] {
            assert!(
                search_history_previews(&conn, probe, 100)
                    .unwrap()
                    .is_empty(),
                "text search must not match a secret for probe {probe:?}"
            );
        }

        // A date-only query has no text term, so the row is listed redacted.
        let by_date = search_history_previews(&conn, "date:2026-01-01", 100).unwrap();
        assert_eq!(by_date.len(), 1);
        assert_eq!(by_date[0].content, SECRET_PLACEHOLDER);
        assert_eq!(by_date[0].category, "secret");
    }

    #[test]
    fn clear_everything_also_clears_secrets_now_that_they_are_visible_rows() {
        // Calls the shipped helper (used by the clear_history command), not a
        // hand-written DELETE — an earlier version of this test used its own
        // SQL and passed no matter what the command did.
        let conn = open_in_memory_db().unwrap();
        let secret_id = insert_item(&conn, &secret_item("password=secret"), 1000)
            .unwrap()
            .id;
        let pinned_secret_id = insert_item(&conn, &secret_item("password=pinned"), 1000)
            .unwrap()
            .id;
        toggle_pin(&conn, pinned_secret_id).unwrap();
        let visible_id = insert_item(&conn, &item("visible", "2026-01-01T00:00:00Z"), 1000)
            .unwrap()
            .id;
        let pinned_id = insert_item(&conn, &item("pinned visible", "2026-01-01T00:00:00Z"), 1000)
            .unwrap()
            .id;
        toggle_pin(&conn, pinned_id).unwrap();

        // "Clear everything" clears all four, secrets included.
        let (deleted, paths) = clear_visible_history(&conn, true).unwrap();
        assert_eq!(deleted, 4);
        assert!(paths.is_empty());
        assert!(get_all_items(&conn, 1000).unwrap().is_empty());

        // "Clear unpinned" spares pinned rows — including a pinned secret.
        let conn = open_in_memory_db().unwrap();
        let pinned_secret_id = insert_item(&conn, &secret_item("password=pinned"), 1000)
            .unwrap()
            .id;
        toggle_pin(&conn, pinned_secret_id).unwrap();
        let fresh = insert_item(&conn, &item("fresh unpinned", "2026-01-03T00:00:00Z"), 1000)
            .unwrap()
            .id;
        let (deleted, _) = clear_visible_history(&conn, false).unwrap();
        assert_eq!(deleted, 1, "only the new unpinned visible row");
        let ids: Vec<i64> = get_all_items(&conn, 1000)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, vec![pinned_secret_id]);
        assert!(!ids.contains(&fresh));
        let _ = (secret_id, visible_id, pinned_id);
    }

    #[test]
    fn user_can_delete_and_pin_a_secret_row() {
        let conn = open_in_memory_db().unwrap();
        let secret_id = insert_item(&conn, &secret_item("password=secret"), 1000)
            .unwrap()
            .id;

        toggle_pin(&conn, secret_id).unwrap();
        let row = get_all_items(&conn, 1000)
            .unwrap()
            .into_iter()
            .find(|r| r.id == secret_id)
            .unwrap();
        assert!(row.pinned);
        // Pinning must not reclassify the row out of the secret lane.
        assert_eq!(row.category, "secret");

        assert_eq!(delete_item(&conn, secret_id).unwrap(), 1);
        assert!(get_all_items(&conn, 1000).unwrap().is_empty());
    }

    #[test]
    fn visible_history_counts_now_include_secret_rows() {
        // Counts must match the list the user actually sees. If these excluded
        // secrets while the list showed them, the Settings number would
        // disagree with the visible row count.
        let conn = open_in_memory_db().unwrap();
        let secret_id = insert_item(&conn, &secret_item("password=secret"), 1000)
            .unwrap()
            .id;
        toggle_pin(&conn, secret_id).unwrap();
        let stored = get_all_items(&conn, 1000)
            .unwrap()
            .into_iter()
            .find(|r| r.id == secret_id)
            .unwrap();
        assert!(stored.pinned, "precondition: the secret really is pinned");

        insert_item(&conn, &item("visible", "2026-01-02T00:00:00Z"), 1000).unwrap();
        let pinned_visible =
            insert_item(&conn, &item("pinned visible", "2026-01-02T00:00:00Z"), 1000)
                .unwrap()
                .id;
        toggle_pin(&conn, pinned_visible).unwrap();

        // 3 rows total (1 secret + 2 visible), 2 of them pinned.
        assert_eq!(get_visible_counts(&conn).unwrap(), (3, 2));
        assert_eq!(get_history_previews(&conn, 100).unwrap().len(), 3);
    }

    #[test]
    fn retained_secret_recapture_cannot_be_reclassified_visible() {
        // The re-capture arrives re-classified (e.g. after a rule change it may
        // no longer look like a secret). The ON CONFLICT CASE must keep the
        // row in the secret lane regardless.
        let conn = open_in_memory_db().unwrap();
        insert_item(&conn, &secret_item("password=secret"), 1000).unwrap();

        let recaptured = item("password=secret", "2026-01-02T00:00:00Z");
        assert_ne!(
            recaptured.category, "secret",
            "precondition: re-capture is NOT a secret"
        );
        insert_item(&conn, &recaptured, 1000).unwrap();

        let stored = get_all_items(&conn, 1000).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].category, "secret");
        // And the list still shows it redacted, not as readable text.
        let previews = get_history_previews(&conn, 100).unwrap();
        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].content, SECRET_PLACEHOLDER);
    }

    #[test]
    fn prune_applies_the_cap_to_secrets_now_that_they_are_listed() {
        // Secrets are visible rows, so an uncapped secret table would grow
        // past max_items with rows the user can see.
        let conn = open_in_memory_db().unwrap();
        let old_secret = insert_item(&conn, &secret_item("password=secret"), 1000)
            .unwrap()
            .id;
        insert_item(&conn, &item("newer", "2026-01-02T00:00:00Z"), 1).unwrap();

        prune_old_items(&conn, 1).unwrap();
        let rows = get_all_items(&conn, 1000).unwrap();
        assert_eq!(
            rows.len(),
            1,
            "cap of 1 unpinned row applies to secrets too"
        );
        assert!(
            !rows.iter().any(|r| r.id == old_secret),
            "the older secret is pruned first, like any other row"
        );
    }

    #[test]
    fn prunes_oldest_unpinned_beyond_limit_but_keeps_pinned() {
        let conn = open_in_memory_db().unwrap();
        for i in 0..5 {
            insert_item(
                &conn,
                &item(&format!("n{i}"), &format!("2026-01-0{}T00:00:0{i}Z", i + 1)),
                3,
            )
            .unwrap();
        }
        // pin the oldest, it must survive pruning
        let items = get_all_items(&conn, 1000).unwrap();
        let oldest_id = items
            .iter()
            .min_by_key(|i| i.created_at.clone())
            .unwrap()
            .id;
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
        insert_item(
            &conn,
            &item("https://example.com", "2026-01-01T00:00:00Z"),
            1000,
        )
        .unwrap();
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
    fn bump_preserves_pin_and_html_on_htmlless_recapture() {
        let conn = open_in_memory_db().unwrap();
        let mut rich = item("pin me", "2026-01-01T00:00:00Z");
        rich.html = Some("<b>pin me</b>".into());
        let out = insert_item(&conn, &rich, 1000).unwrap();
        toggle_pin(&conn, out.id).unwrap();
        // Re-capture with a failed flavor read (html None): bump timestamp
        // but keep pin + formatting — this visually unpinned cards live.
        let mut plain = item("pin me", "2026-01-02T00:00:00Z");
        plain.html = None;
        insert_item(&conn, &plain, 1000).unwrap();
        let row = get_by_hash(&conn, "hash-pin me").unwrap().unwrap();
        assert!(row.pinned, "bump must not unpin");
        assert_eq!(row.html.as_deref(), Some("<b>pin me</b>"));
        assert_eq!(row.created_at, "2026-01-02T00:00:00Z");
        // A re-capture WITH html still updates it.
        let mut richer = item("pin me", "2026-01-03T00:00:00Z");
        richer.html = Some("<i>pin me</i>".into());
        insert_item(&conn, &richer, 1000).unwrap();
        let row = get_by_hash(&conn, "hash-pin me").unwrap().unwrap();
        assert_eq!(row.html.as_deref(), Some("<i>pin me</i>"));
        assert!(get_by_hash(&conn, "nope").unwrap().is_none());
    }
    #[test]
    fn touch_by_hash_returns_none_for_unknown_hash() {
        let conn = open_in_memory_db().unwrap();
        assert!(touch_by_hash(&conn, "nope", "2026-01-03T00:00:00Z")
            .unwrap()
            .is_none());
    }
}
