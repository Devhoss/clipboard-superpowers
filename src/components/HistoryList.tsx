import { useEffect, useRef } from "react";
import { ClipboardCard } from "./ClipboardCard";
import { ImageCard } from "./ImageCard";
import type { ClipboardItem } from "@/lib/types";

// Presentational list (PR1). Card actions live in App so mouse and keyboard
// share one path — this component only renders, highlights, and scrolls.
export function HistoryList({
  items,
  selectedIndex,
  onCopy,
  onPin,
  onDelete,
  onHoverIndex,
}: {
  items: ClipboardItem[];
  selectedIndex: number;
  onCopy: (item: ClipboardItem) => void;
  onPin: (item: ClipboardItem) => void;
  onDelete: (item: ClipboardItem) => void;
  onHoverIndex: (index: number) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  // Keep the selected card in view while arrowing through the list.
  useEffect(() => {
    containerRef.current
      ?.querySelector(`[data-index="${selectedIndex}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  return (
    <div
      ref={containerRef}
      className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto pr-1 [scrollbar-gutter:stable]"
    >
      <div className="flex w-full min-w-0 flex-col gap-2.5 px-0.5 pt-0.5 pb-2">
        {items.map((item, i) => (
          <div
            key={item.id}
            data-index={i}
            onMouseEnter={() => onHoverIndex(i)}
            className={
              i === selectedIndex ? "rounded-xl ring-1 ring-primary/80" : undefined
            }
          >
            {item.kind === "image" ? (
              <ImageCard
                item={item}
                onCopy={() => onCopy(item)}
                onPin={() => onPin(item)}
                onDelete={() => onDelete(item)}
              />
            ) : (
              <ClipboardCard
                item={item}
                onCopy={() => onCopy(item)}
                onPin={() => onPin(item)}
                onDelete={() => onDelete(item)}
              />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
