import { Laptop, Moon, Sun } from "lucide-react";
import { cn } from "@/lib/utils";

export type ThemeMode = "light" | "dark" | "system";

const options: { mode: ThemeMode; label: string; icon: typeof Sun }[] = [
  { mode: "light", label: "Use light appearance", icon: Sun },
  { mode: "system", label: "Use system appearance", icon: Laptop },
  { mode: "dark", label: "Use dark appearance", icon: Moon },
];

export function ThemeSwitcher({
  value,
  onChange,
}: {
  value: ThemeMode;
  onChange: (mode: ThemeMode) => void;
}) {
  return (
    <div
      className="flex items-center rounded-lg border border-border/70 bg-muted/60 p-0.5 shadow-sm"
      role="group"
      aria-label="Appearance"
    >
      {options.map(({ mode, label, icon: Icon }) => (
        <button
          key={mode}
          type="button"
          aria-label={label}
          aria-pressed={value === mode}
          onClick={() => onChange(mode)}
          className={cn(
            "grid size-5 place-items-center rounded-md text-muted-foreground transition-all duration-200 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/70",
            value === mode && "bg-background text-foreground shadow-sm",
          )}
        >
          <Icon className="size-3" strokeWidth={1.8} />
        </button>
      ))}
    </div>
  );
}
