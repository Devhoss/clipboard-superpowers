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
  secret: { label: "Secret", plural: "Secrets", dot: "#ff453a", text: "text-red-300" },
  file: { label: "File", plural: "Files", dot: "#64d2ff", text: "text-sky-200" },
};

export function CategoryLabel({ category, dot }: { category: string; dot?: string | null }) {
  const meta = (CATEGORY_META as Record<string, (typeof CATEGORY_META)[Category]>)[category];
  if (!meta) return null;
  // Optional exact-color override (color cards pass the real clipboard
  // value) so the single dot is always the true color, not the generic pink.
  const dotColor = dot ?? meta.dot;
  // Hex supports an alpha suffix for the glow; rgb()/rgba() values don't,
  // so they get a clean dot with no shadow instead of broken CSS.
  const glow = dotColor.startsWith("#") ? `0 0 6px ${dotColor}66` : "none";
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <span
        aria-hidden="true"
        className="inline-block size-2 shrink-0 rounded-full"
        style={{ background: dotColor, boxShadow: glow }}
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
