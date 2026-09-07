# PR2 — Enter-to-paste directly (spec)

Lane: `feat/paste-directly` → `master`. Status: **spec only, implementation follows.**
Stacks on: PR1 (needs the selection model).

## Goal

Enter copies the selected card **and** pastes it into the previously focused app.

## Scope (deliberately small)

- `src-tauri/Cargo.toml` — add `enigo` (boring, proven key simulation)
- `src-tauri/src/commands.rs` — new `paste_to_previous_app(text: String)` command:
  1. copy text to clipboard (reuse `write_with_retry` + suppress-hash path)
  2. hide popup window
  3. sleep ~150ms (let focus return to previous app)
  4. simulate `Ctrl+V` via enigo
- `src/App.tsx` — Enter on selected card calls copy-then-paste instead of copy-only
- Text only in v1. Images/HTML stay copy-only (their lanes own that)
- Do NOT touch: history, settings, categories

## Acceptance criteria

- [ ] Select card → Enter → popup hides → text appears in previously focused app (Notepad test)
- [ ] Plain click still copy-only (no behavior change)
- [ ] Failure to simulate keys surfaces an error, never silently drops the copy

## Test plan

- `cargo test` — new tests for the paste sequencing helper (pure parts only; OS key simulation is manual)
- Manual: Notepad + VS Code paste runs

## Open risk

`enigo` must compile on Windows CI-less repo — if it won't, lane stops with the exact error in the draft and PR3 starts.
