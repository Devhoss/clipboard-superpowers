import { Button } from "@/components/ui/button";
import { categoryChipClass } from "@/components/CategoryFilter";
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
      className="group rounded-lg border bg-card p-2.5 transition-colors hover:border-primary/40 cursor-pointer"
      onClick={onCopy}
      title="Click to copy"
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5">
          {item.pinned && <Pin className="size-3 text-amber-400" />}
          <span className={categoryChipClass(item.category)}>{item.category}</span>
          {colorValue && (
            <span
              className="inline-block size-3 rounded-sm border"
              style={{ background: colorValue }}
            />
          )}
        </div>
        <span className="text-[10px] text-muted-foreground">
          {formatTime(item.created_at)}
        </span>
      </div>
      <pre className="mt-1.5 max-h-20 overflow-hidden whitespace-pre-wrap break-words font-mono text-xs leading-snug">
        {item.content.slice(0, 500)}
        {item.content.length > 500 ? "…" : ""}
      </pre>
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

function extractColor(content: string): string | null {
  const m = content.trim().match(/^(#[0-9a-fA-F]{3,8}|rgb\([^)]+\))$/);
  return m ? m[1] : null;
}

function formatTime(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
