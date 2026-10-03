import fs from "node:fs";
import path from "node:path";
import { createHash, createHmac, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import { createServer, type IncomingMessage, type Server } from "node:http";
import type { Duplex } from "node:stream";
import { bodyLimit } from "hono/body-limit";
import { deleteCookie, getCookie, setCookie } from "hono/cookie";
import { Hono, type Context } from "hono";
import { getRequestListener } from "@hono/node-server";
import { WebSocketServer } from "ws";
import { b64urlToBytes, bytesToB64url } from "../shared/bytes.ts";
import {
  CONTENT_SECURITY_POLICY,
  effectiveQuota,
  quotaAllowed,
  KDF,
  kdfIsCurrent,
  LIMITS,
  COOKIE_NAME,
  CSRF_HEADER,
} from "../shared/constants.ts";
import { normalizeUsername } from "../shared/username.ts";
import {
  checkpoint,
  countOf,
  hashToken,
  openDatabase,
  statements,
  type InviteRow,
  type Role,
  type SessionRow,
  type UserRow,
} from "./db.ts";
import { Hub } from "./hub.ts";
import { createLogger, safePath, type Logger } from "./log.ts";

type Vars = { userId?: string };
type JsonBody = Record<string, unknown>;

export type CreateAppOptions = {
  dataDir: string;
  setupSecret?: string;
  publicUrl?: string;
  cookieSecure?: boolean;
  trustProxy?: boolean;
  quotaBytes?: number;
  rateLimit?: boolean;
  clientDir?: string | null;
  log?: Logger;
};

export type DropApp = {
  request: Hono["request"];
  listen: (port: number, host?: string) => Promise<{ port: number }>;
  close: () => Promise<void>;
  userCount: () => number;
  dbPath: string;
};

const INVITE_INVALID = "This invite link is invalid or already used.";
const AUTH_INVALID = "Username or password is wrong.";
const JSON_MAX = 65_536;

export function createApp(options: CreateAppOptions): DropApp {
  const publicUrl = (options.publicUrl ?? "").replace(/\/+$/, "");
  const cookieSecure = options.cookieSecure ?? false;
  const trustProxy = options.trustProxy ?? false;
  const defaultQuotaBytes = Math.min(options.quotaBytes ?? LIMITS.quotaBytes, LIMITS.quotaByteCeiling);
  const log = options.log ?? createLogger();
  const clientDir = resolveClientDir(options.clientDir);
  const db = openDatabase(options.dataDir);
  const stmts = statements(db);
  const hub = new Hub();
  const limits = createLimiter(options.rateLimit !== false);
  const dbPath = path.join(options.dataDir, "drop.sqlite");

  const pepperRow = stmts.pepper.get() as { value: Buffer };
  const pepper = Buffer.from(pepperRow.value);
  const dummyVerifierHash = createHash("sha256").update(pepper).digest();

  const app = new Hono<{ Variables: Vars }>();

  app.onError((err, c) => {
    log({
      level: "error",
      msg: err.name || "unhandled",
      method: c.req.method,
      route: c.req.routePath || safePath(c.req.path),
    });
    return c.json({ error: "Something went wrong." }, 500);
  });

  app.use("*", async (c, next) => {
    const started = Date.now();
    await next();
    c.header("X-Content-Type-Options", "nosniff");
    c.header("Referrer-Policy", "no-referrer");
    c.header("X-Frame-Options", "DENY");
    c.header("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
    c.header("Cross-Origin-Resource-Policy", "same-origin");
    if (process.env.NODE_ENV === "production") {
      // wasm-unsafe-eval lets hash-wasm compile Argon2id. It does not allow JavaScript eval.
      c.header("Content-Security-Policy", CONTENT_SECURITY_POLICY);
    }
    if (c.req.path.startsWith("/api")) {
      c.header("Cache-Control", "no-store");
      log({
        level: c.res.status >= 500 ? "error" : "info",
        msg: "request",
        method: c.req.method,
        route: c.req.routePath || safePath(c.req.path),
        status: c.res.status,
        ms: Date.now() - started,
        userId: c.get("userId"),
      });
    }
  });

  app.use(
    "/api/*",
    bodyLimit({
      maxSize: Math.max(LIMITS.quotaByteCeiling, JSON_MAX),
      onError: (c) => c.json({ error: `That item is larger than ${limitLabel(LIMITS.quotaByteCeiling)}.` }, 413),
    }),
  );

  app.use("/api/*", async (c, next) => {
    if (!mutationAllowed(c)) return c.json({ error: "Request was rejected." }, 403);
    await next();
  });

  app.get("/api/health", (c) => c.json({ ok: true }));

  app.get("/api/meta", (c) => {
    return c.json({
      setupRequired: countOf(stmts.userCount.get()) === 0,
      quotaBytes: defaultQuotaBytes,
      quotaByteCeiling: LIMITS.quotaByteCeiling,
      minQuotaBytes: LIMITS.minQuotaBytes,
      itemTtlMs: LIMITS.itemTtlMs,
      kdf: {
        algo: KDF.algo,
        memory: KDF.memory,
        time: KDF.time,
        parallelism: KDF.parallelism,
      },
    });
  });

  app.post("/api/setup", async (c) => {
    if (countOf(stmts.userCount.get()) > 0) {
      return c.json({ error: "Setup is already finished." }, 404);
    }
    if (!options.setupSecret) return c.json({ error: "Setup is not available." }, 503);
    const ip = clientIp(c);
    if (!limits.allow(`setup:${ip}`, 5, 60 * 60 * 1000)) {
      return c.json({ error: "Too many attempts. Wait a few minutes and try again." }, 429);
    }
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: "The account data was not valid." }, 400);
    const username = normalizeUsername(body.username);
    if (!username) return c.json({ error: "Choose a username of 2–32 characters: a letter, then lowercase letters, digits, underscores, or hyphens." }, 400);
    if (typeof body.setupSecret !== "string" || !secretMatches(body.setupSecret, options.setupSecret)) {
      return c.json({ error: "That setup secret is wrong." }, 401);
    }
    const registration = parseRegistration(body);
    if (!registration) return c.json({ error: "The encryption setup was not valid. Reload and try again." }, 400);
    const id = randomUUID();
    stmts.insertUser.run(
      id,
      username,
      "admin",
      registration.salt,
      registration.memory,
      registration.time,
      registration.parallelism,
      registration.verifierHash,
      registration.keyCheck,
      defaultQuotaBytes,
      Date.now(),
    );
    issueSession(c, id);
    checkpoint(db);
    return c.json({ username, role: "admin" as const });
  });

  app.post("/api/auth/params", async (c) => {
    const ip = clientIp(c);
    if (!limits.allow(`params:${ip}`, 60, 60 * 1000)) {
      return c.json({ error: "Too many attempts. Wait a few minutes and try again." }, 429);
    }
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: AUTH_INVALID }, 401);
    const username = normalizeUsername(body.username);
    if (!username) return c.json({ error: AUTH_INVALID }, 401);
    const user = stmts.userByName.get(username) as UserRow | undefined;
    if (!user) {
      const salt = createHmac("sha256", pepper).update(`drop-salt-v1:${username}`).digest().subarray(0, 16);
      return c.json(kdfResponse(Buffer.from(salt), KDF.memory, KDF.time, KDF.parallelism));
    }
    return c.json(kdfResponse(user.kdf_salt, user.kdf_memory, user.kdf_time, user.kdf_parallelism));
  });

  app.post("/api/auth/login", async (c) => {
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: AUTH_INVALID }, 401);
    const username = normalizeUsername(body.username);
    const ip = clientIp(c);
    const bucket = `login:${ip}:${username ?? "-"}`;
    if (!limits.allow(bucket, 8, 15 * 60 * 1000)) {
      return c.json({ error: "Too many attempts. Wait a few minutes and try again." }, 429);
    }
    const user = username ? (stmts.userByName.get(username) as UserRow | undefined) : undefined;
    const provided = decodeExact(body.authVerifier, 32);
    if (!user || !verifierMatches(user, provided)) return c.json({ error: AUTH_INVALID }, 401);
    limits.clear(bucket);
    issueSession(c, user.id);
    return c.json({ username: user.username, role: user.role });
  });

  app.post("/api/auth/logout", (c) => {
    const token = getCookie(c, COOKIE_NAME);
    if (token) stmts.deleteSession.run(hashToken(token));
    deleteCookie(c, COOKIE_NAME, { path: "/", secure: cookieSecure, sameSite: "Lax" });
    return c.json({ ok: true });
  });

  app.get("/api/me", (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    purgeExpired();
    const fresh = stmts.userById.get(user.id) as UserRow;
    return c.json(accountView(fresh));
  });

  app.get("/api/admin/users", (c) => {
    const admin = requireAdmin(c);
    if (admin instanceof Response) return admin;
    purgeExpired();
    const rows = stmts.listUsers.all() as {
      username: string;
      role: Role;
      created_at: number;
      quota_bytes: number;
      used_bytes: number;
    }[];
    return c.json({
      users: rows.map((row) => ({
        username: row.username,
        role: row.role,
        createdAt: row.created_at,
        quotaBytes: effectiveQuota(row.quota_bytes),
        usedBytes: row.used_bytes,
      })),
    });
  });

  app.patch("/api/admin/users/:username", async (c) => {
    const admin = requireAdmin(c);
    if (admin instanceof Response) return admin;
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: "That account change was not valid." }, 400);
    const username = normalizeUsername(c.req.param("username"));
    const target = username ? (stmts.userByName.get(username) as UserRow | undefined) : undefined;
    if (!target) return c.json({ error: "That account is not here." }, 404);
    const hasRole = Object.prototype.hasOwnProperty.call(body, "role");
    const hasQuota = Object.prototype.hasOwnProperty.call(body, "quotaBytes");
    if (!hasRole && !hasQuota) return c.json({ error: "Nothing to change." }, 400);
    let role = target.role;
    let quotaBytes = target.quota_bytes;
    if (hasRole) {
      const parsed = parseRole(body.role, null);
      if (!parsed) return c.json({ error: "Role must be admin or user." }, 400);
      role = parsed;
    }
    if (hasQuota) {
      const parsed = integer(body.quotaBytes);
      if (parsed === null || !quotaAllowed(parsed)) {
        return c.json(
          {
            error: `Quota has to be between ${limitLabel(LIMITS.minQuotaBytes)} and ${limitLabel(LIMITS.quotaByteCeiling)}.`,
          },
          400,
        );
      }
      quotaBytes = parsed;
    }
    const saved = db.transaction(() => {
      const fresh = stmts.userById.get(target.id) as UserRow | undefined;
      if (!fresh) return "missing" as const;
      if (fresh.role === "admin" && role !== "admin" && countOf(stmts.adminCount.get()) <= 1) return "last" as const;
      stmts.updateAccount.run(role, quotaBytes, fresh.id);
      return "ok" as const;
    })();
    if (saved === "missing") return c.json({ error: "That account is not here." }, 404);
    if (saved === "last") return c.json({ error: "Drop needs at least one admin." }, 409);
    const updated = stmts.userById.get(target.id) as UserRow;
    return c.json({
      username: updated.username,
      role: updated.role,
      quotaBytes: effectiveQuota(updated.quota_bytes),
      usedBytes: countOf(stmts.usedBytes.get(updated.id)),
    });
  });

  app.delete("/api/admin/users/:username", (c) => {
    const admin = requireAdmin(c);
    if (admin instanceof Response) return admin;
    const username = normalizeUsername(c.req.param("username"));
    const target = username ? (stmts.userByName.get(username) as UserRow | undefined) : undefined;
    if (!target) return c.json({ error: "That account is not here." }, 404);
    const removed = db.transaction(() => {
      const fresh = stmts.userById.get(target.id) as UserRow | undefined;
      if (!fresh) return "missing" as const;
      if (fresh.role === "admin" && countOf(stmts.adminCount.get()) <= 1) return "last" as const;
      stmts.deleteInvitesByCreator.run(fresh.id);
      stmts.deleteUser.run(fresh.id);
      return "ok" as const;
    })();
    if (removed === "missing") return c.json({ error: "That account is not here." }, 404);
    if (removed === "last") return c.json({ error: "Drop needs at least one admin." }, 409);
    checkpoint(db);
    hub.disconnect(target.id);
    if (target.id === admin.id) {
      deleteCookie(c, COOKIE_NAME, { path: "/", secure: cookieSecure, sameSite: "Lax" });
    }
    return c.json({ ok: true });
  });

  app.get("/api/invites", (c) => {
    const admin = requireAdmin(c);
    if (admin instanceof Response) return admin;
    stmts.deleteExpiredInvites.run(Date.now());
    const rows = stmts.listInvites.all() as {
      id: string;
      username: string;
      role: Role;
      created_at: number;
      expires_at: number;
    }[];
    return c.json({
      invites: rows.map((row) => ({
        id: row.id,
        username: row.username,
        role: row.role,
        createdAt: row.created_at,
        expiresAt: row.expires_at,
      })),
    });
  });

  app.post("/api/invites", async (c) => {
    const admin = requireAdmin(c);
    if (admin instanceof Response) return admin;
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: "The invite was not valid." }, 400);
    const username = normalizeUsername(body.username);
    if (!username) {
      return c.json(
        { error: "Choose a username of 2–32 characters: a letter, then lowercase letters, digits, underscores, or hyphens." },
        400,
      );
    }
    const role = parseRole(body.role, "user");
    if (!role) return c.json({ error: "Role must be admin or user." }, 400);
    const now = Date.now();
    const id = randomUUID();
    const token = randomBytes(32).toString("base64url");
    const created = db.transaction(() => {
      stmts.deleteExpiredInvites.run(now);
      if (stmts.userByName.get(username) || stmts.inviteByName.get(username)) return false;
      stmts.insertInvite.run(id, hashToken(token), username, role, admin.id, now, now + LIMITS.inviteTtlMs);
      return true;
    })();
    if (!created) return c.json({ error: "That username is already in use." }, 409);
    return c.json({
      id,
      username,
      role,
      expiresAt: now + LIMITS.inviteTtlMs,
      link: `${externalOrigin(c)}/invite/${token}`,
    });
  });

  app.delete("/api/invites/:id", (c) => {
    const admin = requireAdmin(c);
    if (admin instanceof Response) return admin;
    const id = c.req.param("id");
    const result = stmts.deleteInvite.run(id);
    if (result.changes === 0) return c.json({ error: "That invite is not here." }, 404);
    return c.json({ ok: true });
  });

  app.post("/api/invites/preview", async (c) => {
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: INVITE_INVALID }, 400);
    const invite = findLiveInvite(body.token);
    if (!invite) return c.json({ error: INVITE_INVALID }, 400);
    return c.json({ username: invite.username, role: invite.role });
  });

  app.post("/api/invites/accept", async (c) => {
    const ip = clientIp(c);
    if (!limits.allow(`accept:${ip}`, 8, 60 * 60 * 1000)) {
      return c.json({ error: "Too many attempts. Wait a few minutes and try again." }, 429);
    }
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: INVITE_INVALID }, 400);
    const registration = parseRegistration(body);
    if (!registration) return c.json({ error: "The encryption setup was not valid. Reload and try again." }, 400);
    if (typeof body.token !== "string") return c.json({ error: INVITE_INVALID }, 400);
    const tokenHash = hashToken(body.token);
    const accepted = db.transaction(() => {
      const invite = stmts.inviteByHash.get(tokenHash) as InviteRow | undefined;
      const now = Date.now();
      if (!invite || invite.expires_at <= now) {
        if (invite) stmts.deleteInvite.run(invite.id);
        return null;
      }
      if (stmts.userByName.get(invite.username)) {
        stmts.deleteInvite.run(invite.id);
        return null;
      }
      const id = randomUUID();
      stmts.insertUser.run(
        id,
        invite.username,
        invite.role,
        registration.salt,
        registration.memory,
        registration.time,
        registration.parallelism,
        registration.verifierHash,
        registration.keyCheck,
        defaultQuotaBytes,
        now,
      );
      stmts.deleteInvite.run(invite.id);
      return { id, username: invite.username, role: invite.role };
    })();
    if (!accepted) return c.json({ error: INVITE_INVALID }, 400);
    issueSession(c, accepted.id);
    checkpoint(db);
    return c.json({ username: accepted.username, role: accepted.role });
  });

  app.get("/api/items", (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    purgeExpired();
    const rows = stmts.listItems.all(user.id) as { id: string; created_at: number; size: number }[];
    const fresh = stmts.userById.get(user.id) as UserRow;
    return c.json({
      items: rows.map((row) => ({ id: row.id, createdAt: row.created_at, size: row.size })),
      usedBytes: countOf(stmts.usedBytes.get(user.id)),
      quotaBytes: effectiveQuota(fresh.quota_bytes),
    });
  });

  app.post("/api/items", async (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    purgeExpired();
    const quota = effectiveQuota(user.quota_bytes);
    const used = countOf(stmts.usedBytes.get(user.id));
    const declared = Number(c.req.header("content-length") ?? "0");
    if (declared > 0 && used + declared > quota) {
      return c.json({ error: quotaMessage(quota, used) }, 413);
    }
    const ciphertext = Buffer.from(await c.req.arrayBuffer());
    if (ciphertext.length < LIMITS.minCiphertextBytes) {
      return c.json({ error: "That item is empty." }, 400);
    }
    const id = randomUUID();
    const createdAt = Date.now();
    const saved = db.transaction(() => {
      const current = countOf(stmts.usedBytes.get(user.id));
      if (current + ciphertext.length > quota) return false;
      stmts.insertItem.run(id, user.id, createdAt, ciphertext.length, ciphertext);
      return true;
    })();
    if (!saved) return c.json({ error: quotaMessage(quota, countOf(stmts.usedBytes.get(user.id))) }, 413);
    checkpoint(db);
    const item = { id, createdAt, size: ciphertext.length };
    hub.publish(user.id, { type: "created", item });
    return c.json(item, 201);
  });

  app.get("/api/items/:id", (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    purgeExpired();
    const row = stmts.itemForOwner.get(c.req.param("id"), user.id) as { ciphertext: Buffer } | undefined;
    if (!row) return c.json({ error: "That item is not here." }, 404);
    return c.body(new Uint8Array(row.ciphertext), 200, {
      "content-type": "application/octet-stream",
      "cache-control": "no-store",
    });
  });

  app.delete("/api/items/:id", (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    purgeExpired();
    const id = c.req.param("id");
    const result = stmts.deleteItem.run(id, user.id);
    if (result.changes === 0) return c.json({ error: "That item is not here." }, 404);
    checkpoint(db);
    hub.publish(user.id, { type: "deleted", id });
    return c.json({ ok: true });
  });

  app.post("/api/account/password/start", async (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: "The current password is wrong." }, 401);
    if (!limits.allow(`rekey:${user.id}`, 8, 15 * 60 * 1000)) {
      return c.json({ error: "Too many attempts. Wait a few minutes and try again." }, 429);
    }
    const provided = decodeExact(body.currentAuthVerifier, 32);
    if (!verifierMatches(user, provided)) return c.json({ error: "The current password is wrong." }, 401);
    limits.clear(`rekey:${user.id}`);
    purgeExpired();
    const rekeyId = randomUUID();
    const now = Date.now();
    db.transaction(() => {
      stmts.deleteOldRekeys.run(now - 15 * 60 * 1000);
      stmts.deleteRekeysForUser.run(user.id);
      stmts.insertRekey.run(rekeyId, user.id, now);
    })();
    const rows = stmts.listItems.all(user.id) as { id: string }[];
    return c.json({ rekeyId, items: rows.map((row) => ({ id: row.id })) });
  });

  app.put("/api/account/password/items/:id", async (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    const rekeyId = c.req.header("x-drop-rekey");
    if (!rekeyId || !stmts.rekeyForUser.get(rekeyId, user.id)) {
      return c.json({ error: "Start the password change again." }, 400);
    }
    const itemId = c.req.param("id");
    const owned = stmts.itemMetaForOwner.get(itemId, user.id) as { id: string } | undefined;
    if (!owned) return c.json({ error: "That item is not here." }, 404);
    const quota = effectiveQuota(user.quota_bytes);
    const declared = Number(c.req.header("content-length") ?? "0");
    if (declared > quota) return c.json({ error: quotaMessage(quota, countOf(stmts.usedBytes.get(user.id))) }, 413);
    const ciphertext = Buffer.from(await c.req.arrayBuffer());
    if (ciphertext.length > quota) {
      return c.json({ error: quotaMessage(quota, countOf(stmts.usedBytes.get(user.id))) }, 413);
    }
    if (ciphertext.length < LIMITS.minCiphertextBytes) return c.json({ error: "That item is empty." }, 400);
    db.transaction(() => {
      stmts.deleteRekeyItem.run(rekeyId, itemId);
      stmts.insertRekeyItem.run(rekeyId, itemId, ciphertext.length, ciphertext);
    })();
    return c.json({ ok: true });
  });

  app.post("/api/account/password/commit", async (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    const body = await readJson(c);
    if (body === "too-large") return c.json({ error: "That request is too large." }, 413);
    if (body === "invalid") return c.json({ error: "The encryption setup was not valid. Reload and try again." }, 400);
    const rekeyId = typeof body.rekeyId === "string" ? body.rekeyId : "";
    const registration = parseRegistration(body);
    if (!registration || !stmts.rekeyForUser.get(rekeyId, user.id)) {
      return c.json({ error: "Start the password change again." }, 400);
    }
    const token = getCookie(c, COOKIE_NAME);
    const tokenHash = token ? hashToken(token) : null;
    if (!tokenHash) return c.json({ error: "Sign in again." }, 401);
    const committed = db.transaction(() => {
      if (!stmts.rekeyForUser.get(rekeyId, user.id)) return "missing" as const;
      const live = stmts.listItems.all(user.id) as { id: string; created_at: number }[];
      const staged = stmts.listRekeyItems.all(rekeyId) as { item_id: string; size: number; ciphertext: Buffer }[];
      const stagedById = new Map(staged.map((row) => [row.item_id, row]));
      const liveIds = new Set(live.map((row) => row.id));
      const same =
        stagedById.size === live.length &&
        staged.length === live.length &&
        live.every((row) => stagedById.has(row.id)) &&
        staged.every((row) => liveIds.has(row.item_id));
      if (!same) {
        stmts.deleteRekey.run(rekeyId, user.id);
        return "changed" as const;
      }
      const quota = effectiveQuota(user.quota_bytes);
      const total = staged.reduce((sum, row) => sum + row.size, 0);
      if (total > quota) {
        stmts.deleteRekey.run(rekeyId, user.id);
        return "quota" as const;
      }
      for (const row of live) {
        const next = stagedById.get(row.id)!;
        stmts.deleteItem.run(row.id, user.id);
        stmts.insertItem.run(row.id, user.id, row.created_at, next.size, next.ciphertext);
      }
      stmts.updateSecrets.run(
        registration.salt,
        registration.memory,
        registration.time,
        registration.parallelism,
        registration.verifierHash,
        registration.keyCheck,
        user.id,
      );
      stmts.deleteOtherSessions.run(user.id, tokenHash);
      stmts.deleteRekey.run(rekeyId, user.id);
      return "ok" as const;
    })();
    if (committed === "changed") {
      checkpoint(db);
      return c.json({ error: "The clipboard changed. Start again. Nothing was switched." }, 409);
    }
    if (committed === "quota") {
      checkpoint(db);
      return c.json({ error: "The re-encrypted items don't fit in the quota. Nothing was switched." }, 413);
    }
    if (committed !== "ok") return c.json({ error: "Start the password change again." }, 400);
    checkpoint(db);
    hub.disconnect(user.id);
    const updated = stmts.userById.get(user.id) as UserRow;
    return c.json(accountView(updated));
  });

  app.delete("/api/account/password/:rekeyId", (c) => {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    stmts.deleteRekey.run(c.req.param("rekeyId"), user.id);
    checkpoint(db);
    return c.json({ ok: true });
  });

  app.get("*", (c) => {
    if (c.req.path.startsWith("/api")) return c.json({ error: "Not found." }, 404);
    return serveClient(c, clientDir);
  });

  app.notFound((c) => {
    if (c.req.path.startsWith("/api") || !clientDir) return c.json({ error: "Not found." }, 404);
    return serveClient(c, clientDir);
  });

  let httpServer: Server | null = null;
  let heartbeat: ReturnType<typeof setInterval> | null = null;
  let expiryTimer: ReturnType<typeof setInterval> | null = null;
  let wss: WebSocketServer | null = null;

  function listen(port: number, host = "127.0.0.1"): Promise<{ port: number }> {
    const server = createServer(getRequestListener(app.fetch));
    const sockets = new WebSocketServer({ noServer: true });
    server.on("upgrade", (req, socket, head) => {
      upgrade(req, socket, head, sockets);
    });
    const timer = setInterval(() => hub.ping(), 25_000);
    timer.unref();
    const expiry = setInterval(() => purgeExpired(), 60 * 60 * 1000);
    expiry.unref();
    httpServer = server;
    wss = sockets;
    heartbeat = timer;
    expiryTimer = expiry;
    return new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, host, () => {
        const address = server.address();
        const bound = typeof address === "object" && address ? address.port : port;
        resolve({ port: bound });
      });
    });
  }

  async function close(): Promise<void> {
    if (heartbeat) clearInterval(heartbeat);
    heartbeat = null;
    if (expiryTimer) clearInterval(expiryTimer);
    expiryTimer = null;
    hub.close();
    wss?.close();
    wss = null;
    const server = httpServer;
    httpServer = null;
    if (server) {
      await new Promise<void>((resolve, reject) => {
        server.close((err) => (err ? reject(err) : resolve()));
      });
    }
    try {
      checkpoint(db);
    } catch {
      // database already closed
    }
    try {
      db.close();
    } catch {
      // database already closed
    }
  }

  purgeExpired();

  return {
    request: app.request.bind(app),
    listen,
    close,
    userCount: () => countOf(stmts.userCount.get()),
    dbPath,
  };

  function upgrade(req: IncomingMessage, socket: Duplex, head: Buffer, sockets: WebSocketServer): void {
    const url = new URL(req.url ?? "/", "http://127.0.0.1");
    if (url.pathname !== "/api/ws") {
      socket.destroy();
      return;
    }
    const origin = headerOne(req.headers.origin);
    const host = headerOne(req.headers.host);
    const forwardedHost = headerOne(req.headers["x-forwarded-host"]);
    if (!originHeaderAllowed(origin, host, forwardedHost)) {
      rejectUpgrade(socket, 403);
      return;
    }
    const token = req.headers.cookie ? readCookie(req.headers.cookie, COOKIE_NAME) : null;
    const user = userFromToken(token);
    if (!user) {
      rejectUpgrade(socket, 401);
      return;
    }
    sockets.handleUpgrade(req, socket, head, (ws) => {
      hub.add(user.id, ws);
    });
  }

  function userFromToken(token: string | null | undefined): UserRow | null {
    if (!token || token.length < 20 || token.length > 200) return null;
    const hash = hashToken(token);
    const session = stmts.sessionByHash.get(hash) as SessionRow | undefined;
    if (!session) return null;
    const now = Date.now();
    if (session.expires_at <= now) {
      stmts.deleteSession.run(hash);
      return null;
    }
    const user = stmts.userById.get(session.user_id) as UserRow | undefined;
    if (!user) {
      stmts.deleteSession.run(hash);
      return null;
    }
    if (now - session.last_seen_at > 60 * 60 * 1000) {
      const expires = Math.min(now + LIMITS.sessionTtlMs, session.created_at + LIMITS.sessionMaxMs);
      stmts.touchSession.run(now, expires, hash);
    }
    return user;
  }

  function accountView(user: UserRow) {
    return {
      username: user.username,
      role: user.role,
      quotaBytes: effectiveQuota(user.quota_bytes),
      usedBytes: countOf(stmts.usedBytes.get(user.id)),
      kdf: kdfResponse(user.kdf_salt, user.kdf_memory, user.kdf_time, user.kdf_parallelism),
      keyCheck: bytesToB64url(user.key_check),
    };
  }

  function purgeExpired(): void {
    const cutoff = Date.now() - LIMITS.itemTtlMs;
    const due = stmts.expiredItems.all(cutoff) as { id: string; owner_id: string }[];
    if (due.length === 0) return;
    db.transaction(() => {
      stmts.deleteExpiredItems.run(cutoff);
    })();
    checkpoint(db);
    for (const row of due) hub.publish(row.owner_id, { type: "deleted", id: row.id });
  }

  function requireUser(c: Context<{ Variables: Vars }>): UserRow | Response {
    const token = getCookie(c, COOKIE_NAME);
    const user = userFromToken(token);
    if (!user || !token) return c.json({ error: "Sign in again." }, 401);
    c.set("userId", user.id);
    // Slide the browser expiry so an active device stays signed in.
    setCookie(c, COOKIE_NAME, token, {
      httpOnly: true,
      secure: cookieSecure,
      sameSite: "Lax",
      path: "/",
      maxAge: Math.floor(LIMITS.sessionTtlMs / 1000),
    });
    return user;
  }

  function requireAdmin(c: Context<{ Variables: Vars }>): UserRow | Response {
    const user = requireUser(c);
    if (user instanceof Response) return user;
    if (user.role !== "admin") return c.json({ error: "Only an admin can do that." }, 403);
    return user;
  }

  function issueSession(c: Context, userId: string): void {
    const token = randomBytes(32).toString("base64url");
    const now = Date.now();
    stmts.insertSession.run(hashToken(token), userId, now, now + LIMITS.sessionTtlMs, now);
    setCookie(c, COOKIE_NAME, token, {
      httpOnly: true,
      secure: cookieSecure,
      sameSite: "Lax",
      path: "/",
      maxAge: Math.floor(LIMITS.sessionTtlMs / 1000),
    });
    c.set("userId", userId);
  }

  function verifierMatches(user: UserRow | undefined, provided: Buffer | null): boolean {
    const stored = user?.auth_verifier_hash ?? dummyVerifierHash;
    const given = provided
      ? createHash("sha256").update(provided).digest()
      : createHash("sha256").update(pepper).update("missing").digest();
    return timingSafeEqual(stored, given) && user !== undefined && provided !== null;
  }

  function findLiveInvite(token: unknown): InviteRow | null {
    if (typeof token !== "string" || token.length < 20 || token.length > 200) return null;
    stmts.deleteExpiredInvites.run(Date.now());
    const invite = stmts.inviteByHash.get(hashToken(token)) as InviteRow | undefined;
    if (!invite || invite.expires_at <= Date.now()) return null;
    return invite;
  }

  function mutationAllowed(c: Context): boolean {
    if (c.req.method === "GET" || c.req.method === "HEAD" || c.req.method === "OPTIONS") return true;
    if (c.req.header(CSRF_HEADER) !== "1") return false;
    return originHeaderAllowed(
      c.req.header("origin"),
      c.req.header("host"),
      trustProxy ? c.req.header("x-forwarded-host") : undefined,
    );
  }

  function originHeaderAllowed(
    origin: string | undefined,
    host: string | undefined,
    forwardedHost: string | undefined,
  ): boolean {
    if (!origin) return true;
    let url: URL;
    try {
      url = new URL(origin);
    } catch {
      return false;
    }
    if (publicUrl && url.origin === publicUrl) return true;
    const forwarded = trustProxy ? forwardedHost?.split(",")[0]?.trim() : undefined;
    const expected = forwarded || host;
    return Boolean(expected && url.host === expected);
  }

  function clientIp(c: Context): string {
    if (trustProxy) {
      const real = c.req.header("x-real-ip")?.trim();
      if (real) return real.slice(0, 80);
      const forwarded = c.req.header("x-forwarded-for");
      if (forwarded) {
        const parts = forwarded.split(",").map((part) => part.trim()).filter(Boolean);
        const last = parts[parts.length - 1];
        if (last) return last.slice(0, 80);
      }
    }
    return "local";
  }

  function externalOrigin(c: Context): string {
    if (publicUrl) return publicUrl;
    const proto = trustProxy
      ? c.req.header("x-forwarded-proto")?.split(",")[0]?.trim() || "http"
      : "http";
    const host = trustProxy
      ? c.req.header("x-forwarded-host")?.split(",")[0]?.trim() || c.req.header("host")
      : c.req.header("host");
    return `${proto}://${host ?? "localhost"}`;
  }
}

