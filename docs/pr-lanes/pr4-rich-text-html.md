# PR4 — Rich-text / HTML clipboard (spec)

Lane: `feat/rich-text-html` → `master`. Status: **spec only, implementation follows.**

## Goal

Copy formatted content (bold stays bold), preview it formatted, paste it formatted.
Today we read only `CF_TEXT`, so everything flattens to gray text.

## Scope (deliberately small)

- `src-tauri/Cargo.toml` — add `clipboard-win` (raw `HTML Format` flavor; `arboard` can't)
- `src-tauri/src/clipboard.rs` — poller also reads `HTML Format`, parses Microsoft's
  byte-offset header (`StartHTML:` / `EndHTML:`), stores fragment
- `src-tauri/src/db.rs` — migration: `ALTER TABLE clipboard_history ADD COLUMN html TEXT`
  (nullable; existing rows stay NULL = plain text)
- Sanitize before preview: backend `ammonia` (or equivalent) allowlist —
  **never raw-inject clipboard HTML into the webview** (stored-XSS by design otherwise)
- Copy path writes all flavors back: `CF_UNICODETEXT` + `HTML Format` so Word/Notion/Discord keep formatting
- v1 text+HTML only. RTF out of scope.

## Acceptance criteria

- [ ] Copy `<h1>` headline from browser → card shows formatted preview (bold/red preserved)
- [ ] Enter/click → paste into Word keeps formatting; paste into Notepad degrades to plain text
- [ ] Malicious `<script>` in clipboard HTML never executes in preview
- [ ] Old rows (html NULL) behave exactly as today

## Test plan

- `cargo test` — CF_HTML header parser (offsets, malformed headers), sanitizer strips script/handlers
- Manual: browser → Word + Notepad paste matrix
