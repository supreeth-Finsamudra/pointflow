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
    <div className="fixed inset-x-3 top-3 z-[70] rounded-2xl border border-white/10 bg-[#151517] p-4 shadow-2xl shadow-black/60">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-xs font-semibold uppercase tracking-wide text-emerald-300/80">
            {event.kind === "stop" ? "Claude finished" : "Claude needs you"}
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
              className="flex-1 select-none rounded-xl bg-emerald-500/20 py-2 text-sm font-medium text-emerald-200 active:bg-emerald-500/30"
            >
              Approve ⏎
            </button>
            <button
              type="button"
              onClick={() => reply("1b")}
              className="flex-1 select-none rounded-xl bg-red-500/15 py-2 text-sm font-medium text-red-300 active:bg-red-500/25"
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
            className="flex-1 select-none rounded-xl bg-white/10 py-2 text-sm font-medium text-white/80 active:bg-white/20"
          >
            Open shell
          </button>
        )}
      </div>
    </div>
  );
}
