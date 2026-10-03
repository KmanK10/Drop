import fs from "node:fs";
import path from "node:path";
import { createHash, randomBytes } from "node:crypto";
import Database from "better-sqlite3";
import { LIMITS } from "../shared/constants.ts";

export type Role = "admin" | "user";

export type UserRow = {
  id: string;
  username: string;
  role: Role;
  kdf_salt: Buffer;
  kdf_memory: number;
  kdf_time: number;
  kdf_parallelism: number;
  auth_verifier_hash: Buffer;
  key_check: Buffer;
  quota_bytes: number;
  created_at: number;
};

export type SessionRow = {
  token_hash: Buffer;
  user_id: string;
  created_at: number;
  expires_at: number;
  last_seen_at: number;
};

export type InviteRow = {
  id: string;
  token_hash: Buffer;
  username: string;
  role: Role;
  created_by: string;
  created_at: number;
  expires_at: number;
};

export type ItemMeta = {
  id: string;
  created_at: number;
  size: number;
};

export function openDatabase(dataDir: string): Database.Database {
  fs.mkdirSync(dataDir, { recursive: true });
  const db = new Database(path.join(dataDir, "drop.sqlite"));
  db.pragma("journal_mode = WAL");
  db.pragma("foreign_keys = ON");
  db.pragma("secure_delete = ON");
  db.pragma("synchronous = FULL");
  db.pragma("busy_timeout = 5000");
  db.pragma("temp_store = MEMORY");
  db.exec(`
    CREATE TABLE IF NOT EXISTS users (
      id TEXT PRIMARY KEY,
      username TEXT NOT NULL UNIQUE,
      role TEXT NOT NULL CHECK (role IN ('admin', 'user')),
      kdf_salt BLOB NOT NULL,
      kdf_memory INTEGER NOT NULL,
      kdf_time INTEGER NOT NULL,
      kdf_parallelism INTEGER NOT NULL,
      auth_verifier_hash BLOB NOT NULL,
      key_check BLOB NOT NULL,
      quota_bytes INTEGER NOT NULL DEFAULT ${LIMITS.quotaBytes},
      created_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS sessions (
      token_hash BLOB PRIMARY KEY,
      user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      created_at INTEGER NOT NULL,
      expires_at INTEGER NOT NULL,
      last_seen_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS invites (
      id TEXT PRIMARY KEY,
      token_hash BLOB NOT NULL UNIQUE,
      username TEXT NOT NULL UNIQUE,
      role TEXT NOT NULL CHECK (role IN ('admin', 'user')),
      created_by TEXT NOT NULL REFERENCES users(id),
      created_at INTEGER NOT NULL,
      expires_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS items (
      id TEXT PRIMARY KEY,
      owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      created_at INTEGER NOT NULL,
      size INTEGER NOT NULL,
      ciphertext BLOB NOT NULL
    );

    CREATE INDEX IF NOT EXISTS items_owner_created ON items (owner_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS sessions_user ON sessions (user_id);

    CREATE TABLE IF NOT EXISTS rekeys (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      created_at INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS rekey_items (
      rekey_id TEXT NOT NULL REFERENCES rekeys(id) ON DELETE CASCADE,
      item_id TEXT NOT NULL,
      size INTEGER NOT NULL,
      ciphertext BLOB NOT NULL,
      PRIMARY KEY (rekey_id, item_id)
    );

    CREATE TABLE IF NOT EXISTS server_meta (
      key TEXT PRIMARY KEY,
      value BLOB NOT NULL
    );
  `);
  const existing = db.prepare("SELECT value FROM server_meta WHERE key = 'pepper'").get() as
    | { value: Buffer }
    | undefined;
  if (!existing) {
    db.prepare("INSERT INTO server_meta (key, value) VALUES ('pepper', ?)").run(randomBytes(32));
  }
  const userColumns = db.prepare("PRAGMA table_info(users)").all() as { name: string }[];
  if (!userColumns.some((column) => column.name === "quota_bytes")) {
    db.exec(`ALTER TABLE users ADD COLUMN quota_bytes INTEGER NOT NULL DEFAULT ${LIMITS.quotaBytes}`);
  }
  const after = db.prepare("PRAGMA table_info(users)").all() as { name: string }[];
  if (after.some((column) => column.name === "max_item_bytes")) {
    db.exec("ALTER TABLE users DROP COLUMN max_item_bytes");
  }
  return db;
}

export function checkpoint(db: Database.Database): void {
  db.pragma("wal_checkpoint(TRUNCATE)");
}

export function hashToken(token: string): Buffer {
  return createHash("sha256").update(token).digest();
}

