"use client";

// WebSocket connection to the agent: pairs with the token from the URL, exposes
// a stable `send`, and auto-reconnects. For the tmux terminal bridge: binary
// frames are the selected pane's output (snapshot + live), `{t:"panes"}` carries
// the shell list, and keystrokes go back as raw binary.

import { useCallback, useEffect, useRef, useState } from "react";
import { Send, msg } from "./protocol";

export type Status = "connecting" | "connected" | "denied" | "disconnected";

export type PaneInfo = {
  id: string;
  label: string;
  cmd: string;
  active: boolean;
  w: number;
  h: number;
  /** Basename of the pane's working directory ("point-flow"). */
  cwd?: string;
  /** Copilot status from Claude Code hooks: "waiting" | "done" | absent. */
  status?: string;
};

/** A Copilot event relayed from a Claude Code hook. */
export type CopilotEvent = {
  kind: "notification" | "stop" | string;
  pane: string;
  label: string;
  message: string;
};

/** An open Terminal.app tab (no tmux needed). */
export type TabInfo = {
  win: number;
  tab: number;
  tty: string;
  busy: boolean;
  procs: string;
  claude: boolean;
  /** Basename of the shell's working directory — the project name. */
  cwd?: string;
};

export type OutputHandler = (bytes: ArrayBuffer) => void;
export type PanesHandler = (panes: PaneInfo[]) => void;
export type EventHandler = (ev: CopilotEvent) => void;
export type TabsHandler = (tabs: TabInfo[]) => void;
/** kind: "hist" = one-time scrollback replay, "scr" = current screen. */
export type TabTextHandler = (kind: "hist" | "scr", text: string) => void;

export type Agent = {
  status: Status;
  send: Send;
  /** Send raw key bytes to the selected tmux pane. */
  sendBytes: (bytes: Uint8Array) => void;
  /** Register a sink for the selected pane's output bytes. */
  onOutput: (handler: OutputHandler) => () => void;
  /** Register a handler for the tmux pane list. */
  onPanes: (handler: PanesHandler) => () => void;
  /** Register a handler for Copilot events (Claude Code needs you / done). */
  onEvent: (handler: EventHandler) => () => void;
  /** Register a handler for the Terminal.app tab list. */
  onTabs: (handler: TabsHandler) => () => void;
  /** Register a handler for a selected tab's text (history + screen). */
  onTabText: (handler: TabTextHandler) => () => void;
};

export function useAgent(): Agent {
  const [status, setStatus] = useState<Status>("connecting");
  const wsRef = useRef<WebSocket | null>(null);
  const tokenRef = useRef<string>("");
  const outRef = useRef<OutputHandler | null>(null);
  const panesRef = useRef<PanesHandler | null>(null);
  const eventRef = useRef<EventHandler | null>(null);
  const tabsRef = useRef<TabsHandler | null>(null);
  const tabTextRef = useRef<TabTextHandler | null>(null);

  const send = useCallback<Send>((obj) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
  }, []);

  const sendBytes = useCallback((bytes: Uint8Array) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(bytes);
  }, []);

  const onOutput = useCallback((handler: OutputHandler) => {
    outRef.current = handler;
    return () => {
      if (outRef.current === handler) outRef.current = null;
    };
  }, []);

  const onPanes = useCallback((handler: PanesHandler) => {
    panesRef.current = handler;
    return () => {
      if (panesRef.current === handler) panesRef.current = null;
    };
  }, []);

  const onEvent = useCallback((handler: EventHandler) => {
    eventRef.current = handler;
    return () => {
      if (eventRef.current === handler) eventRef.current = null;
    };
  }, []);

  const onTabs = useCallback((handler: TabsHandler) => {
    tabsRef.current = handler;
    return () => {
      if (tabsRef.current === handler) tabsRef.current = null;
    };
  }, []);

  const onTabText = useCallback((handler: TabTextHandler) => {
    tabTextRef.current = handler;
    return () => {
      if (tabTextRef.current === handler) tabTextRef.current = null;
    };
  }, []);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const fromUrl = params.get("token");
    try {
      if (fromUrl) localStorage.setItem("pf.token", fromUrl);
      tokenRef.current = fromUrl ?? localStorage.getItem("pf.token") ?? "";
    } catch {
      tokenRef.current = fromUrl ?? "";
    }

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
            else if (m.t === "panes") panesRef.current?.(m.panes ?? []);
            else if (m.t === "event") eventRef.current?.(m);
            else if (m.t === "tabs") tabsRef.current?.(m.tabs ?? []);
            else if (m.t === "tabhist") tabTextRef.current?.("hist", m.text ?? "");
            else if (m.t === "tabscr") tabTextRef.current?.("scr", m.text ?? "");
          } catch {
            /* ignore */
          }
        } else if (ev.data instanceof ArrayBuffer) {
          outRef.current?.(ev.data);
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

    // iOS suspends timers and sockets in background tabs; when the user
    // returns, reconnect immediately instead of waiting out the retry timer.
    const onVisible = () => {
      if (document.visibilityState !== "visible") return;
      const ws = wsRef.current;
      if (!ws || ws.readyState >= WebSocket.CLOSING) {
        if (retry) clearTimeout(retry);
        connect();
      }
    };
    document.addEventListener("visibilitychange", onVisible);

    return () => {
      closed = true;
      if (retry) clearTimeout(retry);
      document.removeEventListener("visibilitychange", onVisible);
      wsRef.current?.close();
    };
  }, []);

  return { status, send, sendBytes, onOutput, onPanes, onEvent, onTabs, onTabText };
}
