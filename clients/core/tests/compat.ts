import { readFileSync } from "node:fs";
import { decrypt, encrypt, importContentKey } from "../../../src/shared/crypto.ts";
import { decodeItem, encodeItem } from "../../../src/shared/item.ts";

const input = JSON.parse(readFileSync(0, "utf8")) as {
  op: "encrypt" | "decrypt";
  key: string;
  blob?: string;
  kind?: "text" | "file";
  name?: string;
  mime?: string;
  body?: string;
};

const key = await importContentKey(Uint8Array.from(Buffer.from(input.key, "hex")));

if (input.op === "decrypt") {
  const plain = decodeItem(await decrypt(key, Uint8Array.from(Buffer.from(input.blob ?? "", "hex"))));
  process.stdout.write(
    JSON.stringify({
      kind: plain.kind,
      name: plain.name,
      mime: plain.mime,
      text: plain.text ?? null,
      body: Buffer.from(plain.body).toString("hex"),
    }),
  );
} else {
  const blob = await encrypt(
    key,
    encodeItem({
      kind: input.kind ?? "text",
      name: input.name ?? "",
      mime: input.mime ?? "text/plain",
      body: Uint8Array.from(Buffer.from(input.body ?? "", "hex")),
    }),
  );
  process.stdout.write(Buffer.from(blob).toString("hex"));
}
