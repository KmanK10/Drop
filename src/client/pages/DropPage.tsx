import { useCallback, useEffect, useRef, useState, type DragEvent as ReactDragEvent, type FormEvent } from "react";
import { LogOut, Lock } from "lucide-react";
import { ApiError, api, apiBytes, apiJson, messageOf } from "@/lib/api";
import { formatBytes, mapPool, retentionLabel } from "@/lib/format";
import { useLiveSync } from "@/lib/sync";
import type { ItemMeta, Me, Meta } from "@/lib/types";
import { getContentKey, setContentKey } from "@/lib/vault";
import { b64urlToBytes } from "../../shared/bytes.ts";
import { decrypt, deriveKeys, encrypt, importContentKey, verifyKeyCheck } from "../../shared/crypto.ts";
import { decodeItem, encodeItem, type ItemPlain } from "../../shared/item.ts";
import { ItemCard, type ItemRecord } from "@/components/item-card";
import { Notice } from "@/components/notice";
import { PasswordField } from "@/components/password-field";
import { PeoplePanel } from "@/components/people-panel";
import { SettingsPanel } from "@/components/settings-panel";
import { Wordmark } from "@/components/wordmark";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

type Cached = { ok: true; plain: ItemPlain } | { ok: false };

export function DropPage({
  me,
  meta,
  onMe,
  onSignedOut,
}: {
  me: Me;
  meta: Meta;
  onMe: (me: Me) => void;
  onSignedOut: () => void;
}) {
  const [tab, setTab] = useState<"clipboard" | "people" | "settings">("clipboard");
  const [used, setUsed] = useState(me.usedBytes);
  const usedRef = useRef(me.usedBytes);
  usedRef.current = used;
  const [items, setItems] = useState<ItemRecord[]>([]);
  const [ready, setReady] = useState(false);
  const [listError, setListError] = useState("");
  const [unlocked, setUnlocked] = useState(() => getContentKey() !== null);
  const [password, setPassword] = useState("");
  const [unlockError, setUnlockError] = useState("");
  const [unlocking, setUnlocking] = useState(false);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState("");
  const [composeError, setComposeError] = useState("");
  const [dragging, setDragging] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const cache = useRef(new Map<string, Cached>());
  const generation = useRef(0);
  const fileInput = useRef<HTMLInputElement>(null);
  const signedOut = useRef(onSignedOut);
  signedOut.current = onSignedOut;

  const reload = useCallback(async () => {
    const gen = ++generation.current;
    try {
      const data = await apiJson<{ items: ItemMeta[]; usedBytes: number }>("/api/items");
      usedRef.current = data.usedBytes;
      setUsed(data.usedBytes);
      if (gen !== generation.current) return;
      const key = getContentKey();
      const show = (pendingIds: Set<string>) =>
        data.items.map((item) => {
          const cached = cache.current.get(item.id);
          return {
            ...item,
            plain: cached?.ok ? cached.plain : undefined,
            broken: Boolean(cached && !cached.ok),
            pending: pendingIds.has(item.id),
          };
        });
      const missing = key ? data.items.filter((item) => !cache.current.has(item.id)) : [];
      const missingIds = new Set(missing.map((item) => item.id));
      setItems(show(missingIds));
      setReady(true);
      setListError("");
      await mapPool(missing, 3, async (item) => {
        if (gen !== generation.current || !key) return;
        try {
          const bytes = await apiBytes(`/api/items/${item.id}`);
          if (gen !== generation.current) return;
          cache.current.set(item.id, { ok: true, plain: decodeItem(await decrypt(key, bytes)) });
        } catch (error) {
          if (error instanceof ApiError && error.status === 401) {
            signedOut.current();
            return;
          }
          if (error instanceof ApiError && error.status === 404) return;
          cache.current.set(item.id, { ok: false });
        }
      });
      if (gen !== generation.current) return;
      const ids = new Set(data.items.map((item) => item.id));
      for (const id of cache.current.keys()) {
        if (!ids.has(id)) cache.current.delete(id);
      }
      setItems(show(new Set()));
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        signedOut.current();
        return;
      }
      if (gen === generation.current) {
        setListError(messageOf(error));
        setReady(true);
      }
    }
  }, []);

  const reloadRef = useRef(reload);
  reloadRef.current = reload;
  const live = useLiveSync(() => {
    void reloadRef.current();
  });

  useEffect(() => {
    void reload();
    const clock = window.setInterval(() => setNow(Date.now()), 30_000);
    function onVisible() {
      if (document.visibilityState === "visible") void reloadRef.current();
    }
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.clearInterval(clock);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [reload]);

  useEffect(() => {
    if (me.role !== "admin" && tab === "people") setTab("clipboard");
  }, [me.role, tab]);

  async function unlock(event: FormEvent) {
    event.preventDefault();
    if (!password) {
      setUnlockError("Enter your password.");
      return;
    }
    setUnlocking(true);
    setUnlockError("");
    try {
      const derived = await deriveKeys(password, b64urlToBytes(me.kdf.salt), me.kdf);
      derived.authVerifier.fill(0);
      const ok = await verifyKeyCheck(derived.contentKey, b64urlToBytes(me.keyCheck));
      if (!ok) {
        derived.contentKey.fill(0);
        setUnlockError("Wrong password.");
        setUnlocking(false);
        return;
      }
      const key = await importContentKey(derived.contentKey);
      derived.contentKey.fill(0);
      setContentKey(key);
      setPassword("");
      setUnlocked(true);
      setUnlocking(false);
      await reload();
    } catch (error) {
      setContentKey(null);
      setUnlockError(messageOf(error));
      setUnlocking(false);
    }
  }

  function lock() {
    setContentKey(null);
    cache.current.clear();
    generation.current += 1;
    setUnlocked(false);
    setItems((current) => current.map((item) => ({ ...item, plain: undefined, broken: false, pending: false })));
  }

  async function logout() {
    lock();
    try {
      await api("/api/auth/logout", { method: "POST" });
    } finally {
      signedOut.current();
    }
  }

  async function savePlain(plain: ItemPlain) {
    const key = getContentKey();
    if (!key) throw new Error("Unlock this device first.");
    const encoded = encodeItem(plain);
    if (usedRef.current + encoded.byteLength + 32 > me.quotaBytes) {
      throw new Error(
        `That item doesn't fit. ${formatBytes(usedRef.current)} of ${formatBytes(me.quotaBytes)} is already used.`,
      );
    }
    const ciphertext = await encrypt(key, encoded);
    const saved = await apiJson<ItemMeta>("/api/items", {
      method: "POST",
      body: new Blob([ciphertext.buffer.slice(ciphertext.byteOffset, ciphertext.byteOffset + ciphertext.byteLength) as ArrayBuffer]),
      headers: { "content-type": "application/octet-stream" },
    });
    cache.current.set(saved.id, { ok: true, plain });
    usedRef.current += saved.size;
    setUsed(usedRef.current);
    setItems((current) => [withPlain(saved, plain), ...current.filter((item) => item.id !== saved.id)]);
  }

  async function saveText(event?: FormEvent) {
    event?.preventDefault();
    if (!text.trim()) {
      setComposeError("Write something first.");
      return;
    }
    setBusy("Encrypting text…");
    setComposeError("");
    try {
      await savePlain({
        kind: "text",
        name: "",
        mime: "text/plain",
        body: new TextEncoder().encode(text),
        text,
      });
      setText("");
    } catch (error) {
      setComposeError(messageOf(error));
    } finally {
      setBusy("");
    }
  }

  async function saveFiles(files: FileList | File[]) {
    const list = [...files];
    if (list.length === 0) return;
    setComposeError("");
    for (const file of list) {
      if (usedRef.current + file.size + 1024 > me.quotaBytes) {
        setComposeError(
          `${file.name} doesn't fit. ${formatBytes(usedRef.current)} of ${formatBytes(me.quotaBytes)} is already used.`,
        );
        continue;
      }
      setBusy(`Encrypting ${file.name}…`);
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        await savePlain({
          kind: "file",
          name: file.name,
          mime: file.type || "application/octet-stream",
          body: bytes,
        });
      } catch (error) {
        setComposeError(messageOf(error));
      }
    }
    setBusy("");
    if (fileInput.current) fileInput.current.value = "";
  }

  async function remove(id: string) {
    setComposeError("");
    try {
      await apiJson(`/api/items/${id}`, { method: "DELETE" });
      cache.current.delete(id);
      usedRef.current = Math.max(0, usedRef.current - (items.find((item) => item.id === id)?.size ?? 0));
      setUsed(usedRef.current);
      setItems((current) => current.filter((item) => item.id !== id));
      setPendingDelete(null);
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) signedOut.current();
      else setComposeError(messageOf(error));
    }
  }

  function onDrop(event: ReactDragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    if (!unlocked || busy) return;
    void saveFiles(event.dataTransfer.files);
  }

  useEffect(() => {
    function onPaste(event: ClipboardEvent) {
      if (!unlocked || busy) return;
      const files = event.clipboardData?.files;
      if (!files || files.length === 0) return;
      event.preventDefault();
      void saveFiles(files);
    }
    function blockNavigation(event: globalThis.DragEvent) {
      if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
    }
    window.addEventListener("paste", onPaste);
    window.addEventListener("dragover", blockNavigation);
    window.addEventListener("drop", blockNavigation);
    return () => {
      window.removeEventListener("paste", onPaste);
      window.removeEventListener("dragover", blockNavigation);
      window.removeEventListener("drop", blockNavigation);
    };
  });

  return (
    <div className="min-h-dvh">
      <header className="sticky top-0 z-10 border-b border-border/80 bg-background/90 backdrop-blur">
        <div className="mx-auto flex w-full max-w-3xl flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex min-w-0 items-center justify-between gap-3">
            <Wordmark />
            <LiveStatus live={live} className="sm:hidden" />
          </div>
          <div className="flex min-w-0 items-center gap-2">
            <p className="min-w-0 flex-1 truncate text-sm sm:max-w-40 sm:flex-none" aria-live="polite">
              <span className="font-medium">{me.username}</span>
              {me.role === "admin" ? <span className="text-muted-foreground"> · admin</span> : null}
            </p>
            <LiveStatus live={live} className="hidden sm:inline-flex" />
            {unlocked ? (
              <Button size="sm" variant="ghost" className="h-11 shrink-0 px-3 sm:h-9" onClick={lock}>
                <Lock />
                Lock
              </Button>
            ) : null}
            <Button size="sm" variant="ghost" className="h-11 shrink-0 px-3 sm:h-9" onClick={() => void logout()}>
              <LogOut />
              Log out
            </Button>
          </div>
        </div>
      </header>

      <main className="mx-auto grid w-full min-w-0 max-w-3xl gap-5 px-4 py-6">
        <div
          className={`grid min-w-0 gap-1 rounded-2xl bg-secondary p-1 sm:flex sm:w-fit sm:rounded-full ${
            me.role === "admin" ? "grid-cols-1 min-[23.5rem]:grid-cols-3" : "grid-cols-2"
          }`}
        >
          <TabButton pressed={tab === "clipboard"} onClick={() => setTab("clipboard")}>
            {items.length > 0 ? `Clipboard · ${items.length}` : "Clipboard"}
          </TabButton>
          <TabButton pressed={tab === "settings"} onClick={() => setTab("settings")}>
            Settings
          </TabButton>
          {me.role === "admin" ? (
            <TabButton pressed={tab === "people"} onClick={() => setTab("people")}>
              People
            </TabButton>
          ) : null}
        </div>

        {tab === "people" && me.role === "admin" ? (
          <PeoplePanel
            me={me}
            meta={meta}
            onChanged={async () => onMe(await apiJson<Me>("/api/me"))}
            onSelfDeleted={onSignedOut}
          />
        ) : null}

        {tab === "settings" ? (
          <SettingsPanel
            me={me}
            onUpdated={onMe}
            onRekeyed={() => {
              cache.current.clear();
              setUnlocked(true);
              void reload();
            }}
          />
        ) : null}

        {tab === "clipboard" ? (
          <>
            <p className="text-sm leading-6 text-muted-foreground">
              {formatBytes(used)} of {formatBytes(me.quotaBytes)} used. Items are deleted {retentionLabel(meta.itemTtlMs)}{" "}
              after they are saved. There is no trash.
            </p>
            {!unlocked ? (
              <form
                className="grid gap-4 rounded-xl border border-border bg-card p-4 shadow-sm sm:p-5"
                onSubmit={(event) => void unlock(event)}
              >
                <div>
                  <h2 className="font-serif text-2xl font-medium">Unlock this device</h2>
                  <p className="mt-1 text-sm leading-6 text-muted-foreground">
                    You're signed in as {me.username}. The password decrypts items in this tab and is kept in memory
                    until you lock, log out, or close it.
                  </p>
                </div>
                {unlockError ? <Notice>{unlockError}</Notice> : null}
                <PasswordField
                  label="Password"
                  name="password"
                  autoComplete="current-password"
                  value={password}
                  onChange={setPassword}
                />
                <Button type="submit" disabled={unlocking}>
                  {unlocking ? "Unlocking…" : "Unlock"}
                </Button>
              </form>
            ) : (
              <section className="grid min-w-0 gap-4 md:grid-cols-2">
                <form className="grid gap-3" onSubmit={(event) => void saveText(event)}>
                  <Textarea
                    value={text}
                    placeholder="Paste a note, a snippet, an address…"
                    aria-label="Text to save"
                    disabled={Boolean(busy)}
                    onChange={(event) => setText(event.target.value)}
                    onKeyDown={(event) => {
                      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
                        event.preventDefault();
                        void saveText();
                      }
                    }}
                  />
                  <Button type="submit" disabled={Boolean(busy)}>
                    {busy.startsWith("Encrypting text") ? busy : "Save text"}
                  </Button>
                </form>
                <div
                  className={`grid place-items-center rounded-xl border border-dashed px-4 py-8 text-center ${
                    dragging ? "border-primary bg-accent" : "border-input bg-card"
                  }`}
                  onDragOver={(event) => {
                    event.preventDefault();
                    setDragging(true);
                  }}
                  onDragLeave={() => setDragging(false)}
                  onDrop={onDrop}
                >
                  <div className="grid gap-2">
                    <p className="font-medium">{dragging ? "Release to encrypt" : "Drop files here"}</p>
                    <p className="text-sm text-muted-foreground">
                      {formatBytes(Math.max(0, me.quotaBytes - used))} left in this account.
                    </p>
                    <Button variant="outline" disabled={Boolean(busy)} onClick={() => fileInput.current?.click()}>
                      Choose files
                    </Button>
                    <input
                      ref={fileInput}
                      className="sr-only"
                      type="file"
                      multiple
                      onChange={(event) => {
                        if (event.target.files) void saveFiles(event.target.files);
                      }}
                    />
                  </div>
                </div>
                <p className="text-xs leading-5 text-muted-foreground md:col-span-2">
                  Names, text, and file bytes are encrypted in this browser before they are saved.
                  {busy && !busy.startsWith("Encrypting text") ? ` ${busy}` : ""}
                </p>
              </section>
            )}

            {composeError ? <Notice>{composeError}</Notice> : null}
            {listError ? <Notice>{listError}</Notice> : null}

            {!ready ? <p className="text-sm text-muted-foreground">Loading items…</p> : null}
            {ready && items.length === 0 ? (
              <p className="rounded-xl border border-dashed border-input px-4 py-8 text-center text-sm leading-6 text-muted-foreground">
                Nothing here yet. Save a note or a file and open Drop on another device. After you unlock it there, the
                item shows up.
              </p>
            ) : null}
            <div className="grid min-w-0 gap-3">
              {items.map((item) => (
                <ItemCard
                  key={item.id}
                  item={item}
                  unlocked={unlocked}
                  now={now}
                  pendingDelete={pendingDelete === item.id}
                  onAskDelete={() => setPendingDelete(item.id)}
                  onDelete={() => void remove(item.id)}
                />
              ))}
            </div>
          </>
        ) : null}
      </main>
    </div>
  );
}

function withPlain(meta: ItemMeta, plain: ItemPlain): ItemRecord {
  return { ...meta, plain, broken: false, pending: false };
}

function LiveStatus({ live, className }: { live: boolean; className?: string }) {
  return (
    <span className={cn("inline-flex shrink-0 items-center gap-1.5 text-sm text-muted-foreground", className)}>
      <span className={`size-2 rounded-full ${live ? "bg-primary" : "bg-destructive"}`} />
      {live ? "Live" : "Reconnecting"}
    </span>
  );
}

function TabButton({
  pressed,
  onClick,
  children,
}: {
  pressed: boolean;
  onClick: () => void;
  children: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={pressed}
      className={`min-h-11 rounded-full px-2.5 py-2 text-center text-sm leading-tight sm:min-h-0 sm:px-3 sm:py-1.5 sm:text-left ${
        pressed ? "bg-card shadow-sm" : "text-muted-foreground"
      }`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
