"use client";

import { Send, msg } from "../lib/protocol";
import { PadButton } from "./PadButton";

const KEYS: { label: string; k: string }[] = [
  { label: "⌫", k: "backspace" },
  { label: "⏎", k: "enter" },
  { label: "Tab", k: "tab" },
  { label: "Esc", k: "escape" },
];

export function ControlButtons({ send }: { send: Send }) {
  return (
    <>
      <div className="grid grid-cols-2 gap-3">
        <PadButton onPress={() => send(msg.click("left"))}>Left click</PadButton>
        <PadButton onPress={() => send(msg.click("right"))}>
          Right click
        </PadButton>
      </div>
      <div className="grid grid-cols-4 gap-3">
        {KEYS.map((key) => (
          <PadButton key={key.k} onPress={() => send(msg.key(key.k))}>
            {key.label}
          </PadButton>
        ))}
      </div>
    </>
  );
}
