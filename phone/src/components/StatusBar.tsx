"use client";

import type { PushState } from "../lib/push";
import type { Status } from "../lib/useAgent";

const LABEL: Record<Status, string> = {
  connecting: "Connecting…",
  connected: "Live",
  denied: "Pairing failed — reopen the QR link",
  disconnected: "Reconnecting…",
};

const DOT: Record<Status, string> = {
  connecting: "bg-amber-400",
  connected: "bg-emerald-400 pf-live",
  denied: "bg-red-500",
  disconnected: "bg-amber-400",
};

export function StatusBar({
  status,
  alert = false,
  push = "unsupported",
  onSettings,
  onTerminal,
  onPush,
}: {
  status: Status;
  /** A Copilot event is pending — badge the terminal button. */
  alert?: boolean;
  /** Lock-screen push state; the 🔔 hides when unsupported. */
  push?: PushState;
  onSettings: () => void;
  onTerminal: () => void;
  onPush?: () => void;
}) {
  return (
    <div className="pf-glass flex items-center justify-between rounded-2xl px-4 py-2.5 text-sm">
      <span className="pf-brand text-base font-bold tracking-tight">
        PointFlow
      </span>
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-2 text-white/60">
          <span className={`h-2 w-2 rounded-full ${DOT[status]}`} />
          {LABEL[status]}
        </span>
        {push !== "unsupported" && (
          <button
            type="button"
            aria-label="Notifications"
            onClick={onPush}
            className={`pf-press select-none rounded-xl border px-2 py-1 text-sm leading-none ${
              push === "on"
                ? "border-emerald-300/25 bg-emerald-400/15 text-emerald-300"
                : push === "denied"
                  ? "border-red-400/25 bg-red-500/10 text-red-300/70"
                  : "border-white/10 bg-white/[0.06] text-white/50"
            }`}
          >
            {push === "on" ? "🔔" : "🔕"}
          </button>
        )}
        <button
          type="button"
          aria-label="Terminal"
          onClick={onTerminal}
          className="pf-press relative select-none rounded-xl border border-emerald-300/20 bg-emerald-400/10 px-2.5 py-1 font-mono text-sm leading-none text-emerald-300"
        >
          {">_"}
          {alert && (
            <span className="pf-live absolute -right-1 -top-1 h-2.5 w-2.5 rounded-full bg-amber-400" />
          )}
        </button>
        <button
          type="button"
          aria-label="Settings"
          onClick={onSettings}
          className="pf-press select-none rounded-xl px-1.5 py-0.5 text-lg leading-none text-white/50"
        >
          ⚙
        </button>
      </div>
    </div>
  );
}
