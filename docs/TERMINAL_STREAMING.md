# Terminal streaming

Drive a real shell (and Claude Code) from your phone — a crisp, text-based
terminal layered on top of the existing trackpad/keyboard, over the same
token-paired WebSocket. No extra macOS permission, no cloud.

## Why text, not video

The phone renders the shell with **xterm.js** fed by raw PTY bytes. That's
pixel-perfect text, ~nothing of bandwidth, and far lower latency than
screen-capturing a window. The tradeoff: it's PointFlow's own login shell, not
a mirror of an existing `Terminal.app` window. You run Claude Code *in this
shell* and watch/drive it from the phone.

## Architecture

```
            binary frames (PTY output)
  [ Mac agent ] ───────────────────────────► [ phone: xterm.js ]
   • one PTY    ◄───────────────────────────   • Terminal sheet
   • login shell   binary frames (keystrokes)   • tap-to-type
                   {t:"tresize",cols,rows}
```

- **One persistent PTY** is spawned at agent startup (`agent/src/term.rs`),
  running `$SHELL -l`. It outlives any single phone connection, so Claude Code
  keeps running across phone sleeps/reconnects.
- A **reader thread** pumps PTY output into (a) a capped **scrollback ring**
  (256 KB) and (b) a Tokio **broadcast** channel — under one lock, so attach is
  race-free.
- Each WebSocket, after the existing token auth, **splits** into:
  - a send task: replays the scrollback snapshot, then streams live output as
    **binary** frames;
  - a recv task: routes **binary** frames → PTY input, `tresize` JSON →
    `master.resize()`, and everything else through the unchanged input path.
- This is fully **additive**: mouse/keyboard injection (`input.rs`) is
  untouched; terminal I/O never goes through `enigo`.

## Wire protocol (additions only)

| Direction | Frame | Meaning |
| --- | --- | --- |
| agent → phone | **binary** | raw PTY output bytes (write straight to xterm) |
| agent → phone | text `{"t":"ok"\|"denied"}` | existing auth handshake (unchanged) |
| phone → agent | **binary** | raw keystroke bytes from xterm `onData` |
| phone → agent | text `{"t":"tresize","cols":C,"rows":R}` | resize the PTY |

Binary vs text is the discriminator: binary is always terminal payload; text is
always JSON control/input. No base64, no ambiguity with existing messages.

## Reconnect / seamlessness

- The PTY is process-lifetime, not connection-lifetime → Claude Code survives
  reconnects.
- On (re)attach the phone gets the scrollback snapshot, then sends `tresize`
  for its viewport, which triggers `SIGWINCH` and makes TUIs (Claude Code,
  vim, etc.) repaint to the current state.

## Phone UX

- A **Terminal** button in the status bar opens a full-screen sheet (xterm.js +
  FitAddon). Closing returns to the trackpad — the trackpad keeps working.
- Tapping the terminal focuses xterm's hidden textarea inside the touch
  gesture, which raises the mobile keyboard. Autocorrect/autocapitalize are
  disabled so prompts aren't mangled.

## Status / follow-ups

- MVP: single shared session, last-resize-wins, no auth beyond the existing
  session token (terminal rides the already-authenticated socket).
- Possible later: detect a frontmost `Terminal.app`/`iTerm` to auto-surface the
  button; `tmux`-backed shared session visible on the Mac too; multiple panes.
