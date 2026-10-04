import fs from "node:fs";
import Database from "better-sqlite3";
import { afterEach, describe, expect, it } from "vitest";
import { bytesToB64url, utf8 } from "../src/shared/bytes.ts";
import {
  CONTENT_SECURITY_POLICY,
  effectiveQuota,
  LIMITS,
  quotaAllowed,
} from "../src/shared/constants.ts";
import {
  createAccountMaterial,
  decrypt,
  deriveKeys,
  encrypt,
  importContentKey,
  registrationBody,
  type AccountMaterial,
} from "../src/shared/crypto.ts";
import { decodeItem, encodeItem } from "../src/shared/item.ts";
import type { DropApp } from "../src/server/app.ts";
import { createTestApp, readDataDir, send, sessionCookie } from "./helpers.ts";

const SETUP = "setup-secret-marker-DoNotStore";

async function boot(quotaBytes?: number): Promise<{
  app: DropApp;
  dir: string;
  admin: AccountMaterial;
  cookie: string;
}> {
  const { app, dir } = createTestApp([], quotaBytes === undefined ? {} : { quotaBytes });
  const admin = await createAccountMaterial("admin-password-ok");
  const setup = await send(app, "/api/setup", {
    json: { ...registrationBody(admin), username: "ada", setupSecret: SETUP },
  });
  return { app, dir, admin, cookie: sessionCookie(setup) };
}

async function addUser(
  app: DropApp,
  adminCookie: string,
  username: string,
  role: "admin" | "user",
  password: string,
): Promise<{ cookie: string; material: AccountMaterial }> {
  const invite = await send(app, "/api/invites", { cookie: adminCookie, json: { username, role } });
  const link = ((await invite.json()) as { link: string }).link;
  const token = link.slice(link.lastIndexOf("/") + 1);
  const material = await createAccountMaterial(password);
  const accepted = await send(app, "/api/invites/accept", {
    json: { ...registrationBody(material), token },
  });
  return { cookie: sessionCookie(accepted), material };
}

