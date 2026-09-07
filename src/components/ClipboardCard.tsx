import { CategoryLabel } from "@/components/CategoryFilter";
import { CardAction } from "@/components/CardShared";
import { cn } from "@/lib/utils";
import { extractColor, formatTime } from "@/lib/format";
import type { ClipboardItem } from "@/lib/types";
import { Copy, Pin, PinOff, Trash2 } from "lucide-react";

export function ClipboardCard({
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
  const isColor = item.category === "color";
  const colorValue = isColor ? extractColor(item.content) : null;
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
      title="Click to copy"
    >
      <div className="flex min-w-0 items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-1.5">
          {item.pinned && <Pin className="size-3 text-amber-400" />}
          <CategoryLabel category={item.category} dot={colorValue} />
        </div>
        <div className="flex shrink-0 items-center gap-0.5">
          <span className="mr-1 text-[10px] text-muted-foreground">{formatTime(item.created_at)}</span>
          <CardAction label="Copy" onClick={onCopy}><Copy /></CardAction>
          <CardAction label={item.pinned ? "Unpin" : "Pin"} onClick={onPin}>
            {item.pinned ? <PinOff /> : <Pin />}
          </CardAction>
          <CardAction label="Delete" onClick={onDelete} destructive><Trash2 /></CardAction>
        </div>
      </div>
      {item.html ? (
        <div
          aria-label="Formatted preview"
          className="rich-preview mt-2 line-clamp-3 break-words text-[13px] leading-relaxed"
          // Backend-sanitized (ammonia allowlist, scripts/handlers stripped).
          // Never render raw clipboard HTML here — see richtext.rs.
          dangerouslySetInnerHTML={{ __html: item.html.slice(0, 2000) }}
        />
      ) : (
        <pre
          className={cn(
            "mt-2 line-clamp-3 whitespace-pre-wrap break-words text-[13px] leading-relaxed",
            isColor || item.category === "code" ? "font-mono" : "font-sans font-medium",
          )}
        >
          {item.content.slice(0, 500)}
          {item.content.length > 500 ? "…" : ""}
        </pre>
      )}
    </div>
  );
}
