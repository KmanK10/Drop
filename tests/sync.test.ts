import { afterAll, beforeAll, describe, expect, it } from "vitest";
import WebSocket from "ws";
import { utf8 } from "../src/shared/bytes.ts";
import { createAccountMaterial, encrypt, importContentKey, registrationBody } from "../src/shared/crypto.ts";
import { encodeItem } from "../src/shared/item.ts";
import type { DropApp } from "../src/server/app.ts";
import { createTestApp, send, sessionCookie } from "./helpers.ts";

const SETUP = "setup-secret-marker-DoNotStore";

function waitFor(socket: WebSocket, type: string, timeoutMs = 3000): Promise<{ type: string; item?: { id: string }; id?: string }> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timed out waiting for ${type}`)), timeoutMs);
    const onMessage = (data: WebSocket.RawData) => {
      const message = JSON.parse(data.toString()) as { type: string; item?: { id: string }; id?: string };
      if (message.type !== type) return;
      clearTimeout(timer);
      socket.off("message", onMessage);
      resolve(message);
    };
    socket.on("message", onMessage);
  });
}

describe("websocket sync", () => {
  let app: DropApp;
  let url = "";
  let adminCookie = "";
  let secondCookie = "";
  let friendCookie = "";
  let adminKey: CryptoKey;

  beforeAll(async () => {
    const started = createTestApp();
    app = started.app;
    const admin = await createAccountMaterial("sync-admin-password");
    const setup = await send(app, "/api/setup", {
      json: { ...registrationBody(admin), username: "ada", setupSecret: SETUP },
    });
    adminCookie = sessionCookie(setup);
    adminKey = await importContentKey(admin.contentKey);
    const params = await send(app, "/api/auth/params", { json: { username: "ada" } });
    const salt = (await params.json()) as { salt: string; memory: number; time: number; parallelism: number; algo: string };
    const { deriveKeys, loginBody } = await import("../src/shared/crypto.ts");
    const { b64urlToBytes } = await import("../src/shared/bytes.ts");
    const again = await deriveKeys("sync-admin-password", b64urlToBytes(salt.salt), salt);
    const login = await send(app, "/api/auth/login", { json: loginBody("ada", again.authVerifier) });
    secondCookie = sessionCookie(login);

    const invite = await send(app, "/api/invites", { json: { username: "bea" }, cookie: adminCookie });
    const link = ((await invite.json()) as { link: string }).link;
    const token = link.split("/invite/")[1] ?? "";
    const friend = await createAccountMaterial("sync-friend-password");
    const accepted = await send(app, "/api/invites/accept", {
      json: { ...registrationBody(friend), token },
    });
    friendCookie = sessionCookie(accepted);
    const listening = await app.listen(0, "127.0.0.1");
    url = `ws://127.0.0.1:${listening.port}/api/ws`;
  });

  afterAll(async () => {
    await app?.close();
  });

  it("tells every session of the owner, and nobody else, when an item is created or deleted", async () => {
    const origin = url.replace("ws://", "http://").replace("/api/ws", "");
    const open = (cookie: string) =>
      new Promise<WebSocket>((resolve, reject) => {
        const socket = new WebSocket(url, { headers: { cookie: `drop_session=${cookie}`, origin } });
        socket.once("open", () => resolve(socket));
        socket.once("error", reject);
      });

    const denied = await new Promise<number>((resolve, reject) => {
      const socket = new WebSocket(url, { headers: { origin } });
      socket.once("open", () => reject(new Error("unauthenticated socket opened")));
      socket.on("unexpected-response", (_request, response) => resolve(response.statusCode ?? 0));
      socket.once("error", () => {});
    });
    expect(denied).toBe(401);

    const [first, second, friend] = await Promise.all([open(adminCookie), open(secondCookie), open(friendCookie)]);
    const friendEvents: string[] = [];
    friend.on("message", (data) => friendEvents.push(data.toString()));
    const firstCreated = waitFor(first, "created");
    const secondCreated = waitFor(second, "created");

    const ciphertext = await encrypt(
      adminKey,
      encodeItem({ kind: "file", name: "sync.bin", mime: "application/octet-stream", body: utf8("sync-body") }),
    );
    const created = await send(app, "/api/items", { method: "POST", body: ciphertext, cookie: adminCookie });
    const item = (await created.json()) as { id: string };
    expect(created.status).toBe(201);

    const [a, b] = await Promise.all([firstCreated, secondCreated]);
    expect(a.item?.id).toBe(item.id);
    expect(b.item?.id).toBe(item.id);
    await new Promise((resolve) => setTimeout(resolve, 250));
    expect(friendEvents.some((entry) => entry.includes(item.id))).toBe(false);

    const firstDeleted = waitFor(first, "deleted");
    const secondDeleted = waitFor(second, "deleted");
    expect((await send(app, `/api/items/${item.id}`, { method: "DELETE", cookie: secondCookie })).status).toBe(200);
    const [deletedA, deletedB] = await Promise.all([firstDeleted, secondDeleted]);
    expect(deletedA.id).toBe(item.id);
    expect(deletedB.id).toBe(item.id);
    expect(friendEvents.some((entry) => entry.includes(`"deleted"`) && entry.includes(item.id))).toBe(false);

    for (const socket of [first, second, friend]) socket.close();
  });
});
