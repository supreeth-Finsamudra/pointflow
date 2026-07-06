# Terminal streaming (tmux bridge)

See and drive **all your running shells** from your phone: list every tmux pane,
pick one, read its **full colored scrollback**, scroll its history, and **type
into it** — even when it isn't the focused window. Crisp text, no cloud, and
**no Screen Recording or Accessibility permission**.

## Why tmux

A screenshot can't give you scrollback, selectable text, or easy multi-shell
switching — it's just the visible pixels of one window. And macOS exposes no API
to read an arbitrary terminal's text buffer. tmux is the one clean way in: it
already owns the shells' text, history, and input, so a handful of `tmux`
commands deliver everything:

| Requirement | tmux command |
| --- | --- |
| List all shells | `list-panes -a` |
| Pick one | target any `#{pane_id}` |
| Text **with color** | `capture-pane -e` |
| **Full** scrollback | `capture-pane -S -` |
| Type into it (unfocused ok) | `send-keys -H` (raw hex bytes) |
| Live output | `pipe-pane` → tail |

The tradeoff: the shells must run **inside tmux** (incl. Claude Code). Anything
in tmux is fully phone-accessible; a plain window opened outside tmux isn't.

## Architecture

```
            {t:"panes",[…]}            select  →  capture-pane -S -e  (snapshot)
  [ phone ] ───────────────► [ agent ] ─────────► pipe-pane → tail   (live)
   xterm.js  ◄── binary (pane output) ──            send-keys -H     (your keys)
   pane list ─── binary (keystrokes) ──►
```

- `agent/src/tmux.rs` shells out to `tmux` (found at `/opt/homebrew/bin/tmux`
  even under a minimal PATH). It tracks one **selected pane**:
  - `panes_json()` → `list-panes -a` formatted to JSON for the picker.
  - `select(id)` → `capture-pane -p -e -S -` broadcast as the **snapshot**, then
    `pipe-pane -O … 'cat >> tmpfile'` + a `tail -F` thread broadcast as **live**.
  - `send_keys(bytes)` → `send-keys -t id -H <hex…>`, so every key (text,
    arrows, Ctrl-C) round-trips byte-exact.
- The WebSocket multiplexes one sink: **binary** frames are pane output (agent→
  phone) and keystrokes (phone→agent); **JSON text** carries `tlist`/`tsel` and
  the `panes` reply. The mouse/keyboard injection path is untouched.

## Wire protocol (additions)

| Direction | Frame | Meaning |
| --- | --- | --- |
| phone → agent | `{"t":"tlist"}` | request the pane list |
| phone → agent | `{"t":"tsel","id":"%3"}` | select a pane to view/drive |
| phone → agent | **binary** | raw key bytes → `send-keys` on the selected pane |
| agent → phone | `{"t":"panes","panes":[…]}` | pane list |
| agent → phone | **binary** | selected pane's output (snapshot, then live) |

## Phone UX

- The `>_` button opens a **shell picker** (every pane, with its command + size).
- Tap a pane → it opens in **xterm.js** (the renderer registers *before* `tsel`
  so the scrollback snapshot isn't missed). Full history is scrollable; **−/+**
  zoom (CSS scale) + scroll to pan for wide panes.
- A **type bar** (diff-based, so dictation/autocorrect work) and a **quick-key
  row** (Esc · Tab · ⏎ · arrows · ⌃C) send raw bytes to the pane.
- Sheet height tracks the visual viewport so the type bar stays above the
  keyboard. Reconnects re-select the pane automatically.

## Using it

```bash
# on the Mac, inside a tmux session:
tmux new -s work
claude            # (or anything) — now visible/drivable from the phone
```

Run the agent (`cd agent && cargo run`), open the QR URL on your phone, tap
`>_`, pick the `work` pane.

## Status / follow-ups

- One selected pane at a time; last-writer-wins for multiple phones.
- Snapshot (rendered history) + live (raw stream) can have a small seam at the
  boundary; TUIs that repaint (Claude Code) reconcile on their next draw.
- The previous PTY shell lives (uncompiled) in `agent/src/term.rs` for a future
  opt-in mode; the screen-capture mirror was removed (couldn't do scrollback or
  text and was slow on macOS 26).
