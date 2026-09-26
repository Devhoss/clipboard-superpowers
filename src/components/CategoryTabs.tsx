import { CATEGORIES, type Category } from "@/lib/types";
import { CATEGORY_META } from "@/lib/categories";

/**
 * Horizontal category filter. Replaces the old "All Types" dropdown: eight
 * targets are reachable in one click instead of two, and the dot colours make
 * the set scannable without opening anything.
 *
 * `secret` is deliberately absent — CATEGORIES omits it, so secrets have no
 * tab of their own and are reached through the list like any other row. A
 * dedicated secret tab would advertise a class of content the user has not
 * asked to browse.
 *
 * The row scrolls horizontally rather than wrapping: wrapping would change the
 * pane's height mid-interaction, which moves the rows under the cursor.
 */
export function CategoryTabs({
  value,
  onChange,
}: {
  value: Category | "all";
  onChange: (c: Category | "all") => void;
}) {
  return (
    <div
      className="flex min-w-0 items-center gap-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      role="tablist"
      aria-label="Filter by type"
    >
      <Tab
        label="All"
        dot={null}
        selected={value === "all"}
        onClick={() => onChange("all")}
      />
      {CATEGORIES.map((c) => (
        <Tab
          key={c}
          label={CATEGORY_META[c].single}
          dot={CATEGORY_META[c].dot}
          selected={value === c}
          onClick={() => onChange(c)}
        />
      ))}
    </div>
  );
}

function Tab({
  label,
  dot,
  selected,
  onClick,
}: {
  label: string;
  dot: string | null;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={selected}
      onClick={onClick}
      className={`inline-flex shrink-0 items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[11.5px] transition-colors duration-100 ${
        selected
          ? "border-[var(--glass-edge)] bg-foreground/10 text-foreground"
          : "border-transparent text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
      }`}
    >
      {dot && (
        <span
          aria-hidden="true"
          className="size-1.5 shrink-0 rounded-full"
          style={{ background: dot }}
        />
      )}
      {/* One element, not a visible/hidden pair. Normally it renders as the
          label; below 700px it collapses to screen-reader-only text so eight
          tabs still fit, and the dot carries the type visually. */}
      <span className="max-[700px]:sr-only">{label}</span>
    </button>
  );
}
