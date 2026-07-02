"use client";

// Copilot notification card: slides in when a Claude Code hook fires (needs
// your permission/input, or finished). Approve sends Enter to that pane, Deny
// sends Esc — via tmux send-keys, so the pane doesn't need to be focused or
// even open on the phone. "Open" jumps into the pane view for context.

import { Send, msg } from "../lib/protocol";
import type { CopilotEvent } from "../lib/useAgent";

type Props = {
  event: CopilotEvent;
  send: Send;
  onOpen: (pane: { id: string; label: string }) => void;
  onDismiss: () => void;
};

export function CopilotCard({ event, send, onOpen, onDismiss }: Props) {
  const needsYou = event.kind === "notification" && event.pane !== "";
  const reply = (hex: string) => {
    send(msg.tkeys(event.pane, hex));
    onDismiss();
  };

  return (
    <div
      className="pf-drop fixed inset-x-3 z-[70] rounded-2xl border border-emerald-300/25 bg-[#0c0c11]/95 p-4 shadow-2xl shadow-emerald-500/10 backdrop-blur-xl"
      style={{
        // Below the iOS status bar in the installed PWA, so ✕ stays tappable.
        top: "max(0.75rem, calc(env(safe-area-inset-top) + 0.25rem))",
        boxShadow: "0 0 40px rgba(16,185,129,.14), 0 18px 50px rgba(0,0,0,.6)",
      }}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="pf-brand text-xs font-bold uppercase tracking-widest">
            {event.kind === "stop" ? "✓ Claude finished" : "✳ Claude needs you"}
          </p>
          <p className="mt-1 break-words text-sm leading-snug text-white/90">
            {event.message}
          </p>
          {event.label && (
            <p className="mt-1 truncate font-mono text-xs text-white/40">
              {event.label}
            </p>
          )}
        </div>
        <button
          type="button"
          aria-label="Dismiss"
          onClick={onDismiss}
          className="shrink-0 select-none rounded-lg px-2 py-0.5 text-white/40 active:text-white"
        >
          ✕
        </button>
      </div>

      <div className="mt-3 flex gap-2">
        {needsYou && (
          <>
            <button
              type="button"
              onClick={() => reply("0d")}
              className="pf-press pf-accent flex-1 select-none rounded-xl py-2 text-sm font-semibold"
            >
              Approve ⏎
            </button>
            <button
              type="button"
              onClick={() => reply("1b")}
              className="pf-press flex-1 select-none rounded-xl border border-red-400/25 bg-red-500/15 py-2 text-sm font-medium text-red-300"
            >
              Deny Esc
            </button>
          </>
        )}
        {event.pane && (
          <button
            type="button"
            onClick={() => {
              onOpen({ id: event.pane, label: event.label });
              onDismiss();
            }}
            className="pf-press flex-1 select-none rounded-xl border border-white/10 bg-white/10 py-2 text-sm font-medium text-white/80"
          >
            Open shell
          </button>
        )}
      </div>
    </div>
  );
}
