// QA-only Tauri bridge mock: lets the real built app render in a plain
// browser with sample history. Served from design/qa-serve (patched index.html
// loads this BEFORE the bundle).
(function () {
  const DAY = 86_400_000;
  const now = Date.now();
  const iso = (msAgo) => new Date(now - msAgo).toISOString();

  let nextId = 100;
  const items = [
    mk("color", "#6663F6", iso(2 * 60_000), true, "Figma"),
    mk("color", "#D459B5", iso(9 * 60_000), false),
    mk("link", "https://ui.shadcn.com/docs/components/scroll-area", iso(14 * 60_000), false, "Google Chrome"),
    mk_app_code("function debounce(fn, ms) {\n  let t;\n  return (...args) => {\n    clearTimeout(t);\n    t = setTimeout(() => fn(...args), ms);\n  };\n}", iso(20 * 60_000), false),
    mk("image", "C:\\img\\shot.png", iso(55 * 60_000), false),
    mk("file", "C:\\work\\pitch-deck-v7.pdf\nC:\\work\\hero-shot.png", iso(2 * 3_600_000), false),
    mk("secret", "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJVadQssw5c", iso(3 * 3_600_000), false),
    mk("plain", "Standup notes: ship the category fix, review the perf PR, then cut the release. Ask Devhoss about the hotkey chip in the title bar.", iso(26 * 3_600_000), false),
    mk("email", "design@vp0.com", iso(1 * DAY + 3_600_000), false),
    mk("plain", "The first Macintosh computer shipped with 128 KB of RAM and a single 400K floppy drive.", iso(1 * DAY + 5 * 3_600_000), false),
  ];
  // 20 more plain rows so pagination/scroll is exercised.
  for (let i = 0; i < 20; i++) {
    items.push(mk("plain", `Archived clip number ${i + 1} — older history row for scroll testing.`, iso(2 * DAY + i * 60_000), false));
  }

  function mk(category, content, created_at, pinned, source_app = null) {
    return {
      id: ++nextId,
      content,
      content_hash: "h" + nextId,
      category,
      kind: category === "image" ? "image" : category === "file" ? "file" : "text",
      pinned,
      created_at,
      html: null,
      source_app,
    };
  }

  // 1x1 red PNG
  const TINY_PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

  const settings = {
    hotkey: "Ctrl+Alt+V",
    max_items: 1000,
    launch_on_login: false,
    capture_text: true,
    capture_images: true,
    hide_on_blur: true,
    skip_secrets: false,
    capture_files: true,
  };

  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    callbacks: [],
    transformCallback(cb) {
      const id = window.__TAURI_INTERNALS__.callbacks.push(cb);
      return id - 1;
    },
    invoke(cmd, args) {
      if (cmd === "plugin:event|listen" || cmd === "plugin:event|unlisten") return Promise.resolve(1);
      const sortRows = () =>
        [...items].sort((a, b) => (b.pinned - a.pinned) || b.created_at.localeCompare(a.created_at));
      switch (cmd) {
        case "get_history":
        case "search_history": {
          let rows = sortRows();
          if (cmd === "search_history" && args.query && args.query.trim()) {
            const q = args.query.trim().toLowerCase();
            rows = rows.filter((r) => r.content.toLowerCase().includes(q));
          }
          return Promise.resolve(
            rows.map((r) => ({ ...r, content: r.content.slice(0, 300) })),
          );
        }
        case "get_history_item":
          return Promise.resolve(items.find((r) => r.id === args.id) ?? null);
        case "toggle_pin": {
          const row = items.find((r) => r.id === args.id);
          if (row) row.pinned = !row.pinned;
          return Promise.resolve();
        }
        case "delete_item": {
          const ix = items.findIndex((r) => r.id === args.id);
          if (ix >= 0) items.splice(ix, 1);
          return Promise.resolve();
        }
        case "copy_history_item":
        case "copy_to_clipboard":
          return Promise.resolve();
        case "paste_history_item":
          return Promise.resolve();
        case "read_image_base64":
          return Promise.resolve(TINY_PNG);
        case "file_meta":
          return Promise.resolve({ total_bytes: 3_500_000, existing: 2, dirs: 0, missing: [] });
        case "get_settings":
          return Promise.resolve({ ...settings });
        case "update_settings":
          return Promise.resolve({ ...settings });
        case "get_stats":
          return Promise.resolve({ total: items.length, pinned: 1, db_bytes: 123456 });
        case "clear_history":
          items.length = 0;
          return Promise.resolve(items.length);
        default:
          console.warn("[mock] unhandled command", cmd, args);
          return Promise.resolve(null);
      }
    },
  };
})();
