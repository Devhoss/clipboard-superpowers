# Clipboard Superpowers

A fast, offline clipboard manager for Windows. Replaces `Win+V` with a searchable popup: every copy is auto-categorized, pinned, and one click away. Text + images + files, global hotkey, system tray, autostart.

> Stack: Tauri v2 + Rust + React 19 + TypeScript + Tailwind v4 + SQLite (`rusqlite` bundled)

## Features

- **Master–detail popup** — compact history list (grouped by day) on the left, large preview + information pane on the right; glassy, keyboard-first, light/dark
- **Capture everything** — polls clipboard every 300ms, dedupes via SHA-256 (file drops are domain-tagged so a dir and its path-as-text stay separate entries), re-copies bump to top
- **Auto-categories** — `link` / `email` / `color` / `code` (incl. ``` fenced blocks and IDE HTML flavors) / `secret` / `image` / `file` / `plain`
- **Secret hygiene** — `password:`-style clips, OTPs, PEM keys, keyword-less JWTs and provider tokens (`sk-`, `ghp_`, `AKIA…`): skipped by default, or retained locally and listed as `••••••••` when the setting is off, revealed one row at a time on request; re-enabling purges them and securely overwrites their bytes on disk. Text search never matches secret content, and a secret's hash is never sent to the renderer, so the list cannot be used to confirm a guess.
- **Application attribution** — the detail pane shows which app the clip was copied from (`GetClipboardOwner`, pretty names, degrades silently when unavailable)
- **Syntax highlighting** — the code preview is tokenized on demand (keywords/calls/strings/numbers/comments); zero libraries, zero capture-time cost, copy-back always uses raw text
- **Rich text** — IDE/web HTML flavor is sanitized (ammonia) for the preview and written back on copy, so Word/Notion keep formatting
- **Search** — debounced 200ms, `LIKE` with `%`/`_` escaped, pinned-first sorting; a date picker filters by local calendar day and combines with typed text (`date:YYYY-MM-DD` still works)
- **Fast by design** — list payloads carry 300-char previews (17× smaller IPC on a 1000-row history), one shared SQLite connection, optimistic pin/delete, copy loads the full row by id; see `src-tauri/tests/perf.rs`
- **Popup UX** — `Ctrl+Alt+V` toggles, `Esc` hides, blur hides, focus refocuses search, paginated list (50/page)
- **Tray + autostart** — hide-to-tray on X, `Show/Hide` + `Quit` menu, launches on login
- **Offline** — SQLite at `%APPDATA%\com.hoss.clipsuper\clipboard.db`, images in `...\images\`

## Install (Windows)

1. Build the installer (see below) or grab it from `src-tauri/target/release/bundle/`
   - `nsis/*.exe` — guided setup (recommended)
   - `msi/*.msi` — enterprise deploy
2. Run it, then press **Ctrl+Alt+V** — the popup appears. Copy anything to see it show up.

## Usage

| Action | How |
|---|---|
| Open / hide | `Ctrl+Alt+V`, tray → Show/Hide, or click taskbar icon |
| Browse | Click a row — the preview pane shows the full entry; ↑/↓ work too |
| Copy | Double-click a row, the **Copy to Clipboard** button, or double-click value |
| Paste into previous app | `Enter` (copies + pastes; file rows open instead) |
| Open links / files | **Open** button in the preview pane (browser / Explorer) |
| Pin / unpin | Pin icon in the preview pane — pinned stays on top, survives pruning |
| Delete | Trash icon in the preview pane — images also delete the PNG + cache |
| Filter by type | **All Types** dropdown |
| Search | Type to filter; pick a day in the date picker beside the box (`date:YYYY-MM-DD` also works); `Esc` clears, `Esc` again hides |
| Reveal a secret | Select a `••••••••` row, then the eye button — re-hides on window blur and restart; copy works while revealed |
| Actions menu | `Ctrl+K` (re-recordable in Settings) — settings + appearance |
| Quit | Tray → Quit (X only hides to tray) |

Secrets are skipped by default. With **Skip passwords & OTPs** off, secret captures are retained in the local database and listed as `••••••••` — you reveal one at a time, and turning the setting back **on** purges them for good, so that toggle asks you to confirm first and names how many rows it will destroy. History caps at **1000 unpinned** items; oldest unpinned items are pruned automatically, secrets included, since they are ordinary visible rows.

## Develop

```bash
cd E:/dev/clipboard-superpowers
npm install
npm run tauri dev      # Vite on :1560 + Tauri window
```

### Useful commands

```bash
npm test                # vitest — api, format, keyboardNav, highlight, imageCache, secret visibility, settings confirm (49 tests)
npx tsc --noEmit        # typecheck
npm run build           # frontend only → dist/
cargo test              # Rust — db, categorize, clipboard, richtext, settings (75 tests), run in src-tauri/
cargo check             # fast Rust check, run in src-tauri/
cargo test --test perf -- --ignored --nocapture   # perf benchmark (run in src-tauri/)
npm run tauri build     # full installer → src-tauri/target/release/bundle/
npm run tauri icon app-icon.png   # regenerate all bundle icons from the source art
```

### Inspect the DB

```bash
cargo run --example dump_db   # prints last 20 non-secret rows + total (run in src-tauri/)
                                # secret rows are excluded by design — never printable
```

## Project structure

```
src/
  App.tsx                 # master-detail shell: topbar, sidebar+preview, toolbar, keyboard
  components/
    EntryList.tsx         # left pane: day-grouped rows, double-click copy, pagination
    DetailPane.tsx        # right pane: stage previews, Information rows, pin/delete/open, secret reveal
    TypeDropdown.tsx      # "All Types" filter menu
    SettingsPanel.tsx     # hotkeys (global + actions), capture toggles, history cap, arm-to-confirm purge, clear
  lib/
    api.ts                # Tauri invoke wrappers + event name
    types.ts              # ClipboardItem, Category, Kind
    categories.ts         # category meta (dot colors, labels)
    highlight.ts          # lazy regex tokenizer for the code preview
    format.ts             # time/day labels, colors, preview truncation
    keyboardNav.ts        # selection math + shortcut combo matching
    imageCache.ts         # base64 thumbnail cache (100-cap)
    metaCache.ts          # file_meta cache
src-tauri/src/
  main.rs / lib.rs        # tray, global shortcut, autostart, window events
  clipboard.rs            # polling, source-app capture, file-drop hashing, PNG save, emit
  db.rs                   # SQLite schema+migrations, preview payloads, insert/bump, prune, secret purge
  commands.rs             # get/search/delete/pin/copy(by id)/paste-by-id/file_meta/ocr
  categorize.rs           # regex categorizer + secret detection (KV, OTP, JWT, provider tokens)
  richtext.rs             # CF_HTML parse/build, sanitize, IDE-code detection
  ocr.rs                  # local WinRT OCR
  settings.rs             # settings load/save (hotkey, actions_hotkey, toggles)
src-tauri/capabilities/default.json  # explicit perms: autostart, global-shortcut, window
design/                  # interactive redesign mockups (v1 cards, v2 master-detail) + QA harness
```

## Configuration

- **Hotkey:** `Ctrl+Alt+V` — re-recordable in Settings
- **Actions shortcut:** `Ctrl+K` (in-popup only) — re-recordable in Settings
- **Poll interval / cap:** `POLL_INTERVAL_MS = 300`, `MAX_ITEMS = 1000` in `clipboard.rs`
- **WebView2 profile:** app-owned at `E:\AppData\ClipboardSuperpowers\WebView2` (`webview_profile.rs`). Existing profiles are migrated in with transient lock/cache state excluded; the source is kept as `*.migrated-bak` until the new profile is verified alive; stale `LOCK`/`lockfile` artifacts are cleaned on every startup. An inherited user-wide `WEBVIEW2_USER_DATA_FOLDER` (e.g. another app's) is deliberately overridden — that inheritance caused dead-WebView startups after Windows restarts.
- **Window:** 820×560 (resizable 640–1100), frameless, `visible: false` + `skipTaskbar: true` (tray-first)
- **Theme:** always dark. The palette lives on `:root` in `src/index.css` — there is no light variant, no `localStorage` key, and no Appearance control. A tray-only hotkey window renders the same glass over any wallpaper, and a light recipe would need its own tint, text contrast, and edge treatment.

## Troubleshooting

- **Hotkey does nothing** — another app grabbed `Ctrl+Alt+V`, or capabilities missing `global-shortcut:allow-register`. Check `src-tauri/capabilities/default.json`.
- **App seems dead on launch** — by design it starts hidden to tray. Look for the tray icon, press `Ctrl+Alt+V`.
- **Images re-appear after copy** — fixed via unified `image_hash`; if you see it, poller and copy path diverged again.
- **DB locked** — WAL + 2s busy-timeout is on; don't open `clipboard.db` with an exclusive lock while running.
- **Transparent-window GPU errors** — not used; window is solid (`transparent: false`).
- **Application shows "Windows service"/"Windows app"** — the copying process was a service or a UWP host; elevated apps may show no Application row at all (owner is unreadable).

## Roadmap

Shipped: global hotkey (re-mappable in Settings) · settings surface (hotkeys, capture toggles incl. files + secret-skip, history cap, autostart, hide-on-blur, clear, stats) · search · auto-categories (fenced code, IDE flavors, anchored colors) · secret hygiene with keyword-less token detection · pins · image capture · tray + autostart · always-dark glass theme · optimistic instant copy · re-copy bumps to top · full keyboard control · Enter-to-paste directly into the previous app · password/OTP hygiene · rich-text/HTML clipboard with sanitized preview · Explorer file drops with sizes + open/reveal · local WinRT OCR extraction (zero dependencies, on-device only) · master–detail v2 UI with preview pane and Information rows · application attribution · syntax-highlighted code preview · 17× smaller list payloads + shared DB connection + optimistic actions.

Dropped by design review: cloud sync (fights local-only guarantees, duplicates OS vendors, needs accounts + E2EE + a forever server) · plugin system (multiplier with nothing to multiply yet; custom-actions-lite is the fallback if real extensibility demand appears) · full grammar syntax highlighting (syntect: 10–30MB RAM for preview fidelity we don't need — the lazy regex tokenizer covers the mock's fidelity class).

## License

MIT
