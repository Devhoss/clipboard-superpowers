import { ClipboardCard } from "./ClipboardCard";
import { ImageCard } from "./ImageCard";
import { api } from "@/lib/api";
import { evictCachedImage } from "@/lib/imageCache";
import type { ClipboardItem } from "@/lib/types";

export function HistoryList({
  items,
  onMutate,
  onCopyMove,
}: {
  items: ClipboardItem[];
  onMutate: () => void;
  onCopyMove: (item: ClipboardItem) => void;
}) {
  const copy = (item: ClipboardItem) => {
    // Optimistic: card jumps to top synchronously. The backend bump + live
    // event follow and dedupe by id — no full refresh waits on a click.
    onCopyMove(item);
    const p =
      item.kind === "image"
        ? api.copyImageToClipboard(item.content)
        : api.copyToClipboard(item.content);
    p.catch(console.error);
  };
  const pin = (item: ClipboardItem) =>
    api.togglePin(item.id).then(onMutate).catch(console.error);
  const remove = (item: ClipboardItem) => {
    if (item.kind === "image") evictCachedImage(item.content);
    api.deleteItem(item.id).then(onMutate).catch(console.error);
  };

  return (
    <div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto pr-1 [scrollbar-gutter:stable]">
      <div className="flex w-full min-w-0 flex-col gap-2.5 px-0.5 pt-0.5 pb-2">
        {items.map((item) =>
          item.kind === "image" ? (
            <ImageCard
              key={item.id}
              item={item}
              onCopy={() => copy(item)}
              onPin={() => pin(item)}
              onDelete={() => remove(item)}
            />
          ) : (
            <ClipboardCard
              key={item.id}
              item={item}
              onCopy={() => copy(item)}
              onPin={() => pin(item)}
              onDelete={() => remove(item)}
            />
          ),
        )}
      </div>
    </div>
  );
}
