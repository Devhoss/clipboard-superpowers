import { useEffect, useState } from "react";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { CategoryLabel } from "@/components/CategoryFilter";
import { CardAction } from "@/components/CardShared";
import { formatBytes, formatTime } from "@/lib/format";
import { getCachedFileMeta } from "@/lib/metaCache";
import type { ClipboardItem } from "@/lib/types";
import { Copy, File as FileIcon, FolderOpen, Pin, PinOff, Trash2 } from "lucide-react";

function fileName(p: string): string {
  const seg = p.split(/[/\\]/).filter(Boolean);
  return seg.length > 0 ? seg[seg.length - 1] : p;
}

// File card (PR5). content is the newline-joined path list (stored, not
// copied — see clipboard.rs). Click copies the paths as text through the
// existing copy path; the folder action opens without touching history.
export function FileCard({
  item,
  onCopy,
  onPin,
  onDelete,
}: {
  item: ClipboardItem;
  onCopy: () => void;
  onPin: () => void;
  onDelete: () => void;
}) {
  const paths = item.content.split("\n").filter(Boolean);
  const [meta, setMeta] = useState<{
    total: string;
    missing: number;
    gone: boolean;
    dirs: number;
    files: number;
  } | null>(null);

  useEffect(() => {
    let live = true;
    getCachedFileMeta(item.content)
      .then((m) => {
        if (!live) return;
        setMeta({
          total: formatBytes(m.total_bytes),
          missing: m.missing.length,
          gone: m.existing === 0,
          dirs: m.dirs,
          files: m.existing - m.dirs,
        });
      })
      .catch(() => {
        if (live) setMeta(null);
      });
    return () => {
      live = false;
    };
  }, [item.content]);

  const open = () => {
    // Single file opens with its app; several reveal the first in Explorer.
    // CardAction already stops propagation, so this never also copies.
    const p = paths.length === 1 ? openPath(paths[0]) : revealItemInDir(paths[0]);
    p.catch(console.error);
  };

  const shown = paths.slice(0, 5);
  const rest = paths.length - shown.length;

  return (
    <div
      className="group min-w-0 cursor-pointer rounded-xl border border-border/80 bg-card/85 p-3 shadow-sm transition-[border-color,box-shadow,background-color] duration-200 hover:border-primary/40 hover:shadow-md"
      onClick={onCopy}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onCopy();
        }
      }}
      role="button"
      tabIndex={0}
      title="Click to copy paths"
    >
      <div className="flex min-w-0 items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          {item.pinned && <Pin className="size-3 text-amber-400" />}
          <CategoryLabel category={item.category} />
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          <span className="mr-1 text-[10px] text-muted-foreground">{formatTime(item.created_at)}</span>
          <CardAction label="Copy" onClick={onCopy}><Copy /></CardAction>
          <CardAction label={paths.length === 1 ? "Open file" : "Reveal in Explorer"} onClick={open}>
            <FolderOpen />
          </CardAction>
          <CardAction label={item.pinned ? "Unpin" : "Pin"} onClick={onPin}>
            {item.pinned ? <PinOff /> : <Pin />}
          </CardAction>
          <CardAction label="Delete" onClick={onDelete} destructive><Trash2 /></CardAction>
        </div>
      </div>
      <div className="mt-2 space-y-1">
        {shown.map((p) => (
          <div key={p} className="flex min-w-0 items-center gap-1.5 text-[13px]">
            <FileIcon className="size-3.5 shrink-0 text-muted-foreground" />
            <span className="truncate font-medium">{fileName(p)}</span>
          </div>
        ))}
        {rest > 0 && (
          <div className="pl-5 text-[11px] text-muted-foreground">+{rest} more</div>
        )}
      </div>
      <div className="mt-1.5 pl-5 text-[11px] text-muted-foreground">
        {/* Folder stub sizes never reach the total (backend excludes them):
            all-folders shows no size, mixed shows file bytes + folder count. */}
        {meta === null ? (
          `${paths.length} file${paths.length === 1 ? "" : "s"}…`
        ) : meta.gone ? (
          <span className="text-destructive/90">Moved or deleted</span>
        ) : meta.dirs > 0 && meta.files === 0 ? (
          <>
            {paths.length} folder{paths.length === 1 ? "" : "s"}
            {meta.missing > 0 && ` · ${meta.missing} missing`}
          </>
        ) : (
          <>
            {paths.length} file{paths.length === 1 ? "" : "s"} · {meta.total}
            {meta.dirs > 0 && ` · ${meta.dirs} folder${meta.dirs === 1 ? "" : "s"}`}
            {meta.missing > 0 && ` · ${meta.missing} missing`}
          </>
        )}
      </div>
    </div>
  );
}
