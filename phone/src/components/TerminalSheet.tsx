"use client";

// tmux terminal sheet. First a picker of every tmux pane; tap one to open it in
// xterm.js with full scrollback, type into it (raw bytes → tmux send-keys), and
// scroll its history. xterm is dynamically imported so it never runs during the
// static build.

import { useEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";
import { Send, msg } from "../lib/protocol";
import type {
  OutputHandler,
  PaneInfo,
  PanesHandler,
  Status,
} from "../lib/useAgent";

type Props = {
  onClose: () => void;
  status: Status;
  send: Send;
  sendBytes: (bytes: Uint8Array) => void;
  onOutput: (handler: OutputHandler) => () => void;
  onPanes: (handler: PanesHandler) => () => void;
};

export function TerminalSheet({
  onClose,
  status,
  send,
  sendBytes,
  onOutput,
  onPanes,
}: Props) {
  const [panes, setPanes] = useState<PaneInfo[] | null>(null);
  const [selected, setSelected] = useState<PaneInfo | null>(null);

  useEffect(() => {
    const off = onPanes(setPanes);
    send(msg.tlist());
    return off;
  }, [onPanes, send]);

  // Refresh the list whenever we (re)connect and aren't inside a pane.
  useEffect(() => {
    if (status === "connected" && !selected) send(msg.tlist());
  }, [status, selected, send]);

  if (selected) {
    return (
      <PaneView
        pane={selected}
        status={status}
        send={send}
        sendBytes={sendBytes}
        onOutput={onOutput}
        onBack={() => setSelected(null)}
        onClose={onClose}
      />
    );
  }
  return (
    <PaneList
      panes={panes}
      onRefresh={() => send(msg.tlist())}
      onPick={setSelected}
      onClose={onClose}
    />
  );
}

function PaneList({
  panes,
  onRefresh,
  onPick,
  onClose,
}: {
  panes: PaneInfo[] | null;
  onRefresh: () => void;
  onPick: (p: PaneInfo) => void;
  onClose: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-[#0a0a0a]">
      <div className="flex items-center justify-between px-3 py-2">
        <span className="font-mono text-sm font-semibold tracking-tight text-emerald-300/90">
          {">_ shells"}
        </span>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onRefresh}
            className="select-none rounded-lg bg-white/10 px-3 py-1 text-sm text-white/80 active:bg-white/20"
          >
            Refresh
          </button>
          <button
            type="button"
            onClick={onClose}
            className="select-none rounded-lg bg-white/10 px-3 py-1 text-sm text-white/80 active:bg-white/20"
          >
            Done
          </button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-3 pb-4">
        {panes === null ? (
          <p className="mt-8 text-center text-sm text-white/40">Loading…</p>
        ) : panes.length === 0 ? (
          <div className="mt-8 px-4 text-center text-sm leading-relaxed text-white/50">
            No tmux panes found.
            <br />
            On your Mac, start one and run Claude Code inside it:
            <pre className="mt-3 rounded-lg bg-white/5 px-3 py-2 text-left font-mono text-xs text-emerald-300/80">
              tmux new -s work{"\n"}claude
            </pre>
            then tap Refresh.
          </div>
        ) : (
          <ul className="flex flex-col gap-2">
            {panes.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  onClick={() => onPick(p)}
                  className="flex w-full items-center justify-between rounded-xl bg-white/5 px-4 py-3 text-left active:bg-white/10"
                >
                  <span className="flex flex-col">
                    <span className="font-medium">{p.label}</span>
                    <span className="font-mono text-xs text-white/45">
                      {p.cmd} · {p.w}×{p.h}
                    </span>
                  </span>
                  {p.active && (
                    <span className="h-2 w-2 rounded-full bg-emerald-400" />
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

const QUICK_KEYS: { label: string; bytes: number[] }[] = [
  { label: "Esc", bytes: [0x1b] },
  { label: "Tab", bytes: [0x09] },
  { label: "⏎", bytes: [0x0d] },
  { label: "↑", bytes: [0x1b, 0x5b, 0x41] },
  { label: "↓", bytes: [0x1b, 0x5b, 0x42] },
  { label: "←", bytes: [0x1b, 0x5b, 0x44] },
  { label: "→", bytes: [0x1b, 0x5b, 0x43] },
  { label: "⌃C", bytes: [0x03] },
];

function PaneView({
  pane,
  status,
  send,
  sendBytes,
  onOutput,
  onBack,
  onClose,
}: {
  pane: PaneInfo;
  status: Status;
  send: Send;
  sendBytes: (b: Uint8Array) => void;
  onOutput: (h: OutputHandler) => () => void;
  onBack: () => void;
  onClose: () => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const termRef = useRef<any>(null);
  const [zoom, setZoom] = useState(1);
  // Track the visible viewport so the input bar stays above the keyboard.
  const [viewH, setViewH] = useState<number | null>(null);
  const sawConnected = useRef(false);

  useEffect(() => {
    const vv = window.visualViewport;
    const update = () =>
      setViewH(vv ? Math.round(vv.height) : window.innerHeight);
    update();
    vv?.addEventListener("resize", update);
    vv?.addEventListener("scroll", update);
    window.addEventListener("resize", update);
    return () => {
      vv?.removeEventListener("resize", update);
      vv?.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let off: (() => void) | null = null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let term: any = null;
    const enc = new TextEncoder();

    (async () => {
      const { Terminal } = await import("@xterm/xterm");
      if (disposed || !hostRef.current) return;
      term = new Terminal({
        cols: pane.w || 80,
        rows: pane.h || 24,
        scrollback: 10000,
        fontFamily:
          'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
        fontSize: 12,
        theme: { background: "#0a0a0a", foreground: "#e5e5e5" },
        cursorBlink: false,
      });
      term.open(hostRef.current);
      termRef.current = term;
      term.onData((d: string) => sendBytes(enc.encode(d)));
      // Register the output sink *before* selecting, so we don't miss the
      // scrollback snapshot the agent sends on select.
      off = onOutput((buf) => term.write(new Uint8Array(buf)));
      send(msg.tsel(pane.id));
    })();

    return () => {
      disposed = true;
      off?.();
      term?.dispose();
      termRef.current = null;
    };
  }, [pane, send, sendBytes, onOutput]);

  // Re-select (re-stream) after a reconnect, but not on the initial connect
  // (the effect above already did that).
  useEffect(() => {
    if (status !== "connected") return;
    if (sawConnected.current) send(msg.tsel(pane.id));
    else sawConnected.current = true;
  }, [status, pane.id, send]);

  return (
    <div
      className="fixed left-0 top-0 z-50 flex w-full flex-col bg-[#0a0a0a]"
      style={{ height: viewH ? `${viewH}px` : "100dvh" }}
    >
      <div className="flex items-center justify-between px-3 py-2">
        <button
          type="button"
          onClick={onBack}
          className="select-none rounded-lg bg-white/10 px-3 py-1 text-sm text-white/80 active:bg-white/20"
        >
          ‹ Shells
        </button>
        <span className="truncate px-2 font-mono text-xs text-white/60">
          {pane.label}
        </span>
        <button
          type="button"
          onClick={onClose}
          className="select-none rounded-lg bg-white/10 px-3 py-1 text-sm text-white/80 active:bg-white/20"
        >
          Done
        </button>
      </div>

      {/* terminal — scroll to pan, scale to zoom */}
      <div
        className="relative min-h-0 flex-1 overflow-auto bg-[#0a0a0a]"
        style={{ touchAction: "pan-x pan-y" }}
      >
        <div
          ref={hostRef}
          style={{ transform: `scale(${zoom})`, transformOrigin: "0 0", width: "max-content" }}
        />
      </div>

      {/* quick keys + zoom */}
      <div className="flex items-center gap-1.5 overflow-x-auto px-2 py-2">
        {QUICK_KEYS.map((k) => (
          <KeyBtn key={k.label} onPress={() => sendBytes(new Uint8Array(k.bytes))}>
            {k.label}
          </KeyBtn>
        ))}
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          <KeyBtn onPress={() => setZoom((z) => Math.max(0.5, +(z * 0.85).toFixed(2)))}>
            −
          </KeyBtn>
          <span className="w-10 text-center text-xs tabular-nums text-white/50">
            {Math.round(zoom * 100)}%
          </span>
          <KeyBtn onPress={() => setZoom((z) => Math.min(3, +(z * 1.18).toFixed(2)))}>
            +
          </KeyBtn>
        </div>
      </div>

      {/* type bar → raw bytes → tmux send-keys */}
      <div className="px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]">
        <TypeBar sendBytes={sendBytes} />
      </div>
    </div>
  );
}

/** Diff-based input so soft keyboards, autocorrect and dictation all turn into
 *  the right bytes streamed to the pane. */
function TypeBar({ sendBytes }: { sendBytes: (b: Uint8Array) => void }) {
  const last = useRef("");
  const [value, setValue] = useState("");
  const enc = useRef(new TextEncoder());

  const diff = (next: string) => {
    const prev = last.current;
    if (next === prev) return;
    let i = 0;
    const max = Math.min(next.length, prev.length);
    while (i < max && next[i] === prev[i]) i++;
    for (let b = 0; b < prev.length - i; b++) sendBytes(new Uint8Array([0x7f]));
    const typed = next.slice(i);
    if (typed) sendBytes(enc.current.encode(typed));
    last.current = next;
  };

  const submit = () => {
    sendBytes(new Uint8Array([0x0d]));
    last.current = "";
    setValue("");
  };

  return (
    <div className="flex items-end gap-2">
      <textarea
        rows={1}
        value={value}
        onChange={(e) => {
          const v = e.target.value;
          if (v.includes("\n")) {
            diff(v.slice(0, v.indexOf("\n")));
            submit();
            return;
          }
          diff(v);
          setValue(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            submit();
          }
        }}
        placeholder="Type here → goes to this shell"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        className="min-h-11 flex-1 resize-none rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-base outline-none placeholder:text-white/30 focus:border-white/30"
      />
      <button
        type="button"
        onClick={submit}
        className="min-h-11 select-none rounded-xl bg-emerald-500/20 px-4 text-emerald-200 active:bg-emerald-500/30"
      >
        ⏎
      </button>
    </div>
  );
}

function KeyBtn({
  children,
  onPress,
}: {
  children: React.ReactNode;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPress}
      className="shrink-0 select-none rounded-lg bg-white/10 px-3 py-1.5 font-mono text-sm text-white/80 active:bg-white/20"
    >
      {children}
    </button>
  );
}
