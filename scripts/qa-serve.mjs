// QA harness runner. One command instead of the four manual steps in
// design/qa-serve/README.md:
//
//   1. vite build
//   2. copy dist/* into the serve dir
//   3. inject <script src="/mock-tauri.js"> into the served index.html
//   4. serve it and print the URL
//
// Step 3 used to be a hand edit of index.html, which dirtied the working tree
// and was easy to leave in place. The injection happens on the COPY, so the
// real dist/index.html and the real index.html are never touched.
//
// Static server is Node stdlib only — no new dependency to install offline.
//
//   node scripts/qa-serve.mjs [--port 8123] [--no-build]

import { createServer } from "node:http";
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SITE = join(ROOT, "design", "qa-serve", "site");
const MOCK = join(ROOT, "design", "qa-serve", "mock-tauri.js");

const argv = process.argv.slice(2);
const flag = (name) => {
  const i = argv.indexOf(`--${name}`);
  return i === -1 ? null : argv[i + 1];
};
const PORT = Number(flag("port") ?? 8123);
const SKIP_BUILD = argv.includes("--no-build");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".webp": "image/webp",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
  ".map": "application/json; charset=utf-8",
};

if (!SKIP_BUILD) {
  process.stdout.write("building… ");
  const r = spawnSync(
    process.execPath,
    [join(ROOT, "node_modules", "vite", "bin", "vite.js"), "build"],
    { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"], encoding: "utf8" },
  );
  if (r.status !== 0) {
    process.stdout.write("FAILED\n");
    process.stderr.write(`${r.stdout ?? ""}\n${r.stderr ?? ""}\n`);
    process.exit(r.status ?? 1);
  }
  process.stdout.write("ok\n");
}

const dist = join(ROOT, "dist");
if (!existsSync(join(dist, "index.html"))) {
  process.stderr.write("no dist/index.html — run without --no-build\n");
  process.exit(1);
}

rmSync(SITE, { recursive: true, force: true });
mkdirSync(SITE, { recursive: true });
cpSync(dist, SITE, { recursive: true });
cpSync(MOCK, join(SITE, "mock-tauri.js"));

// Inject into the copy only. The tag must precede the app bundle, since the
// app checks for __TAURI_INTERNALS__ at module-eval time.
const indexPath = join(SITE, "index.html");
const html = readFileSync(indexPath, "utf8");
const tag = '<script src="/mock-tauri.js"></script>';
if (!html.includes(tag)) {
  const patched = html.includes("</head>")
    ? html.replace("</head>", `  ${tag}\n  </head>`)
    : html.replace(/<body[^>]*>/, (m) => `${tag}\n${m}`);
  writeFileSync(indexPath, patched);
}

const server = createServer((req, res) => {
  const url = new URL(req.url ?? "/", "http://localhost");
  let rel = decodeURIComponent(url.pathname);
  if (rel.endsWith("/")) rel += "index.html";
  const file = join(SITE, rel);
  // Contain path traversal: everything served must stay under SITE.
  if (!file.startsWith(SITE)) {
    res.writeHead(403).end("forbidden");
    return;
  }
  if (!existsSync(file)) {
    res.writeHead(404, { "content-type": "text/plain" }).end("not found");
    return;
  }
  res.writeHead(200, {
    "content-type": MIME[extname(file).toLowerCase()] ?? "application/octet-stream",
    "cache-control": "no-store",
  });
  res.end(readFileSync(file));
});

server.listen(PORT, "127.0.0.1", () => {
  process.stdout.write(
    `QA ready on http://localhost:${PORT}/  (mock injected · Ctrl-C to stop)\n`,
  );
});
