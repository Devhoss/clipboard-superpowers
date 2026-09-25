import { useEffect, useMemo, useState } from "react";
import { api } from "@/lib/api";
import { CATEGORY_META } from "@/lib/categories";
import { tokenizeCode, type TokenKind } from "@/lib/highlight";
import { dayLabel, extractColor, formatBytes, formatTime } from "@/lib/format";
import { getCachedImage } from "@/lib/imageCache";
import { getCachedFileMeta } from "@/lib/metaCache";
import type { ClipboardItem } from "@/lib/types";
import { Link2, Mail, Pin, PinOff, Trash2, Eye, EyeOff } from "lucide-react";
import { FolderOpen, ExternalLink } from "lucide-react";

function fileName(p: string): string {
  const seg = p.split(/[/\\]/).filter(Boolean);
  return seg.length > 0 ? seg[seg.length - 1] : p;
}

function hostOrUrl(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

const TOKEN_CLASS: Record<TokenKind, string> = {
  kw: "tok-kw",
  fn: "tok-fn",
  str: "tok-str",
  num: "tok-num",
  cm: "tok-cm",
};

/** Syntax-highlighted code stage (mock v2 colors). Tokenizing is lazy — only
 * the selected clip, only when rendered — and never touches copy-back.
 * max-h-full: the block must shrink to the stage's content box (which
 * reserves the action-button/hint bands) — a centered block taller than the
 * stage spills into those bands and underlaps the absolute UI. */
function StageCode({ code }: { code: string }) {
  const lines = useMemo(() => tokenizeCode(code), [code]);
  return (
    <pre className="h-full max-h-full w-full max-w-[440px] overflow-auto rounded-xl bg-card p-4 font-mono text-[12px] leading-relaxed shadow-[0_0_0_1px_var(--border),0_12px_32px_rgba(0,0,0,0.10)]">
      <code className="whitespace-pre">
        {lines.map((tokens, li) => (
          <span key={li}>
            {tokens.map((t, ti) =>
              t.kind ? (
                <span key={ti} className={TOKEN_CLASS[t.kind]}>
                  {t.text}
                </span>
              ) : (
                <span key={ti}>{t.text}</span>
              ),
            )}
            {"\n"}
          </span>
        ))}
      </code>
    </pre>
  );
}

/** Render a category in the big stage. Secret rows never reach this
 * boundary: `DetailPane` returns a redaction state before rendering it. */
function Stage({
  item,
  onOpenUrl,
  onOpenPath,
}: {
  item: ClipboardItem;
  onOpenUrl: (url: string) => void;
  onOpenPath: (path: string) => void;
}) {
  switch (item.category) {
    case "color": {
      const hex = extractColor(item.content) ?? item.content;
      return (
        <div className="text-center">
          <div
            aria-hidden="true"
            className="mx-auto mb-3.5 size-32 rounded-full"
            style={{
              background: hex,
              boxShadow: `0 12px 40px ${hex}59, inset 0 0 0 0.5px rgba(0,0,0,0.15)`,
            }}
          />
          <div className="font-mono text-[14px] font-medium tracking-wider">{hex}</div>
        </div>
      );
    }
    case "code":
      return <StageCode code={item.content} />;
    case "link":
      return (
        <div className="text-center">
          <div className="mx-auto mb-3 grid size-[60px] place-items-center rounded-2xl bg-primary/10 text-primary shadow-[0_12px_32px_rgba(10,132,255,0.2)]">
            <Link2 className="size-6" />
          </div>
          <div className="text-[15px] font-semibold">{hostOrUrl(item.content)}</div>
          <button
            type="button"
            title="Open in browser"
            onClick={() => onOpenUrl(item.content)}
            className="mt-0.5 max-w-[380px] break-all text-[12px] text-primary underline-offset-2 hover:underline"
          >
            {item.content}
          </button>
        </div>
      );
    case "email":
      return (
        <div className="text-center">
          <div className="mx-auto mb-3 grid size-[60px] place-items-center rounded-2xl bg-amber-400/15 text-amber-400">
            <Mail className="size-6" />
          </div>
          <div className="break-all font-mono text-[13px]">{item.content}</div>
        </div>
      );
    case "image":
      return <StageImage item={item} />;
    case "file": {
      const paths = item.content.split("\n").filter(Boolean);
      const shown = paths.slice(0, 8);
      return (
        <div className="w-[300px] max-w-full">
          {shown.map((p) => (
            <button
              key={p}
              type="button"
              title="Open in Explorer"
              onClick={() => onOpenPath(p)}
              className="flex min-w-0 w-full items-center gap-2.5 border-b border-border/50 py-2 text-left first:border-0"
            >
              <span className="shrink-0 rounded-lg bg-sky-400/15 px-1.5 py-1 font-mono text-[9px] font-bold text-sky-300">
                {((fileName(p).split(".").pop() ?? "?").toUpperCase()).slice(0, 4)}
              </span>
              <span className="min-w-0 truncate text-[13px] font-medium">{fileName(p)}</span>
            </button>
          ))}
          {paths.length > shown.length && (
            <div className="pt-2 text-[11.5px] text-muted-foreground">+{paths.length - shown.length} more</div>
          )}
          <StageFileFooter content={item.content} count={paths.length} />
        </div>
      );
    }
    case "secret":
      return (
        <div className="text-center text-[12.5px] text-muted-foreground">
          Secret hidden from history
        </div>
      );
    default:
      return (
        <div className="max-h-full max-w-[420px] overflow-auto whitespace-pre-wrap break-words text-[14px] leading-relaxed">
          {item.content}
        </div>
      );
  }
}

function StageImage({ item }: { item: ClipboardItem }) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const [dims, setDims] = useState<{ w: number; h: number } | null>(null);
  useEffect(() => {
    let cancelled = false;
    setSrc(null);
    setFailed(false);
    setDims(null);
    getCachedImage(item.content)
      .then((url) => !cancelled && setSrc(url))
      .catch(() => !cancelled && setFailed(true));
    return () => {
      cancelled = true;
    };
  }, [item.content]);

  if (failed) return <span className="text-[12.5px] text-muted-foreground">image unavailable</span>;
  if (!src) return <span className="text-[12.5px] text-muted-foreground">loading…</span>;
  return (
    <div className="relative">
      <img
        src={src}
        alt="Clipboard image"
        className="max-h-[220px] max-w-[380px] rounded-xl object-contain shadow-[0_20px_50px_rgba(0,0,0,0.35)]"
        onLoad={(e) => {
          const img = e.currentTarget;
          setDims({ w: img.naturalWidth, h: img.naturalHeight });
        }}
      />
      {dims && (
        <span className="absolute bottom-2 right-2 rounded-md bg-black/35 px-2 py-0.5 text-[10.5px] font-semibold text-white backdrop-blur">
          {dims.w} × {dims.h}
        </span>
      )}
    </div>
  );
}