function kdfResponse(salt: Uint8Array, memory: number, time: number, parallelism: number) {
  return {
    algo: KDF.algo,
    salt: bytesToB64url(salt),
    memory,
    time,
    parallelism,
  };
}

function parseRole(value: unknown, fallback: Role | null): Role | null {
  if (value === undefined || value === "") return fallback;
  if (value === "admin" || value === "user") return value;
  return null;
}

type Registration = {
  salt: Buffer;
  memory: number;
  time: number;
  parallelism: number;
  verifierHash: Buffer;
  keyCheck: Buffer;
};

function parseRegistration(body: JsonBody): Registration | null {
  const memory = integer(body.kdfMemory);
  const time = integer(body.kdfTime);
  const parallelism = integer(body.kdfParallelism);
  if (
    memory === null ||
    time === null ||
    parallelism === null ||
    !kdfIsCurrent({ algo: KDF.algo, memory, time, parallelism })
  ) {
    return null;
  }
  const salt = decodeExact(body.kdfSalt, KDF.saltLength);
  const verifier = decodeExact(body.authVerifier, 32);
  const keyCheck = decodeBetween(body.keyCheck, LIMITS.minCiphertextBytes, 128);
  if (!salt || !verifier || !keyCheck) return null;
  return {
    salt,
    memory,
    time,
    parallelism,
    verifierHash: createHash("sha256").update(verifier).digest(),
    keyCheck,
  };
}

