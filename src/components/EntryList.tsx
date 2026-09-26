import { useEffect, useRef } from "react";
import { dayLabel } from "@/lib/format";
import { CATEGORY_META } from "@/lib/categories";
import type { ClipboardItem } from "@/lib/types";
import { Pin } from "lucide-react";

function fileName(p: string): string {
  const seg = p.split(/[/\\]/).filter(Boolean);
  return seg.length > 0 ? seg[seg.length - 1] : p;
}

/** One-line sidebar label per category (v2 design: dot+hex, "Image", names…). */
function rowLabel(item: ClipboardItem): string {
  switch (item.category) {
    case "image":
      return "Image";
    case "file": {
      const paths = item.content.split("\n").filter(Boolean);
      const first = fileName(paths[0] ?? "");
      return paths.length > 1 ? `${first} +${paths.length - 1}` : first;
    }
    default:
      return item.content.replace(/\s+/g, " ").trim();
  }
}

/** Type marker. The design uses a 6px category dot rather than a 26px icon
 *  tile: the tile spent 26px of a 280px row on a glyph that repeated what the
 *  dot already encodes. Rows carry no type/timestamp text — that lives in the
 *  detail pane's Information block. Colour rows keep their swatch — there the
 *  swatch IS the data, not a category label. */
function RowIcon({ item }: { item: ClipboardItem }) {
  if (item.category === "color") {
    return (
      <span aria-hidden="true" className="grid size-3 shrink-0 place-items-center">
        <span
          className="size-2.5 rounded-full shadow-[inset_0_0_0_0.5px_rgba(0,0,0,0.12)]"
          style={{ background: item.content.trim() }}
        />
      </span>
    );
  }
  return (
    <span
      aria-hidden="true"
      className="size-1.5 shrink-0 rounded-full"
      style={{ background: CATEGORY_META[item.category].dot }}
    />
  );
}

/** Left pane: compact rows grouped under Today / Yesterday / date headers.
 * Single click only SELECTS (preview on the right) — actions are explicit:
 * double-click copies (paths as text for file rows), the toolbar/Enter paste,
 * and links/files open via the detail pane's Open button. */
export function EntryList({
  items,
  selectedId,
  scrollKey,
  totalCount,
  visibleCount,
  onShowMore,
  onSelect,
  onDblCopy,
}: {
  items: ClipboardItem[];
  selectedId: number | null;
  /** Bumped by keyboard moves — scrolls the selected row into view. */
  scrollKey: number;
  totalCount: number;
  visibleCount: number;
  onShowMore: () => void;
  onSelect: (id: number) => void;
  onDblCopy: (item: ClipboardItem) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollKey <= 0) return;
    containerRef.current
      ?.querySelector(`[data-id="${selectedId}"]`)
      ?.scrollIntoView({ block: "nearest" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollKey]);

  // Secret rows are ordinary visible rows now; the backend already redacts
  // their preview. No component-level filtering.
  const safeItems = items;
  const safeTotal = totalCount;
  let lastDay = "";
  return (
    <div
      ref={containerRef}
      /* Scales gently instead of staying stubby at 1100px, while the detail
         pane still takes every extra pixel — 36:64 at the 820px default.
         The black layer is the dark half of the two-tone split: the overview
         side is lifted, the list side stays down. Denser content wants the
         darker surface. */
      className="w-[clamp(280px,33%,360px)] shrink-0 overflow-y-auto border-r border-[var(--glass-edge)] bg-black/45 px-2 pb-2 [scrollbar-width:thin]"
      role="listbox"
      aria-label="Clipboard entries"
    >
      {safeItems.map((item) => {
        const day = dayLabel(item.created_at);
        const header = day !== lastDay ? day : null;
        lastDay = day;
        return (
          <div key={item.id}>
            {header && (
              <div className="px-2 pb-1 pt-2.5 text-[11px] font-semibold text-muted-foreground">
                {header}
              </div>
            )}
            <div
              data-id={item.id}
              role="option"
              aria-selected={item.id === selectedId}
              tabIndex={-1}
              title="Double-click to copy"
              onClick={() => onSelect(item.id)}
              onDoubleClick={() => onDblCopy(item)}
              className={`flex min-w-0 cursor-pointer select-none items-center gap-2.5 rounded-lg px-2 py-1.5 transition-colors duration-100 ${
                item.id === selectedId
                  ? "bg-foreground/10"
                  : "hover:bg-foreground/5"
              }`}
            >
              <RowIcon item={item} />
              <span
                className={`min-w-0 flex-1 truncate text-[12.5px] ${
                  item.category === "color" ? "font-mono text-[11.5px]" : ""
                }`}
              >
                {rowLabel(item)}
              </span>
              {/* No type tag and no timestamp: the dot already encodes the
                  category and the detail pane's Information block carries the
                  type, time and source. Rows stay one scannable line. */}
              {item.pinned && <Pin className="size-3 shrink-0 text-amber-400" />}
            </div>
          </div>
        );
      })}
      {safeItems.length === 0 && (
        <div className="grid place-items-center px-4 py-10 text-center text-[12px] leading-relaxed text-muted-foreground">
          No matching entries.
          <br />
          Adjust the search or type filter.
        </div>
      )}
      {visibleCount < safeTotal && (
        <button
          type="button"
          onClick={onShowMore}
          className="mt-2 w-full rounded-lg py-1.5 text-center text-[11.5px] text-muted-foreground transition-colors hover:bg-foreground/5 hover:text-foreground"
        >
          Show more ({safeTotal - visibleCount} remaining)
        </button>
      )}
    </div>
  );
}
