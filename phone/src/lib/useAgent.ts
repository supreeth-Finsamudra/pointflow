"use client";

// WebSocket connection to the agent: pairs with the token from the URL, exposes
// a stable `send`, and auto-reconnects. Also carries the terminal stream —
// binary frames are raw PTY bytes (agent→phone) and keystrokes (phone→agent),
// kept separate from the JSON input/control messages.

import { useCallback, useEffect, useRef, useState } from "react";
import { Send, msg } from "./protocol";

export type Status = "connecting" | "connected" | "denied" | "disconnected";

export type TermHandler = (data: Uint8Array) => void;

export type Agent = {
  status: Status;
  send: Send;
  /** Send raw keystroke bytes to the streamed shell. */
  sendBytes: (bytes: Uint8Array) => void;
  /** Tell the agent the terminal viewport size (cols × rows). */
  sendResize: (cols: number, rows: number) => void;
  /**
   * Register a sink for terminal output. Immediately replays buffered history
   * (so a freshly-opened terminal shows current state), then streams live
   * bytes. Returns an unsubscribe function.
   */
  onTerm: (handler: TermHandler) => () => void;
};

/** How much shell output to mirror on the client for replay-on-open. */
const TERM_BUFFER_CAP = 256 * 1024;

export function useAgent(): Agent {
  const [status, setStatus] = useState<Status>("connecting");
  const wsRef = useRef<WebSocket | null>(null);
  const tokenRef = useRef<string>("");

  // Rolling mirror of the shell's recent output. The terminal sheet creates a
  // fresh xterm each time it opens, so we replay this buffer into it — even for
  // output that streamed in while the sheet was closed.
  const termBufRef = useRef<Uint8Array[]>([]);
  const termBufLenRef = useRef(0);
  const termHandlerRef = useRef<TermHandler | null>(null);

  const pushTerm = useCallback((bytes: Uint8Array) => {
    termBufRef.current.push(bytes);
    termBufLenRef.current += bytes.length;
    while (
      termBufLenRef.current > TERM_BUFFER_CAP &&
      termBufRef.current.length > 1
    ) {
      const removed = termBufRef.current.shift();
      if (removed) termBufLenRef.current -= removed.length;
    }
    termHandlerRef.current?.(bytes);
  }, []);

  const send = useCallback<Send>((obj) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
  }, []);

  const sendBytes = useCallback((bytes: Uint8Array) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(bytes);
  }, []);

  const sendResize = useCallback(
    (cols: number, rows: number) => send(msg.tresize(cols, rows)),
    [send],
  );

  const onTerm = useCallback((handler: TermHandler) => {
    termHandlerRef.current = handler;
    // Replay history into the freshly-opened terminal.
    for (const chunk of termBufRef.current) handler(chunk);
    return () => {
      if (termHandlerRef.current === handler) termHandlerRef.current = null;
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
          // Raw PTY output.
          pushTerm(new Uint8Array(ev.data));
        }
      };
      ws.onclose = () => {
        if (closed) return;
        // The next connection replays a full scrollback snapshot, so drop the
        // local mirror to avoid double-rendering history on reconnect.
        termBufRef.current = [];
        termBufLenRef.current = 0;
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
  }, [pushTerm]);

  return { status, send, sendBytes, sendResize, onTerm };
}