describe("quota, roles, expiry, and password changes", () => {
  const opened: { app: DropApp; dir: string }[] = [];

  afterEach(async () => {
    while (opened.length) {
      const next = opened.pop()!;
      await next.app.close();
      fs.rmSync(next.dir, { recursive: true, force: true });
    }
  });

  it("keeps the production CSP limited to wasm-unsafe-eval", async () => {
    const { app, dir } = createTestApp();
    opened.push({ app, dir });
    const previous = process.env.NODE_ENV;
    process.env.NODE_ENV = "production";
    try {
      const res = await send(app, "/api/health");
      const policy = res.headers.get("content-security-policy");
      expect(policy).toBe(CONTENT_SECURITY_POLICY);
      const script = policy?.split(";").map((part) => part.trim()).find((part) => part.startsWith("script-src"));
      expect(script).toBe("script-src 'self' 'wasm-unsafe-eval'");
    } finally {
      process.env.NODE_ENV = previous;
    }
  });

  it("rejects quota changes from a member, protects the last admin, and enforces the ceiling", async () => {
    const session = await boot();
    opened.push(session);
    const { app, cookie } = session;
    const member = await addUser(app, cookie, "bea", "user", "member-password-ok");

    expect(quotaAllowed(LIMITS.quotaByteCeiling)).toBe(true);
    expect(quotaAllowed(LIMITS.quotaByteCeiling + 1)).toBe(false);
    expect(effectiveQuota(LIMITS.quotaByteCeiling * 10)).toBe(LIMITS.quotaByteCeiling);

    const listed = (await (await send(app, "/api/admin/users", { method: "GET", cookie })).json()) as {
      users: { username: string; quotaBytes: number; role: string }[];
    };
    expect(listed.users.find((user) => user.username === "bea")?.quotaBytes).toBe(LIMITS.quotaBytes);

    expect(
      (await send(app, "/api/admin/users/ada", { method: "PATCH", cookie: member.cookie, json: { role: "user" } }))
        .status,
    ).toBe(403);
    expect(
      (await send(app, "/api/admin/users/ada", { method: "DELETE", cookie: member.cookie })).status,
    ).toBe(403);
    expect((await send(app, "/api/admin/users", { method: "GET", cookie: member.cookie })).status).toBe(403);

    expect(
      (await send(app, "/api/admin/users/ada", { method: "PATCH", cookie, json: { role: "user" } })).status,
    ).toBe(409);
    expect((await send(app, "/api/admin/users/ada", { method: "DELETE", cookie })).status).toBe(409);

    const tooBig = await send(app, "/api/admin/users/bea", {
      method: "PATCH",
      cookie,
      json: { quotaBytes: LIMITS.quotaByteCeiling + 1024 },
    });
    expect(tooBig.status).toBe(400);
    const tooSmall = await send(app, "/api/admin/users/bea", {
      method: "PATCH",
      cookie,
      json: { quotaBytes: 128 },
    });
    expect(tooSmall.status).toBe(400);

    const lowered = await send(app, "/api/admin/users/bea", {
      method: "PATCH",
      cookie,
      json: { quotaBytes: 2048 },
    });
    expect(lowered.status).toBe(200);

    const key = await importContentKey(member.material.contentKey);
    const text = await encrypt(
      key,
      encodeItem({ kind: "text", name: "", mime: "text/plain", body: utf8("bea-secret-marker") }),
    );
    expect(text.byteLength).toBeLessThan(2048);
    const created = await send(app, "/api/items", { method: "POST", body: text, cookie: member.cookie });
    expect(created.status).toBe(201);
    const item = (await created.json()) as { id: string };
    const downloaded = new Uint8Array(
      await (await send(app, `/api/items/${item.id}`, { method: "GET", cookie: member.cookie })).arrayBuffer(),
    );
    const piece = Buffer.from(downloaded.subarray(16, 40));
    expect(readDataDir(session.dir).includes(piece)).toBe(true);
    const over = new Uint8Array(2048);
    expect((await send(app, "/api/items", { method: "POST", body: over, cookie: member.cookie })).status).toBe(413);

    const other = await addUser(app, cookie, "cara", "admin", "other-admin-password");
    expect(
      (await send(app, "/api/admin/users/ada", { method: "PATCH", cookie: other.cookie, json: { role: "user" } }))
        .status,
    ).toBe(200);
    expect((await send(app, "/api/admin/users/cara", { method: "DELETE", cookie: other.cookie })).status).toBe(409);

    expect((await send(app, "/api/admin/users/bea", { method: "DELETE", cookie: other.cookie })).status).toBe(200);
    expect((await send(app, "/api/me", { method: "GET", cookie: member.cookie })).status).toBe(401);
    expect(readDataDir(session.dir).includes(piece)).toBe(false);
    expect(readDataDir(session.dir).includes("bea-secret-marker")).toBe(false);

    expect(
      (await send(app, "/api/admin/users/ada", { method: "PATCH", cookie: other.cookie, json: { role: "admin" } }))
        .status,
    ).toBe(200);
    expect((await send(app, "/api/admin/users/cara", { method: "DELETE", cookie: other.cookie })).status).toBe(200);
    expect((await send(app, "/api/me", { method: "GET", cookie: other.cookie })).status).toBe(401);
    expect((await send(app, "/api/me", { method: "GET", cookie })).status).toBe(200);
  });

  it("hard-deletes items 30 days after they were created", async () => {
    const session = await boot();
    opened.push(session);
    const key = await importContentKey(session.admin.contentKey);
    const blob = await encrypt(
      key,
      encodeItem({ kind: "text", name: "", mime: "text/plain", body: utf8("expires-marker") }),
    );
    const created = await send(session.app, "/api/items", { method: "POST", body: blob, cookie: session.cookie });
    const item = (await created.json()) as { id: string };
    const downloaded = new Uint8Array(
      await (await send(session.app, `/api/items/${item.id}`, { method: "GET", cookie: session.cookie })).arrayBuffer(),
    );
    const piece = Buffer.from(downloaded.subarray(16, 40));
    const db = new Database(session.app.dbPath);
    db.prepare("UPDATE items SET created_at = ? WHERE id = ?").run(Date.now() - LIMITS.itemTtlMs - 1000, item.id);
    db.close();
    const list = (await (await send(session.app, "/api/items", { method: "GET", cookie: session.cookie })).json()) as {
      items: { id: string }[];
    };
    expect(list.items).toEqual([]);
    expect((await send(session.app, `/api/items/${item.id}`, { method: "GET", cookie: session.cookie })).status).toBe(404);
    expect(readDataDir(session.dir).includes(piece)).toBe(false);
    expect(readDataDir(session.dir).includes("expires-marker")).toBe(false);
  });

  it("re-encrypts text and a file under a new password and drops other sessions", async () => {
    const session = await boot();
    opened.push(session);
    const member = await addUser(session.app, session.cookie, "bea", "user", "member-password-ok");
    const oldKey = await importContentKey(member.material.contentKey);
    const textPlain = "rekey-text-marker";
    const fileName = "rekey-notes.txt";
    const fileBody = utf8("rekey-file-marker");
    const text = await encrypt(oldKey, encodeItem({ kind: "text", name: "", mime: "text/plain", body: utf8(textPlain) }));
    const file = await encrypt(
      oldKey,
      encodeItem({ kind: "file", name: fileName, mime: "text/plain", body: fileBody }),
    );
    expect((await send(session.app, "/api/items", { method: "POST", body: text, cookie: member.cookie })).status).toBe(201);
    expect((await send(session.app, "/api/items", { method: "POST", body: file, cookie: member.cookie })).status).toBe(201);

    const second = await send(session.app, "/api/auth/login", {
      json: {
        username: "bea",
        authVerifier: bytesToB64url(member.material.authVerifier),
      },
    });
    const otherCookie = sessionCookie(second);

    const me = (await (await send(session.app, "/api/me", { method: "GET", cookie: member.cookie })).json()) as {
      kdf: { salt: string; memory: number; time: number; parallelism: number; algo: string };
    };
    const current = await deriveKeys("member-password-ok", Buffer.from(me.kdf.salt, "base64url"), me.kdf);
    const started = await send(session.app, "/api/account/password/start", {
      method: "POST",
      cookie: member.cookie,
      json: { currentAuthVerifier: bytesToB64url(current.authVerifier) },
    });
    expect(started.status).toBe(200);
    const rekey = (await started.json()) as { rekeyId: string; items: { id: string }[] };
    expect(rekey.items).toHaveLength(2);

    const next = await createAccountMaterial("member-password-new");
    const newKey = await importContentKey(next.contentKey);
    for (const item of rekey.items) {
      const bytes = new Uint8Array(
        await (await send(session.app, `/api/items/${item.id}`, { method: "GET", cookie: member.cookie })).arrayBuffer(),
      );
      const plain = await decrypt(oldKey, bytes);
      const wrapped = await encrypt(newKey, plain);
      const put = await send(session.app, `/api/account/password/items/${item.id}`, {
        method: "PUT",
        cookie: member.cookie,
        body: wrapped,
        headers: { "x-drop-rekey": rekey.rekeyId },
      });
      expect(put.status).toBe(200);
    }
    const commit = await send(session.app, "/api/account/password/commit", {
      method: "POST",
      cookie: member.cookie,
      json: { rekeyId: rekey.rekeyId, ...registrationBody(next), password: "member-password-new" },
    });
    expect(commit.status).toBe(200);
    expect((await send(session.app, "/api/me", { method: "GET", cookie: member.cookie })).status).toBe(200);
    expect((await send(session.app, "/api/me", { method: "GET", cookie: otherCookie })).status).toBe(401);

    const list = (await (await send(session.app, "/api/items", { method: "GET", cookie: member.cookie })).json()) as {
      items: { id: string }[];
    };
    const decoded = [];
    for (const item of list.items) {
      const bytes = new Uint8Array(
        await (await send(session.app, `/api/items/${item.id}`, { method: "GET", cookie: member.cookie })).arrayBuffer(),
      );
      decoded.push(decodeItem(await decrypt(newKey, bytes)));
      await expect(decrypt(oldKey, bytes)).rejects.toThrow();
    }
    expect(decoded.map((item) => item.text ?? item.name).sort()).toEqual([fileName, textPlain].sort());
    const disk = readDataDir(session.dir);
    expect(disk.includes(textPlain)).toBe(false);
    expect(disk.includes(fileName)).toBe(false);
    expect(disk.includes("member-password-new")).toBe(false);
    expect(disk.includes("member-password-ok")).toBe(false);
  });

  it("deletes the signed-in account and their items, and refuses the last admin", async () => {
    const session = await boot();
    opened.push(session);
    const { app, cookie } = session;
    expect((await send(app, "/api/account", { method: "DELETE" })).status).toBe(401);

    const member = await addUser(app, cookie, "bea", "user", "member-password-ok");
    const key = await importContentKey(member.material.contentKey);
    const text = await encrypt(
      key,
      encodeItem({ kind: "text", name: "", mime: "text/plain", body: utf8("bea-delete-marker") }),
    );
    const created = await send(app, "/api/items", { method: "POST", body: text, cookie: member.cookie });
    expect(created.status).toBe(201);
    const item = (await created.json()) as { id: string };
    const downloaded = new Uint8Array(
      await (await send(app, `/api/items/${item.id}`, { method: "GET", cookie: member.cookie })).arrayBuffer(),
    );
    const piece = Buffer.from(downloaded.subarray(16, 40));
    expect(readDataDir(session.dir).includes(piece)).toBe(true);

    const refused = await send(app, "/api/account", { method: "DELETE", cookie });
    expect(refused.status).toBe(409);
    expect(((await refused.json()) as { error: string }).error).toBe("Drop needs at least one admin.");
    expect((await send(app, "/api/me", { method: "GET", cookie })).status).toBe(200);

    expect((await send(app, "/api/account", { method: "DELETE", cookie: member.cookie })).status).toBe(200);
    expect((await send(app, "/api/me", { method: "GET", cookie: member.cookie })).status).toBe(401);
    expect((await send(app, `/api/items/${item.id}`, { method: "GET", cookie })).status).toBe(404);
    expect(readDataDir(session.dir).includes(piece)).toBe(false);
    expect(readDataDir(session.dir).includes("bea-delete-marker")).toBe(false);

    const other = await addUser(app, cookie, "cara", "admin", "other-admin-password");
    expect((await send(app, "/api/account", { method: "DELETE", cookie })).status).toBe(200);
    expect((await send(app, "/api/me", { method: "GET", cookie })).status).toBe(401);
    expect((await send(app, "/api/account", { method: "DELETE", cookie: other.cookie })).status).toBe(409);
    expect((await send(app, "/api/me", { method: "GET", cookie: other.cookie })).status).toBe(200);
  });
});
