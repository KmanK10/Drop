import { createHash } from "node:crypto";
import { argon2id } from "hash-wasm";
import { describe, expect, it } from "vitest";
import { bytesToB64url, utf8 } from "../src/shared/bytes.ts";
import { KDF } from "../src/shared/constants.ts";
import {
  createAccountMaterial,
  decrypt,
  deriveKeys,
  encrypt,
  importContentKey,
  loginBody,
  registrationBody,
  verifyKeyCheck,
} from "../src/shared/crypto.ts";
import { decodeItem, encodeItem } from "../src/shared/item.ts";
import { passwordError } from "../src/shared/password.ts";
import { normalizeUsername } from "../src/shared/username.ts";

const VECTOR = "4bd8ebfb202d2c08d9f467099ed686f4d05dc192458b76b0b4d925526abbed76607e940d53e0478f9bbffe3314fb22164beedacb4171bd29807fbbc54a760323";

describe("split-key derivation", () => {
  it("matches the Argon2id vector and splits verifier from content key", async () => {
    const salt = new Uint8Array(16).fill(0x11);
    const derived = Buffer.from(
      await argon2id({
        password: "drop-test-password",
        salt,
        parallelism: KDF.parallelism,
        iterations: KDF.time,
        memorySize: KDF.memory,
        hashLength: KDF.hashLength,
        outputType: "binary",
      }),
    );
    expect(derived.toString("hex")).toBe(VECTOR);

    const { authVerifier, contentKey } = await deriveKeys("drop-test-password", salt, KDF);
    expect(Buffer.from(contentKey).equals(derived.subarray(32))).toBe(true);
    expect(Buffer.from(authVerifier).equals(createHash("sha256").update(derived.subarray(0, 32)).digest())).toBe(true);
    expect(Buffer.from(authVerifier).equals(Buffer.from(contentKey))).toBe(false);
  });

  it("normalizes passwords so devices agree, and a different password does not", async () => {
    const salt = crypto.getRandomValues(new Uint8Array(16));
    const latin = await deriveKeys("password1", salt, KDF);
    const fullwidth = await deriveKeys("password\uFF11", salt, KDF);
    const other = await deriveKeys("password2", salt, KDF);
    expect(Buffer.from(latin.contentKey).equals(Buffer.from(fullwidth.contentKey))).toBe(true);
    expect(Buffer.from(latin.authVerifier).equals(Buffer.from(fullwidth.authVerifier))).toBe(true);
    expect(Buffer.from(latin.contentKey).equals(Buffer.from(other.contentKey))).toBe(false);
  });

  it("refuses weak KDF parameters", async () => {
    const salt = crypto.getRandomValues(new Uint8Array(16));
    await expect(deriveKeys("drop-test-password", salt, { ...KDF, memory: 1024 })).rejects.toThrow(/too weak/i);
  });
});

describe("item encryption", () => {
  it("round-trips text and files and rejects the wrong key or a tampered blob", async () => {
    const material = await createAccountMaterial("a-fine-password");
    const key = await importContentKey(material.contentKey);
    const name = "notes/secret-plan.txt";
    const text = "meet at the north door";
    const encoded = encodeItem({ kind: "text", name, mime: "text/plain", body: utf8(text) });
    const blob = await encrypt(key, encoded);
    expect(Buffer.from(blob).includes(Buffer.from(name))).toBe(false);
    expect(Buffer.from(blob).includes(Buffer.from(text))).toBe(false);
    const decoded = decodeItem(await decrypt(key, blob));
    expect(decoded.kind).toBe("text");
    expect(decoded.name).toBe(name);
    expect(decoded.text).toBe(text);

    const fileBytes = Uint8Array.from([1, 2, 3, 255, 0, 9]);
    const fileBlob = await encrypt(
      key,
      encodeItem({ kind: "file", name: "photo.bin", mime: "application/octet-stream", body: fileBytes }),
    );
    const file = decodeItem(await decrypt(key, fileBlob));
    expect(file.kind).toBe("file");
    expect(Buffer.from(file.body).equals(Buffer.from(fileBytes))).toBe(true);

    const tampered = new Uint8Array(blob);
    tampered[tampered.length - 1] ^= 0xff;
    await expect(decrypt(key, tampered)).rejects.toThrow();

    const wrong = await importContentKey(crypto.getRandomValues(new Uint8Array(32)));
    await expect(decrypt(wrong, blob)).rejects.toThrow();
    expect(await verifyKeyCheck(material.contentKey, material.keyCheck)).toBe(true);
    expect(await verifyKeyCheck(crypto.getRandomValues(new Uint8Array(32)), material.keyCheck)).toBe(false);
  });
});

describe("registration payload", () => {
  it("sends a verifier and key check, never the password or content key", async () => {
    const password = "a-fine-password";
    const material = await createAccountMaterial(password);
    const body = registrationBody(material);
    expect(Object.keys(body).sort()).toEqual([
      "authVerifier",
      "kdfMemory",
      "kdfParallelism",
      "kdfSalt",
      "kdfTime",
      "keyCheck",
    ]);
    const json = JSON.stringify(body);
    expect(json.includes(password)).toBe(false);
    expect(json.includes(bytesToB64url(material.contentKey))).toBe(false);
    const login = loginBody("ada", material.authVerifier);
    expect(JSON.stringify(login).includes(bytesToB64url(material.contentKey))).toBe(false);
    expect(passwordError(password, password)).toBeNull();
    expect(passwordError("short")).toMatch(/10/);
    expect(normalizeUsername("Ada")).toBe("ada");
    expect(normalizeUsername("no spaces")).toBeNull();
  });
});
