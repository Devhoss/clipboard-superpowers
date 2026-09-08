# PR6 — OCR for images (spec)

Lane: `feat/ocr-images` → `master`. Status: **spec only, implementation follows.**

## Goal

Pull text out of image cards ("Extract text" action → new text item).

## Scope (deliberately small)

- **Local OCR only. No cloud calls, ever** — screenshots contain secrets
- Engine TBD by spike: `tesseract` bindings vs `ocrs`/`ort` ONNX. Spike criteria:
  compile on Windows without a C toolchain nightmare, <100MB binary growth, English-first
- New command `ocr_image(path) -> String`; result inserted as a normal text item
  (categorized, searchable) linked after the source image
- UI: one "Extract text" button on image cards + spinner state; failure shows inline error on the card
- If the spike fails (no engine compiles cleanly): lane converts to RFC with findings, **no half-wired dep ships**

## Acceptance criteria

- [ ] Screenshot of terminal text → Extract → text item with ≥90% readable words
- [ ] Works offline (Wi-Fi off test)
- [ ] Binary size delta recorded in PR body

## Test plan

- `cargo test` — command error paths (missing file, corrupt PNG), empty-result handling
- Manual: screenshot → extract → search round-trip
