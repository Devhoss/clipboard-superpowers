//! Performance benchmarks for the DB-layer operations behind every UI action.
//! Not part of the normal test run (debug SQLite overstates release numbers —
//! but the dev server the user experiences IS a debug build, so the numbers
//! are representative of `tauri dev`).
//!
//! Run:  cargo test --test perf -- --ignored --nocapture
//!
//! Seeded like a heavy real history: 900 small clips, 60 six-hundred-line
//! code clips (~66 KB each), 40 rich clips carrying 32k-char HTML.

use clipboard_superpowers_lib::categorize::hash_content;
use clipboard_superpowers_lib::db::{
    delete_item, expire_secrets, get_all_items, get_history_previews, get_item_by_id,
    insert_item, open_db, search_history_previews, search_items, toggle_pin, touch_by_hash,
    ClipboardItem,
};
use std::time::Instant;

fn item(content: String, html: Option<String>, created_at: String) -> ClipboardItem {
    ClipboardItem {
        id: 0,
        content_hash: hash_content(content.as_bytes()),
        category: categorize_of(&content),
        content,
        kind: "text".into(),
        pinned: false,
        created_at,
        html,
        source_app: None,
    }
}

fn with_app(mut it: ClipboardItem, app: &str) -> ClipboardItem {
    it.source_app = Some(app.into());
    it
}

fn categorize_of(content: &str) -> String {
    clipboard_superpowers_lib::categorize::categorize(content).to_string()
}

fn code_600() -> String {
    let mut s = String::with_capacity(70_000);
    for i in 0..600 {
        s.push_str(&format!(
            "function handler_{i}(event) {{\n  const value = computeValue(event, {i});\n  return value * 2; // rgb(255, 99, 71)\n}}\n"
        ));
    }
    s
}

fn rich_html() -> String {
    // VS Code-ish: one styled span per token, ~30k chars (under MAX_HTML_CHARS).
    let mut s = String::with_capacity(34_000);
    s.push_str("<div style=\"font-family: Consolas, monospace\">");
    for i in 0..380 {
        s.push_str(&format!(
            "<span style=\"color:#569cd6\">const</span> <span style=\"color:#9cdcfe\">v{i}</span> = <span style=\"color:#b5cea8\">{i}</span>; "
        ));
    }
    s.push_str("</div>");
    s
}

fn time_op(label: &str, iters: u32, mut f: impl FnMut(usize)) {
    let start = Instant::now();
    for i in 0..iters {
        f(i as usize);
    }
    let total = start.elapsed();
    println!(
        "{label:<58} {iters:>4} ops  avg {:>12.2?}",
        total / iters
    );
}

