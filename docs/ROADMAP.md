# PointFlow Roadmap

The thesis: PointFlow started as "phone = trackpad for your Mac" and is becoming
**"mission control for your machine and its AI coding agents, from any phone."**
Everything below serves two goals: work on more machines (Windows first), and be
installable/measurable enough that many devs actually adopt it.

The phone side is already universal — it's a browser PWA (Android Chrome, iOS
Safari 16.4+ for push). The portability work is all in the agent.

---

## Phase 1 — Windows compatibility (in progress, `feat/windows-compat`)

Make the agent compile and genuinely work on Windows. No mocks: every feature is
either implemented natively or explicitly disabled with a printed reason and a
roadmap line here.

| Area | macOS today | Windows plan | Status |
| --- | --- | --- | --- |
| Trackpad / click / type / chords | enigo | enigo (already cross-platform) | ✅ works via cfg-clean build |
| Pixel scrolling | CGEvent pixel units | `SendInput` wheel deltas (fractional-notch; smooth in modern apps) | ✅ implemented |
| Keep-awake | `/usr/bin/caffeinate` | `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED)` | ✅ implemented |
| Terminal streaming | tmux bridge (`tmux.rs`) | **Owned ConPTY shells** (`shells.rs` over `term.rs` via portable-pty): `+ New` spawns pwsh/powershell/cmd; list/select/type/approve use the same wire protocol, phone unchanged | ✅ implemented |
| Terminal.app tabs bridge | AppleScript (`tabs.rs`) | No Windows analog (Windows Terminal has no read-buffer API) — hidden on Windows | N/A by OS design |
| Claude Code hooks | sh command w/ `$TMUX_PANE` | Shell-agnostic curl command (token embedded at install; no sh-isms) | ✅ implemented |
| Web Push (lock screen) | web-push crate (ece→openssl) | Disabled stub for now (openssl unbuildable in std Windows CI); see Phase 4 | ⏸ stubbed w/ notice |
| VAPID keygen | shell-out to `/usr/bin/openssl` | Pure-Rust `p256` SEC1 PEM on all non-Windows (removes runtime openssl everywhere) | ✅ implemented |
| Paths (`~/.pointflow`, uploads) | `$HOME` | `home_dir()` = HOME → USERPROFILE fallback | ✅ implemented |
| `--tunnel` | cloudflared via brew paths | cloudflared from PATH (`winget install Cloudflare.cloudflared`) | ✅ implemented |

**Verification gates for this phase**
- [ ] `cargo build` still green on macOS (no behavior change)
- [ ] `cargo check --target x86_64-pc-windows-msvc` green
- [ ] Hardware validation pass on a real Windows machine: scroll direction/feel,
      ConPTY shells with Claude Code, PWA from Android + iPhone
- [ ] Three-finger gestures: currently macOS Mission Control mappings; map to
      Win+Tab / virtual-desktop switch on Windows (follow-up in this phase)

Known platform truth: like macOS, Windows offers **no API to read an arbitrary
terminal's buffer**, and there is no scriptable Terminal.app equivalent — so on
Windows, phone-visible shells are the ones PointFlow spawns (`+ New`). That is a
physics limit, not a missing feature.

## Phase 2 — Source-agnostic agent dashboard

The `>_` sheet becomes a mission-control list of every shell/agent with live
`waiting / running / done` badges and inline Approve/Deny (scoped in detail in
the PR discussion; ~60 lines, no new wire messages):

- Rebroadcast `panes` on every Copilot hook event (live badges).
- `clear_status` + rebroadcast on `tkeys` (badge clears on tap).
- Waiting-first sort, count header, inline ⏎/Esc buttons on waiting rows.
- Non-tmux Claude sessions: map hook events to Terminal tabs by `cwd` so the
  dashboard is wrapper-free on macOS.
- Later: iTerm2 adapter (it has a real read API), badge on the `>_` button.

## Phase 3 — Distribution & growth (see docs/DISTRIBUTION.md for the runbook)

- ✅ **Single-file binary**: `phone/out` embedded via `rust-embed`; verified
  serving with no files on disk (~4 MB binary).
- ✅ **Release pipeline**: `dist` (cargo-dist 0.32) → tag push builds
  mac arm64/x64 + Windows x64, publishes GitHub Release with sh/ps1
  installers + checksums, pushes the Homebrew formula to the tap.
- ⏳ **Tap repo + token**: owner-only one-time setup (runbook §one-time).
- ⏳ **winget**: submit after the first release exists (needs artifact hashes).
- ⏸ **Signing (last)**: Apple Developer account already in hand — wire
  Developer ID + notarization into dist config after Windows validation;
  Azure Trusted Signing for Windows when warranted.
- Usage metric: GitHub Release download counts (telemetry deliberately not
  built — decision 2026-07).
- Later: macOS menu-bar app wrapper so "install → running" is one step.

## Phase 4 — Platform completeness

- **Push on Windows**: replace `web-push`(ece/openssl) with a RustCrypto-based
  web-push implementation so lock-screen notifications work on all agent OSes.
- **Linux**: enigo needs libxdo/X11 or libei (Wayland) — document, add keep-awake
  (`systemd-inhibit`), CI build. tmux bridge already works there.
- **Owned shells on macOS too**: `+ New` without tmux installed falls back to
  ConPTY-style owned PTY (same `shells.rs`), removing the tmux install
  requirement for first-run.
- Gesture parity: three-finger swipe → Task View / virtual desktops (Windows),
  Overview (GNOME).

---

*Sequencing rationale: Windows compat (P1) multiplies the audience; the
dashboard (P2) is the shareable "aha"; distribution (P3) converts attention
into installs; P4 rounds out the promise. Ship P1 → P3 before broad promotion.*
