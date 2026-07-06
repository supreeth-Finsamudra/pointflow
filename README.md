# PointFlow

**Your phone is now your computer's trackpad, keyboard, terminal — and the
remote control for your AI coding agents.**

Swipe to move the cursor. Type or dictate into whatever app has focus. Open
your shells in a real terminal on your phone, watch Claude Code work, and
**approve its next step from the couch, the café, or the lock screen.** No app
install on the phone, no cloud, no account — the agent on your computer serves
a web page over WiFi and injects real input locally.

```
            ┌───────────────────────────────────────┐
[ Phone ] ──┤  Agent (Rust, single ~4 MB binary)     │──► moves cursor, clicks,
 browser    │   • serves the phone UI (embedded)     │    types into ANY app
  trackpad  │   • WebSocket for input + terminals    │──► streams your shells
  keyboard  │   • token-paired (QR), LAN or tunnel   │◄── Claude Code hooks:
  terminal  └───────────────────────────────────────┘    "needs you" → phone
```

## Why people use it

- 🖱 **A genuinely good trackpad** — sub-pixel pointer with acceleration,
  momentum scrolling, a full gesture vocabulary (tap, drag, two-finger scroll,
  three-finger Mission Control), tunable to your thumb.
- ⌨️ **Type anywhere** — your phone keyboard (including voice dictation,
  swipe, emoji) lands in whatever has focus on the desktop. Like Whispr
  Flow's "insert text anywhere," but the input device is your phone.
- 🖥 **Real terminals on your phone** — full-color xterm.js views of your
  shells with scrollback, quick keys (Esc·Tab·⏎·arrows·⌃C), a compose box
  built for prompts, even photo-upload-to-file-path for multimodal prompts.
- ✳️ **Claude Code copilot mode** — hook events surface as cards and
  lock-screen push notifications: *"Claude needs your permission"* →
  **Approve ⏎ / Deny Esc** from your phone, without the Mac unlocked or the
  window focused. Prompt it, watch the diff stream by, approve, repeat.
- 🌍 **Works away from home** — `--tunnel` prints a public HTTPS QR
  (Cloudflare quick tunnel): drive your desktop from cellular, no VPN.
- 🔒 **Local-first** — no cloud, no account, no telemetry. Every connection
  needs the session token from the QR. Your keystrokes never leave your
  network unless *you* start the tunnel.

## Install

**macOS (Homebrew):**
```bash
brew install supreeth-Finsamudra/pointflow/pointflow
pointflow-agent
```

**macOS (one-liner):**
```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/supreeth-Finsamudra/pointflow/releases/latest/download/pointflow-agent-installer.sh | sh
```

**Windows (PowerShell):** *(new — beta)*
```powershell
irm https://github.com/supreeth-Finsamudra/pointflow/releases/latest/download/pointflow-agent-installer.ps1 | iex
```

**From source:**
```bash
git clone https://github.com/supreeth-Finsamudra/pointflow && cd pointflow
cd phone && pnpm install && pnpm build && cd ..   # build the phone UI once
cd agent && cargo run                              # debug builds read it live
```

Run the agent, scan the QR with your phone (same WiFi), start swiping.
Away from home: `pointflow-agent --tunnel` (needs
[cloudflared](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/)).

### macOS: Accessibility permission
Injecting input needs one approval: **System Settings → Privacy & Security →
Accessibility** → enable your terminal (or the app that launches the agent),
then restart the agent.

## The terminal, by platform

| Platform | What you get |
| --- | --- |
| **macOS** | Bridge to your **tmux panes** (full color/TUIs/scrollback — best with Claude Code) *and* your already-open **Terminal.app tabs** (zero setup). `+ New` on the phone spawns a fresh shell. |
| **Windows** *(beta)* | `+ New` spawns PointFlow-owned **ConPTY** shells (pwsh/powershell/cmd) with the same phone UX. (Windows offers no API to read other terminals' buffers — so shells live inside PointFlow.) |
| **Linux** | tmux bridge (agent builds; input layer needs X11/libei — see roadmap). |

## Claude Code copilot

```bash
pointflow-agent --install-hooks   # one-time; merges into ~/.claude/settings.json
```

Now every Claude Code session reports to your phone: **"✳ Claude needs you"**
cards with Approve/Deny, **"✓ finished"** when a task completes — as real
lock-screen push notifications when installed as a PWA over the tunnel
(Share → Add to Home Screen). Multiple sessions are tracked per shell.

## Gestures

| Gesture | Action |
| --- | --- |
| Swipe | Move cursor (sub-pixel, accelerated) |
| Tap / double-tap | Click / double click |
| Double-tap-drag, hold-drag | Drag, text select |
| Hold then lift / two-finger tap | Right click |
| Two-finger swipe | Scroll (momentum) |
| Three-finger swipe | Mission Control · App Exposé · switch spaces (macOS) |
| Right-edge strip | Scrollbar-style scroll |
| ⚙ settings | Pointer/scroll speed, natural scroll, tap-to-click — persisted |

## Configuration

| Env var | Default | Purpose |
| --- | --- | --- |
| `POINTFLOW_PORT` | `8742` | Listen port |
| `POINTFLOW_WEB_DIR` | *(embedded UI)* | Serve the phone UI from a directory instead (development) |

## Security model

Anyone on your network could otherwise control your computer, so **every
connection must present the session token** embedded in the QR/URL (persisted
in `~/.pointflow/token`; `--qr` reprints it). The `--tunnel` URL is public —
treat it like a password. No data leaves your machine otherwise.

## Project

- [Roadmap](docs/ROADMAP.md) — Windows parity, the agent dashboard, packaging
- [Distribution runbook](docs/DISTRIBUTION.md) — how releases ship
- [Terminal streaming design](docs/TERMINAL_STREAMING.md)
- License: [MIT](LICENSE)

Built as two pieces: `phone/` (Next.js static PWA) and `agent/` (Rust: axum +
enigo + portable-pty). The release binary embeds the UI — one file, ~4 MB.
