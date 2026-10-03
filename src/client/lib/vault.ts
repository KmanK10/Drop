/** Content key for this tab. A reload clears it. It is never written to storage. */
let contentKey: CryptoKey | null = null;

export function setContentKey(key: CryptoKey | null): void {
  contentKey = key;
}

export function getContentKey(): CryptoKey | null {
  return contentKey;
}
