import { useEffect, useState, type FormEvent } from "react";
import { useNavigate } from "@/lib/nav";
import { apiJson, messageOf } from "@/lib/api";
import { setContentKey } from "@/lib/vault";
import { createAccountMaterial, importContentKey, registrationBody } from "../../shared/crypto.ts";
import { passwordError } from "../../shared/password.ts";
import { normalizeUsername } from "../../shared/username.ts";
import { Notice } from "@/components/notice";
import { PasswordField } from "@/components/password-field";
import { Wordmark } from "@/components/wordmark";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { Meta } from "@/lib/types";

export function SetupPage() {
  const go = useNavigate();
  const [meta, setMeta] = useState<Meta | null>(null);
  const [loadError, setLoadError] = useState("");
  const [username, setUsername] = useState("");
  const [setupSecret, setSetupSecret] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void apiJson<Meta>("/api/meta")
      .then(setMeta)
      .catch((reason: unknown) => setLoadError(messageOf(reason)));
  }, []);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    const name = normalizeUsername(username);
    if (!name) {
      setError("Choose a username of 2–32 characters: a letter, then lowercase letters, digits, underscores, or hyphens.");
      return;
    }
    const pwError = passwordError(password, confirm);
    if (pwError) {
      setError(pwError);
      return;
    }
    if (setupSecret.length < 16) {
      setError("Enter the setup secret from the server environment. It is at least 16 characters.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const material = await createAccountMaterial(password);
      const key = await importContentKey(material.contentKey);
      material.contentKey.fill(0);
      await apiJson("/api/setup", {
        method: "POST",
        json: { ...registrationBody(material), username: name, setupSecret },
      });
      material.authVerifier.fill(0);
      setContentKey(key);
      setPassword("");
      setConfirm("");
      setSetupSecret("");
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
          <CardTitle>Set up this server</CardTitle>
          <CardDescription>
            This creates the first admin. The setup secret proves you operate the server. The password encrypts
            your items in this browser and is not sent to Drop.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {loadError ? <Notice>{loadError}</Notice> : null}
          {meta && !meta.setupRequired ? (
            <div className="grid gap-4">
              <p className="text-sm leading-6">This server already has an admin. Sign in, or ask them for an invite.</p>
              <Button onClick={() => go("/")}>Go to sign in</Button>
            </div>
          ) : (
            <form className="grid gap-4" onSubmit={(event) => void onSubmit(event)}>
              {error ? <Notice>{error}</Notice> : null}
              <div className="grid gap-2">
                <Label htmlFor="username">Admin username</Label>
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
              <div className="grid gap-2">
                <Label htmlFor="setup-secret">Setup secret</Label>
                <Input
                  id="setup-secret"
                  name="setup-secret"
                  type="password"
                  autoComplete="off"
                  value={setupSecret}
                  onChange={(event) => setSetupSecret(event.target.value)}
                  required
                />
                <p className="text-xs leading-5 text-muted-foreground">
                  This is the SETUP_SECRET from the server environment, not your item password.
                </p>
              </div>
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
              <Button type="submit" disabled={busy || Boolean(loadError)}>
                {busy ? "Creating account…" : "Create admin account"}
              </Button>
            </form>
          )}
        </CardContent>
      </Card>
      </div>
    </div>
  );
}
