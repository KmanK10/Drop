import { useState, type FormEvent } from "react";
import { useNavigate } from "@/lib/nav";
import { apiJson, messageOf } from "@/lib/api";
import { setContentKey } from "@/lib/vault";
import { b64urlToBytes } from "../../shared/bytes.ts";
import { deriveKeys, importContentKey, loginBody } from "../../shared/crypto.ts";
import { normalizeUsername } from "../../shared/username.ts";
import { Notice } from "@/components/notice";
import { PasswordField } from "@/components/password-field";
import { Wordmark } from "@/components/wordmark";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { Meta } from "@/lib/types";

export function LoginPage({
  meta,
  onSignedIn,
}: {
  meta: Meta | null;
  onSignedIn: () => Promise<"anon" | "in" | "error">;
}) {
  const go = useNavigate();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    const name = normalizeUsername(username);
    if (!name) {
      setError("Enter the username you were given.");
      return;
    }
    if (!password) {
      setError("Enter your password.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const params = await apiJson<{
        algo: string;
        salt: string;
        memory: number;
        time: number;
        parallelism: number;
      }>("/api/auth/params", { method: "POST", json: { username: name } });
      const { authVerifier, contentKey } = await deriveKeys(password, b64urlToBytes(params.salt), params);
      const key = await importContentKey(contentKey);
      contentKey.fill(0);
      try {
        await apiJson("/api/auth/login", { method: "POST", json: loginBody(name, authVerifier) });
      } finally {
        authVerifier.fill(0);
      }
      setContentKey(key);
      setPassword("");
      const result = await onSignedIn();
      if (result !== "in") {
        setContentKey(null);
        setError(result === "anon" ? "Sign-in didn't stick. Try again." : "Can't reach the server.");
        setBusy(false);
      }
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
          <CardTitle>Sign in</CardTitle>
        </CardHeader>
        <CardContent>
          <form className="grid gap-4" onSubmit={(event) => void onSubmit(event)}>
            {error ? <Notice>{error}</Notice> : null}
            <div className="grid gap-2">
              <Label htmlFor="username">Username</Label>
              <Input
                id="username"
                name="username"
                autoComplete="username"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                value={username}
                onChange={(event) => setUsername(event.target.value)}
                required
              />
            </div>
            <PasswordField
              label="Password"
              name="password"
              autoComplete="current-password"
              value={password}
              onChange={setPassword}
            />
            <Button type="submit" disabled={busy}>
              {busy ? "Signing in…" : "Sign in"}
            </Button>
          </form>
        </CardContent>
      </Card>
      {meta?.setupRequired ? (
        <button
          type="button"
          className="mt-4 min-h-11 text-sm text-muted-foreground underline-offset-4 hover:underline"
          onClick={() => go("/setup")}
        >
          First time on this server? Set up the admin account.
        </button>
      ) : (
        <p className="mt-4 text-sm leading-6 text-muted-foreground">
          Accounts are invite-only. Ask the person who runs this Drop for a link.
        </p>
      )}
      </div>
    </div>
  );
}
