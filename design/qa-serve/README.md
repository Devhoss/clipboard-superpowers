# QA harness

`mock-tauri.js` stubs `window.__TAURI_INTERNALS__` with sample history so the
real built frontend can be screenshot-tested in a plain browser.

Regenerate + run:

    npm run build
    cp -r dist/* design/qa-serve/
    # patch index.html: insert <script src="/mock-tauri.js"></script> after <head>
    npx http-server design/qa-serve -p 8123
    chrome --headless --screenshot=... http://localhost:8123/
