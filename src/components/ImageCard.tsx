import { useEffect, useState } from "react";
import { CardAction } from "@/components/CardShared";
import { formatTime } from "@/lib/format";
import { getCachedImage } from "@/lib/imageCache";
import type { ClipboardItem } from "@/lib/types";
import { Copy, ImageIcon, Pin, PinOff, Trash2 } from "lucide-react";

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
    setSrc(null);
    setFailed(false);
    getCachedImage(item.content)
      .then((url) => {
        if (!cancelled) setSrc(url);
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
      title="Click to copy image"
    >
      <div className="flex min-w-0 items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          {item.pinned && <Pin className="size-3 text-amber-400" />}
          <ImageIcon className="size-3 text-emerald-400" />
          <span className="text-[10px] font-medium uppercase tracking-wider text-emerald-400">image</span>
        </div>
        <div className="flex items-center gap-0.5">
          <span className="mr-1 text-[10px] text-muted-foreground">{formatTime(item.created_at)}</span>
          <CardAction label="Copy image" onClick={onCopy}><Copy /></CardAction>
          <CardAction label={item.pinned ? "Unpin image" : "Pin image"} onClick={onPin}>
            {item.pinned ? <PinOff /> : <Pin />}
          </CardAction>
          <CardAction label="Delete image" onClick={onDelete} destructive><Trash2 /></CardAction>
        </div>
      </div>
      <div className="relative mt-2 flex h-36 items-center justify-center overflow-hidden rounded-lg border border-border/60 bg-muted/70">
        {src ? (
          <>
            <img
              src={src}
              alt=""
              aria-hidden="true"
              className="absolute inset-0 h-full w-full scale-110 object-cover opacity-20 blur-2xl"
            />
            <img
              src={src}
              alt="Clipboard image"
              className="relative h-full w-full object-contain p-1.5 drop-shadow-md"
            />
          </>
        ) : failed ? (
          <span className="text-xs text-muted-foreground">image unavailable</span>
        ) : (
          <span className="text-xs text-muted-foreground">loading…</span>
        )}
      </div>
    </div>
  );
}