function integer(value: unknown): number | null {
  if (typeof value !== "number" || !Number.isInteger(value)) return null;
  return value;
}

function decodeExact(value: unknown, length: number): Buffer | null {
  const bytes = decodeBytes(value, length, length);
  return bytes;
}

function decodeBetween(value: unknown, min: number, max: number): Buffer | null {
  return decodeBytes(value, min, max);
}

function decodeBytes(value: unknown, min: number, max: number): Buffer | null {
  if (typeof value !== "string" || value.length > 4096) return null;
  try {
    const bytes = b64urlToBytes(value);
    if (bytes.length < min || bytes.length > max) return null;
    return Buffer.from(bytes);
  } catch {
    return null;
  }
}

async function readJson(c: Context): Promise<JsonBody | "invalid" | "too-large"> {
  const declared = c.req.header("content-length");
  if (declared && Number(declared) > JSON_MAX) return "too-large";
  let text: string;
  try {
    text = await c.req.text();
  } catch {
    return "invalid";
  }
  if (text.length > JSON_MAX) return "too-large";
  try {
    const value = JSON.parse(text) as unknown;
    if (!value || typeof value !== "object" || Array.isArray(value)) return "invalid";
    return value as JsonBody;
  } catch {
    return "invalid";
  }
}

function secretMatches(provided: string, expected: string): boolean {
  const a = createHash("sha256").update(provided, "utf8").digest();
  const b = createHash("sha256").update(expected, "utf8").digest();
  return timingSafeEqual(a, b);
}

