import { useEffect, useRef, useState } from "react";
import { CATEGORY_META } from "@/lib/categories";
import { CATEGORIES, type Category } from "@/lib/types";
import { ChevronDown, Check } from "lucide-react";
import { cn } from "@/lib/utils";

/** "All Types" dropdown from the v2 design — pips + checkmark, closes on
 * outside click. `allLabel` is the unfiltered option ("All Types"). */
export function TypeDropdown({
  value,
  onChange,
}: {
  value: Category | "all";
  onChange: (v: Category | "all") => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("click", close);
    return () => document.removeEventListener("click", close);
  }, [open]);

  const label =
    value === "all" ? "All Types" : (CATEGORY_META[value]?.plural ?? value);

  return (
    <div ref={ref} className="relative shrink-0">
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-1.5 rounded-lg bg-card px-2.5 py-1.5 text-[12.5px] font-medium text-foreground shadow-[0_0_0_1px_var(--border)] transition-colors hover:bg-accent/50"
      >
        {label}
        <ChevronDown className={cn("size-2.5 text-muted-foreground transition-transform", open && "rotate-180")} />
      </button>
      {open && (
        <div
          role="listbox"
          className="absolute right-0 top-[calc(100%+6px)] z-30 min-w-[150px] rounded-xl border border-border/80 bg-popover p-1 shadow-xl shadow-black/25"
        >
          {(["all", ...CATEGORIES] as const).map((c) => {
            const meta = c === "all" ? null : CATEGORY_META[c];
            const active = value === c;
            return (
              <button
                key={c}
                type="button"
                role="option"
                aria-selected={active}
                onClick={() => {
                  onChange(c);
                  setOpen(false);
                }}
                className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-[12.5px] text-foreground transition-colors hover:bg-foreground/5"
              >
                <span
                  aria-hidden="true"
                  className="size-[7px] shrink-0 rounded-full"
                  style={{ background: meta ? meta.dot : "transparent" }}
                />
                {c === "all" ? "All Types" : meta!.plural}
                <Check className={cn("ml-auto size-3 text-primary", !active && "invisible")} />
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
