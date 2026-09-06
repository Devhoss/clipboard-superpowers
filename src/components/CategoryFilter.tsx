import { Button } from "@/components/ui/button";
import { CATEGORIES, type Category } from "@/lib/types";
import { cn } from "@/lib/utils";

// Apple HIG dark-mode palette: vivid dot + bright label, one identity per
// category, shared by pills and card headers.
export const CATEGORY_META: Record<Category, { label: string; plural: string; dot: string; text: string }> = {
  plain: { label: "Plain", plural: "Plain", dot: "#98989d", text: "text-zinc-400" },
  link: { label: "Link", plural: "Links", dot: "#0a84ff", text: "text-sky-300" },
  code: { label: "Code", plural: "Code", dot: "#bf5af2", text: "text-purple-300" },
  color: { label: "Color", plural: "Colors", dot: "#ff375f", text: "text-rose-300" },
  email: { label: "Email", plural: "Emails", dot: "#ff9f0a", text: "text-amber-300" },
  image: { label: "Image", plural: "Images", dot: "#30d158", text: "text-green-300" },
};

export function CategoryLabel({ category }: { category: string }) {
  const meta = (CATEGORY_META as Record<string, (typeof CATEGORY_META)[Category]>)[category];
  if (!meta) return null;
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <span
        aria-hidden="true"
        className="inline-block size-2 shrink-0 rounded-full"
        style={{ background: meta.dot, boxShadow: `0 0 6px ${meta.dot}66` }}
      />
      <span className={cn("text-[10px] font-semibold uppercase tracking-wider", meta.text)}>
        {meta.label}
      </span>
    </span>
  );
}

export function CategoryFilter({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex gap-1 flex-wrap">
      {["all", ...CATEGORIES].map((c) => {
        const dot = c === "all" ? undefined : CATEGORY_META[c as Category]?.dot;
        const label = c === "all" ? "All" : (CATEGORY_META[c as Category]?.plural ?? c);
        return (
          <Button
            key={c}
            variant={value === c ? "default" : "outline"}
            size="sm"
            className="h-6 rounded-full px-2.5 text-[11px] font-normal"
            onClick={() => onChange(c)}
          >
            {dot && (
              <span
                aria-hidden="true"
                className="inline-block size-1.5 rounded-full"
                style={{ background: dot }}
              />
            )}
            {label}
          </Button>
        );
      })}
    </div>
  );
}
