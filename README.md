# PointFlow

**Your phone is now your computer's trackpad, keyboard, terminal — and the
remote control for your AI coding agents. From anywhere on Earth.**

Kick off a Claude Code task, leave the house. Your phone buzzes on the lock
screen: *"✳ Claude needs your permission."* You tap **Approve ⏎** from the
café, watch the diff stream by in a real terminal, and prompt the next task —
over cellular, with no VPN, no phone app, no cloud account. One ~4 MB binary
on your computer and a QR code is the entire setup.

On your WiFi it's also the best trackpad+keyboard your computer never had:
swipe to move the cursor, dictate into whatever app has focus, drive your
shells full-color from the couch.

## The 60-second anywhere demo

```bash
brew install supreeth-Finsamudra/pointflow/pointflow cloudflared
pointflow-agent --tunnel
```

1. Scan the **✦ public QR** it prints (give it ~30 s to go live)
2. On the phone: Share → **Add to Home Screen** → open from the icon
3. Tap 🔔 to enable lock-screen notifications
4. Walk out the door. Your computer — and your Claude Code sessions — are in
   your pocket.

*Straight talk on security: the tunnel URL is public, so treat it like a
password — every connection still requires the session token baked into the
QR, and `pointflow-agent --qr` reprints/rotates your pairing. Skip `--tunnel`
and nothing ever leaves your LAN.*

```
            ┌───────────────────────────────────────┐
[ Phone ] ──┤  Agent (Rust, single ~4 MB binary)     │──► moves cursor, clicks,
 browser    │   • serves the phone UI (embedded)     │    types into ANY app
  trackpad  │   • WebSocket for input + terminals    │──► streams your shells
  keyboard  │   • token-paired (QR), LAN or tunnel   │◄── Claude Code hooks:
  terminal  └───────────────────────────────────────┘    "needs you" → phone
```

## Why people use it

- 🌍 **Works from anywhere** — `--tunnel` prints a public HTTPS QR (Cloudflare
  quick tunnel): drive your desktop from cellular, a café, another country.
  No VPN, no port forwarding, no phone app.
- ✳️ **Claude Code copilot mode** — hook events surface as cards and
  lock-screen push notifications: *"Claude needs your permission"* →
  **Approve ⏎ / Deny Esc** from your phone, without the Mac unlocked or the
  window focused. Prompt it, watch the diff stream by, approve, repeat.
- 🖥 **Real terminals on your phone** — full-color xterm.js views of your
  shells with scrollback, quick keys (Esc·Tab·⏎·arrows·⌃C), a compose box
  built for prompts, even photo-upload-to-file-path for multimodal prompts.
- 🖱 **A genuinely good trackpad** — sub-pixel pointer with acceleration,
  momentum scrolling, a full gesture vocabulary (tap, drag, two-finger scroll,
  three-finger Mission Control), tunable to your thumb.
- ⌨️ **Type anywhere** — your phone keyboard (including voice dictation,
  swipe, emoji) lands in whatever has focus on the desktop. Like Whispr
  Flow's "insert text anywhere," but the input device is your phone.
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
