"use client";

import { useCallback, useRef, useState } from "react";
import { Send, msg } from "../lib/protocol";
import { PadButton } from "./PadButton";

/**
 * Text input that streams keystrokes to the Mac. Each edit is diffed against
 * the previous value so typing, dictation, and autocorrect all inject as the
 * right mix of backspaces + typed text at the Mac's current focus.
 */
export function TextBar({ send }: { send: Send }) {
  const last = useRef("");
  const [value, setValue] = useState("");

  const diffSend = useCallback(
    (next: string) => {
      const prev = last.current;
      if (next === prev) return;
      let i = 0;
      const max = Math.min(next.length, prev.length);
      while (i < max && next[i] === prev[i]) i++;
      for (let b = 0; b < prev.length - i; b++) send(msg.key("backspace"));
      const typed = next.slice(i);
      if (typed) send(msg.text(typed));
      last.current = next;
    },
    [send],
  );

  // Submit = send Enter and reset the buffer (text is already gone on the Mac).
  const submit = useCallback(() => {
    send(msg.key("enter"));
    last.current = "";
    setValue("");
  }, [send]);

  return (
    <div className="flex items-end gap-2">
      <textarea
        rows={2}
        value={value}
        onChange={(e) => {
          const v = e.target.value;
          // Soft keyboards often don't fire keydown for Return — they insert a
          // newline. Treat any newline as submit instead of typing it.
          if (v.includes("\n")) {
            diffSend(v.slice(0, v.indexOf("\n")));
            submit();
            return;
          }
          diffSend(v);
          setValue(v);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            submit();
          } else if (e.key === "Backspace" && value === "") {
            e.preventDefault();
            send(msg.key("backspace"));
          }
        }}
        placeholder="Type or dictate → your Mac"
        autoCapitalize="sentences"
        className="min-h-12 flex-1 resize-none rounded-xl border border-white/10 bg-white/[0.06] px-3 py-2 text-base outline-none backdrop-blur placeholder:text-white/30 focus:border-emerald-300/40"
      />
      <PadButton accent onPress={submit}>
        ⏎
      </PadButton>
    </div>
  );
}