#[test]
#[ignore = "benchmark; run with --ignored --nocapture"]
fn perf_actions_on_full_history() {
    let dir = std::env::temp_dir().join("clipboard-superpowers-perf");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("perf.db").to_string_lossy().to_string();

    // --- seed -------------------------------------------------------------
    let seed_start = Instant::now();
    let conn = open_db(&db_path).unwrap();
    let mut stamp = 0u64;
    let mut next_ts = || {
        stamp += 1;
        format!("2026-09-11T{:02}:{:02}:{:02}.{:06}Z", stamp / 3600 % 24, stamp / 60 % 60, stamp % 60, stamp % 1_000_000)
    };
    let (n_small, n_code, n_rich) = (900u32, 60u32, 40u32);
    for i in 0..n_small {
        let c = format!("small clip #{i} — hello world, a short note with a link https://x.com/{i}");
        insert_item(
            &conn,
            &with_app(item(c, None, next_ts()), if i % 2 == 0 { "Notepad" } else { "Google Chrome" }),
            1000,
        )
        .unwrap();
    }
    let code = code_600();
    for i in 0..n_code {
        let c = format!("// variant {i}\n{code}");
        insert_item(&conn, &with_app(item(c, None, next_ts()), "VS Code"), 1000).unwrap();
    }
    let html = rich_html();
    for i in 0..n_rich {
        let c = format!("rich snippet {i}: formatted text from a web page, a couple of sentences with <b>bold</b> bits. #{i}");
        insert_item(
            &conn,
            &with_app(item(c, Some(html.clone()), next_ts()), "Google Chrome"),
            1000,
        )
        .unwrap();
    }
    println!(
        "seeded {} items ({n_small} small / {n_code} 600-line / {n_rich} rich) in {:?}\n",
        n_small + n_code + n_rich,
        seed_start.elapsed()
    );

    // --- per-command fixed cost: every Tauri command calls with_conn, which
    // re-opens the DB and re-runs init_schema --------------------------------
    time_op("open_db (per-command fixed cost)", 20, |_| {
        open_db(&db_path).unwrap();
    });

    // --- capture path -------------------------------------------------------
    let conn = open_db(&db_path).unwrap();
    time_op("insert_item — small clip (capture)", 200, |i| {
        insert_item(&conn, &with_app(item(format!("bench small {i} xyz"), None, next_ts()), "Notepad"), 1000).unwrap();
    });
    time_op("insert_item — 600-line code clip (capture)", 10, |i| {
        insert_item(&conn, &item(format!("// bench {i}\n{code}"), None, next_ts()), 1000).unwrap();
    });
    time_op("insert_item — rich clip w/ 32k html (capture)", 10, |i| {
        insert_item(&conn, &with_app(item(format!("bench rich {i}"), Some(html.clone()), next_ts()), "Google Chrome"), 1000).unwrap();
    });

    // --- the refresh after delete/pin: now preview-only --------------------
    let mut preview_bytes = 0usize;
    time_op("get_history_previews + JSON (refresh, NEW path)", 5, |_| {
        let rows = get_history_previews(&conn, 1000).unwrap();
        preview_bytes = serde_json::to_vec(&rows).unwrap().len();
    });
    println!(
        "    ^ JSON payload sent over IPC to the webview: {:.2} MB\n",
        preview_bytes as f64 / 1_048_576.0
    );

    // --- the OLD full read, for comparison ---------------------------------
    let mut payload_bytes = 0usize;
    time_op("get_all_items + JSON (OLD full read, for scale)", 5, |_| {
        let rows = get_all_items(&conn, 1000).unwrap();
        payload_bytes = serde_json::to_vec(&rows).unwrap().len();
    });
    println!(
        "    ^ was {:.1} MB ({:.0}x larger)\n",
        payload_bytes as f64 / 1_048_576.0,
        payload_bytes as f64 / preview_bytes as f64
    );

    // --- per-command fixed cost is now just a mutex lock --------------------
    let shared = std::sync::Arc::new(std::sync::Mutex::new(open_db(&db_path).unwrap()));
    time_op("lock shared conn (per-command fixed cost, NEW)", 20, |_| {
        let _c = shared.lock().unwrap();
    });

    // --- click-to-copy bump + the new on-demand full-row load ---------------
    let bump_hash = hash_content(b"bench small 0 xyz");
    time_op("touch_by_hash (copy-click bump + emit row)", 100, |_| {
        touch_by_hash(&conn, &bump_hash, &next_ts()).unwrap();
    });
    let all = get_all_items(&conn, 1000).unwrap();
    let big_row = all
        .iter()
        .find(|i| i.kind == "text" && i.content.len() > 60_000)
        .expect("seeded 600-line clip missing");
    let big_id = big_row.id;
    time_op("get_item_by_id — 66KB row (copy-by-id loader)", 100, |_| {
        get_item_by_id(&conn, big_id).unwrap();
    });

    // --- pin / delete ---------------------------------------------------------
    let ids: Vec<i64> = all.iter().map(|i| i.id).collect();
    time_op("toggle_pin", 100, |i| {
        toggle_pin(&conn, ids[(i as usize) % ids.len()]).unwrap();
    });
    time_op("delete_item", 50, |i| {
        delete_item(&conn, ids[(i as usize + 700) % ids.len()]).unwrap();
    });

    // --- search (typed every keystroke after 200ms debounce) ----------------
    time_op("search_history_previews(\"clip\") (NEW)", 20, |_| {
        search_history_previews(&conn, "clip", 200).unwrap();
    });
    time_op("search_items(\"clip\") (OLD full rows, for scale)", 20, |_| {
        search_items(&conn, "clip", 200).unwrap();
    });

    // --- the 300ms poller's secret sweep -------------------------------------
    time_op("expire_secrets (every 300ms tick)", 20, |_| {
        expire_secrets(&conn, "2026-09-12T00:00:00Z").unwrap();
    });

    let _ = std::fs::remove_dir_all(&dir);
}
