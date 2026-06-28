# PointFlow

Use your phone as a **trackpad and keyboard for your Mac** — swipe to move the
cursor, tap to click, and type (or dictate) text that lands in whatever app has
focus. Like Whispr Flow's "insert text anywhere," but the input is your thumb
(and your phone keyboard's voice/swipe/emoji) instead of your voice.

No app install on the phone, no cloud — the Mac agent serves the phone UI over
your WiFi and injects real input locally.

```
            ┌─────────────────────────────────────┐
[ Phone ] ──┤  Mac agent (Rust)                    │
 browser    │   • serves the phone UI over WiFi    │──► moves cursor,
  trackpad  │   • WebSocket for input              │     clicks, types
  + keyboard│   • token-paired (QR)                │     into ANY app
            └─────────────────────────────────────┘
```

## Layout

```
phone/   Next.js static UI (the trackpad/keyboard the phone loads)
agent/   Rust desktop agent (serves the UI + injects mouse/keyboard via enigo)
```

## Run it (macOS)

```bash
# 1. Build the phone UI (static export → phone/out)
cd phone && pnpm install && pnpm build && cd ..

# 2. Start the agent
cd agent && cargo run
```

The agent prints a QR code and a URL like `http://192.168.1.4:8742/?token=…`.
Scan it (or open it) on a phone that's **on the same WiFi**. You'll see the
trackpad; start swiping.

### macOS Accessibility permission

Injecting input requires permission. The first time, grant it to the app that
launched the agent (e.g. Terminal/iTerm during development):

**System Settings → Privacy & Security → Accessibility** → enable your terminal,
then restart the agent. If the engine can't initialize, the agent prints a
reminder.

## Gestures

| Gesture | Action |
| --- | --- |
| Swipe | Move cursor |
| Tap | Left click |
| Two-finger swipe | Scroll |
| Hold then drag | Click-and-drag |
| Text box | Type / dictate → injected at the Mac's focus |
| ⌫ ⏎ Tab Esc | Special keys |

## Config

| Env var | Default | Purpose |
| --- | --- | --- |
| `POINTFLOW_PORT` | `8742` | Listen port |
| `POINTFLOW_WEB_DIR` | auto | Path to the built phone UI (`phone/out`) |

## Security

Anyone on your WiFi could otherwise control your Mac, so every connection must
present the session token embedded in the QR/URL. The token is regenerated each
time the agent starts.

## Status

v0 — touchpad pointing, click/scroll/drag, and text injection. macOS only.
Roadmap: packaged menu-bar app, keyboard modifiers/shortcuts, multi-monitor
tuning, optional gyro pointing.
