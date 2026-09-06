import { Input } from "@/components/ui/input";
import { Search } from "lucide-react";

export function SearchBar({
  value,
  onChange,
  inputRef,
}: {
  value: string;
  onChange: (v: string) => void;
  inputRef?: React.RefObject<HTMLInputElement | null>;
}) {
  return (
    <div className="relative">
      <Search className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input
        ref={inputRef}
        autoFocus
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Search clipboard…"
        className="pl-8 h-9 rounded-lg bg-background"
      />
    </div>
  );
}
