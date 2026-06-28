"use client";

import { Send } from "../lib/protocol";
import { useSettings } from "../lib/settings";
import { useTrackpad } from "../lib/useTrackpad";

export function Trackpad({ send }: { send: Send }) {
  const { settings } = useSettings();
  const ref = useTrackpad(send, settings);

  return (
    <div
      ref={ref}
      className="trackpad relative flex flex-1 select-none items-center justify-center rounded-2xl border border-white/10 bg-white/[0.03]"
    >
      <span className="pointer-events-none px-6 text-center text-sm leading-relaxed text-white/25">
        Swipe to move · tap to click
        <br />
        hold to drag/select · two fingers to scroll
        <br />
        three fingers to switch spaces
      </span>
    </div>
  );
}
