import { fromUtf8 } from "../../shared/bytes.ts";

export type CopyEnv = {
  supports(type: string): boolean;
  writeText(text: string): Promise<void>;
  write(items: Record<string, Blob | Promise<Blob>>): Promise<void>;
  pngFrom(blob: Blob): Promise<Blob>;
};

const KNOWN_COPY_ERRORS = new Set([
  "Clipboard permission was denied.",
  "This browser can't copy that to the clipboard.",
  "This file can't be copied.",
  "Couldn't copy that.",
]);

const TEXT_TYPES = new Set([
  "application/json",
  "application/xml",
  "application/javascript",
  "application/x-javascript",
  "application/ecmascript",
  "application/sql",
  "application/graphql",
  "application/yaml",
  "application/x-yaml",
  "application/toml",
  "application/x-toml",
  "application/x-sh",
  "application/csv",
  "image/svg+xml",
]);

export function baseMime(mime: string): string {
  return (mime.split(";")[0] ?? "").trim().toLowerCase();
}

function isTextMime(base: string): boolean {
  if (base.startsWith("text/")) return true;
  if (TEXT_TYPES.has(base)) return true;
  return base.startsWith("application/") && (base.endsWith("+json") || base.endsWith("+xml"));
}

/** What a decrypted file can become on the system clipboard. Archives and other binary files are omitted. */
export function clipboardFileKind(mime: string): "text" | "html" | "image" | null {
  const base = baseMime(mime);
  if (!base || base === "application/octet-stream") return null;
  if (base === "text/html" || base === "application/xhtml+xml") return "html";
  if (isTextMime(base)) return "text";
  if (base.startsWith("image/")) return "image";
  return null;
}

export function canCopyFile(mime: string): boolean {
  return clipboardFileKind(mime) !== null;
}

/** Prefer the file's own image type when the browser accepts it. Otherwise redraw as PNG. */
export function imageWritePlan(
  mime: string,
  supports: (type: string) => boolean,
): { type: string; convert: boolean } {
  const base = baseMime(mime);
  if (base === "image/png") return { type: "image/png", convert: false };
  try {
    if (base && supports(base)) return { type: base, convert: false };
  } catch {
    // An unsure answer means the file has to be redrawn as PNG.
  }
  return { type: "image/png", convert: true };
}

function blobFrom(body: Uint8Array, type: string): Blob {
  const copy = body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength) as ArrayBuffer;
  return new Blob([copy], { type });
}

function isPermissionError(error: unknown): boolean {
  return (
    error instanceof Error &&
    error.name === "NotAllowedError" &&
    !/type|mime|format|supported/i.test(error.message)
  );
}

function toCopyError(error: unknown): Error {
  if (error instanceof Error && KNOWN_COPY_ERRORS.has(error.message)) return error;
  if (isPermissionError(error)) return new Error("Clipboard permission was denied.");
  if (error instanceof Error && error.name === "SecurityError") {
    return new Error("This browser can't copy that to the clipboard.");
  }
  return new Error("Couldn't copy that.");
}

async function writeFile(plain: { mime: string; body: Uint8Array }, env: CopyEnv): Promise<void> {
  const kind = clipboardFileKind(plain.mime);
  if (!kind) throw new Error("This file can't be copied.");

  if (kind === "text") {
    await env.writeText(fromUtf8(plain.body));
    return;
  }

  if (kind === "html") {
    const text = fromUtf8(plain.body);
    try {
      await env.write({
        "text/plain": new Blob([text], { type: "text/plain" }),
        "text/html": new Blob([text], { type: "text/html" }),
      });
    } catch {
      await env.writeText(text);
    }
    return;
  }

  const plan = imageWritePlan(plain.mime, env.supports);
  const source = blobFrom(plain.body, baseMime(plain.mime) || "application/octet-stream");
  const attempt = (type: string, data: Blob | Promise<Blob>) => env.write({ [type]: data });
  try {
    await attempt(plan.type, plan.convert ? env.pngFrom(source) : source);
  } catch (error) {
    if (plan.type === "image/png" || isPermissionError(error)) throw error;
    await attempt("image/png", env.pngFrom(source));
  }
}

export async function copyFileItem(
  plain: { mime: string; body: Uint8Array },
  env: CopyEnv = browserCopyEnv(),
): Promise<void> {
  try {
    await writeFile(plain, env);
  } catch (error) {
    throw toCopyError(error);
  }
}

function clipboardSupports(type: string): boolean {
  const ctor = globalThis.ClipboardItem as { supports?: (mime: string) => boolean } | undefined;
  if (typeof ctor?.supports !== "function") {
    return type === "text/plain" || type === "text/html" || type === "image/png";
  }
  try {
    return ctor.supports(type);
  } catch {
    return type === "text/plain" || type === "text/html" || type === "image/png";
  }
}

function browserCopyEnv(): CopyEnv {
  return {
    supports: clipboardSupports,
    writeText(text) {
      const writeText = globalThis.navigator?.clipboard?.writeText;
      if (!writeText) return Promise.reject(new Error("This browser can't copy that to the clipboard."));
      return writeText.call(globalThis.navigator.clipboard, text);
    },
    write(items) {
      const write = globalThis.navigator?.clipboard?.write;
      const Ctor = globalThis.ClipboardItem;
      if (!write || typeof Ctor !== "function") {
        return Promise.reject(new Error("This browser can't copy that to the clipboard."));
      }
      try {
        // The promise is passed through so write() starts in the click, before PNG conversion finishes.
        const payload: Record<string, Promise<Blob>> = {};
        for (const [type, data] of Object.entries(items)) {
          payload[type] = Promise.resolve(data).then((blob) => (blob.type === type ? blob : new Blob([blob], { type })));
        }
        return write.call(globalThis.navigator.clipboard, [new Ctor(payload)]);
      } catch (error) {
        return Promise.reject(error);
      }
    },
    pngFrom: rasterToPng,
  };
}

async function rasterToPng(blob: Blob): Promise<Blob> {
  if (typeof createImageBitmap !== "function" || typeof document === "undefined") {
    throw new Error("This browser can't copy that to the clipboard.");
  }
  const bitmap = await createImageBitmap(blob);
  try {
    if (bitmap.width < 1 || bitmap.height < 1) throw new Error("Couldn't prepare the image.");
    const canvas = document.createElement("canvas");
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Couldn't prepare the image.");
    context.drawImage(bitmap, 0, 0);
    const png = await new Promise<Blob | null>((resolve) => {
      canvas.toBlob((result) => resolve(result), "image/png");
    });
    if (!png) throw new Error("Couldn't prepare the image.");
    return png;
  } finally {
    bitmap.close();
  }
}
