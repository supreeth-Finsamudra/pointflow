# Copilot mode — Claude Code notifications on your phone

Kick off Claude Code on the Mac and walk away: your phone shows a card the
moment Claude **needs your permission/input** or **finishes**, and you can
**Approve / Deny / open the shell** with one tap — from anywhere in the app,
without watching the terminal.

## How it works

```
Claude Code ──hooks──► POST /event (Bearer token) ──► agent ──WS──► phone card
     ▲                                                                │
     └───────────── tmux send-keys ◄──── tap Approve/Deny ────────────┘
```

- **Detection** — official Claude Code hooks (no scraping):
  - `Notification` → fires when Claude needs permission or input.
  - `Stop` → fires when Claude finishes responding.
  Each hook `curl`s the agent's `/event` endpoint, forwarding the hook's JSON
  (stdin) plus the tmux pane it ran in (`$TMUX_PANE`).
- **Auth** — `/event` requires the same session token as the WebSocket
  (`Authorization: Bearer …`, read from `~/.pointflow/token`), so nothing else
  on the network can spoof notifications. Wrong token → 401.
- **Relay** — the agent broadcasts `{"t":"event",kind,pane,label,message}` to
  connected phones and records a per-pane status (`waiting` / `done`).
- **Action** — the card's Approve sends **Enter** (accepts Claude's default
  option) and Deny sends **Esc**, delivered by `tmux send-keys` to that exact
  pane — it doesn't need to be focused on the Mac or open on the phone.
  "Open shell" jumps straight into the pane view for context. Responding
  clears the pane's badge.

## Install (one-time)

```bash
cd agent && cargo run -- --install-hooks
```

Merges the two hooks into `~/.claude/settings.json` **non-destructively**
(backup written to `settings.json.bak-pointflow`; idempotent — safe to re-run).
Claude Code sessions started *after* the install will fire the hooks. Remove by
deleting the two entries containing `.pointflow/token`, or restore the backup.

## What you see on the phone

- A **card** slides in: *"Claude needs your permission to use Bash"* with
  **Approve ⏎ / Deny Esc / Open shell**. Finished tasks show *"Claude
  finished"* with **Open shell**.
- The `>_` button gets an **amber dot** while a card is pending.
- The shell picker shows per-pane badges: **⏸ needs you** / **✓ done**.
- Android phones vibrate on arrival (iOS Safari has no vibration API; the card
  itself is the signal there).

## Lock-screen push (PWA)

Real notifications with the app closed and the phone locked:

1. Start the agent with the tunnel: `cargo run -- --tunnel` (push needs HTTPS).
2. Open the **✦ public URL** on the phone → Share → **Add to Home Screen**
   (iOS requires an installed PWA for push; the app icon opens
   pre-authenticated — the manifest bakes the session token into `start_url`).
3. Open PointFlow from the icon → tap **🔕** in the status bar → allow.

From then on every hook event is *also* delivered as an encrypted Web Push:
the agent keeps VAPID keys in `~/.pointflow/vapid.pem`, device subscriptions
in `~/.pointflow/push.json` (expired ones are pruned automatically), and
`/push/key` + `/push/subscribe` are token-gated. Tapping the notification
opens/focuses the app, which restores the shell you were in.

## Caveats (v1)

- Without the PWA/push setup above, cards arrive only while the page is open
  (LAN http:// has no secure context, so 🔔 shows the install hint instead).
- Approve sends Enter = Claude's default choice ("Yes"). For the other options
  ("don't ask again", numbered choices), tap **Open shell** and answer there.
- Events outside tmux still show a card (no pane → no Approve/Open buttons).

## Remote access (outside your WiFi)

The agent binds `0.0.0.0`, so any routable path to the Mac works:

1. **Tailscale (recommended)** — install on Mac + phone, then open
   `http://<mac-tailscale-ip>:8742/?token=…`. Works from anywhere, WireGuard-
   encrypted, no code changes, free for personal use.
2. **Cloudflare Tunnel / ngrok** — gives a public **HTTPS** URL
   (`cloudflared tunnel --url http://localhost:8742`). The phone client
   auto-switches to `wss://` on HTTPS pages, so it works as-is — and HTTPS is
   what later unlocks real Web Push. Caution: the URL is public; the session
   token is the only gate, so treat the link like a password.
3. **Router port-forwarding** — don't; plain HTTP on the open internet.
