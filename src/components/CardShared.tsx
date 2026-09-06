import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function CardAction({
  label,
  onClick,
  destructive = false,
  children,
}: {
  label: string;
  onClick: () => void;
  destructive?: boolean;
  children: React.ReactNode;
}) {
  return (
    <Button
      type="button"
      size="icon-xs"
      variant="ghost"
      title={label}
      aria-label={label}
      className={cn("text-muted-foreground hover:text-foreground", destructive && "hover:text-destructive")}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      {children}
    </Button>
  );
}
