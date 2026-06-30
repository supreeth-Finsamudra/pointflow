"use client";

// WebSocket connection to the agent: pairs with the token from the URL, exposes
// a stable `send`, and auto-reconnects. Binary frames are live JPEG snapshots
// of the Mac's active terminal window (the screen mirror); JSON text carries
// the auth handshake.

import { useCallback, useEffect, useRef, useState } from "react";
import { Send, msg } from "./protocol";

export type Status = "connecting" | "connected" | "denied" | "disconnected";

export type FrameHandler = (jpeg: ArrayBuffer) => void;

export type Agent = {
  status: Status;
  send: Send;
  /**
   * Register a sink for live mirror frames (JPEG bytes). Returns an
   * unsubscribe function. Only the latest frame matters, so there's no replay.
   */
  onFrame: (handler: FrameHandler) => () => void;
};

export function useAgent(): Agent {
  const [status, setStatus] = useState<Status>("connecting");
  const wsRef = useRef<WebSocket | null>(null);
  const tokenRef = useRef<string>("");
  const frameHandlerRef = useRef<FrameHandler | null>(null);

  const send = useCallback<Send>((obj) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
  }, []);

  const onFrame = useCallback((handler: FrameHandler) => {
    frameHandlerRef.current = handler;
    return () => {
      if (frameHandlerRef.current === handler) frameHandlerRef.current = null;
    };
  }, []);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    tokenRef.current = params.get("token") ?? "";

    let closed = false;
    let retry: ReturnType<typeof setTimeout> | undefined;

    const connect = () => {
      const proto = window.location.protocol === "https:" ? "wss" : "ws";
      const ws = new WebSocket(`${proto}://${window.location.host}/ws`);
      ws.binaryType = "arraybuffer";
      wsRef.current = ws;
      setStatus("connecting");

      ws.onopen = () => ws.send(JSON.stringify(msg.auth(tokenRef.current)));
      ws.onmessage = (ev) => {
        if (typeof ev.data === "string") {
          try {
            const m = JSON.parse(ev.data);
            if (m.t === "ok") setStatus("connected");
            else if (m.t === "denied") setStatus("denied");
          } catch {
            /* ignore */
          }
        } else if (ev.data instanceof ArrayBuffer) {
          frameHandlerRef.current?.(ev.data);
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

  return { status, send, onFrame };
}