function readCookie(header: string, name: string): string | null {
  for (const part of header.split(";")) {
    const eq = part.indexOf("=");
    if (eq === -1) continue;
    if (part.slice(0, eq).trim() !== name) continue;
    const raw = part.slice(eq + 1).trim();
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  }
  return null;
}

function headerOne(value: string | string[] | undefined): string | undefined {
  if (Array.isArray(value)) return value[0];
  return value;
}

function rejectUpgrade(socket: Duplex, status: number): void {
  const reason = status === 401 ? "Unauthorized" : "Forbidden";
  socket.write(`HTTP/1.1 ${status} ${reason}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n`);
  socket.destroy();
}

function limitLabel(bytes: number): string {
  const gb = 1024 * 1024 * 1024;
  if (bytes >= gb) {
    const value = bytes / gb;
    return `${value >= 10 || Number.isInteger(value) ? Math.round(value) : value.toFixed(1)} GB`;
  }
  if (bytes >= 1024 * 1024) return `${Math.round(bytes / (1024 * 1024))} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} bytes`;
}

function quotaMessage(quota: number, used: number): string {
  return `That item doesn't fit. ${limitLabel(used)} of ${limitLabel(quota)} is already used.`;
}

function createLimiter(enabled: boolean) {
  const buckets = new Map<string, { count: number; reset: number }>();
  return {
    allow(key: string, limit: number, windowMs: number): boolean {
      if (!enabled) return true;
      const now = Date.now();
      if (buckets.size > 2000) {
        for (const [bucketKey, bucket] of buckets) {
          if (bucket.reset <= now) buckets.delete(bucketKey);
        }
      }
      const bucket = buckets.get(key);
      if (!bucket || bucket.reset <= now) {
        buckets.set(key, { count: 1, reset: now + windowMs });
        return true;
      }
      if (bucket.count >= limit) return false;
      bucket.count += 1;
      return true;
    },
    clear(key: string) {
      buckets.delete(key);
    },
  };
}

