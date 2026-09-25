// QA-only Tauri bridge mock: lets the real built app render in a plain
// browser with sample history. Served from design/qa-serve (patched index.html
// loads this BEFORE the bundle).
(function () {
  const DAY = 86_400_000;
  const now = Date.now();
  const iso = (msAgo) => new Date(now - msAgo).toISOString();

  let nextId = 100;
  let items = [
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

  let settings = {
    hotkey: "Ctrl+Alt+V",
    actions_hotkey: "Ctrl+K",
    max_items: 1000,
    launch_on_login: false,
    capture_text: true,
    capture_images: true,
    hide_on_blur: true,
    skip_secrets: false,
    capture_files: true,
  };

  const parseDateToken = (value) => {
    const match = /^date:(\d{4})-(\d{2})-(\d{2})$/.exec(value);
    if (!match) return null;
    const date = new Date(`${match[1]}-${match[2]}-${match[3]}T00:00:00`);
    if (Number.isNaN(date.getTime()) || date.getFullYear() !== Number(match[1]) || date.getMonth() + 1 !== Number(match[2]) || date.getDate() !== Number(match[3])) return null;
    return `${match[1]}-${match[2]}-${match[3]}`;
  };
  const localDate = (value) => {
    const date = new Date(value);
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
  };
  const sortRows = () =>
    [...items].sort((a, b) => (b.pinned - a.pinned) || b.created_at.localeCompare(a.created_at));
  const searchRows = (query) => {
    const tokens = String(query || "").trim().split(/\s+/).filter(Boolean);
    let date = null;
    const text = [];
    for (const token of tokens) {
      const parsed = parseDateToken(token);
      if (parsed && !date) {
        date = parsed;
      } else {
        text.push(token);
      }
    }
    const residual = text.join(" ").toLowerCase();
    return sortRows().filter((r) => {
      if (r.category === "secret") return false;
      if (residual && !r.content.toLowerCase().includes(residual)) return false;
      return !date || localDate(r.created_at) === date;
    });
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
      switch (cmd) {
        case "get_history":
          return Promise.resolve(
            sortRows()
              .filter((r) => r.category !== "secret")
              .map((r) => ({ ...r, content: r.content.slice(0, 300) })),
          );
        case "search_history":
          return Promise.resolve(
            searchRows(args.query).map((r) => ({ ...r, content: r.content.slice(0, 300) })),
          );
        case "get_history_item":
          return Promise.resolve(
            items.find((r) => r.id === args.id && r.category !== "secret") ?? null,
          );
        case "toggle_pin": {
          const row = items.find((r) => r.id === args.id && r.category !== "secret");
          if (row) row.pinned = !row.pinned;
          return Promise.resolve();
        }
        case "delete_item": {
          const ix = items.findIndex((r) => r.id === args.id && r.category !== "secret");
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
        case "update_settings": {
          Object.assign(settings, args.settings || {});
          if (settings.skip_secrets) {
            for (let i = items.length - 1; i >= 0; i--) {
              if (items[i].category === "secret") items.splice(i, 1);
            }
          }
          return Promise.resolve({ ...settings });
        }
        case "get_stats":
          return Promise.resolve({
            total: items.filter((r) => r.category !== "secret").length,
            pinned: items.filter((r) => r.category !== "secret" && r.pinned).length,
            db_bytes: 123456,
          });
        case "clear_history": {
          const deletePinned = !!args.deletePinned;
          const before = items.filter(
            (r) => r.category !== "secret" && (deletePinned || !r.pinned),
          ).length;
          items = items.filter(
            (r) => r.category === "secret" || (!deletePinned && r.pinned),
          );
          return Promise.resolve(before);
        }
        default:
          console.warn("[mock] unhandled command", cmd, args);
          return Promise.resolve(null);
      }
    },
  };
})();
