import { useEffect, useState, type FormEvent } from "react";
import { useNavigate } from "@/lib/nav";
import { apiJson, messageOf } from "@/lib/api";
import { retentionLabel } from "@/lib/format";
import { setContentKey } from "@/lib/vault";
import { createAccountMaterial, importContentKey, registrationBody } from "../../shared/crypto.ts";
import { passwordError } from "../../shared/password.ts";
import { Notice } from "@/components/notice";
import { PasswordField } from "@/components/password-field";
import { Wordmark } from "@/components/wordmark";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import type { Meta, Role } from "@/lib/types";

export function InvitePage({ token }: { token: string }) {
  const go = useNavigate();
  const [preview, setPreview] = useState<{ username: string; role: Role } | null>(null);
  const [loadError, setLoadError] = useState("");
  const [loading, setLoading] = useState(true);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [retention, setRetention] = useState("30 days");

  useEffect(() => {
    let cancelled = false;
    void apiJson<{ username: string; role: Role }>("/api/invites/preview", {
      method: "POST",
      json: { token },
    })
      .then((result) => {
        if (!cancelled) setPreview(result);
      })
      .catch((reason: unknown) => {
        if (!cancelled) setLoadError(messageOf(reason));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  useEffect(() => {
    void apiJson<Meta>("/api/meta")
      .then((meta) => setRetention(retentionLabel(meta.itemTtlMs)))
      .catch(() => undefined);
  }, []);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    const pwError = passwordError(password, confirm);
    if (pwError) {
      setError(pwError);
      return;
    }
    setBusy(true);
    setError("");
    try {
      const material = await createAccountMaterial(password);
      const key = await importContentKey(material.contentKey);
      material.contentKey.fill(0);
      await apiJson("/api/invites/accept", {
        method: "POST",
        json: { ...registrationBody(material), token },
      });
      material.authVerifier.fill(0);
      setContentKey(key);
      setPassword("");
      setConfirm("");
      go("/");
    } catch (reason) {
      setContentKey(null);
      setError(messageOf(reason));
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto flex min-h-dvh w-full min-w-0 max-w-md flex-col px-4 py-6 sm:py-10">
      <div className="my-auto w-full min-w-0">
      <Wordmark />
      <Card className="mt-6">
        <CardHeader>
          <CardTitle>You've been invited</CardTitle>
          <CardDescription>
            Choose a password for this account. It never leaves this browser. The person who invited you cannot read
            what you save. Finishing this signs this browser into the new account. Items are deleted {retention} after
            they are saved. That delete is permanent. There is no trash.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? <p className="text-sm text-muted-foreground">Checking the invite…</p> : null}
          {loadError ? (
            <div className="grid gap-4">
              <Notice>{loadError}</Notice>
              <Button variant="outline" onClick={() => go("/")}>
                Go to sign in
              </Button>
            </div>
          ) : null}
          {preview ? (
            <form className="grid gap-4" onSubmit={(event) => void onSubmit(event)}>
              <p className="rounded-lg bg-secondary px-3 py-2 text-sm leading-6">
                Anything you save is deleted {retention} after it is saved. Drop removes the encrypted copy for good.
                There is no trash.
              </p>
              <p className="rounded-lg bg-secondary px-3 py-2 text-sm">
                Username <span className="font-medium">{preview.username}</span>
                <span className="text-muted-foreground"> · {preview.role === "admin" ? "admin" : "member"}</span>
              </p>
              {error ? <Notice>{error}</Notice> : null}
              <PasswordField
                label="Password"
                name="new-password"
                autoComplete="new-password"
                value={password}
                onChange={setPassword}
              />
              <PasswordField
                label="Confirm password"
                name="confirm-password"
                autoComplete="new-password"
                value={confirm}
                onChange={setConfirm}
              />
              <Button type="submit" disabled={busy}>
                {busy ? "Saving password…" : "Create account"}
              </Button>
            </form>
          ) : null}
        </CardContent>
      </Card>
      </div>
    </div>
  );
}
