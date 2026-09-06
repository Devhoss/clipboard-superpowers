import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import type { ClipboardItem } from "@/lib/types";
import { Copy, Pin, PinOff, Trash2 } from "lucide-react";

export function ImageCard({
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
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api
      .readImageBase64(item.content)
      .then((b64) => {
        if (!cancelled) setSrc(`data:image/png;base64,${b64}`);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [item.content]);

  return (
    <div
      className="group rounded-lg border bg-card p-2.5 transition-colors hover:border-primary/40 cursor-pointer"
      onClick={onCopy}
      title="Click to copy image"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          {item.pinned && <Pin className="size-3 text-amber-400" />}
          <span className="text-[10px] uppercase tracking-wider text-green-400">image</span>
        </div>
        <span className="text-[10px] text-muted-foreground">
          {formatTime(item.created_at)}
        </span>
      </div>
      <div className="mt-1.5 flex h-20 items-center justify-center overflow-hidden rounded bg-muted">
        {src ? (
          <img src={src} alt="clipboard image" className="max-h-20 max-w-full object-contain" />
        ) : failed ? (
          <span className="text-xs text-muted-foreground">image unavailable</span>
        ) : (
          <span className="text-xs text-muted-foreground">loading…</span>
        )}
      </div>
      <div className="mt-1.5 flex gap-1 opacity-0 transition-opacity group-hover:opacity-100">
        <Button
          size="sm"
          variant="secondary"
          className="h-6 px-2 text-[11px]"
          onClick={(e) => {
            e.stopPropagation();
            onCopy();
          }}
        >
          <Copy className="size-3" /> Copy
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="h-6 px-2 text-[11px]"
          onClick={(e) => {
            e.stopPropagation();
            onPin();
          }}
        >
          {item.pinned ? <PinOff className="size-3" /> : <Pin className="size-3" />}
          {item.pinned ? "Unpin" : "Pin"}
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="ml-auto h-6 px-2 text-[11px] text-destructive hover:text-destructive"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
        >
          <Trash2 className="size-3" />
        </Button>
      </div>
    </div>
  );
}

function formatTime(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
