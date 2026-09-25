# PR3 — Password / OTP hygiene (spec)

Lane: `feat/password-hygiene` → `master`. Status: **implemented and verified.**
Independent of PR1/PR2 — no shared files beyond `settings.rs` conventions.

## Goal

Secrets never reach the renderer. Default: skip them entirely. When skipping is off, keep them locally but hidden until the setting is enabled again.

## Scope (deliberately small)

- `src-tauri/src/categorize.rs` — `is_secret(text) -> bool`:
  - `password=`, `passwd`, `pwd`, `api_key`, `apikey`, `secret`, `token=` patterns (case-insensitive, `key=value` shape)
  - OTP shape: standalone 4–8 digits
  - PEM/private-key headers (`-----BEGIN ... PRIVATE KEY-----`)
- `src-tauri/src/settings.rs` — `skip_secrets: bool` (default **true**), surfaced in SettingsPanel Capture section
- `src-tauri/src/clipboard.rs` — poller stores secrets internally when `skip_secrets` is off; the renderer never receives them
- If `skip_secrets = false`: secret captures are retained in the database but hidden from the renderer, search, copy/paste, stats, and public mutations. They are removed when the setting is enabled again.
- Do NOT touch: copy paths, paste (PR2), UI beyond the redaction state and one toggle

## Acceptance criteria

- [x] Copy `password=hunter2` with default settings → no new card, no DB row
- [x] Toggle off → secret is retained but absent from all renderer-facing reads/actions
- [x] OTP `482910` skipped by default
- [x] Normal code/URLs/emails unaffected (no false positives on `tokenize()` etc.)
- [x] Search accepts `date:YYYY-MM-DD` alone or AND-combined with text
- [x] Two date tokens: first wins, second stays literal text (documented in `parse_search_query`, covered by test)
- [x] Re-enabling **Skip passwords & OTPs** purges retained secrets and overwrites their bytes on disk

## Test plan

- `cargo test` — `is_secret` true/false matrix (incl. near-miss negatives), settings default `skip_secrets == true`
- `cargo test` — date search uses the local calendar day and excludes secret rows
- `cargo test` — purge zeroes secret bytes in the raw `clipboard.db` + WAL, not just the query result
- `cargo test` — "Clear everything" and "Clear unpinned" both call the shipped `clear_visible_history` SQL and leave retained secrets (incl. pinned) behind; verified RED when the `category <> 'secret'` clause is removed
- `cargo test` — visible counts exclude a *pinned* secret (pinned via raw SQL, since `toggle_pin` refuses secrets by design); verified RED when the filter is removed
- `cargo test` — two date tokens: first wins, second stays literal text (documented in `parse_search_query`)
- `npm test` — Actions shortcut accepts the focused search box while rejecting other inputs; secret rows render no content/actions
- Manual: copy samples from spec, watch history
