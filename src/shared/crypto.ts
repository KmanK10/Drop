import { argon2id } from "hash-wasm";
import { assertStrongKdf, KDF, KEY_CHECK_TEXT, type KdfParams } from "./constants.ts";
import { bytesToB64url, sha256, timingEqual, utf8 } from "./bytes.ts";
import { normalizePassword } from "./password.ts";

/**
 * Split a password into two independent secrets:
 * - authVerifier is sent to the server and stored only as a hash
 * - contentKey never leaves the browser and encrypts item bytes
 *
 * One Argon2id run produces 64 bytes. The halves are domain-separated by
 * position, then the auth half is hashed again before it is sent.
 */
export async function deriveKeys(
  password: string,
  salt: Uint8Array,
  params: KdfParams,
): Promise<{ authVerifier: Uint8Array; contentKey: Uint8Array }> {
  assertStrongKdf(params);
  if (salt.length < 16) throw new Error("Salt is too short.");
  const normalized = normalizePassword(password);
  const derived = new Uint8Array(
    await argon2id({
      password: normalized,
      salt,
      parallelism: params.parallelism,
      iterations: params.time,
      memorySize: params.memory,
      hashLength: KDF.hashLength,
      outputType: "binary",
    }),
  );
  const authSecret = derived.subarray(0, 32);
  const contentKey = derived.slice(32, 64);
  const authVerifier = await sha256(authSecret);
  derived.fill(0);
  return { authVerifier, contentKey };
}

export async function importContentKey(raw: Uint8Array): Promise<CryptoKey> {
  const copy = raw.buffer.slice(raw.byteOffset, raw.byteOffset + raw.byteLength) as ArrayBuffer;
  return crypto.subtle.importKey("raw", copy, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
}

export async function encrypt(key: CryptoKey, plaintext: Uint8Array): Promise<Uint8Array> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const copy = plaintext.buffer.slice(
    plaintext.byteOffset,
    plaintext.byteOffset + plaintext.byteLength,
  ) as ArrayBuffer;
  const ciphertext = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, copy));
  const out = new Uint8Array(iv.length + ciphertext.length);
  out.set(iv, 0);
  out.set(ciphertext, iv.length);
  return out;
}

export async function decrypt(key: CryptoKey, blob: Uint8Array): Promise<Uint8Array> {
  if (blob.length < 12 + 16) throw new Error("Ciphertext is too short.");
  const iv = blob.subarray(0, 12);
  const ciphertext = blob.subarray(12);
  const ct = ciphertext.buffer.slice(
    ciphertext.byteOffset,
    ciphertext.byteOffset + ciphertext.byteLength,
  ) as ArrayBuffer;
  const ivCopy = iv.buffer.slice(iv.byteOffset, iv.byteOffset + iv.byteLength) as ArrayBuffer;
  const plain = await crypto.subtle.decrypt({ name: "AES-GCM", iv: ivCopy }, key, ct);
  return new Uint8Array(plain);
}

export async function makeKeyCheck(contentKey: Uint8Array): Promise<Uint8Array> {
  const key = await importContentKey(contentKey);
  return encrypt(key, utf8(KEY_CHECK_TEXT));
}

export async function verifyKeyCheck(contentKey: Uint8Array, blob: Uint8Array): Promise<boolean> {
  try {
    const key = await importContentKey(contentKey);
    const plain = await decrypt(key, blob);
    return timingEqual(plain, utf8(KEY_CHECK_TEXT));
  } catch {
    return false;
  }
}

export type AccountMaterial = {
  salt: Uint8Array;
  authVerifier: Uint8Array;
  contentKey: Uint8Array;
  keyCheck: Uint8Array;
};

export async function createAccountMaterial(password: string): Promise<AccountMaterial> {
  const salt = crypto.getRandomValues(new Uint8Array(KDF.saltLength));
  const { authVerifier, contentKey } = await deriveKeys(password, salt, KDF);
  const keyCheck = await makeKeyCheck(contentKey);
  return { salt, authVerifier, contentKey, keyCheck };
}

/** Fields the browser is allowed to send when creating an account. */
export function registrationBody(material: AccountMaterial): {
  authVerifier: string;
  kdfSalt: string;
  kdfMemory: number;
  kdfTime: number;
  kdfParallelism: number;
  keyCheck: string;
} {
  return {
    authVerifier: bytesToB64url(material.authVerifier),
    kdfSalt: bytesToB64url(material.salt),
    kdfMemory: KDF.memory,
    kdfTime: KDF.time,
    kdfParallelism: KDF.parallelism,
    keyCheck: bytesToB64url(material.keyCheck),
  };
}

export function loginBody(username: string, authVerifier: Uint8Array): {
  username: string;
  authVerifier: string;
} {
  return { username, authVerifier: bytesToB64url(authVerifier) };
}
