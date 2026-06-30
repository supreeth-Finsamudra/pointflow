"use client";

// Full-screen terminal overlay. Renders the agent's streamed shell with
// xterm.js: PTY output → screen, taps focus the input so the phone keyboard
// pops up, keystrokes → PTY. xterm is dynamically imported so it never runs
// during the static build/prerender.

import { useEffect, useRef } from "react";
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

  useEffect(() => {
    let disposed = false;
    let cleanupTerm: (() => void) | null = null;
    let ro: ResizeObserver | null = null;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let term: any = null;
    let doFit: (() => void) | null = null;
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

      doFit = () => {
        try {
          fit.fit();
          sendResize(term.cols, term.rows);
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

      ro = new ResizeObserver(() => doFit?.());
      ro.observe(hostRef.current);
      window.addEventListener("resize", () => doFit?.());
      // The mobile keyboard changes the visual viewport, not window size.
      window.visualViewport?.addEventListener("resize", () => doFit?.());
    })();

    return () => {
      disposed = true;
      cleanupTerm?.();
      ro?.disconnect();
      if (doFit) {
        window.removeEventListener("resize", doFit);
        window.visualViewport?.removeEventListener("resize", doFit);
      }
      term?.dispose();
    };
  }, [onTerm, sendBytes, sendResize]);

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-[#0a0a0a]">
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
        className="min-h-0 flex-1 overflow-hidden px-2 pb-[env(safe-area-inset-bottom)]"
      />
    </div>
  );
}
