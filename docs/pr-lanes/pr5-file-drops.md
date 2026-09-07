# PR5 — File drops (spec)

Lane: `feat/file-drops` → `master`. Status: **spec only, implementation follows.**

## Goal

Copying files (Ctrl+C in Explorer) shows a card instead of being ignored.

## Scope (deliberately small)

- `src-tauri/src/clipboard.rs` — poller reads `CF_HDROP` (file list) via `clipboard-win`;
  new item `kind = "file"`, `content` = newline-joined absolute paths
- **Store paths, don't copy files.** Copying bytes doubles disk use for zero benefit
- Card: file icon + name + count (`3 files`) + total size; click copies paths as text (existing path);
  double-click/Enter opens via Tauri opener (separate action, same card)
- Missing-file tolerance: card renders dimmed with "moved or deleted" if path no longer exists
- Do NOT touch: text/image capture, paste (PR2), sync

## Acceptance criteria

- [ ] Ctrl+C 3 files in Explorer → one `file` card with names + sizes
- [ ] Click → paths on clipboard as text; open-action launches Explorer selection
- [ ] Delete a file on disk → card degrades gracefully, no crash

## Test plan

- `cargo test` — HDROP path-list parsing (multi-file, unicode names, empty)
- Manual: Explorer copy → card → open round-trip
