import type { WebSocket } from "ws";

export type LiveEvent =
  | { type: "hello" }
  | { type: "ping" }
  | { type: "created"; item: { id: string; createdAt: number; size: number } }
  | { type: "deleted"; id: string };

type Entry = { userId: string; ws: WebSocket };

export class Hub {
  private sockets = new Set<Entry>();

  add(userId: string, ws: WebSocket): void {
    const entry = { userId, ws };
    this.sockets.add(entry);
    ws.on("close", () => {
      this.sockets.delete(entry);
    });
    this.send(ws, { type: "hello" });
  }

  publish(userId: string, event: Exclude<LiveEvent, { type: "hello" } | { type: "ping" }>): void {
    for (const entry of this.sockets) {
      if (entry.userId === userId) this.send(entry.ws, event);
    }
  }

  disconnect(userId: string): void {
    for (const entry of this.sockets) {
      if (entry.userId !== userId) continue;
      this.sockets.delete(entry);
      try {
        entry.ws.close();
      } catch {
        // already closed
      }
    }
  }

  ping(): void {
    for (const entry of this.sockets) this.send(entry.ws, { type: "ping" });
  }

  close(): void {
    for (const entry of this.sockets) {
      try {
        entry.ws.close();
      } catch {
        // already closed
      }
    }
    this.sockets.clear();
  }

  private send(ws: WebSocket, event: LiveEvent): void {
    if (ws.readyState !== ws.OPEN) return;
    ws.send(JSON.stringify(event));
  }
}
