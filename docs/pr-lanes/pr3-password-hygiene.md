# PR3 — Password / OTP hygiene (spec)

Lane: `feat/password-hygiene` → `master`. Status: **spec only, implementation follows.**
Independent of PR1/PR2 — no shared files beyond `settings.rs` conventions.

## Goal

Secrets never linger in history. Default: skip them entirely.

## Scope (deliberately small)

- `src-tauri/src/categorize.rs` — `is_secret(text) -> bool`:
  - `password=`, `passwd`, `pwd`, `api_key`, `apikey`, `secret`, `token=` patterns (case-insensitive, `key=value` shape)
  - OTP shape: standalone 4–8 digits
  - PEM/private-key headers (`-----BEGIN ... PRIVATE KEY-----`)
- `src-tauri/src/settings.rs` — `skip_secrets: bool` (default **true**), surfaced in SettingsPanel Capture section
- `src-tauri/src/clipboard.rs` — poller drops secrets before DB write when `skip_secrets`
- If `skip_secrets = false`: secrets are captured but auto-expire after 60s via prune (no new setting, fixed TTL)
- Do NOT touch: copy paths, paste (PR2), UI beyond one toggle

## Acceptance criteria

- [ ] Copy `password=hunter2` with default settings → no new card, no DB row
- [ ] Toggle off → secret captured, gone after 60s prune tick
- [ ] OTP `482910` skipped by default
- [ ] Normal code/URLs/emails unaffected (no false positives on `tokenize()` etc.)

## Test plan

- `cargo test` — `is_secret` true/false matrix (incl. near-miss negatives), settings default `skip_secrets == true`
- Manual: copy samples from spec, watch history
