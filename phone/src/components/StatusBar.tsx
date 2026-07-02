"use client";

import type { Status } from "../lib/useAgent";

const LABEL: Record<Status, string> = {
  connecting: "Connecting…",
  connected: "Connected",
  denied: "Pairing failed — reopen the QR link",
  disconnected: "Reconnecting…",
};

const DOT: Record<Status, string> = {
  connecting: "bg-amber-400",
  connected: "bg-emerald-400",
  denied: "bg-red-500",
  disconnected: "bg-amber-400",
};

export function StatusBar({
  status,
  alert = false,
  onSettings,
  onTerminal,
}: {
  status: Status;
  /** A Copilot event is pending — badge the terminal button. */
  alert?: boolean;
  onSettings: () => void;
  onTerminal: () => void;
}) {
  return (
    <div className="flex items-center justify-between rounded-xl bg-white/5 px-4 py-2 text-sm">
      <span className="font-semibold tracking-tight">PointFlow</span>
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-2 text-white/70">
          <span className={`h-2 w-2 rounded-full ${DOT[status]}`} />
          {LABEL[status]}
        </span>
        <button
          type="button"
          aria-label="Terminal"
          onClick={onTerminal}
          className="relative select-none rounded-lg bg-emerald-400/10 px-2 py-0.5 font-mono text-sm leading-none text-emerald-300/90 active:bg-emerald-400/20"
        >
          {">_"}
          {alert && (
            <span className="absolute -right-1 -top-1 h-2.5 w-2.5 rounded-full bg-amber-400" />
          )}
        </button>
        <button
          type="button"
          aria-label="Settings"
          onClick={onSettings}
          className="select-none rounded-lg px-1.5 py-0.5 text-lg leading-none text-white/60 active:text-white"
        >
          ⚙
        </button>
      </div>
    </div>
  );
}
