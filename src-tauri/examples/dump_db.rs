fn main() {
    let dir = std::env::var("APPDATA").unwrap_or_else(|_| "C:/Users/Hossa/AppData/Roaming".into());
    let path = format!("{}/com.hoss.clipsuper/clipboard.db", dir);
    let conn = rusqlite::Connection::open(&path).expect("open db");
    let mut stmt = conn
        .prepare("SELECT id, kind, category, substr(content, 1, 60), created_at FROM clipboard_history ORDER BY id DESC LIMIT 20")
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok(format!(
                "{} | {} | {} | {:?} | {}",
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?
            ))
        })
        .unwrap();
    for row in rows {
        println!("{}", row.unwrap());
    }
    println!("total: {}", conn.query_row("SELECT COUNT(*) FROM clipboard_history", [], |r| r.get::<_, i64>(0)).unwrap_or(-1));
}
