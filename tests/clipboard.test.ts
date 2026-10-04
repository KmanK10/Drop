import { readFileSync } from "node:fs";
import { createElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ItemCard, type ItemRecord } from "../src/client/components/item-card.tsx";
import {
  canCopyFile,
  clipboardFileKind,
  copyFileItem,
  imageWritePlan,
  type CopyEnv,
} from "../src/client/lib/clipboard.ts";
import { daysLeft, formatWhen } from "../src/client/lib/format.ts";
import { utf8 } from "../src/shared/bytes.ts";

function install(overrides: Partial<CopyEnv> = {}) {
  const texts: string[] = [];
  const writes: { type: string; bytes: Uint8Array }[][] = [];
  const pngInputs: Uint8Array[] = [];
  const env: CopyEnv = {
    supports: () => false,
    async writeText(text) {
      texts.push(text);
    },
    async write(items) {
      const done: { type: string; bytes: Uint8Array }[] = [];
      for (const [type, data] of Object.entries(items)) {
        done.push({ type, bytes: new Uint8Array(await (await data).arrayBuffer()) });
      }
      writes.push(done);
    },
    async pngFrom(blob) {
      const bytes = new Uint8Array(await blob.arrayBuffer());
      pngInputs.push(bytes);
      return new Blob([Uint8Array.of(0x89, 0x50, 0x4e, 0x47)], { type: "image/png" });
    },
    ...overrides,
  };
  return { env, texts, writes, pngInputs };
}

describe("clipboard file kinds", () => {
  it("copies images, text-like files, and SVG, and leaves other files alone", () => {
    expect(clipboardFileKind("image/png")).toBe("image");
    expect(clipboardFileKind("IMAGE/JPEG")).toBe("image");
    expect(clipboardFileKind("image/gif")).toBe("image");
    expect(clipboardFileKind("image/webp")).toBe("image");
    expect(clipboardFileKind("image/avif")).toBe("image");
    expect(clipboardFileKind("text/plain")).toBe("text");
    expect(clipboardFileKind("text/plain; charset=utf-8")).toBe("text");
    expect(clipboardFileKind(" text/markdown ")).toBe("text");
    expect(clipboardFileKind("text/csv")).toBe("text");
    expect(clipboardFileKind("application/json")).toBe("text");
    expect(clipboardFileKind("application/ld+json")).toBe("text");
    expect(clipboardFileKind("application/xml")).toBe("text");
    expect(clipboardFileKind("application/atom+xml")).toBe("text");
    expect(clipboardFileKind("image/svg+xml")).toBe("text");
    expect(clipboardFileKind("image/svg+xml; charset=utf-8")).toBe("text");
    expect(clipboardFileKind(" Text/HTML ; charset=UTF-8")).toBe("html");
    expect(clipboardFileKind("application/xhtml+xml")).toBe("html");

    for (const mime of [
      "application/pdf",
      "application/zip",
      "application/gzip",
      "application/epub+zip",
      "application/octet-stream",
      "video/mp4",
      "audio/mpeg",
      "font/woff2",
      "",
      "   ",
    ]) {
      expect(clipboardFileKind(mime), mime).toBeNull();
      expect(canCopyFile(mime), mime).toBe(false);
    }

    expect(canCopyFile("image/png")).toBe(true);
    expect(canCopyFile("text/plain")).toBe(true);
  });

  it("redraws an image as PNG unless the browser already accepts that type", () => {
    expect(imageWritePlan("image/png", () => false)).toEqual({ type: "image/png", convert: false });
    expect(imageWritePlan("image/jpeg", () => false)).toEqual({ type: "image/png", convert: true });
    expect(imageWritePlan("image/gif", () => false)).toEqual({ type: "image/png", convert: true });
    expect(imageWritePlan("image/jpeg", (type) => type === "image/jpeg")).toEqual({
      type: "image/jpeg",
      convert: false,
    });
    expect(imageWritePlan("image/webp", () => {
      throw new Error("no supports()");
    })).toEqual({ type: "image/png", convert: true });
  });
});

