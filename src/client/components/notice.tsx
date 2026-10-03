export function Notice({ children, tone = "error" }: { children: string; tone?: "error" | "info" }) {
  return (
    <p
      role={tone === "error" ? "alert" : "status"}
      className={
        tone === "error"
          ? "rounded-lg bg-destructive/10 px-3 py-2 text-sm text-destructive"
          : "rounded-lg bg-accent px-3 py-2 text-sm text-accent-foreground"
      }
    >
      {children}
    </p>
  );
}
