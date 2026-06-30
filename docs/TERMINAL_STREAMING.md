# Terminal streaming (live window mirror)

See and drive whatever is **already running** in your Mac's terminal — Claude
Code included — from your phone. The phone shows a live picture of your real
terminal window and types into it through the input path PointFlow already has.
No new shell, no cloud.

## Why a window mirror (not a PTY)

An earlier approach spawned its own PTY shell, but that's a *separate* process —
it can't show the terminal (and Claude Code session) you already have open. To
mirror "whatever is running," the agent captures your actual terminal **window**
and streams it; typing reuses the existing keystroke injection so it lands in
the real, focused terminal.

Tradeoff: it's video (JPEG frames), so it needs **Screen Recording** permission
and is heavier than text. The win: zero workflow change — keep using
Terminal.app/iTerm and Claude Code exactly as you do now.

## Architecture

```
            JPEG frames (active terminal window)
  [ Mac agent ] ───────────────────────────────► [ phone: <img> mirror ]
   • xcap capture  ◄──── text / key / chord ─────   • zoom + pan
   • JPEG encode        (existing injection)         • quick keys + type bar
   • input injection ──► drives the REAL terminal    • Focus button
```

- **Capture** (`agent/src/mirror.rs`): a background thread picks the focused (or
  front-most) terminal window via `xcap` — which uses macOS **ScreenCaptureKit**
  under the hood — captures it, downscales to ≤1280 px wide, JPEG-encodes
  (q=70), and broadcasts the frame. Runs ~10 fps, and **only while a phone is
  viewing** (a viewer counter gates capture so it idles cheaply otherwise).
- **Transport**: frames go over the existing token-paired WebSocket as **binary**
  frames. JSON text still carries the auth handshake and all control/input.
- **Typing**: the phone sends the same `text` / `key` / `chord` messages the
  trackpad keyboard uses; they inject into the focused app — i.e. the terminal
  being mirrored. `mfocus` brings that terminal to the front so keystrokes land.
- **Untouched**: the trackpad/keyboard injection path (`input.rs`) is unchanged;
  the mirror only *adds* the picture.

## Wire protocol (additions)

| Direction | Frame | Meaning |
| --- | --- | --- |
| agent → phone | **binary** | one JPEG frame of the active terminal window |
| agent → phone | text `{"t":"ok"\|"denied"}` | existing auth handshake |
| phone → agent | text `{"t":"mstart"}` | start mirroring (increments viewers) |
| phone → agent | text `{"t":"mstop"}` | stop mirroring |
| phone → agent | text `{"t":"mfocus"}` | bring the mirrored terminal to front |
| phone → agent | text `{"t":"text"\|"key"\|"chord",…}` | inject into the real terminal |

## Phone UX

- A `>_` button in the status bar opens a full-screen mirror sheet.
- The live window renders into an `<img>`; **zoom −/+** and scroll-to-pan let you
  read small text. The sheet height tracks the visual viewport so the type bar
  stays above the on-screen keyboard.
- A **type bar** (reused from the trackpad) injects what you type/dictate; a
  **quick-key row** sends Esc / Tab / arrows / ⌃C for Claude Code's prompts.
- A **Focus** button re-fronts the terminal if focus drifted.

## Permission

First capture needs **Screen Recording**: System Settings → Privacy & Security →
Screen Recording → enable the app that launched the agent (Terminal/iTerm),
then restart the agent. Until granted, window enumeration returns nothing and
the phone shows a "waiting for your terminal" hint.

## Status / follow-ups

- MVP: mirrors the focused/front-most terminal; last-writer-wins for multiple
  phones; ~10 fps JPEG.
- Possible later: H.264/VideoToolbox for smoother/cheaper streaming; let the
  phone pick which window from a list; map two-finger swipe to inject scroll for
  terminal scrollback; tap-on-image → click via coordinate mapping.
- The previous PTY implementation is kept (uncompiled) in `agent/src/term.rs`
  for a future opt-in "PointFlow shell"/tmux mode.
