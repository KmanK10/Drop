import { useState, type FormEvent } from "react";
import { ApiError, api, apiBytes, apiJson, errorMessage, messageOf } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import type { Me } from "@/lib/types";
import { setContentKey } from "@/lib/vault";
import { b64urlToBytes, bytesToB64url } from "../../shared/bytes.ts";
import {
  createAccountMaterial,
  decrypt,
  deriveKeys,
  encrypt,
  importContentKey,
  registrationBody,
  verifyKeyCheck,
} from "../../shared/crypto.ts";
import { passwordError } from "../../shared/password.ts";
import { Notice } from "@/components/notice";
import { PasswordField } from "@/components/password-field";
import { Button } from "@/components/ui/button";

export function SettingsPanel({
  me,
  onUpdated,
  onRekeyed,
}: {
  me: Me;
  onUpdated: (me: Me) => void;
  onRekeyed: () => void;
}) {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [done, setDone] = useState("");
  const [busy, setBusy] = useState("");

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!current) {
      setError("Enter your current password.");
      return;
    }
    const pwError = passwordError(next, confirm);
    if (pwError) {
      setError(pwError);
      return;
    }
    setBusy("Checking the current password…");
    setError("");
    setDone("");
    let oldKey: CryptoKey | null = null;
    let newKey: CryptoKey | null = null;
    let rekeyId = "";
    try {
      const derived = await deriveKeys(current, b64urlToBytes(me.kdf.salt), me.kdf);
      const matches = await verifyKeyCheck(derived.contentKey, b64urlToBytes(me.keyCheck));
      if (!matches) {
        derived.authVerifier.fill(0);
        derived.contentKey.fill(0);
        setError("The current password is wrong.");
        return;
      }
      oldKey = await importContentKey(derived.contentKey);
      const material = await createAccountMaterial(next);
      newKey = await importContentKey(material.contentKey);
      const started = await apiJson<{ rekeyId: string; items: { id: string }[] }>("/api/account/password/start", {
        method: "POST",
        json: { currentAuthVerifier: bytesToB64url(derived.authVerifier) },
      });
      derived.authVerifier.fill(0);
      derived.contentKey.fill(0);
      rekeyId = started.rekeyId;
      for (let index = 0; index < started.items.length; index++) {
        const item = started.items[index]!;
        setBusy(`Re-encrypting ${index + 1} of ${started.items.length}…`);
        const bytes = await apiBytes(`/api/items/${item.id}`);
        const plain = await decrypt(oldKey, bytes);
        const ciphertext = await encrypt(newKey, plain);
        plain.fill(0);
        const response = await api(`/api/account/password/items/${item.id}`, {
          method: "PUT",
          body: new Blob([
            ciphertext.buffer.slice(ciphertext.byteOffset, ciphertext.byteOffset + ciphertext.byteLength) as ArrayBuffer,
          ]),
          headers: { "content-type": "application/octet-stream", "x-drop-rekey": rekeyId },
        });
        if (!response.ok) throw new ApiError(response.status, await errorMessage(response));
      }
      setBusy("Saving the new password…");
      const updated = await apiJson<Me>("/api/account/password/commit", {
        method: "POST",
        json: { rekeyId, ...registrationBody(material) },
      });
      material.authVerifier.fill(0);
      material.contentKey.fill(0);
      setContentKey(newKey);
      newKey = null;
      onUpdated(updated);
      onRekeyed();
      setCurrent("");
      setNext("");
      setConfirm("");
      setDone("Password changed. Other signed-in devices need the new password.");
      rekeyId = "";
    } catch (reason) {
      if (rekeyId) {
        await api(`/api/account/password/${rekeyId}`, { method: "DELETE" }).catch(() => undefined);
      }
      setError(messageOf(reason));
    } finally {
      setBusy("");
    }
  }

  return (
    <section className="min-w-0 rounded-xl border border-border bg-card p-4 shadow-sm sm:p-5">
      <h2 className="font-serif text-2xl font-medium">Password</h2>
      <p className="mt-1 text-sm leading-6 text-muted-foreground">
        This account is using {formatBytes(me.usedBytes)} of {formatBytes(me.quotaBytes)}. Changing the password
        re-encrypts every item in this browser before the old key is dropped. Drop never receives the password or
        either key. Other devices are signed out.
      </p>
      <form className="mt-4 grid w-full min-w-0 max-w-md gap-4" onSubmit={(event) => void onSubmit(event)}>
        {error ? <Notice>{error}</Notice> : null}
        {done ? <Notice tone="info">{done}</Notice> : null}
        <PasswordField
          label="Current password"
          name="current-password"
          autoComplete="current-password"
          value={current}
          onChange={setCurrent}
        />
        <PasswordField
          label="New password"
          name="new-password"
          autoComplete="new-password"
          value={next}
          onChange={setNext}
        />
        <PasswordField
          label="Confirm new password"
          name="confirm-password"
          autoComplete="new-password"
          value={confirm}
          onChange={setConfirm}
        />
        <Button type="submit" disabled={Boolean(busy)}>
          {busy || "Change password"}
        </Button>
      </form>
    </section>
  );
}
