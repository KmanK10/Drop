const USERNAME = /^[a-z][a-z0-9_-]{1,31}$/;

export function normalizeUsername(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const name = value.normalize("NFKC").trim().toLowerCase();
  if (!USERNAME.test(name)) return null;
  return name;
}

export function usernameHint(): string {
  return "2–32 characters: start with a letter, then lowercase letters, digits, underscores, or hyphens.";
}
