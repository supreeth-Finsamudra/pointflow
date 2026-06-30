"use client";

// Full-screen mirror of the Mac's active terminal window. Live JPEG frames are
// painted into an <img>; typing/keys reuse the existing injection path so they
// drive the *real* focused terminal (where Claude Code is running). The sheet
// height tracks the visual viewport so the type bar stays above the on-screen
// keyboard.

import { useEffect, useRef, useState } from "react";
import { Send, msg } from "../lib/protocol";
import type { FrameHandler } from "../lib/useAgent";
import { TextBar } from "./TextBar";

type Props = {
  onClose: () => void;
  send: Send;
  onFrame: (handler: FrameHandler) => () => void;
};

const QUICK_KEYS: { label: string; k: string }[] = [
  { label: "Esc", k: "escape" },
  { label: "Tab", k: "tab" },
  { label: "↑", k: "up" },
  { label: "↓", k: "down" },
  { label: "←", k: "left" },
  { label: "→", k: "right" },
];

export function TerminalSheet({ onClose, send, onFrame }: Props) {
  const imgRef = useRef<HTMLImageElement>(null);
  const urlRef = useRef<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [gotFrame, setGotFrame] = useState(false);
  // Shrink the sheet to the visible viewport when the keyboard opens, so the
  // type bar/quick keys don't end up hidden behind it.
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
    // Begin capture and bring the terminal to the front so typing lands in it.
    send(msg.mstart());
    send(msg.mfocus());

    const cleanup = onFrame((bytes) => {
      const img = imgRef.current;
      if (!img) return;
      const url = URL.createObjectURL(
        new Blob([bytes], { type: "image/jpeg" }),
      );
      const old = urlRef.current;
      img.src = url;
      urlRef.current = url;
      // The previous frame is already decoded/displayed; safe to free.
      if (old) URL.revokeObjectURL(old);
      setGotFrame(true);
    });

    return () => {
      cleanup();
      send(msg.mstop());
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
      urlRef.current = null;
    };
  }, [onFrame, send]);

  return (
    <div
      className="fixed left-0 top-0 z-50 flex w-full flex-col bg-[#0a0a0a]"
      style={{ height: viewH ? `${viewH}px` : "100dvh" }}
    >
      {/* header */}
      <div className="flex items-center justify-between px-3 py-2">
        <span className="font-mono text-sm font-semibold tracking-tight text-emerald-300/90">
          {">_ live terminal"}
        </span>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => send(msg.mfocus())}
            className="select-none rounded-lg bg-white/10 px-3 py-1 text-sm text-white/80 active:bg-white/20"
          >
            Focus
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

      {/* mirror viewport — scroll to pan when zoomed in */}
      <div
        className="relative min-h-0 flex-1 overflow-auto bg-black"
        style={{ touchAction: "pan-x pan-y" }}
      >
        {!gotFrame && (
          <div className="absolute inset-0 flex items-center justify-center px-6 text-center text-sm leading-relaxed text-white/40">
            Waiting for your terminal… If it stays blank, grant{" "}
            <span className="text-white/70">Screen Recording</span> permission to
            your terminal on the Mac (System Settings → Privacy &amp; Security)
            and restart the agent.
          </div>
        )}
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          ref={imgRef}
          alt="Live terminal"
          style={{ width: `${zoom * 100}%`, height: "auto", display: "block" }}
        />
      </div>

      {/* quick keys + zoom */}
      <div className="flex items-center gap-1.5 overflow-x-auto px-2 py-2">
        {QUICK_KEYS.map((key) => (
          <KeyBtn key={key.k} onPress={() => send(msg.key(key.k))}>
            {key.label}
          </KeyBtn>
        ))}
        <KeyBtn onPress={() => send(msg.chord(["ctrl"], "c"))}>⌃C</KeyBtn>
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          <KeyBtn
            onPress={() => setZoom((z) => Math.max(0.5, +(z * 0.8).toFixed(2)))}
          >
            −
          </KeyBtn>
          <span className="w-10 text-center text-xs tabular-nums text-white/50">
            {Math.round(zoom * 100)}%
          </span>
          <KeyBtn
            onPress={() => setZoom((z) => Math.min(4, +(z * 1.25).toFixed(2)))}
          >
            +
          </KeyBtn>
        </div>
      </div>

      {/* typing → injected into the focused (mirrored) terminal */}
      <div className="px-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]">
        <TextBar send={send} />
      </div>
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
