import { LIMITS } from "./constants.ts";

export function normalizePassword(password: string): string {
  return password.normalize("NFKC");
}

export function passwordError(password: string, confirm?: string): string | null {
  const pw = normalizePassword(password);
  if (pw.length < LIMITS.minPasswordLength) {
    return `Use at least ${LIMITS.minPasswordLength} characters.`;
  }
  if (pw.length > LIMITS.maxPasswordLength) {
    return "That password is too long.";
  }
  if (confirm !== undefined && normalizePassword(confirm) !== pw) {
    return "Those passwords don't match.";
  }
  return null;
}
