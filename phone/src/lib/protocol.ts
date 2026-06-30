// Wire protocol — message builders the phone sends to the agent over WebSocket.
// Single source of truth for the message shapes (mirrors agent/src/protocol.rs).

export type Send = (obj: Record<string, unknown>) => void;

export type ButtonName = "left" | "right" | "middle";

export const msg = {
  auth: (token: string) => ({ t: "auth", token }),
  move: (dx: number, dy: number) => ({ t: "move", dx, dy }),
  scroll: (dx: number, dy: number) => ({ t: "scroll", dx, dy }),
  click: (button: ButtonName = "left", double = false) => ({
    t: "click",
    button,
    double,
  }),
  down: (button: ButtonName = "left") => ({ t: "down", button }),
  up: (button: ButtonName = "left") => ({ t: "up", button }),
  text: (s: string) => ({ t: "text", s }),
  key: (k: string) => ({ t: "key", k }),
  chord: (mods: string[], key: string) => ({ t: "chord", mods, key }),
  // tmux terminal bridge. Output arrives as raw binary frames; keystrokes go
  // back as raw binary too (not through these JSON builders).
  tlist: () => ({ t: "tlist" }),
  tsel: (id: string) => ({ t: "tsel", id }),
};