function resolveClientDir(explicit: string | null | undefined): string | null {
  if (explicit === null) return null;
  const candidate = explicit ? path.resolve(explicit) : path.resolve("dist/client");
  return fs.existsSync(path.join(candidate, "index.html")) ? candidate : null;
}

function serveClient(c: Context, clientDir: string | null): Response {
  if (!clientDir) return c.text("Drop is running, but the interface has not been built.", 503);
  const requested = c.req.path === "/" ? "/index.html" : c.req.path;
  const file = resolveInside(clientDir, requested);
  if (file && fs.existsSync(file) && fs.statSync(file).isFile()) {
    return fileResponse(file);
  }
  const ext = path.posix.extname(requested);
  if (ext && ext !== ".html") return c.text("Not found.", 404);
  const index = path.join(clientDir, "index.html");
  if (!fs.existsSync(index)) return c.text("Not found.", 404);
  return fileResponse(index);
}

function resolveInside(root: string, requestPath: string): string | null {
  let decoded: string;
  try {
    decoded = decodeURIComponent(requestPath);
  } catch {
    return null;
  }
  if (decoded.includes("\0")) return null;
  const rel = decoded.replace(/^\/+/, "");
  const rootResolved = path.resolve(root);
  const full = path.resolve(rootResolved, rel);
  if (full !== rootResolved && !full.startsWith(rootResolved + path.sep)) return null;
  return full;
}

function fileResponse(file: string): Response {
  const data = fs.readFileSync(file);
  const html = file.endsWith(`${path.sep}index.html`) || file.endsWith(".html");
  const asset = file.includes(`${path.sep}assets${path.sep}`);
  return new Response(data, {
    headers: {
      "content-type": contentType(file),
      "cache-control": html ? "no-store" : asset ? "public, max-age=31536000, immutable" : "public, max-age=3600",
    },
  });
}

function contentType(file: string): string {
  switch (path.extname(file).toLowerCase()) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
      return "text/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".svg":
      return "image/svg+xml";
    case ".woff2":
      return "font/woff2";
    case ".woff":
      return "font/woff";
    case ".ttf":
      return "font/ttf";
    case ".png":
      return "image/png";
    case ".ico":
      return "image/x-icon";
    case ".json":
      return "application/json";
    case ".map":
      return "application/json";
    default:
      return "application/octet-stream";
  }
}
