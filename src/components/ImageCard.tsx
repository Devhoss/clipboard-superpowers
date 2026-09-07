import { useEffect, useState } from "react";
import { CardAction } from "@/components/CardShared";
import { CategoryLabel } from "@/components/CategoryFilter";
import { formatTime } from "@/lib/format";
import { api } from "@/lib/api";
import { getCachedImage } from "@/lib/imageCache";
import type { ClipboardItem } from "@/lib/types";
import { Copy, Loader2, Pin, PinOff, ScanText, Trash2 } from "lucide-react";

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
  // PR6: extraction runs in the backend; the new text item arrives through
  // the live new-item event, so this card only tracks busy/error.
  const [ocrBusy, setOcrBusy] = useState(false);
  const [ocrError, setOcrError] = useState<string | null>(null);

  const extract = () => {
    if (ocrBusy) return;
    setOcrBusy(true);
    setOcrError(null);
    api
      .ocrImage(item.content)
      .catch((e: unknown) => setOcrError(e instanceof Error ? e.message : String(e)))
      .finally(() => setOcrBusy(false));
  };

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
          <CategoryLabel category={item.category} />
        </div>
        <div className="flex items-center gap-0.5">
          <span className="mr-1 text-[10px] text-muted-foreground">{formatTime(item.created_at)}</span>
          <CardAction label="Copy image" onClick={onCopy}><Copy /></CardAction>
          <CardAction label="Extract text" onClick={extract}>
            {ocrBusy ? <Loader2 className="animate-spin" /> : <ScanText />}
          </CardAction>
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
      {ocrBusy && (
        <div className="mt-2 text-[11px] text-muted-foreground">Reading text…</div>
      )}
      {ocrError && (
        <div className="mt-2 truncate text-[11px] text-destructive" title={ocrError}>
          Couldn&apos;t read text: {ocrError}
        </div>
      )}
    </div>
  );
}
