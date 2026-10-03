/** Public Argon2id parameters. The server stores these; they are not secret. */
export const KDF = {
  algo: "argon2id" as const,
  /** KiB. OWASP minimum for Argon2id. */
  memory: 19_456,
  time: 2,
  parallelism: 1,
  hashLength: 64,
  saltLength: 16,
};

export const LIMITS = {
  /** Default storage quota for a new account. Names and file bytes are inside the ciphertext. */
  quotaBytes: 5 * 1024 * 1024 * 1024,
  /** Largest quota an admin can assign. Uploads are also clamped to this. */
  quotaByteCeiling: 32 * 1024 * 1024 * 1024,
  /** Smallest quota an admin can assign. */
  minQuotaBytes: 1024,
  /** Hard-delete items this long after they are created. */
  itemTtlMs: 30 * 24 * 60 * 60 * 1000,
  minPasswordLength: 10,
  maxPasswordLength: 200,
  inviteTtlMs: 48 * 60 * 60 * 1000,
  sessionTtlMs: 30 * 24 * 60 * 60 * 1000,
  sessionMaxMs: 180 * 24 * 60 * 60 * 1000,
  minCiphertextBytes: 28,
};

export const COOKIE_NAME = "drop_session";
export const CSRF_HEADER = "x-drop-request";
export const KEY_CHECK_TEXT = "drop-key-check-v1";

/** Production policy. wasm-unsafe-eval is only there so hash-wasm can compile Argon2id. */
export const CONTENT_SECURITY_POLICY =
  "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'";

/** Cap a stored quota. Values below the admin minimum are kept so tests can use a tiny quota. */
export function effectiveQuota(stored: number): number {
  if (!Number.isInteger(stored) || stored < 1) return LIMITS.quotaBytes;
  return Math.min(stored, LIMITS.quotaByteCeiling);
}

export function quotaAllowed(value: number): boolean {
  return Number.isInteger(value) && value >= LIMITS.minQuotaBytes && value <= LIMITS.quotaByteCeiling;
}

export type KdfParams = {
  algo: string;
  memory: number;
  time: number;
  parallelism: number;
};

export function kdfIsCurrent(params: KdfParams): boolean {
  return (
    params.algo === KDF.algo &&
    params.memory === KDF.memory &&
    params.time === KDF.time &&
    params.parallelism === KDF.parallelism
  );
}

/** Reject parameters that would make guessing a password cheap. */
export function assertStrongKdf(params: KdfParams): void {
  if (params.algo !== "argon2id") {
    throw new Error("Unsupported key derivation.");
  }
  if (params.memory < KDF.memory || params.time < KDF.time || params.parallelism < 1) {
    throw new Error("Key derivation parameters are too weak.");
  }
}
