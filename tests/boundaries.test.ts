import { createHash } from "node:crypto";
import fs from "node:fs";
import Database from "better-sqlite3";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { utf8 } from "../src/shared/bytes.ts";
import { KDF, KEY_CHECK_TEXT } from "../src/shared/constants.ts";
import {
  createAccountMaterial,
  decrypt,
  deriveKeys,
  encrypt,
  importContentKey,
  loginBody,
  registrationBody,
  type AccountMaterial,
} from "../src/shared/crypto.ts";
import { encodeItem } from "../src/shared/item.ts";
import { setupConfigError } from "../src/server/config.ts";
import type { LogEntry } from "../src/server/log.ts";
import type { DropApp } from "../src/server/app.ts";
import { createTestApp, readDataDir, send, sessionCookie, tempDir } from "./helpers.ts";

const PASSWORD = "pw-marker-DoNotStore-8f21aa";
const SETUP = "setup-secret-marker-DoNotStore";
const PLAIN = "PLAINTEXT-MARKER-9f3c1a7e-drop-should-not-store-this";
const FILENAME = "secret-name-marker-9f3c1a7e.txt";

describe("setup, invites, and account isolation", () => {
  const logs: LogEntry[] = [];
  const consoleLines: string[] = [];
  const original = {
    log: console.log,
    error: console.error,
    info: console.info,
    warn: console.warn,
  };
  let app: DropApp;
  let dir: string;
  let adminCookie = "";
  let friendCookie = "";
  let adminMaterial: AccountMaterial;
  let friendMaterial: AccountMaterial;
  let inviteToken = "";

  beforeAll(async () => {
    for (const method of ["log", "error", "info", "warn"] as const) {
      console[method] = (...args: unknown[]) => {
        consoleLines.push(args.map((part) => String(part)).join(" "));
      };
    }
    ({ app, dir } = createTestApp(logs, { setupSecret: SETUP }));
    adminMaterial = await createAccountMaterial(PASSWORD);
    const setup = await send(app, "/api/setup", {
      json: {
        ...registrationBody(adminMaterial),
        username: "Ada",
        setupSecret: SETUP,
        password: PASSWORD,
      },
    });
    expect(setup.status).toBe(200);
    expect(setup.headers.get("set-cookie") ?? "").toMatch(/HttpOnly/i);
    expect(setup.headers.get("set-cookie") ?? "").not.toMatch(/Secure/i);
    adminCookie = sessionCookie(setup);
    expect(((await setup.json()) as { username: string }).username).toBe("ada");

    const invite = await send(app, "/api/invites", {
      json: { username: "Bea", role: "user" },
      cookie: adminCookie,
    });
    expect(invite.status).toBe(200);
    const created = (await invite.json()) as { link: string };
    expect(created.link.startsWith("http://drop.test/invite/")).toBe(true);
    inviteToken = created.link.split("/invite/")[1] ?? "";

    const preview = await send(app, "/api/invites/preview", { json: { token: inviteToken } });
    expect(preview.status).toBe(200);
    expect(await preview.json()).toMatchObject({ username: "bea", role: "user" });

    friendMaterial = await createAccountMaterial("friend-password-marker-99");
    const accepted = await send(app, "/api/invites/accept", {
      json: { ...registrationBody(friendMaterial), token: inviteToken, password: "friend-password-marker-99" },
    });
    expect(accepted.status).toBe(200);
    friendCookie = sessionCookie(accepted);
  });

  afterAll(async () => {
    await app?.close();
    console.log = original.log;
    console.error = original.error;
    console.info = original.info;
    console.warn = original.warn;
  });

  it("finishes setup once and never stores the secret, password, verifier, or content key", () => {
    const again = app.userCount();
    expect(again).toBe(2);
    const stored = readDataDir(dir);
    expect(stored.includes(SETUP)).toBe(false);
    expect(stored.includes(PASSWORD)).toBe(false);
    expect(stored.includes("friend-password-marker-99")).toBe(false);
    expect(stored.includes(inviteToken)).toBe(false);
    expect(stored.includes(KEY_CHECK_TEXT)).toBe(false);
    expect(stored.includes(Buffer.from(adminMaterial.contentKey))).toBe(false);
    expect(stored.includes(Buffer.from(adminMaterial.authVerifier))).toBe(false);
    const verifierHash = createHash("sha256").update(adminMaterial.authVerifier).digest();
    expect(stored.includes(verifierHash)).toBe(true);
    const dumped = `${JSON.stringify(logs)}\n${consoleLines.join("\n")}`;
    expect(dumped.includes(PASSWORD)).toBe(false);
    expect(dumped.includes(SETUP)).toBe(false);
    expect(dumped.includes(inviteToken)).toBe(false);
  });

  it("rejects a second setup, a bad secret, a missing csrf header, and a foreign origin", async () => {
    const second = await send(app, "/api/setup", {
      json: { ...registrationBody(adminMaterial), username: "cara", setupSecret: SETUP },
    });
    expect(second.status).toBe(404);

    const { app: fresh, dir: freshDir } = createTestApp([], { setupSecret: SETUP });
    try {
      const wrong = await send(fresh, "/api/setup", {
        json: { ...registrationBody(await createAccountMaterial("another-password")), username: "cara", setupSecret: "not-the-setup-secret" },
      });
      expect(wrong.status).toBe(401);
      expect(fresh.userCount()).toBe(0);
      expect(readDataDir(freshDir).includes("not-the-setup-secret")).toBe(false);

      const csrf = await send(fresh, "/api/setup", {
        json: { username: "cara", setupSecret: SETUP },
        csrf: false,
      });
      expect(csrf.status).toBe(403);
      expect(fresh.userCount()).toBe(0);

      const origin = await send(fresh, "/api/setup", {
        json: { username: "cara", setupSecret: SETUP },
        origin: "https://evil.example",
      });
      expect(origin.status).toBe(403);
    } finally {
      await fresh.close();
    }
  });

  it("lets the invite work once and keeps admin tools away from members", async () => {
    const reused = await send(app, "/api/invites/accept", {
      json: { ...registrationBody(friendMaterial), token: inviteToken },
    });
    expect(reused.status).toBe(400);
    expect(app.userCount()).toBe(2);

    expect((await send(app, "/api/invites", { json: { username: "cara" }, cookie: friendCookie })).status).toBe(403);
    expect((await send(app, "/api/admin/users", { method: "GET", cookie: friendCookie })).status).toBe(403);

    const adminInvite = await send(app, "/api/invites", {
      json: { username: "Cara", role: "admin" },
      cookie: adminCookie,
    });
    expect(adminInvite.status).toBe(200);
    const body = (await adminInvite.json()) as { id: string; link: string; role: string };
    expect(body.role).toBe("admin");
    const token = body.link.split("/invite/")[1] ?? "";
    const preview = await send(app, "/api/invites/preview", { json: { token } });
    expect(await preview.json()).toMatchObject({ username: "cara", role: "admin" });
    expect((await send(app, `/api/invites/${body.id}`, { method: "DELETE", cookie: adminCookie })).status).toBe(200);
    expect((await send(app, "/api/invites/preview", { json: { token } })).status).toBe(400);

    const users = await send(app, "/api/admin/users", { method: "GET", cookie: adminCookie });
    const listed = (await users.json()) as { users: Record<string, unknown>[] };
    expect(listed.users.map((user) => user.username)).toEqual(["ada", "bea"]);
    expect(Object.keys(listed.users[0] ?? {}).sort()).toEqual([
      "createdAt",
      "quotaBytes",
      "role",
      "usedBytes",
      "username",
    ]);
  });

  it("hides items between users and deletes ciphertext for good", async () => {
    const key = await importContentKey(adminMaterial.contentKey);
    const ciphertext = await encrypt(
      key,
      encodeItem({ kind: "text", name: FILENAME, mime: "text/plain", body: utf8(PLAIN) }),
    );
    const created = await send(app, "/api/items", { method: "POST", body: ciphertext, cookie: adminCookie });
    expect(created.status).toBe(201);
    const item = (await created.json()) as { id: string; size: number };
    expect(item.size).toBe(ciphertext.byteLength);

    const adminList = (await (await send(app, "/api/items", { method: "GET", cookie: adminCookie })).json()) as {
      items: { id: string }[];
    };
    expect(adminList.items.map((entry) => entry.id)).toContain(item.id);

    const friendList = (await (await send(app, "/api/items", { method: "GET", cookie: friendCookie })).json()) as {
      items: unknown[];
    };
    expect(friendList.items).toEqual([]);
    expect((await send(app, `/api/items/${item.id}`, { method: "GET", cookie: friendCookie })).status).toBe(404);
    expect((await send(app, `/api/items/${item.id}`, { method: "DELETE", cookie: friendCookie })).status).toBe(404);
    expect((await send(app, "/api/items", { method: "GET" })).status).toBe(401);

    const downloaded = await send(app, `/api/items/${item.id}`, { method: "GET", cookie: adminCookie });
    const storedBytes = new Uint8Array(await downloaded.arrayBuffer());
    const decoded = new TextDecoder().decode(await decrypt(key, storedBytes));
    expect(decoded).toContain(PLAIN);
    expect(decoded).toContain(FILENAME);
    const friendKey = await importContentKey(friendMaterial.contentKey);
    await expect(decrypt(friendKey, storedBytes)).rejects.toThrow();

    const onDisk = readDataDir(dir);
    expect(onDisk.includes(PLAIN)).toBe(false);
    expect(onDisk.includes(FILENAME)).toBe(false);
    expect(onDisk.includes(Buffer.from(storedBytes.subarray(16, 16 + 24)))).toBe(true);

    const removed = await send(app, `/api/items/${item.id}`, { method: "DELETE", cookie: adminCookie });
    expect(removed.status).toBe(200);
    expect((await send(app, `/api/items/${item.id}`, { method: "GET", cookie: adminCookie })).status).toBe(404);

    const after = readDataDir(dir);
    expect(after.includes(PLAIN)).toBe(false);
    expect(after.includes(FILENAME)).toBe(false);
    expect(after.includes(Buffer.from(storedBytes.subarray(16, 16 + 24)))).toBe(false);

    const db = new Database(app.dbPath, { readonly: true });
    const tables = (db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name").all() as { name: string }[]).map(
      (row) => row.name,
    );
    expect(tables).toEqual(["invites", "items", "rekey_items", "rekeys", "server_meta", "sessions", "users"]);
    const columns = (db.prepare("PRAGMA table_info(items)").all() as { name: string }[]).map((row) => row.name);
    expect(columns).toEqual(["id", "owner_id", "created_at", "size", "ciphertext"]);
    const left = db.prepare("SELECT COUNT(*) AS n FROM items").get() as { n: number };
    expect(left.n).toBe(0);
    db.close();

    const dumped = `${JSON.stringify(logs)}\n${consoleLines.join("\n")}`;
    expect(dumped.includes(PLAIN)).toBe(false);
    expect(dumped.includes(FILENAME)).toBe(false);
    expect(dumped.includes(PASSWORD)).toBe(false);
  });

  it("returns the real salt for a login and a fake salt that cannot create a session", async () => {
    const params = await send(app, "/api/auth/params", { json: { username: "ada" } });
    const real = (await params.json()) as { salt: string; memory: number; time: number; parallelism: number; algo: string };
    expect(real).toMatchObject({ algo: KDF.algo, memory: KDF.memory, time: KDF.time, parallelism: KDF.parallelism });
    const { authVerifier, contentKey } = await deriveKeys(PASSWORD, Buffer.from(real.salt, "base64url"), real);
    expect(Buffer.from(contentKey).equals(Buffer.from(adminMaterial.contentKey))).toBe(true);
    const login = await send(app, "/api/auth/login", { json: loginBody("Ada", authVerifier) });
    expect(login.status).toBe(200);
    const secondCookie = sessionCookie(login);
    expect(secondCookie).not.toBe(adminCookie);
    expect((await send(app, "/api/me", { method: "GET", cookie: secondCookie })).status).toBe(200);

    const unknown = await send(app, "/api/auth/params", { json: { username: "nobody" } });
    const fake = (await unknown.json()) as { salt: string };
    expect(fake.salt).not.toBe(real.salt);
    const guessed = await deriveKeys(PASSWORD, Buffer.from(fake.salt, "base64url"), real);
    const rejected = await send(app, "/api/auth/login", { json: loginBody("nobody", guessed.authVerifier) });
    expect(rejected.status).toBe(401);
    expect(app.userCount()).toBe(2);

    const bad = await send(app, "/api/auth/login", {
      json: { username: "ada", authVerifier: registrationBody(adminMaterial).kdfSalt, password: PASSWORD },
    });
    expect(bad.status).toBe(401);
  });

  it("drops an expired session", async () => {
    const db = new Database(app.dbPath);
    db.prepare("UPDATE sessions SET expires_at = 1").run();
    db.close();
    expect((await send(app, "/api/items", { method: "GET", cookie: adminCookie })).status).toBe(401);
  });
});

