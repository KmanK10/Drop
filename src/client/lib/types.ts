export type Role = "admin" | "user";

export type Meta = {
  setupRequired: boolean;
  quotaBytes: number;
  quotaByteCeiling: number;
  minQuotaBytes: number;
  itemTtlMs: number;
  kdf: {
    algo: string;
    memory: number;
    time: number;
    parallelism: number;
  };
};

export type Me = {
  username: string;
  role: Role;
  quotaBytes: number;
  usedBytes: number;
  kdf: {
    algo: string;
    salt: string;
    memory: number;
    time: number;
    parallelism: number;
  };
  keyCheck: string;
};

export type ItemMeta = {
  id: string;
  createdAt: number;
  size: number;
};

export type InviteSummary = {
  id: string;
  username: string;
  role: Role;
  createdAt: number;
  expiresAt: number;
};

export type AccountSummary = {
  username: string;
  role: Role;
  createdAt: number;
  quotaBytes: number;
  usedBytes: number;
};
