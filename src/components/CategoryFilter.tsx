import { Button } from "@/components/ui/button";
import { CATEGORIES } from "@/lib/types";
import { cn } from "@/lib/utils";

const LABELS: Record<string, string> = {
  all: "All",
  plain: "Plain",
  link: "Links",
  code: "Code",
  color: "Colors",
  email: "Emails",
  image: "Images",
};

export function CategoryFilter({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex gap-1 flex-wrap">
      {["all", ...CATEGORIES].map((c) => (
        <Button
          key={c}
          variant={value === c ? "default" : "outline"}
          size="sm"
          className="h-6 rounded-full px-2.5 text-[11px] font-normal"
          onClick={() => onChange(c)}
        >
          {LABELS[c] ?? c}
        </Button>
      ))}
    </div>
  );
}

export const categoryChipClass = (category: string) =>
  cn(
    "text-[10px] uppercase tracking-wider",
    category === "link" && "text-blue-400",
    category === "code" && "text-violet-400",
    category === "color" && "text-pink-400",
    category === "email" && "text-amber-400",
    category === "image" && "text-green-400",
    category === "plain" && "text-muted-foreground",
  );