describe("limits", () => {
  it("caps item count and size, and slows repeated login failures", async () => {
    const logs: LogEntry[] = [];
    const { app, dir } = createTestApp(logs, { quotaBytes: 80, rateLimit: true });
    try {
      const material = await createAccountMaterial("limit-password-ok");
      const setup = await send(app, "/api/setup", {
        json: { ...registrationBody(material), username: "ada", setupSecret: SETUP },
      });
      const cookie = sessionCookie(setup);
      const key = await importContentKey(material.contentKey);
      const blob = await encrypt(key, encodeItem({ kind: "text", name: "", mime: "text/plain", body: utf8("ok") }));
      expect(blob.byteLength).toBeLessThan(80);
      expect((await send(app, "/api/items", { method: "POST", body: blob, cookie })).status).toBe(201);
      const db = new Database(app.dbPath);
      db.prepare("UPDATE users SET quota_bytes = ?").run(blob.byteLength);
      db.close();
      expect((await send(app, "/api/items", { method: "POST", body: blob, cookie })).status).toBe(413);
      const huge = new Uint8Array(200);
      expect((await send(app, "/api/items", { method: "POST", body: huge, cookie })).status).toBe(413);

      for (let attempt = 0; attempt < 8; attempt++) {
        const res = await send(app, "/api/auth/login", {
          json: { username: "ada", authVerifier: "a".repeat(43) },
        });
        expect(res.status).toBe(401);
      }
      expect((await send(app, "/api/auth/login", { json: { username: "ada", authVerifier: "a".repeat(43) } })).status).toBe(429);
      expect(readDataDir(dir).includes("limit-password-ok")).toBe(false);
    } finally {
      await app.close();
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("refuses to describe a server with no admin and no setup secret", () => {
    expect(setupConfigError(0, undefined)).toMatch(/SETUP_SECRET/);
    expect(setupConfigError(0, "short")).toMatch(/16/);
    expect(setupConfigError(1, undefined)).toBeNull();
    expect(tempDir()).toBeTruthy();
  });
});
