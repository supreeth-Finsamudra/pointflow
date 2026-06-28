"use client";

// WebSocket connection to the agent: pairs with the token from the URL, exposes
// a stable `send`, and auto-reconnects.

import { useCallback, useEffect, useRef, useState } from "react";
import { Send, msg } from "./protocol";

export type Status = "connecting" | "connected" | "denied" | "disconnected";

export function useAgent(): { status: Status; send: Send } {
  const [status, setStatus] = useState<Status>("connecting");
  const wsRef = useRef<WebSocket | null>(null);
  const tokenRef = useRef<string>("");

  const send = useCallback<Send>((obj) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
  }, []);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    tokenRef.current = params.get("token") ?? "";

    let closed = false;
    let retry: ReturnType<typeof setTimeout> | undefined;

    const connect = () => {
      const proto = window.location.protocol === "https:" ? "wss" : "ws";
      const ws = new WebSocket(`${proto}://${window.location.host}/ws`);
      wsRef.current = ws;
      setStatus("connecting");

      ws.onopen = () => ws.send(JSON.stringify(msg.auth(tokenRef.current)));
      ws.onmessage = (ev) => {
        try {
          const m = JSON.parse(ev.data);
          if (m.t === "ok") setStatus("connected");
          else if (m.t === "denied") setStatus("denied");
        } catch {
          /* ignore */
        }
      };
      ws.onclose = () => {
        if (closed) return;
        setStatus((s) => (s === "denied" ? s : "disconnected"));
        retry = setTimeout(connect, 1200);
      };
      ws.onerror = () => ws.close();
    };

    connect();
    return () => {
      closed = true;
      if (retry) clearTimeout(retry);
      wsRef.current?.close();
    };
  }, []);

  return { status, send };
}
