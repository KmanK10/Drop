import { cn } from "@/lib/utils";

export function Wordmark({ className }: { className?: string }) {
  return (
    <div className={cn("flex min-w-0 items-center gap-3", className)}>
      <span className="grid size-9 shrink-0 place-items-center rounded-full bg-primary font-serif text-lg italic text-primary-foreground">
        D
      </span>
      <div className="min-w-0">
        <div className="font-serif text-2xl leading-none tracking-tight">Drop</div>
        <div className="mt-1 text-xs text-muted-foreground">Private clipboard</div>
      </div>
    </div>
  );
}
