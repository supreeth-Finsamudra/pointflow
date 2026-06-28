"use client";

import { useCallback, useEffect, useRef, useState } from "react";

type Status = "connecting" | "connected" | "denied" | "disconnected";

// Pointer feel — tuned for thumb use; refine later.
const MOVE_GAIN = 1.7; // base cursor speed multiplier
const ACCEL = 0.05; // extra gain proportional to swipe speed
const SCROLL_DIV = 6; // px of finger travel per scroll notch
const TAP_MS = 250; // max touch duration to count as a tap
const TAP_SLOP = 8; // max movement (px) to still count as a tap
const HOLD_MS = 450; // hold-still duration that starts a drag

export default function Page() {
  const [status, setStatus] = useState<Status>("connecting");
  const wsRef = useRef<WebSocket | null>(null);
  const tokenRef = useRef<string>("");

  const send = useCallback((obj: Record<string, unknown>) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
  }, []);

  // --- connection (with auto-reconnect) ---
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    tokenRef.current = params.get("token") ?? "";

    let closed = false;
    let retry: ReturnType<typeof setTimeout> | undefined;

    const connect = () => {
      const proto = window.location.protocol === "https:" ? "wss" : "ws";
      const ws = new WebSocket(`${proto}://${window.location.host}/ws`);
      wsRef.current = ws;
      setStatus("connecting");

      ws.onopen = () =>
        ws.send(JSON.stringify({ t: "auth", token: tokenRef.current }));
      ws.onmessage = (ev) => {
        try {
          const msg = JSON.parse(ev.data);
          if (msg.t === "ok") setStatus("connected");
          else if (msg.t === "denied") setStatus("denied");
        } catch {
          /* ignore */
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

  return (
    <main className="flex h-dvh flex-col gap-3 p-3">
      <StatusBar status={status} />
      <Trackpad send={send} />
      <ClickRow send={send} />
      <KeyRow send={send} />
      <TextBar send={send} />
    </main>
  );
}

function StatusBar({ status }: { status: Status }) {
  const label: Record<Status, string> = {
    connecting: "Connecting…",
    connected: "Connected",
    denied: "Pairing failed — reopen the QR link",
    disconnected: "Reconnecting…",
  };
  const dot: Record<Status, string> = {
    connecting: "bg-amber-400",
    connected: "bg-emerald-400",
    denied: "bg-red-500",
    disconnected: "bg-amber-400",
  };
  return (
    <div className="flex items-center justify-between rounded-xl bg-white/5 px-4 py-2 text-sm">
      <span className="font-semibold tracking-tight">PointFlow</span>
      <span className="flex items-center gap-2 text-white/70">
        <span className={`h-2 w-2 rounded-full ${dot[status]}`} />
        {label[status]}
      </span>
    </div>
  );
}

function Trackpad({ send }: { send: (o: Record<string, unknown>) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  // Mutable gesture state kept in a ref to avoid re-renders mid-swipe.
  const g = useRef({
    active: false,
    id: -1,
    x: 0,
    y: 0,
    startX: 0,
    startY: 0,
    startT: 0,
    moved: false,
    dragging: false,
    holdTimer: undefined as ReturnType<typeof setTimeout> | undefined,
    scroll: false,
    scrollY: 0,
    scrollAcc: 0,
  });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const s = g.current;

    const clearHold = () => {
      if (s.holdTimer) clearTimeout(s.holdTimer);
      s.holdTimer = undefined;
    };

    const onStart = (e: TouchEvent) => {
      e.preventDefault();
      if (e.touches.length === 2) {
        // Two fingers → scroll mode.
        s.scroll = true;
        s.scrollY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
        s.scrollAcc = 0;
        clearHold();
        return;
      }
      const t = e.changedTouches[0];
      s.active = true;
      s.id = t.identifier;
      s.x = t.clientX;
      s.y = t.clientY;
      s.startX = t.clientX;
      s.startY = t.clientY;
      s.startT = e.timeStamp;
      s.moved = false;
      s.dragging = false;
      // Hold still briefly → begin a drag (press-and-hold).
      clearHold();
      s.holdTimer = setTimeout(() => {
        if (s.active && !s.moved) {
          s.dragging = true;
          send({ t: "down", button: "left" });
          navigator.vibrate?.(20);
        }
      }, HOLD_MS);
    };

    const onMove = (e: TouchEvent) => {
      e.preventDefault();
      if (s.scroll && e.touches.length >= 2) {
        const y = (e.touches[0].clientY + e.touches[1].clientY) / 2;
        s.scrollAcc += s.scrollY - y;
        s.scrollY = y;
        const notches = Math.trunc(s.scrollAcc / SCROLL_DIV);
        if (notches !== 0) {
          s.scrollAcc -= notches * SCROLL_DIV;
          send({ t: "scroll", dx: 0, dy: notches });
        }
        return;
      }
      if (!s.active) return;
      const t = Array.from(e.changedTouches).find((c) => c.identifier === s.id);
      if (!t) return;
      const dx = t.clientX - s.x;
      const dy = t.clientY - s.y;
      s.x = t.clientX;
      s.y = t.clientY;
      if (Math.hypot(t.clientX - s.startX, t.clientY - s.startY) > TAP_SLOP) {
        s.moved = true;
        clearHold();
      }
      const speed = Math.hypot(dx, dy);
      const gain = MOVE_GAIN * (1 + speed * ACCEL);
      send({ t: "move", dx: dx * gain, dy: dy * gain });
    };

    const onEnd = (e: TouchEvent) => {
      e.preventDefault();
      if (s.scroll) {
        if (e.touches.length === 0) s.scroll = false;
        return;
      }
      if (!s.active) return;
      clearHold();
      const dt = e.timeStamp - s.startT;
      if (s.dragging) {
        send({ t: "up", button: "left" });
      } else if (!s.moved && dt < TAP_MS) {
        send({ t: "click", button: "left" });
      }
      s.active = false;
      s.dragging = false;
    };

    // Non-passive so preventDefault actually blocks page scroll/zoom.
    el.addEventListener("touchstart", onStart, { passive: false });
    el.addEventListener("touchmove", onMove, { passive: false });
    el.addEventListener("touchend", onEnd, { passive: false });
    el.addEventListener("touchcancel", onEnd, { passive: false });
    return () => {
      el.removeEventListener("touchstart", onStart);
      el.removeEventListener("touchmove", onMove);
      el.removeEventListener("touchend", onEnd);
      el.removeEventListener("touchcancel", onEnd);
    };
  }, [send]);

  return (
    <div
      ref={ref}
      className="trackpad relative flex flex-1 select-none items-center justify-center rounded-2xl border border-white/10 bg-white/[0.03]"
    >
      <span className="pointer-events-none text-center text-sm leading-relaxed text-white/25">
        Swipe to move · tap to click
        <br />
        two fingers to scroll · hold to drag
      </span>
    </div>
  );
}

function ClickRow({ send }: { send: (o: Record<string, unknown>) => void }) {
  return (
    <div className="grid grid-cols-2 gap-3">
      <PadButton onPress={() => send({ t: "click", button: "left" })}>
        Left click
      </PadButton>
      <PadButton onPress={() => send({ t: "click", button: "right" })}>
        Right click
      </PadButton>
    </div>
  );
}

function KeyRow({ send }: { send: (o: Record<string, unknown>) => void }) {
  const keys: { label: string; k: string }[] = [
    { label: "⌫", k: "backspace" },
    { label: "⏎", k: "enter" },
    { label: "Tab", k: "tab" },
    { label: "Esc", k: "escape" },
  ];
  return (
    <div className="grid grid-cols-4 gap-3">
      {keys.map((key) => (
        <PadButton key={key.k} onPress={() => send({ t: "key", k: key.k })}>
          {key.label}
        </PadButton>
      ))}
    </div>
  );
}

/**
 * Text input that streams keystrokes to the Mac. Each edit is diffed against
 * the previous value so dictation, typing, and autocorrect all inject as the
 * right mix of backspaces + typed text at the Mac's current focus.
 */
function TextBar({ send }: { send: (o: Record<string, unknown>) => void }) {
  const last = useRef("");
  const [value, setValue] = useState("");

  const diffSend = useCallback(
    (next: string) => {
      const prev = last.current;
      if (next === prev) return;
      let i = 0;
      const max = Math.min(next.length, prev.length);
      while (i < max && next[i] === prev[i]) i++;
      const backspaces = prev.length - i;
      for (let b = 0; b < backspaces; b++) send({ t: "key", k: "backspace" });
      const typed = next.slice(i);
      if (typed) send({ t: "text", s: typed });
      last.current = next;
    },
    [send],
  );

  return (
    <div className="flex items-end gap-2">
      <textarea
        rows={2}
        value={value}
        onChange={(e) => {
          diffSend(e.target.value);
          setValue(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send({ t: "key", k: "enter" });
            // The remote field was just submitted — reset our buffer locally
            // (no backspaces; the typed text is already gone on the Mac side).
            last.current = "";
            setValue("");
          } else if (e.key === "Backspace" && value === "") {
            // Delete already-sent text once the local buffer is empty.
            e.preventDefault();
            send({ t: "key", k: "backspace" });
          }
        }}
        placeholder="Type or dictate here → goes to your Mac"
        autoCapitalize="sentences"
        className="min-h-12 flex-1 resize-none rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-base outline-none placeholder:text-white/30 focus:border-white/30"
      />
      <PadButton
        accent
        onPress={() => {
          // Submit: send Enter, then reset the local buffer (the typed text is
          // already gone on the Mac side, so no backspaces).
          send({ t: "key", k: "enter" });
          last.current = "";
          setValue("");
        }}
      >
        ⏎
      </PadButton>
    </div>
  );
}

function PadButton({
  children,
  onPress,
  accent = false,
}: {
  children: React.ReactNode;
  onPress: () => void;
  accent?: boolean;
}) {
  const base =
    "select-none rounded-xl border px-3 py-3 text-base font-medium";
  const theme = accent
    ? "border-emerald-400/30 bg-emerald-500/20 text-emerald-200 active:bg-emerald-500/35"
    : "border-white/10 bg-white/5 text-white/90 active:bg-white/15";
  return (
    <button
      type="button"
      // Fire on press for snappy, repeatable taps.
      onPointerDown={(e) => {
        e.preventDefault();
        onPress();
      }}
      className={`${base} ${theme}`}
    >
      {children}
    </button>
  );
}