describe("copyFileItem", () => {
  it("copies a PNG as itself and does not download or convert it", async () => {
    const body = Uint8Array.of(1, 2, 3, 4);
    const { env, writes, pngInputs, texts } = install();
    await copyFileItem({ mime: "image/png", body }, env);
    expect(pngInputs).toHaveLength(0);
    expect(texts).toHaveLength(0);
    expect(writes).toEqual([[{ type: "image/png", bytes: body }]]);
  });

  it("converts an unsupported image to PNG before writing", async () => {
    const body = Uint8Array.of(9, 8, 7);
    const { env, writes, pngInputs } = install();
    await copyFileItem({ mime: "image/jpeg", body }, env);
    expect(pngInputs).toEqual([body]);
    expect(writes).toEqual([[{ type: "image/png", bytes: Uint8Array.of(0x89, 0x50, 0x4e, 0x47) }]]);
  });

  it("keeps a supported image type without converting", async () => {
    const body = Uint8Array.of(4, 5, 6);
    const { env, writes, pngInputs } = install({ supports: (type) => type === "image/jpeg" });
    await copyFileItem({ mime: "image/jpeg", body }, env);
    expect(pngInputs).toHaveLength(0);
    expect(writes).toEqual([[{ type: "image/jpeg", bytes: body }]]);
  });

  it("retries as PNG when the browser rejects the original image type", async () => {
    const body = Uint8Array.of(1, 2);
    const seen: string[] = [];
    const { env, pngInputs } = install({
      supports: () => true,
      async write(items) {
        const type = Object.keys(items)[0] ?? "";
        seen.push(type);
        if (type !== "image/png") {
          const error = new Error("Type image/jpeg not supported on write.");
          error.name = "NotAllowedError";
          throw error;
        }
        await items[type];
      },
    });
    await copyFileItem({ mime: "image/jpeg", body }, env);
    expect(seen).toEqual(["image/jpeg", "image/png"]);
    expect(pngInputs).toEqual([body]);
  });

  it("does not retry a PNG when clipboard permission is denied", async () => {
    let writes = 0;
    const { env, pngInputs } = install({
      async write() {
        writes += 1;
        const error = new Error("The request is not allowed by the user agent or the platform in the current context.");
        error.name = "NotAllowedError";
        throw error;
      },
    });
    await expect(copyFileItem({ mime: "image/png", body: Uint8Array.of(1) }, env)).rejects.toThrow(
      "Clipboard permission was denied.",
    );
    expect(writes).toBe(1);
    expect(pngInputs).toHaveLength(0);
  });

  it("does not convert after a permission error on a supported image type", async () => {
    let writes = 0;
    const { env, pngInputs } = install({
      supports: () => true,
      async write() {
        writes += 1;
        const error = new Error("Document is not focused.");
        error.name = "NotAllowedError";
        throw error;
      },
    });
    await expect(copyFileItem({ mime: "image/jpeg", body: Uint8Array.of(1, 2, 3) }, env)).rejects.toThrow(
      "Clipboard permission was denied.",
    );
    expect(writes).toBe(1);
    expect(pngInputs).toHaveLength(0);
  });

  it("reports a failed image conversion without writing a second time", async () => {
    let writes = 0;
    let conversions = 0;
    const { env } = install({
      async write(items) {
        writes += 1;
        await items["image/png"];
      },
      async pngFrom() {
        conversions += 1;
        throw new Error("decode failed");
      },
    });
    await expect(copyFileItem({ mime: "image/gif", body: Uint8Array.of(1) }, env)).rejects.toThrow(
      "Couldn't copy that.",
    );
    expect(conversions).toBe(1);
    expect(writes).toBe(1);
  });

  it("copies text-like files and SVG as text", async () => {
    const { env, texts, writes, pngInputs } = install();
    await copyFileItem({ mime: "text/plain; charset=utf-8", body: utf8("héllo 🔐") }, env);
    await copyFileItem({ mime: "application/json", body: utf8('{"a":1}') }, env);
    await copyFileItem({ mime: "image/svg+xml", body: utf8("<svg></svg>") }, env);
    expect(texts).toEqual(["héllo 🔐", '{"a":1}', "<svg></svg>"]);
    expect(writes).toHaveLength(0);
    expect(pngInputs).toHaveLength(0);
  });

  it("copies HTML as HTML and plain text, then falls back to plain text", async () => {
    const html = "<p>hi</p>";
    const first = install();
    await copyFileItem({ mime: "text/html", body: utf8(html) }, first.env);
    expect(first.texts).toHaveLength(0);
    expect(first.writes[0]?.map((part) => part.type).sort()).toEqual(["text/html", "text/plain"]);
    expect(new TextDecoder().decode(first.writes[0]?.find((part) => part.type === "text/html")?.bytes)).toBe(html);

    const texts: string[] = [];
    await copyFileItem(
      { mime: "application/xhtml+xml", body: utf8(html) },
      {
        supports: () => false,
        async writeText(text) {
          texts.push(text);
        },
        async write() {
          throw new Error("ClipboardItem missing");
        },
        async pngFrom() {
          throw new Error("unused");
        },
      },
    );
    expect(texts).toEqual([html]);
  });

  it("does not write a PDF, zip, or unlabeled file", async () => {
    const { env, texts, writes, pngInputs } = install();
    for (const mime of ["application/pdf", "application/zip", "application/octet-stream"]) {
      await expect(copyFileItem({ mime, body: Uint8Array.of(1, 2, 3) }, env)).rejects.toThrow(
        "This file can't be copied.",
      );
    }
    expect(texts).toHaveLength(0);
    expect(writes).toHaveLength(0);
    expect(pngInputs).toHaveLength(0);
  });

  it("fails closed when this runtime has no clipboard", async () => {
    await expect(copyFileItem({ mime: "text/plain", body: utf8("secret") })).rejects.toThrow(
      "This browser can't copy that to the clipboard.",
    );
    await expect(copyFileItem({ mime: "image/png", body: Uint8Array.of(1, 2, 3) })).rejects.toThrow(
      "This browser can't copy that to the clipboard.",
    );
  });

  it("shows Copy for an image and text, and only Download for a PDF", () => {
    const now = Date.now();
    function labels(item: ItemRecord): string[] {
      const html = renderToStaticMarkup(
        createElement(ItemCard, {
          item,
          unlocked: true,
          now,
          ttlMs: 30 * 24 * 60 * 60 * 1000,
          pendingDelete: false,
          onAskDelete: () => {},
          onDelete: () => {},
        }) as ReactNode,
      );
      return [...html.matchAll(/<button\b[^>]*>([\s\S]*?)<\/button>/g)].map((match) =>
        match[1].replace(/<[^>]+>/g, "").trim(),
      );
    }
    const file = (name: string, mime: string): ItemRecord => ({
      id: name,
      createdAt: now,
      size: 4,
      plain: { kind: "file", name, mime, body: Uint8Array.of(1, 2, 3, 4) },
    });

    expect(labels(file("shot.png", "image/png"))).toEqual(["Copy", "Download", "Delete"]);
    expect(labels(file("notes.txt", "text/plain"))).toContain("Copy");
    expect(labels(file("picture.jpg", "image/jpeg"))).toContain("Copy");
    expect(labels(file("icon.svg", "image/svg+xml"))).toContain("Copy");
    expect(labels(file("archive.zip", "application/zip"))).toEqual(["Download", "Delete"]);
    expect(labels(file("paper.pdf", "application/pdf"))).toEqual(["Download", "Delete"]);
    expect(labels({
      id: "text",
      createdAt: now,
      size: 5,
      plain: { kind: "text", name: "", mime: "text/plain", body: utf8("hello"), text: "hello" },
    })).toEqual(["Copy", "Delete"]);
  });

  it("shows the days left in red beside the date only under 4 days", () => {
    const now = 1_700_000_000_000;
    const day = 24 * 60 * 60 * 1000;
    const ttlMs = 30 * day;
    function html(createdAt: number): string {
      const item: ItemRecord = {
        id: "note",
        createdAt,
        size: 5,
        plain: { kind: "text", name: "", mime: "text/plain", body: utf8("hello"), text: "hello" },
      };
      return renderToStaticMarkup(
        createElement(ItemCard, {
          item,
          unlocked: true,
          now,
          ttlMs,
          pendingDelete: false,
          onAskDelete: () => {},
          onDelete: () => {},
        }) as ReactNode,
      );
    }

    const soon = html(now - ttlMs + 4 * day - 1);
    expect(daysLeft(now - ttlMs + 4 * day - 1, ttlMs, now)).toBe("3 days left");
    expect(soon).toContain(formatWhen(now - ttlMs + 4 * day - 1, now));
    expect(soon).toContain("3 days left");
    expect(soon).toContain("text-destructive");

    const steady = html(now - ttlMs + 4 * day);
    expect(daysLeft(now - ttlMs + 4 * day, ttlMs, now)).toBeNull();
    expect(steady).toContain(formatWhen(now - ttlMs + 4 * day, now));
    expect(steady).not.toContain("days left");
    expect(steady).not.toContain("Less than a day");
    expect(steady).not.toContain("text-destructive");
  });

  it("stays in the browser: no fetch, object URL, or download", () => {
    const source = readFileSync(new URL("../src/client/lib/clipboard.ts", import.meta.url), "utf8");
    expect(source).not.toMatch(/\bfetch\s*\(|createObjectURL|\.download\b|XMLHttpRequest|sendBeacon/);
  });
});
