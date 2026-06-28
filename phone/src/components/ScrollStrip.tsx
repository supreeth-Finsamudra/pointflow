"use client";

import { useEffect, useRef } from "react";
import { Send } from "../lib/protocol";
import { createScroller } from "../lib/scroller";
import { useSettings } from "../lib/settings";

/** Right-edge strip — drag with a thumb to scroll (scrollbar-style + momentum). */
export function ScrollStrip({ send }: { send: Send }) {
  const { settings } = useSettings();
  const settingsRef = useRef(settings);
  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const scroller = createScroller(send, () => ({
      speed: settingsRef.current.scrollSpeed,
      momentum: settingsRef.current.momentum,
    }));
    const st = { active: false, y: 0 };

    const onStart = (e: TouchEvent) => {
      e.preventDefault();
      st.active = true;
      st.y = e.touches[0].clientY;
      scroller.begin();
    };
    const onMove = (e: TouchEvent) => {
      e.preventDefault();
      if (!st.active) return;
      const y = e.touches[0].clientY;
      const dy = y - st.y; // finger down → scroll down
      st.y = y;
      scroller.move(0, dy, e.timeStamp);
    };
    const onEnd = (e: TouchEvent) => {
      e.preventDefault();
      st.active = false;
      scroller.release();
    };

    el.addEventListener("touchstart", onStart, { passive: false });
    el.addEventListener("touchmove", onMove, { passive: false });
    el.addEventListener("touchend", onEnd, { passive: false });
    el.addEventListener("touchcancel", onEnd, { passive: false });
    return () => {
      el.removeEventListener("touchstart", onStart);
      el.removeEventListener("touchmove", onMove);
      el.removeEventListener("touchend", onEnd);
      el.removeEventListener("touchcancel", onEnd);
      scroller.cancel();
    };
  }, [send]);

  return (
    <div
      ref={ref}
      className="flex w-12 shrink-0 touch-none select-none flex-col items-center justify-between rounded-2xl border border-white/10 bg-white/[0.03] py-4 text-white/30 active:bg-white/[0.06]"
    >
      <span className="text-lg leading-none">⌃</span>
      <span
        className="text-[10px] tracking-widest"
        style={{ writingMode: "vertical-rl" }}
      >
        SCROLL
      </span>
      <span className="text-lg leading-none">⌄</span>
    </div>
  );
}
