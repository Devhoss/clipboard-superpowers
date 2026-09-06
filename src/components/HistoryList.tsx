import { ClipboardCard } from "./ClipboardCard";
import { ImageCard } from "./ImageCard";
import { api } from "@/lib/api";
import type { ClipboardItem } from "@/lib/types";
import { ScrollArea } from "@/components/ui/scroll-area";

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
  const remove = (item: ClipboardItem) =>
    api.deleteItem(item.id).then(onMutate).catch(console.error);

  return (
    <ScrollArea className="flex-1 min-h-0">
      <div className="flex flex-col gap-2 pr-2">
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
    </ScrollArea>
  );
}
