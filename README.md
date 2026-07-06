# PointFlow

**Every shell on your computer, live and typeable from your phone. Which means
every AI coding agent — Claude Code, Codex, Aider, anything that runs in a
terminal — now has a remote control in your pocket.**

Your agent is mid-refactor and asks for permission. Your machine is locked,
you're not home. Your phone shows the pane in a **real full-color terminal**
— scrollback, TUIs, the actual diff — you read it, type into the shell, and
the agent keeps moving. Over cellular. No VPN, no phone app, no cloud
account. One ~4 MB Rust binary and a QR code is the entire setup.

```
            ┌───────────────────────────────────────┐
[ Phone ] ──┤  Agent (Rust, single ~4 MB binary)     │──► your tmux panes &
 xterm.js   │   • serves the phone UI (embedded)     │    shells: view, type,
 terminal   │   • WebSocket: keystrokes ⇄ output     │    approve — any of them
 + trackpad │   • token-paired (QR), LAN or tunnel   │◄── Claude Code hooks:
 + keyboard └───────────────────────────────────────┘    "needs you" → phone
```

## The 60-second anywhere demo

```bash
brew install supreeth-Finsamudra/pointflow/pointflow cloudflared
pointflow-agent --tunnel
```

1. Scan the **✦ public QR** it prints (give it ~30 s to go live)
2. On the phone: Share → **Add to Home Screen** → open from the icon
3. Tap `>_` — every tmux pane on your machine is right there. Tap one, type.
4. Walk out the door. Your shells — and whatever agents run in them — come with you.

*Straight talk on security: the tunnel URL is public, so treat it like a
password — every connection still requires the session token baked into the
QR, and `pointflow-agent --qr` reprints/rotates your pairing. Skip `--tunnel`
and nothing ever leaves your LAN.*

## Why people use it

- 🖥 **Your real shells, not a toy** — full-color xterm.js views of your tmux
  panes with complete scrollback, live TUIs, quick keys (Esc·Tab·⏎·arrows·⌃C),
  pinch-free font zoom, and byte-exact keystroke round-tripping. If it runs in
  a terminal, you can watch it and drive it from your phone.
- 🤖 **Agent-agnostic by design** — the phone is just another client of your
  shells. Claude Code, Codex CLI, Aider, custom scripts, long test runs,
  builds: anything that prints and reads stdin is remotely yours. A compose
  box built for prompts (newlines don't submit; dictation and autocorrect
  work) plus photo-upload-to-file-path for multimodal prompts.
- ✳️ **Claude Code gets superpowers** — one command installs official hooks:
  *"Claude needs your permission"* arrives as a card and a **lock-screen push
  notification** → **Approve ⏎ / Deny Esc** without unfocusing a window or
  unlocking the machine. Prompt, watch the diff stream, approve, repeat.
- 🌍 **Works from anywhere** — `--tunnel` prints a public HTTPS QR (Cloudflare
  quick tunnel): cellular, café, another country. No VPN, no port forwarding.
- 🖱 **Trackpad + keyboard included** — when you're on the couch instead of
  across the world: sub-pixel pointer with acceleration and momentum scroll,
  and your phone keyboard (voice dictation included) typing into whatever has
  focus.
- 🔒 **Local-first** — no cloud, no account, no telemetry. Every connection
  needs the session token from the QR. Your keystrokes never leave your
  network unless *you* start the tunnel.

## Install

**macOS (Homebrew):**
```bash
brew install supreeth-Finsamudra/pointflow/pointflow
pointflow-agent
```

**macOS/Linux (one-liner):**
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

Run the agent, scan the QR (same WiFi) or use `--tunnel` for anywhere-access.

### macOS: Accessibility permission
Only the trackpad/keyboard injection needs it (the terminal bridge doesn't):
**System Settings → Privacy & Security → Accessibility** → enable your
terminal, restart the agent.

## The terminal bridge, by platform

| Platform | What you get |
| --- | --- |
| **macOS / Linux** | Your **tmux panes**: list all, pick one, full scrollback with colors, type into unfocused panes (`send-keys` byte-exact), live streaming via a real attached client. macOS additionally bridges **already-open Terminal.app tabs** — zero setup, no tmux needed. `+ New` on the phone spawns a fresh shell. |
| **Windows** *(beta)* | `+ New` spawns PointFlow-owned **ConPTY** shells (pwsh/powershell/cmd) with the same phone UX. (Windows exposes no API to read other terminals' buffers — so shells live inside PointFlow.) |

Why tmux underneath: it's the one clean way to read *and* drive
already-running shells — it owns the text, history, and stdin, so the phone
gets real data instead of screenshots. Full design: [docs/TERMINAL_STREAMING.md](docs/TERMINAL_STREAMING.md).

## Claude Code copilot

```bash
pointflow-agent --install-hooks   # one-time; merges into ~/.claude/settings.json
```

Every Claude Code session then reports to your phone: **"✳ Claude needs you"**
cards with one-tap Approve/Deny (keys are sent straight to that pane — it
doesn't even need to be the one you're viewing), **"✓ finished"** when a task
completes, and lock-screen push when installed as a PWA over the tunnel.
Multiple sessions tracked per pane. Other agents work in the terminal today;
hook-style notifications for them are on the roadmap.

## Also in the box

- **Trackpad**: sub-pixel pointer, acceleration, momentum + edge-strip scroll,
  tap/drag/two-finger/three-finger gestures (Mission Control on macOS), all
  tunable in ⚙ settings, persisted on the phone.
- **Keyboard**: type or dictate into whatever has desktop focus; special keys
  and chords (⌘C-style) included.
- **Photo → path**: send a phone photo; it lands in `~/Downloads/PointFlow`
  and the file path drops into your compose box — multimodal prompts from the
  couch.

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

- [Roadmap](docs/ROADMAP.md) — agent dashboard, Windows parity, packaging
- [Distribution runbook](docs/DISTRIBUTION.md) — how releases ship
- [Terminal streaming design](docs/TERMINAL_STREAMING.md)
- License: [MIT](LICENSE)

Built as two pieces: `phone/` (Next.js static PWA) and `agent/` (Rust: axum +
portable-pty + enigo). The release binary embeds the UI — one file, ~4 MB.
