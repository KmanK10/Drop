import { useCallback, useEffect, useState } from "react";
import { api, apiJson, errorMessage, messageOf } from "@/lib/api";
import { PathProvider, useNavigate, usePath } from "@/lib/nav";
import type { Me, Meta } from "@/lib/types";
import { setContentKey } from "@/lib/vault";
import { Button } from "@/components/ui/button";
import { Wordmark } from "@/components/wordmark";
import { DropPage } from "@/pages/DropPage";
import { InvitePage } from "@/pages/InvitePage";
import { LoginPage } from "@/pages/LoginPage";
import { SetupPage } from "@/pages/SetupPage";

function Screen() {
  const path = usePath();
  if (path === "/setup") return <SetupPage />;
  if (path.startsWith("/invite/")) {
    const token = decodeURIComponent(path.slice("/invite/".length).split("/")[0] ?? "");
    return <InvitePage token={token} />;
  }
  return <Home />;
}

function Home() {
  const go = useNavigate();
  const [phase, setPhase] = useState<"loading" | "anon" | "in" | "error">("loading");
  const [me, setMe] = useState<Me | null>(null);
  const [meta, setMeta] = useState<Meta | null>(null);
  const [error, setError] = useState("");

  const load = useCallback(async (): Promise<"anon" | "in" | "error"> => {
    try {
      const nextMeta = await apiJson<Meta>("/api/meta");
      const meResponse = await api("/api/me");
      setMeta(nextMeta);
      if (meResponse.status === 401) {
        setMe(null);
        setPhase("anon");
        if (nextMeta.setupRequired) go("/setup");
        return "anon";
      }
      if (!meResponse.ok) {
        setError(await errorMessage(meResponse));
        setPhase("error");
        return "error";
      }
      setMe((await meResponse.json()) as Me);
      setPhase("in");
      return "in";
    } catch (reason) {
      setError(messageOf(reason));
      setPhase("error");
      return "error";
    }
  }, [go]);

  useEffect(() => {
    void load();
  }, [load]);

  if (phase === "loading") {
    return (
      <div className="grid min-h-dvh place-items-center px-4">
        <div className="grid justify-items-center gap-3">
          <Wordmark />
          <p className="text-sm text-muted-foreground">Opening Drop…</p>
        </div>
      </div>
    );
  }

  if (phase === "error") {
    return (
      <div className="mx-auto grid min-h-dvh max-w-md place-items-center px-4">
        <div className="grid gap-4">
          <Wordmark />
          <p role="alert" className="text-sm leading-6">
            {error || "Can't reach the server. Check that Drop is running."}
          </p>
          <Button onClick={() => window.location.reload()}>Try again</Button>
        </div>
      </div>
    );
  }

  if (phase === "in" && me && meta) {
    return (
      <DropPage
        me={me}
        meta={meta}
        onMe={setMe}
        onSignedOut={() => {
          setContentKey(null);
          setMe(null);
          setPhase("anon");
        }}
      />
    );
  }

  return <LoginPage meta={meta} onSignedIn={load} />;
}

export function App() {
  return (
    <PathProvider>
      <Screen />
    </PathProvider>
  );
}
