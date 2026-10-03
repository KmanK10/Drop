import { fromUtf8, utf8 } from "./bytes.ts";

const MAGIC = [0x44, 0x52, 0x50, 0x31]; // DRP1

export type ItemPlain = {
  kind: "text" | "file";
  name: string;
  mime: string;
  body: Uint8Array;
  text?: string;
};

export function encodeItem(item: { kind: "text" | "file"; name: string; mime: string; body: Uint8Array }): Uint8Array {
  const nameBytes = utf8(item.name);
  const mimeBytes = utf8(item.mime);
  if (nameBytes.length > 512) throw new Error("That name is too long.");
  if (mimeBytes.length > 200) throw new Error("That file type is too long.");
  const out = new Uint8Array(4 + 1 + 2 + nameBytes.length + 2 + mimeBytes.length + item.body.length);
  const view = new DataView(out.buffer);
  out.set(MAGIC, 0);
  out[4] = item.kind === "text" ? 1 : 2;
  view.setUint16(5, nameBytes.length, false);
  out.set(nameBytes, 7);
  const mimeAt = 7 + nameBytes.length;
  view.setUint16(mimeAt, mimeBytes.length, false);
  out.set(mimeBytes, mimeAt + 2);
  out.set(item.body, mimeAt + 2 + mimeBytes.length);
  return out;
}

export function decodeItem(bytes: Uint8Array): ItemPlain {
  if (bytes.length < 4 + 1 + 2 + 2) throw new Error("Item is truncated.");
  for (let i = 0; i < MAGIC.length; i++) {
    if (bytes[i] !== MAGIC[i]) throw new Error("Item format is not recognized.");
  }
  const kindByte = bytes[4];
  if (kindByte !== 1 && kindByte !== 2) throw new Error("Item format is not recognized.");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const nameLen = view.getUint16(5, false);
  let offset = 7;
  if (offset + nameLen + 2 > bytes.length) throw new Error("Item is truncated.");
  const name = fromUtf8(bytes.subarray(offset, offset + nameLen));
  offset += nameLen;
  const mimeLen = view.getUint16(offset, false);
  offset += 2;
  if (offset + mimeLen > bytes.length) throw new Error("Item is truncated.");
  const mime = fromUtf8(bytes.subarray(offset, offset + mimeLen));
  offset += mimeLen;
  const body = bytes.slice(offset);
  const kind = kindByte === 1 ? "text" : "file";
  return {
    kind,
    name,
    mime,
    body,
    text: kind === "text" ? fromUtf8(body) : undefined,
  };
}

export function safeDownloadName(name: string): string {
  const base = name.replace(/[/\\]/g, "").replace(/^\.+/, "").trim().slice(0, 180);
  return base || "download";
}
