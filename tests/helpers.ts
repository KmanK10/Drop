import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { COOKIE_NAME } from "../src/shared/constants.ts";
import { createApp, type DropApp } from "../src/server/app.ts";
import type { LogEntry } from "../src/server/log.ts";

export function tempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "drop-"));
}

export function sessionCookie(res: Response): string {
  const lines =
    typeof res.headers.getSetCookie === "function"
      ? res.headers.getSetCookie()
      : [res.headers.get("set-cookie") ?? ""];
  const line = lines.find((value) => value.startsWith(`${COOKIE_NAME}=`));
  if (!line) throw new Error("missing session cookie");
  const raw = line.split(";")[0]?.slice(COOKIE_NAME.length + 1) ?? "";
  return decodeURIComponent(raw);
}

export function readDataDir(dir: string): Buffer {
  const parts: Buffer[] = [];
  for (const name of fs.readdirSync(dir)) {
    const full = path.join(dir, name);
    if (fs.statSync(full).isFile()) parts.push(fs.readFileSync(full));
  }
  return Buffer.concat(parts);
}

export async function send(
  app: DropApp,
  path: string,
  init: {
    method?: string;
    json?: unknown;
    body?: Uint8Array;
    cookie?: string;
    origin?: string;
    csrf?: boolean;
    headers?: Record<string, string>;
  } = {},
): Promise<Response> {
  const headers = new Headers();
  for (const [name, value] of Object.entries(init.headers ?? {})) headers.set(name, value);
  const method = init.method ?? (init.json !== undefined || init.body ? "POST" : "GET");
  if (init.csrf !== false && method !== "GET" && method !== "HEAD") headers.set("x-drop-request", "1");
  if (init.cookie) headers.set("cookie", `${COOKIE_NAME}=${init.cookie}`);
  if (init.origin) headers.set("origin", init.origin);
  let body: BodyInit | undefined;
  if (init.body) {
    headers.set("content-type", "application/octet-stream");
    const copy = new ArrayBuffer(init.body.byteLength);
    new Uint8Array(copy).set(init.body);
    body = copy;
  } else if (init.json !== undefined) {
    headers.set("content-type", "application/json");
    body = JSON.stringify(init.json);
  }
  return app.request(`http://drop.test${path}`, { method, headers, body });
}

export function createTestApp(
  logs: LogEntry[] = [],
  overrides: Partial<Parameters<typeof createApp>[0]> = {},
): { app: DropApp; dir: string } {
  const dir = tempDir();
  const app = createApp({
    setupSecret: "setup-secret-marker-DoNotStore",
    publicUrl: "http://drop.test",
    cookieSecure: false,
    trustProxy: false,
    rateLimit: false,
    clientDir: null,
    log: (entry) => logs.push(entry),
    ...overrides,
    dataDir: dir,
  });
  return { app, dir };
}
