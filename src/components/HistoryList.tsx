import { ClipboardCard } from "./ClipboardCard";
import { ImageCard } from "./ImageCard";
import { api } from "@/lib/api";
import { evictCachedImage } from "@/lib/imageCache";
import type { ClipboardItem } from "@/lib/types";

export function HistoryList({
  items,
  onMutate,
}: {
  items: ClipboardItem[];
  onMutate: () => void;
}) {
  const copy = (item: ClipboardItem) => {
    if (item.kind === "image") {
      api.copyImageToClipboard(item.content).then(onMutate).catch(console.error);
    } else {
      api.copyToClipboard(item.content).then(onMutate).catch(console.error);
    }
  };
  const pin = (item: ClipboardItem) =>
    api.togglePin(item.id).then(onMutate).catch(console.error);
  const remove = (item: ClipboardItem) => {
    if (item.kind === "image") evictCachedImage(item.content);
    api.deleteItem(item.id).then(onMutate).catch(console.error);
  };

  return (
    <div className="min-w-0 flex-1 overflow-x-hidden overflow-y-auto pr-1 [scrollbar-gutter:stable]">
      <div className="flex w-full min-w-0 flex-col gap-2.5 pb-2">
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
