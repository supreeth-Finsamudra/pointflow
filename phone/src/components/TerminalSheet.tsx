"use client";

// Full-screen terminal overlay. Renders the agent's streamed shell with
// xterm.js: PTY output → screen, taps focus the input so the phone keyboard
// pops up, keystrokes → PTY. xterm is dynamically imported so it never runs
// during the static build/prerender.

import { useEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";
import type { TermHandler } from "../lib/useAgent";

type Props = {
  onClose: () => void;
  sendBytes: (bytes: Uint8Array) => void;
  sendResize: (cols: number, rows: number) => void;
  onTerm: (handler: TermHandler) => () => void;
};

export function TerminalSheet({
  onClose,
  sendBytes,
  sendResize,
  onTerm,
}: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const termRef = useRef<any>(null);
  // Track the *visible* viewport height. When the on-screen keyboard opens it
  // shrinks the visual viewport; we shrink the sheet to match so the active
  // prompt line (and what you type) stays above the keyboard instead of behind
  // it. The ResizeObserver below then refits xterm to the new height.
  const [viewH, setViewH] = useState<number | null>(null);

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
    let cleanupTerm: (() => void) | null = null;
    let ro: ResizeObserver | null = null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let term: any = null;
    const enc = new TextEncoder();

    (async () => {
      const [{ Terminal }, { FitAddon }] = await Promise.all([
        import("@xterm/xterm"),
        import("@xterm/addon-fit"),
      ]);
      if (disposed || !hostRef.current) return;

      term = new Terminal({
        cursorBlink: true,
        fontFamily:
          'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
        fontSize: 13,
        theme: { background: "#0a0a0a", foreground: "#e5e5e5" },
        macOptionIsMeta: true,
        scrollback: 5000,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.open(hostRef.current);
      termRef.current = term;

      const doFit = () => {
        try {
          fit.fit();
          sendResize(term.cols, term.rows);
          term.scrollToBottom();
        } catch {
          /* ignore transient measurement errors */
        }
      };
      doFit();
      term.focus();

      // Keystrokes → PTY.
      term.onData((d: string) => sendBytes(enc.encode(d)));

      // PTY output → screen (history replay + live).
      cleanupTerm = onTerm((bytes) => term.write(bytes));

      // Refit whenever the host box changes size — including when `viewH`
      // shrinks the sheet for the keyboard.
      ro = new ResizeObserver(() => doFit());
      ro.observe(hostRef.current);
    })();

    return () => {
      disposed = true;
      cleanupTerm?.();
      ro?.disconnect();
      term?.dispose();
      termRef.current = null;
    };
  }, [onTerm, sendBytes, sendResize]);

  return (
    <div
      className="fixed left-0 top-0 z-50 flex w-full flex-col bg-[#0a0a0a]"
      style={{ height: viewH ? `${viewH}px` : "100dvh" }}
    >
      <div className="flex items-center justify-between px-3 py-2">
        <span className="font-mono text-sm font-semibold tracking-tight text-emerald-300/90">
          {">_ terminal"}
        </span>
        <button
          type="button"
          onClick={onClose}
          className="select-none rounded-lg bg-white/10 px-3 py-1 text-sm text-white/80 active:bg-white/20"
        >
          Done
        </button>
      </div>
      <div
        ref={hostRef}
        // Tapping focuses xterm's input inside the gesture, which reliably
        // raises the phone keyboard.
        onPointerDown={() => termRef.current?.focus()}
        className="min-h-0 flex-1 overflow-hidden px-2"
      />
    </div>
  );
}
