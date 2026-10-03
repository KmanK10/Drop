export type LogEntry = {
  level: "info" | "error";
  msg: string;
  method?: string;
  route?: string;
  status?: number;
  ms?: number;
  userId?: string;
  bytes?: number;
};

export type Logger = (entry: LogEntry) => void;

/** Logs method, route template, status, and sizes. Never pass bodies, passwords, or tokens. */
export function createLogger(sink?: (entry: LogEntry) => void): Logger {
  return (entry) => {
    sink?.(entry);
    const parts = [entry.level, entry.msg];
    if (entry.method) parts.push(entry.method);
    if (entry.route) parts.push(entry.route);
    if (entry.status !== undefined) parts.push(String(entry.status));
    if (entry.ms !== undefined) parts.push(`${entry.ms}ms`);
    if (entry.userId) parts.push(`user=${entry.userId}`);
    if (entry.bytes !== undefined) parts.push(`bytes=${entry.bytes}`);
    const line = parts.join(" ");
    if (entry.level === "error") console.error(line);
    else console.log(line);
  };
}

export function safePath(path: string): string {
  const pathname = path.split("?")[0] ?? path;
  return pathname.replace(/^\/invite\/[^/]+/, "/invite/[redacted]");
}