export function statements(db: Database.Database) {
  return {
    userCount: db.prepare("SELECT COUNT(*) AS n FROM users"),
    userByName: db.prepare("SELECT * FROM users WHERE username = ?"),
    userById: db.prepare("SELECT * FROM users WHERE id = ?"),
    insertUser: db.prepare(`
      INSERT INTO users (
        id, username, role, kdf_salt, kdf_memory, kdf_time, kdf_parallelism,
        auth_verifier_hash, key_check, quota_bytes, created_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `),
    listUsers: db.prepare(`
      SELECT username, role, created_at, quota_bytes,
             COALESCE((SELECT SUM(size) FROM items WHERE owner_id = users.id), 0) AS used_bytes
      FROM users ORDER BY created_at ASC
    `),
    adminCount: db.prepare("SELECT COUNT(*) AS n FROM users WHERE role = 'admin'"),
    usedBytes: db.prepare("SELECT COALESCE(SUM(size), 0) AS n FROM items WHERE owner_id = ?"),
    expiredItems: db.prepare("SELECT id, owner_id FROM items WHERE created_at <= ?"),
    deleteExpiredItems: db.prepare("DELETE FROM items WHERE created_at <= ?"),
    updateAccount: db.prepare("UPDATE users SET role = ?, quota_bytes = ? WHERE id = ?"),
    updateSecrets: db.prepare(`
      UPDATE users
      SET kdf_salt = ?, kdf_memory = ?, kdf_time = ?, kdf_parallelism = ?,
          auth_verifier_hash = ?, key_check = ?
      WHERE id = ?
    `),
    deleteInvitesByCreator: db.prepare("DELETE FROM invites WHERE created_by = ?"),
    deleteUser: db.prepare("DELETE FROM users WHERE id = ?"),
    deleteOtherSessions: db.prepare("DELETE FROM sessions WHERE user_id = ? AND token_hash != ?"),
    deleteOldRekeys: db.prepare("DELETE FROM rekeys WHERE created_at <= ?"),
    deleteRekeysForUser: db.prepare("DELETE FROM rekeys WHERE user_id = ?"),
    insertRekey: db.prepare("INSERT INTO rekeys (id, user_id, created_at) VALUES (?, ?, ?)"),
    rekeyForUser: db.prepare("SELECT id, user_id, created_at FROM rekeys WHERE id = ? AND user_id = ?"),
    deleteRekey: db.prepare("DELETE FROM rekeys WHERE id = ? AND user_id = ?"),
    deleteRekeyItem: db.prepare("DELETE FROM rekey_items WHERE rekey_id = ? AND item_id = ?"),
    insertRekeyItem: db.prepare(
      "INSERT INTO rekey_items (rekey_id, item_id, size, ciphertext) VALUES (?, ?, ?, ?)",
    ),
    listRekeyItems: db.prepare("SELECT item_id, size, ciphertext FROM rekey_items WHERE rekey_id = ?"),
    pepper: db.prepare("SELECT value FROM server_meta WHERE key = 'pepper'"),
    insertSession: db.prepare(
      "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_seen_at) VALUES (?, ?, ?, ?, ?)",
    ),
    sessionByHash: db.prepare("SELECT * FROM sessions WHERE token_hash = ?"),
    touchSession: db.prepare(
      "UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?",
    ),
    deleteSession: db.prepare("DELETE FROM sessions WHERE token_hash = ?"),
    deleteExpiredInvites: db.prepare("DELETE FROM invites WHERE expires_at <= ?"),
    inviteByHash: db.prepare("SELECT * FROM invites WHERE token_hash = ?"),
    inviteByName: db.prepare("SELECT id FROM invites WHERE username = ?"),
    insertInvite: db.prepare(
      "INSERT INTO invites (id, token_hash, username, role, created_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    ),
    listInvites: db.prepare(
      "SELECT id, username, role, created_at, expires_at FROM invites ORDER BY created_at DESC",
    ),
    deleteInvite: db.prepare("DELETE FROM invites WHERE id = ?"),
    listItems: db.prepare(
      "SELECT id, created_at, size FROM items WHERE owner_id = ? ORDER BY created_at DESC, id DESC",
    ),
    itemMetaForOwner: db.prepare(
      "SELECT id, created_at FROM items WHERE id = ? AND owner_id = ?",
    ),
    insertItem: db.prepare(
      "INSERT INTO items (id, owner_id, created_at, size, ciphertext) VALUES (?, ?, ?, ?, ?)",
    ),
    itemForOwner: db.prepare(
      "SELECT ciphertext FROM items WHERE id = ? AND owner_id = ?",
    ),
    deleteItem: db.prepare("DELETE FROM items WHERE id = ? AND owner_id = ?"),
  };
}

export type Statements = ReturnType<typeof statements>;

export function countOf(row: unknown): number {
  return (row as { n: number }).n;
}
