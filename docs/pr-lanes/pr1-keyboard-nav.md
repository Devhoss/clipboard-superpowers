# PR1 — Full keyboard control (spec)

Lane: `feat/keyboard-nav` → `master`. Status: **spec only, implementation follows.**

## Goal

Drive the whole popup without a mouse: move selection, copy, delete.

## Scope (deliberately small)

- `src/App.tsx` — `selectedIndex` state, global key handler:
  - `ArrowDown` / `ArrowUp` (and `j`/`k` when search is empty?) — move selection
  - `Enter` — copy selected card (existing `api.copyToClipboard` / `copyImageToClipboard` path)
  - `Delete` / `Backspace` — delete selected card (existing `api.deleteItem` path)
  - `Escape` keeps current behavior (settings → back, list → hide)
- `src/components/HistoryList.tsx` — selected card gets visible ring, `scrollIntoView({ block: "nearest" })` on selection change
- Reset `selectedIndex` to 0 on new fetch (`fetchFor`), clamp on list shrink
- Do NOT touch: paste-directly (PR2), pins UI, settings

## Acceptance criteria

- [ ] Summon popup, press Down ×3 → 4th card highlighted, visible without manual scroll
- [ ] Enter copies highlighted card (same path as click, incl. optimistic move-to-top)
- [ ] Del removes highlighted card, selection clamps to list end
- [ ] Works with active search filter (index maps to filtered rows)
- [ ] Esc still hides from list view

## Test plan

- `npm test` — new `keyboardNav.test.ts` for index reducer (clamp/wrap/reset)
- `tsc --noEmit` clean
- Manual: `npm run tauri dev`, summon with `Ctrl+Alt+V`, keyboard-only run

## Why this PR is first

Enter-to-paste (PR2) needs a selected card. This PR creates the selection model PR2 reuses.
