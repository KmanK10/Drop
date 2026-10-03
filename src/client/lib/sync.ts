import { useEffect, useRef, useState } from "react";

/** Live updates for the signed-in user. The socket carries ids only, never item contents. */
export function useLiveSync(onSignal: () => void): boolean {
  const handler = useRef(onSignal);
  handler.current = onSignal;
  const [live, setLive] = useState(false);

  useEffect(() => {
    let socket: WebSocket | null = null;
    let stopped = false;
    let delay = 500;
    let timer = 0;

    function connect() {
      const proto = window.location.protocol === "https:" ? "wss" : "ws";
      socket = new WebSocket(`${proto}://${window.location.host}/api/ws`);
      socket.onopen = () => {
        delay = 500;
        setLive(true);
      };
      socket.onmessage = (event) => {
        if (typeof event.data !== "string") return;
        try {
          const message = JSON.parse(event.data) as { type?: string };
          if (message.type === "ping") return;
          if (message.type === "hello" || message.type === "created" || message.type === "deleted") {
            handler.current();
          }
        } catch {
          // Ignore malformed frames.
        }
      };
      socket.onclose = () => {
        setLive(false);
        if (stopped) return;
        timer = window.setTimeout(connect, delay);
        delay = Math.min(delay * 2, 10_000);
      };
    }

    connect();
    return () => {
      stopped = true;
      window.clearTimeout(timer);
      socket?.close();
    };
  }, []);

  return live;
}