function StageFileFooter({ content, count }: { content: string; count: number }) {
  const [total, setTotal] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    getCachedFileMeta(content)
      .then((m) => live && setTotal(formatBytes(m.total_bytes)))
      .catch(() => live && setTotal(null));
    return () => {
      live = false;
    };
  }, [content]);
  return (
    <div className="pt-1.5 text-[11px] text-muted-foreground">
      {count} path{count === 1 ? "" : "s"}
      {total && ` · ${total}`}
    </div>
  );
}

/** Right pane: big preview + Information key-value rows (v2 design).
 * Links/files open ONLY via the Open button (or their clickable stage rows) —
 * never implicitly, so browsing the list can't yank you to another window. */
export function DetailPane({
  item,
  onPin,
  onDelete,
  onOpen,
  onOpenUrl,
  onOpenPath,
  secretEpoch = 0,
}: {
  item: ClipboardItem | null;
  onPin: (item: ClipboardItem) => void;
  onDelete: (item: ClipboardItem) => void;
  onOpen: (item: ClipboardItem) => void;
  onOpenUrl: (url: string) => void;
  onOpenPath: (path: string) => void;
  /** Bumped by the parent on window blur; drops any revealed secret. */
  secretEpoch?: number;
}) {
  // List rows carry a 300-char preview; load the full row for the stage.
  const [full, setFull] = useState<ClipboardItem | null>(null);
  // Revealed secret content. Deliberately local state, never lifted into the
  // item list and never persisted. It is dropped on selection change AND by
  // the secretEpoch bump on window blur, so it cannot outlive the window.
  const [revealed, setRevealed] = useState<string | null>(null);
  const [revealBusy, setRevealBusy] = useState(false);
  useEffect(() => {
    setRevealed(null);
  }, [secretEpoch]);
  useEffect(() => {
    let cancelled = false;
    setFull(null);
    setRevealed(null);
    if (!item || item.category === "secret") return;
    api
      .getHistoryItem(item.id)
      .then((row) => !cancelled && setFull(row))
      .catch(() => !cancelled && setFull(null));
    return () => {
      cancelled = true;
    };
  }, [item?.id, item?.content_hash]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!item) {
    return (
      <div className="grid min-w-0 flex-1 place-items-center text-[12.5px] text-muted-foreground">
        No entry selected
      </div>
    );
  }

  if (item.category === "secret") {
    const meta2 = CATEGORY_META.secret;
    return (
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="relative grid min-h-0 flex-1 place-items-center overflow-hidden border-b border-border/60 bg-muted/30 px-5 pb-8 pt-12">
          <div className="absolute right-2.5 top-2.5 z-10 flex gap-1">
            <button
              type="button"
              title={revealed === null ? "Reveal secret" : "Hide secret"}
              aria-label={revealed === null ? "Reveal secret" : "Hide secret"}
              onClick={() => {
                if (revealed !== null) {
                  setRevealed(null);
                  return;
                }
                setRevealBusy(true);
                api
                  .revealSecret(item.id)
                  .then(setRevealed)
                  .catch((e) => console.error(e))
                  .finally(() => setRevealBusy(false));
              }}
              className="grid size-7 place-items-center rounded-lg bg-card/80 text-muted-foreground shadow-sm transition-colors hover:text-foreground"
            >
              {revealed === null ? <Eye className="size-3.5" /> : <EyeOff className="size-3.5" />}
            </button>
            <button
              type="button"
              title={item.pinned ? "Unpin" : "Pin"}
              aria-label={item.pinned ? "Unpin" : "Pin"}
              onClick={() => onPin(item)}
              className={`grid size-7 place-items-center rounded-lg bg-card/80 text-muted-foreground shadow-sm transition-colors hover:text-foreground ${
                item.pinned ? "text-amber-400" : ""
              }`}
            >
              {item.pinned ? <Pin className="size-3.5" /> : <PinOff className="size-3.5" />}
            </button>
            <button
              type="button"
              title="Delete"
              aria-label="Delete"
              onClick={() => onDelete(item)}
              className="grid size-7 place-items-center rounded-lg bg-card/80 text-muted-foreground shadow-sm transition-colors hover:bg-destructive/10 hover:text-destructive"
            >
              <Trash2 className="size-3.5" />
            </button>
          </div>
          {revealed === null ? (
            <div className="flex flex-col items-center gap-2 text-center">
              <div className="text-[12.5px] text-muted-foreground">
                {revealBusy ? "Revealing…" : "Secret hidden"}
              </div>
              <div className="text-[11px] text-muted-foreground/70">
                Use the eye button to show it
              </div>
            </div>
          ) : (
            <pre className="h-full max-h-full w-full max-w-[440px] overflow-auto whitespace-pre-wrap break-words rounded-xl bg-card p-4 font-mono text-[12px] leading-relaxed shadow-[0_0_0_1px_var(--border),0_12px_32px_rgba(0,0,0,0.10)]">
              {revealed}
            </pre>
          )}
          <div className="absolute bottom-2.5 left-3.5 truncate text-[10.5px] text-muted-foreground">
            {revealed === null
              ? "Hidden · re-hides when the window loses focus"
              : "Double-click a row to copy · Enter pastes into the previous app"}
          </div>
        </div>
        <div className="shrink-0 px-4 pb-3 pt-2.5">
          <div className="mb-0.5 text-[11px] font-semibold text-muted-foreground">Information</div>
          {(
            [
              ["Content Type", meta2?.single ?? "secret"],
              ["Copied", `${dayLabel(item.created_at)}, ${formatTime(item.created_at)}`],
              ...(item.source_app ? [["Application", item.source_app] as [string, string]] : []),
            ] as [string, React.ReactNode][]
          ).map(([k, v]) => (
            <div
              key={k}
              className="flex items-baseline justify-between gap-3 border-b border-border/50 py-[7px] text-[12.5px] last:border-0"
            >
              <span className="shrink-0 text-muted-foreground">{k}</span>
              <span className="min-w-0 truncate text-right">{v}</span>
            </div>
          ))}
        </div>
      </div>
    );
  }
  const view = full ?? item;
  const meta = CATEGORY_META[item.category];

  const infoRows: [string, React.ReactNode][] = [];
  if (view.source_app) {
    infoRows.push(["Application", view.source_app]);
  }
  infoRows.push(["Content Type", meta?.single ?? item.category]);
  infoRows.push(["Copied", `${dayLabel(item.created_at)}, ${formatTime(item.created_at)}`]);
  if (item.category === "plain" || item.category === "code") {
    infoRows.push(["Size", `${view.content.length.toLocaleString()} characters`]);
  }
  if (item.category === "file") {
    infoRows.push(["Size", `${view.content.split("\n").filter(Boolean).length} paths`]);
  }
  if (item.category === "color" || item.category === "link") {
    infoRows.push([
      item.category === "color" ? "Value" : "URL",
      <span className="font-mono text-[11.5px]" key="v">
        {view.content}
      </span>,
    ]);
  }

  const hint =
    item.category === "link"
        ? "Open opens the browser · double-click copies the URL"
        : item.kind === "file"
          ? "Open launches Explorer · double-click copies the path"
          : "Double-click a row to copy · Enter pastes into the previous app";

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      {/* pt/pb clear the absolute stage actions (top) and hint (bottom) so
          long lines can never render underneath them. */}
      <div className="relative grid min-h-0 flex-1 place-items-center overflow-hidden border-b border-border/60 bg-muted/30 px-5 pb-8 pt-12">
        <div className="absolute right-2.5 top-2.5 z-10 flex gap-1">
          {(item.category === "link" || item.kind === "file") && (
            <button
              type="button"
              title={item.category === "link" ? "Open in browser" : "Open / reveal in Explorer"}
              aria-label={item.category === "link" ? "Open in browser" : "Open in Explorer"}
              onClick={() => onOpen(item)}
              className="grid size-7 place-items-center rounded-lg bg-card/80 text-muted-foreground shadow-sm transition-colors hover:text-foreground"
            >
              {item.category === "link" ? (
                <ExternalLink className="size-3.5" />
              ) : (
                <FolderOpen className="size-3.5" />
              )}
            </button>
          )}
          <button
            type="button"
            title={item.pinned ? "Unpin" : "Pin"}
            aria-label={item.pinned ? "Unpin" : "Pin"}
            onClick={() => onPin(item)}
            className={`grid size-7 place-items-center rounded-lg bg-card/80 text-muted-foreground shadow-sm transition-colors hover:text-foreground ${
              item.pinned ? "text-amber-400" : ""
            }`}
          >
            {item.pinned ? <Pin className="size-3.5" /> : <PinOff className="size-3.5" />}
          </button>
          <button
            type="button"
            title="Delete"
            aria-label="Delete"
            onClick={() => onDelete(item)}
            className="grid size-7 place-items-center rounded-lg bg-card/80 text-muted-foreground shadow-sm transition-colors hover:bg-destructive/10 hover:text-destructive"
          >
            <Trash2 className="size-3.5" />
          </button>
        </div>
        <Stage item={view} onOpenUrl={onOpenUrl} onOpenPath={onOpenPath} />
        <div className="absolute bottom-2.5 left-3.5 truncate text-[10.5px] text-muted-foreground">
          {hint}
        </div>
      </div>
      <div className="shrink-0 px-4 pb-3 pt-2.5">
        <div className="mb-0.5 text-[11px] font-semibold text-muted-foreground">Information</div>
        {infoRows.map(([k, v]) => (
          <div
            key={k}
            className="flex items-baseline justify-between gap-3 border-b border-border/50 py-[7px] text-[12.5px] last:border-0"
          >
            <span className="shrink-0 text-muted-foreground">{k}</span>
            <span className="min-w-0 truncate text-right">{v}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
