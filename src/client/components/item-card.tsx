import { useRef, useState } from "react";
import { Copy, Download, FileText, Lock, Trash2 } from "lucide-react";
import { messageOf } from "@/lib/api";
import { canCopyFile, copyFileItem } from "@/lib/clipboard";
import { daysLeft, formatBytes, formatWhen } from "@/lib/format";
import { safeDownloadName, type ItemPlain } from "../../shared/item.ts";
import { Button } from "@/components/ui/button";

export type ItemRecord = {
  id: string;
  createdAt: number;
  size: number;
  plain?: ItemPlain;
  broken?: boolean;
  pending?: boolean;
};

export function ItemCard({
  item,
  unlocked,
  now,
  ttlMs,
  pendingDelete,
  onAskDelete,
  onDelete,
}: {
  item: ItemRecord;
  unlocked: boolean;
  now: number;
  ttlMs: number;
  pendingDelete: boolean;
  onAskDelete: () => void;
  onDelete: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [copying, setCopying] = useState(false);
  const [copyError, setCopyError] = useState("");
  const copyingRef = useRef(false);
  const plain = item.plain;
  const expiresIn = daysLeft(item.createdAt, ttlMs, now);

  async function copyText() {
    if (!plain?.text) return;
    await navigator.clipboard.writeText(plain.text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }

  async function copyFile() {
    if (!plain || plain.kind !== "file" || copyingRef.current) return;
    copyingRef.current = true;
    setCopying(true);
    setCopyError("");
    try {
      await copyFileItem(plain);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch (error) {
      setCopied(false);
      setCopyError(messageOf(error));
    } finally {
      copyingRef.current = false;
      setCopying(false);
    }
  }

  function download() {
    if (!plain) return;
    const bytes = plain.body.buffer.slice(
      plain.body.byteOffset,
      plain.body.byteOffset + plain.body.byteLength,
    ) as ArrayBuffer;
    const blob = new Blob([bytes], { type: plain.mime || "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = safeDownloadName(plain.name);
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 1500);
  }

  return (
    <article className="min-w-0 max-w-full rounded-xl border border-border bg-card px-4 py-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-xs">
            <span className="uppercase tracking-wide text-muted-foreground">
              {plain ? (plain.kind === "file" ? "File" : "Text") : item.broken ? "Unreadable" : "Locked"}
              <span className="px-1.5">·</span>
              {formatWhen(item.createdAt, now)}
            </span>
            {expiresIn ? (
              <>
                <span className="px-1.5 uppercase tracking-wide text-muted-foreground">·</span>
                <span className="text-destructive">{expiresIn}</span>
              </>
            ) : null}
          </p>
          {plain?.kind === "file" ? (
            <h3 className="mt-1 font-medium [overflow-wrap:anywhere]">{plain.name || "Untitled file"}</h3>
          ) : null}
        </div>
        <span className="shrink-0 text-xs text-muted-foreground">
          {formatBytes(plain?.kind === "file" ? plain.body.byteLength : item.size)}
        </span>
      </div>

      {plain?.kind === "text" ? (
        <p className="mt-3 max-h-64 overflow-x-hidden overflow-y-auto whitespace-pre-wrap text-[15px] leading-6 [overflow-wrap:anywhere]">
          {plain.text}
        </p>
      ) : null}

      {plain?.kind === "file" ? (
        <p className="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
          <FileText className="size-4" aria-hidden="true" />
          {canCopyFile(plain.mime)
            ? "Copy puts it on the clipboard. Download saves the file."
            : "Download decrypts it on this device."}
        </p>
      ) : null}

      {copyError ? (
        <p role="alert" className="mt-3 text-sm text-destructive">
          {copyError}
        </p>
      ) : null}

      {!plain && !item.broken && !unlocked ? (
        <p className="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
          <Lock className="size-4" aria-hidden="true" />
          Unlock to read this item.
        </p>
      ) : null}

      {item.pending ? <p className="mt-3 text-sm text-muted-foreground">Decrypting…</p> : null}
      {item.broken ? (
        <p className="mt-3 text-sm text-destructive">This item couldn't be decrypted with the current password.</p>
      ) : null}

      <div className="mt-4 flex items-start justify-between gap-2">
        <div className="flex min-w-0 flex-wrap gap-2">
          {plain?.kind === "text" ? (
            <Button size="sm" variant="outline" className="h-11 sm:h-9" onClick={() => void copyText()}>
              <Copy />
              {copied ? "Copied" : "Copy"}
            </Button>
          ) : null}
          {plain?.kind === "file" && canCopyFile(plain.mime) ? (
            <Button size="sm" variant="outline" className="h-11 sm:h-9" disabled={copying} onClick={() => void copyFile()}>
              <Copy />
              {copied ? "Copied" : copying ? "Copying…" : "Copy"}
            </Button>
          ) : null}
          {plain?.kind === "file" ? (
            <Button size="sm" variant="outline" className="h-11 sm:h-9" onClick={download}>
              <Download />
              Download
            </Button>
          ) : null}
        </div>
        {pendingDelete ? (
          <Button size="sm" variant="destructive" className="h-11 shrink-0 sm:h-9" onClick={onDelete}>
            <Trash2 />
            Delete forever
          </Button>
        ) : (
          <Button size="sm" variant="ghost" className="h-11 shrink-0 sm:h-9" onClick={onAskDelete}>
            <Trash2 />
            Delete
          </Button>
        )}
      </div>
    </article>
  );
}
