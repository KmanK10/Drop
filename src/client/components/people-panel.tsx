import { useEffect, useState, type FormEvent } from "react";
import { apiJson, messageOf } from "@/lib/api";
import { formatBytes, formatDateTime } from "@/lib/format";
import { normalizeUsername } from "../../shared/username.ts";
import { Notice } from "@/components/notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { AccountSummary, InviteSummary, Me, Meta, Role } from "@/lib/types";

type Draft = { role: Role; gb: string };

export function PeoplePanel({
  me,
  meta,
  onChanged,
  onSelfDeleted,
}: {
  me: Me;
  meta: Meta;
  onChanged: () => Promise<void>;
  onSelfDeleted: () => void;
}) {
  const [username, setUsername] = useState("");
  const [role, setRole] = useState<Role>("user");
  const [link, setLink] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [invites, setInvites] = useState<InviteSummary[]>([]);
  const [users, setUsers] = useState<AccountSummary[]>([]);
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [pendingRevoke, setPendingRevoke] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<string | null>(null);
  const [rowBusy, setRowBusy] = useState("");
  const [loadError, setLoadError] = useState("");

  async function load() {
    try {
      const [inviteData, userData] = await Promise.all([
        apiJson<{ invites: InviteSummary[] }>("/api/invites"),
        apiJson<{ users: AccountSummary[] }>("/api/admin/users"),
      ]);
      setInvites(inviteData.invites);
      setUsers(userData.users);
      setDrafts(Object.fromEntries(userData.users.map((user) => [user.username, draftFrom(user)])));
      setLoadError("");
    } catch (reason) {
      setLoadError(messageOf(reason));
    }
  }

  useEffect(() => {
    void load();
  }, []);

  async function createInvite(event: FormEvent) {
    event.preventDefault();
    const name = normalizeUsername(username);
    if (!name) {
      setError("Choose a username of 2–32 characters: a letter, then lowercase letters, digits, underscores, or hyphens.");
      return;
    }
    setBusy(true);
    setError("");
    setCopied(false);
    try {
      const created = await apiJson<{ link: string }>("/api/invites", {
        method: "POST",
        json: { username: name, role },
      });
      setLink(created.link);
      setUsername("");
      await load();
    } catch (reason) {
      setError(messageOf(reason));
    } finally {
      setBusy(false);
    }
  }

  async function copyLink() {
    await navigator.clipboard.writeText(link);
    setCopied(true);
  }

  async function revoke(id: string) {
    setError("");
    try {
      await apiJson(`/api/invites/${id}`, { method: "DELETE" });
      setPendingRevoke(null);
      if (pendingRevoke === id) setLink("");
      await load();
    } catch (reason) {
      setError(messageOf(reason));
    }
  }

  async function saveAccount(user: AccountSummary) {
    const draft = drafts[user.username] ?? draftFrom(user);
    const gb = Number(draft.gb);
    if (!Number.isFinite(gb) || gb <= 0) {
      setError("Enter the quota in gigabytes.");
      return;
    }
    const quotaBytes = Math.round(gb * 1024 * 1024 * 1024);
    setRowBusy(user.username);
    setError("");
    try {
      await apiJson(`/api/admin/users/${encodeURIComponent(user.username)}`, {
        method: "PATCH",
        json: { role: draft.role, quotaBytes },
      });
      await load();
      await onChanged();
    } catch (reason) {
      setError(messageOf(reason));
    } finally {
      setRowBusy("");
    }
  }

  async function removeAccount(user: AccountSummary) {
    setRowBusy(user.username);
    setError("");
    try {
      await apiJson(`/api/admin/users/${encodeURIComponent(user.username)}`, { method: "DELETE" });
      setPendingDelete(null);
      if (user.username === me.username) {
        onSelfDeleted();
        return;
      }
      await load();
    } catch (reason) {
      setError(messageOf(reason));
    } finally {
      setRowBusy("");
    }
  }

  const adminCount = users.filter((user) => user.role === "admin").length;
  const ceilingGb = meta.quotaByteCeiling / (1024 * 1024 * 1024);

  return (
    <div className="grid gap-6">
      {error ? <Notice>{error}</Notice> : null}
      <section className="min-w-0 rounded-xl border border-border bg-card p-4 shadow-sm sm:p-5">
        <h2 className="font-serif text-2xl font-medium">Invite someone</h2>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          They open the link once and choose their own password. You won't be able to read what they save. The link
          expires in 48 hours and is not stored — copy it now. If you lose it, revoke the invite and create another.
          New accounts start with a {formatBytes(meta.quotaBytes)} quota.
        </p>
        <form className="mt-4 grid gap-4" onSubmit={(event) => void createInvite(event)}>
          <div className="grid gap-4 sm:grid-cols-[1fr_9rem_auto] sm:items-end">
            <div className="grid gap-2">
              <Label htmlFor="invite-username">Username</Label>
              <Input
                id="invite-username"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                value={username}
                onChange={(event) => setUsername(event.target.value)}
                required
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="invite-role">Role</Label>
              <select
                id="invite-role"
                className="h-11 w-full min-w-0 rounded-lg border border-input bg-card px-3 text-base text-foreground"
                value={role}
                onChange={(event) => setRole(event.target.value === "admin" ? "admin" : "user")}
              >
                <option value="user">Member</option>
                <option value="admin">Admin</option>
              </select>
            </div>
            <Button type="submit" disabled={busy}>
              {busy ? "Creating…" : "Create link"}
            </Button>
          </div>
        </form>
        {link ? (
          <div className="mt-4 grid gap-2">
            <Label htmlFor="invite-link">One-time link</Label>
            <a href={link} className="text-sm text-primary underline underline-offset-4">
              {link}
            </a>
            <div className="flex min-w-0 flex-col gap-2 sm:flex-row">
              <Input
                id="invite-link"
                className="sm:w-auto sm:flex-1"
                readOnly
                value={link}
                onFocus={(event) => event.currentTarget.select()}
              />
              <Button variant="outline" className="shrink-0" onClick={() => void copyLink()}>
                {copied ? "Copied" : "Copy link"}
              </Button>
            </div>
          </div>
        ) : null}
      </section>

      {loadError ? <Notice>{loadError}</Notice> : null}

      <section>
        <h2 className="font-serif text-2xl font-medium">Pending invites</h2>
        {invites.length === 0 ? (
          <p className="mt-2 text-sm text-muted-foreground">No open invites.</p>
        ) : (
          <ul className="mt-3 grid gap-2">
            {invites.map((invite) => (
              <li
                key={invite.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-card px-4 py-3"
              >
                <div>
                  <p className="font-medium">{invite.username}</p>
                  <p className="text-xs text-muted-foreground">
                    {invite.role === "admin" ? "Admin" : "Member"} · expires {formatDateTime(invite.expiresAt)}
                  </p>
                </div>
                {pendingRevoke === invite.id ? (
                  <Button size="sm" variant="destructive" className="h-11 sm:h-9" onClick={() => void revoke(invite.id)}>
                    Revoke invite
                  </Button>
                ) : (
                  <Button size="sm" variant="ghost" className="h-11 sm:h-9" onClick={() => setPendingRevoke(invite.id)}>
                    Revoke
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section>
        <h2 className="font-serif text-2xl font-medium">Accounts</h2>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          Quota is the total encrypted size of that person's items, from {formatBytes(meta.minQuotaBytes)} to{" "}
          {formatBytes(meta.quotaByteCeiling)}. The server rejects anything above the ceiling.
        </p>
        <ul className="mt-3 grid gap-3">
          {users.map((user) => {
            const draft = drafts[user.username] ?? draftFrom(user);
            const lastAdmin = user.role === "admin" && adminCount <= 1;
            return (
              <li key={user.username} className="grid gap-3 rounded-xl border border-border bg-card px-4 py-3">
                <div>
                  <p className="font-medium">
                    {user.username}
                    {user.username === me.username ? <span className="text-muted-foreground"> · you</span> : null}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {formatBytes(user.usedBytes)} of {formatBytes(user.quotaBytes)} used · joined{" "}
                    {formatDateTime(user.createdAt)}
                  </p>
                </div>
                <div className="grid gap-3 sm:grid-cols-[9rem_1fr_auto] sm:items-end">
                  <div className="grid gap-2">
                    <Label htmlFor={`role-${user.username}`}>Role</Label>
                    <select
                      id={`role-${user.username}`}
                      className="h-11 w-full min-w-0 rounded-lg border border-input bg-card px-3 text-base text-foreground"
                      value={draft.role}
                      disabled={lastAdmin || rowBusy === user.username}
                      onChange={(event) =>
                        setDrafts((current) => ({
                          ...current,
                          [user.username]: {
                            ...draft,
                            role: event.target.value === "admin" ? "admin" : "user",
                          },
                        }))
                      }
                    >
                      <option value="user">Member</option>
                      <option value="admin">Admin</option>
                    </select>
                  </div>
                  <div className="grid gap-2">
                    <Label htmlFor={`quota-${user.username}`}>Quota (GB)</Label>
                    <Input
                      id={`quota-${user.username}`}
                      inputMode="decimal"
                      value={draft.gb}
                      disabled={rowBusy === user.username}
                      onChange={(event) =>
                        setDrafts((current) => ({
                          ...current,
                          [user.username]: { ...draft, gb: event.target.value },
                        }))
                      }
                    />
                  </div>
                  <Button type="button" disabled={rowBusy === user.username} onClick={() => void saveAccount(user)}>
                    {rowBusy === user.username ? "Saving…" : "Save"}
                  </Button>
                </div>
                {lastAdmin ? (
                  <p className="text-xs leading-5 text-muted-foreground">
                    This is the only admin. Promote someone else before changing the role or deleting the account.
                  </p>
                ) : (
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">Up to {Math.round(ceilingGb)} GB.</p>
                    {pendingDelete === user.username ? (
                      <Button
                        size="sm"
                        variant="destructive"
                        className="h-11 sm:h-9"
                        disabled={rowBusy === user.username}
                        onClick={() => void removeAccount(user)}
                      >
                        Delete forever
                      </Button>
                    ) : (
                      <Button size="sm" variant="ghost" className="h-11 sm:h-9" onClick={() => setPendingDelete(user.username)}>
                        Delete account
                      </Button>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      </section>
    </div>
  );
}

function draftFrom(user: AccountSummary): Draft {
  const gb = user.quotaBytes / (1024 * 1024 * 1024);
  const text = Number.isInteger(gb) ? String(gb) : String(Math.round(gb * 100) / 100);
  return { role: user.role, gb: text };
}
