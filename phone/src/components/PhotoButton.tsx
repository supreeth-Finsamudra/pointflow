"use client";

// Photo → Mac: picks/takes a photo, uploads it to the agent (saved under
// ~/Downloads/PointFlow), then hands the saved path to the caller — which
// inserts it into the terminal prompt so Claude Code can read the image.

import { useRef, useState } from "react";
import { getToken } from "../lib/useViewport";

export function PhotoButton({
  onPath,
}: {
  onPath: (path: string) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [state, setState] = useState<"idle" | "up" | "ok" | "err">("idle");

  const pick = async (file: File) => {
    setState("up");
    try {
      const res = await fetch(
        `/upload?token=${encodeURIComponent(getToken())}&name=${encodeURIComponent(file.name || "photo.jpg")}`,
        { method: "POST", body: file },
      );
      if (!res.ok) throw new Error(String(res.status));
      const { path } = await res.json();
      onPath(path);
      setState("ok");
    } catch {
      setState("err");
    }
    setTimeout(() => setState("idle"), 1600);
  };

  return (
    <>
      <button
        type="button"
        aria-label="Send a photo"
        onClick={() => inputRef.current?.click()}
        className={`pf-press flex min-h-11 w-11 shrink-0 items-center justify-center rounded-xl border text-lg ${
          state === "ok"
            ? "border-emerald-400/40 bg-emerald-500/25"
            : state === "err"
              ? "border-red-400/40 bg-red-500/20"
              : "border-white/10 bg-white/[0.06]"
        }`}
      >
        {state === "up" ? (
          <span className="pf-spin inline-block">◌</span>
        ) : state === "ok" ? (
          "✓"
        ) : state === "err" ? (
          "!"
        ) : (
          "📷"
        )}
      </button>
      <input
        ref={inputRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) pick(f);
          e.target.value = "";
        }}
      />
    </>
  );
}
