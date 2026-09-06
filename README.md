# Clipboard Superpowers

A fast, offline clipboard manager for Windows. Replaces `Win+V` with a searchable popup: every copy is auto-categorized, pinned, and one click away. Text + images, global hotkey, system tray, autostart.

> Stack: Tauri v2 + Rust + React 19 + TypeScript + Tailwind v4 + SQLite (`rusqlite` bundled)

## Features

- **Capture everything** — polls clipboard every 300ms, dedupes via SHA-256, re-copies bump to top
- **Auto-categories** — `link` / `email` / `color` (hex, `rgb()`, `rgba()`) / `code` / `plain` / `image`
- **Search** — debounced 200ms, `LIKE` with `%`/`_` escaped, pinned-first sorting
- **Images** — PNG thumbnails with cached base64, copy-back to clipboard, files cleaned on delete
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
| Copy item back | Click card (or `Enter` when focused) |
| Pin / unpin | Pin icon — pinned stays on top, survives pruning |
| Delete | Trash icon — images also delete the PNG + cache |
| Filter | Category pills (All/Plain/Links/Code/Colors/Emails/Images) |
| Quit | Tray → Quit (X only hides to tray) |

History caps at **1000 unpinned** items; oldest pruned automatically.

## Develop

```bash
cd E:/dev/clipboard-superpowers
npm install
npm run tauri dev      # Vite on :1420 + Tauri window
```

### Useful commands

```bash
npm test                # vitest — api, format, imageCache (12 tests)
npx tsc --noEmit        # typecheck
npm run build           # frontend only → dist/
cargo test              # Rust — db, categorize (12 tests), run in src-tauri/
cargo check             # fast Rust check, run in src-tauri/
npm run tauri build     # full installer → src-tauri/target/release/bundle/
```

### Inspect the DB

```bash
cargo run --example dump_db   # prints last 20 rows + total (run in src-tauri/)
```

## Project structure

```
src/
  App.tsx                 # popup shell, search debounce, pagination, focus/blur, theme
  lib/api.ts              # Tauri invoke wrapper + event name
  lib/types.ts            # ClipboardItem, Category, Kind
  lib/format.ts           # formatTime, extractColor (shared)
  lib/imageCache.ts       # base64 thumbnail cache (100-cap)
  components/
    SearchBar.tsx  CategoryFilter.tsx  HistoryList.tsx
    ClipboardCard.tsx  ImageCard.tsx  CardShared.tsx  ThemeSwitcher.tsx
src-tauri/src/
  main.rs / lib.rs        # tray, global shortcut, autostart, window events
  clipboard.rs            # polling, image_hash, save PNG, emit with real id
  db.rs                   # SQLite schema, insert/bump, prune, escaped search
  commands.rs             # get/search/delete/pin/copy/read_image_base64
  categorize.rs           # hash_content + regex categorizer
src-tauri/capabilities/default.json  # explicit perms: autostart, global-shortcut, window
```

## Configuration

- **Hotkey:** `Ctrl+Alt+V` in `src-tauri/src/lib.rs` (`Shortcut::new(CONTROL|ALT, KeyV)`)
- **Poll interval / cap:** `POLL_INTERVAL_MS = 300`, `MAX_ITEMS = 1000` in `clipboard.rs`
- **Window:** `src-tauri/tauri.conf.json` — 460×580, frameless, `visible: false` + `skipTaskbar: true` (tray-first)
- **Theme:** light/dark/system via `localStorage`, defaults to system

## Troubleshooting

- **Hotkey does nothing** — another app grabbed `Ctrl+Alt+V`, or capabilities missing `global-shortcut:allow-register`. Check `src-tauri/capabilities/default.json`.
- **App seems dead on launch** — by design it starts hidden to tray. Look for the tray icon, press `Ctrl+Alt+V`.
- **Images re-appear after copy** — fixed via unified `image_hash`; if you see it, poller and copy path diverged again.
- **DB locked** — WAL + 2s busy-timeout is on; don't open `clipboard.db` with an exclusive lock while running.
- **Transparent-window GPU errors** — not used; window is solid (`transparent: false`).

## Roadmap (not in v1)

Cloud sync · rich-text/HTML clipboard · file drops · plugin system · configurable hotkey UI · OCR.

## License

Private — Hoss / Operation Winter. No distribution without permission.
