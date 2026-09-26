# QA harness

`mock-tauri.js` stubs `window.__TAURI_INTERNALS__` with sample history so the
real built frontend can be screenshot-tested in a plain browser.

## Run it

    npm run qa

Then open <http://localhost:8123/> and screenshot it, e.g.

    chrome --headless --disable-gpu --hide-scrollbars \
      --virtual-time-budget=9000 --window-size=820,560 \
      --screenshot=qa.png http://localhost:8123/

`npm run qa` builds, copies `dist/` and this mock into `site/`, injects the
`<script src="/mock-tauri.js">` tag into the **copy**, and serves it on a
stdlib-only static server. Nothing outside `site/` is written, so the working
tree stays clean. `site/` is gitignored.

Useful flags:

    npm run qa -- --port 9000     # different port
    npm run qa -- --no-build      # serve the existing dist as-is

## Why the injection is automated

This used to be a hand edit of `dist/index.html` (see git history). Two
problems: it was easy to leave in place, and a stale injected `index.html` in
`dist/` silently served the mock even when you wanted the real thing. The tag
is now added to a throwaway copy, so `dist/index.html` is always pristine.

## Gotcha that cost an hour

`mock-tauri.js` once called `mk_app_code(...)`, a helper that was defined
nowhere in the file. The IIFE threw on load, `__TAURI_INTERNALS__` was never
assigned, and the app rendered an **empty history** rather than erroring —
which looks exactly like "the app has no entries."

If the page comes up empty, check the console before touching app code:

    chrome --headless --enable-logging=stderr --v=0 --dump-dom http://localhost:8123/
