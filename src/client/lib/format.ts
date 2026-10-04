export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) {
    const value = bytes / 1024;
    return `${value < 10 ? value.toFixed(1) : Math.round(value)} KB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    const value = bytes / (1024 * 1024);
    return `${value < 10 ? value.toFixed(1) : Math.round(value)} MB`;
  }
  const value = bytes / (1024 * 1024 * 1024);
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} GB`;
}

export function retentionLabel(ttlMs: number): string {
  const days = Math.max(1, Math.round(ttlMs / (24 * 60 * 60 * 1000)));
  return days === 1 ? "1 day" : `${days} days`;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/** Days until `createdAt + ttlMs`. Null at 4 days or more, or when there is no lifetime. */
export function daysLeft(createdAt: number, ttlMs: number, now = Date.now()): string | null {
  if (!Number.isFinite(createdAt) || !Number.isFinite(ttlMs) || !Number.isFinite(now) || ttlMs <= 0) return null;
  const expires = createdAt + ttlMs;
  if (!Number.isFinite(expires)) return null;
  const remaining = expires - now;
  if (remaining >= 4 * DAY_MS) return null;
  if (remaining <= 0) return "0 days left";
  const days = Math.floor(remaining / DAY_MS);
  if (days <= 0) return "Less than a day";
  if (days === 1) return "1 day left";
  return `${days} days left`;
}

export function formatWhen(timestamp: number, now = Date.now()): string {
  const seconds = Math.round((now - timestamp) / 1000);
  if (seconds < 15) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 7) return `${days}d ago`;
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" }).format(timestamp);
}

export function formatDateTime(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(timestamp);
}

export async function mapPool<T>(items: T[], limit: number, fn: (item: T) => Promise<void>): Promise<void> {
  if (items.length === 0) return;
  let index = 0;
  async function worker(): Promise<void> {
    for (;;) {
      const current = index;
      index += 1;
      if (current >= items.length) return;
      await fn(items[current]!);
    }
  }
  const workers = Math.min(limit, items.length);
  await Promise.all(Array.from({ length: workers }, () => worker()));
}
