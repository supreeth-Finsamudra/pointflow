//! Bridges the phone to the Mac's *already-open* Terminal.app tabs.
//!
//! Unlike the tmux bridge (which needs shells started inside tmux), this reads
//! what's on screen right now in every Terminal window/tab via Apple Events:
//! the tab list, each tab's tty (paired with `ps` to know what's running —
//! e.g. Claude Code), its visible screen, and its full scrollback history.
//! Plain text, no colors, polled ~1.5×/sec — the tradeoff for zero setup.
//!
//! Typing into a tab = `focus()` (select the tab + bring Terminal frontmost)
//! plus PointFlow's existing keystroke injection, which types into whatever
//! has focus on the Mac.

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;

/// How often the selected tab's screen is re-read.
const POLL: Duration = Duration::from_millis(700);
/// Idle poll when no tab is selected.
const IDLE: Duration = Duration::from_millis(200);
/// Cap on replayed scrollback history.
const HISTORY_CAP: usize = 200 * 1024;

#[derive(Serialize, Clone)]
pub struct TabInfo {
    /// Terminal.app window id (stable per window).
    pub win: i64,
    /// 1-based tab index within the window.
    pub tab: i64,
    pub tty: String,
    pub busy: bool,
    /// Non-shell commands running on the tab's tty, comma-joined.
    pub procs: String,
    /// True if Claude Code is running in this tab.
    pub claude: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct Sel {
    win: i64,
    tab: i64,
}

pub struct Tabs {
    /// JSON text messages fanned out to phones (shares the events channel).
    out_tx: broadcast::Sender<String>,
    sel: Mutex<Option<Sel>>,
}

impl Tabs {
    /// Start the polling thread. `out_tx` is the agent's phone-facing JSON
    /// broadcast (same one Copilot events use).
    pub fn start(out_tx: broadcast::Sender<String>) -> Arc<Tabs> {
        let tabs = Arc::new(Tabs {
            out_tx,
            sel: Mutex::new(None),
        });
        {
            let tabs = tabs.clone();
            std::thread::spawn(move || tabs.poll_loop());
        }
        tabs
    }

    /// `{"t":"tabs","tabs":[…]}` for the phone's picker.
    pub fn tabs_json(&self) -> String {
        #[derive(Serialize)]
        struct Msg {
            t: &'static str,
            tabs: Vec<TabInfo>,
        }
        serde_json::to_string(&Msg {
            t: "tabs",
            tabs: list_tabs(),
        })
        .unwrap_or_else(|_| "{\"t\":\"tabs\",\"tabs\":[]}".to_string())
    }

    /// Select a tab: replay its scrollback history once, focus it on the Mac
    /// (so typed keystrokes land in it), then let the poll loop stream the
    /// screen.
    pub fn select(&self, win: i64, tab: i64) {
        if let Some(hist) = read_tab_text(win, tab, "history") {
            let tail = tail_chars(&hist, HISTORY_CAP);
            let _ = self.out_tx.send(json_text("tabhist", tail));
        }
        self.focus(win, tab);
        *self.sel.lock().unwrap() = Some(Sel { win, tab });
    }

    /// Bring the tab frontmost so injected keystrokes reach it.
    pub fn focus(&self, win: i64, tab: i64) {
        let script = format!(
            "tell application \"Terminal\"\n\
             set selected tab of window id {win} to tab {tab} of window id {win}\n\
             set index of window id {win} to 1\n\
             activate\n\
             end tell"
        );
        let _ = Command::new("/usr/bin/osascript").arg("-e").arg(script).output();
    }

    /// Stop streaming (phone closed the tab view).
    pub fn stop(&self) {
        *self.sel.lock().unwrap() = None;
    }

    fn poll_loop(&self) {
        let mut last_sel: Option<Sel> = None;
        let mut last_screen = String::new();
        loop {
            let sel = *self.sel.lock().unwrap();
            let Some(s) = sel else {
                last_sel = None;
                std::thread::sleep(IDLE);
                continue;
            };
            if last_sel != Some(s) {
                last_sel = Some(s);
                last_screen.clear();
            }
            match read_tab_text(s.win, s.tab, "contents") {
                Some(screen) => {
                    if screen != last_screen {
                        let _ = self.out_tx.send(json_text("tabscr", &screen));
                        last_screen = screen;
                    }
                }
                None => {
                    // Tab/window closed — tell the phone and stop.
                    let _ = self.out_tx.send(json_text("tabscr", "(tab closed)"));
                    *self.sel.lock().unwrap() = None;
                }
            }
            std::thread::sleep(POLL);
        }
    }
}

fn json_text(t: &str, text: &str) -> String {
    serde_json::json!({ "t": t, "text": text }).to_string()
}

fn tail_chars(s: &str, cap: usize) -> &str {
    if s.len() <= cap {
        return s;
    }
    // Cut on a char boundary near the cap.
    let start = s.len() - cap;
    let start = (start..s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(0);
    &s[start..]
}

/// Read a tab's `contents` (visible screen) or `history` (full scrollback).
fn read_tab_text(win: i64, tab: i64, what: &str) -> Option<String> {
    let script =
        format!("tell application \"Terminal\" to {what} of tab {tab} of window id {win}");
    let out = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Enumerate every Terminal.app window/tab and what's running in each.
fn list_tabs() -> Vec<TabInfo> {
    let script = "tell application \"Terminal\"\n\
        set out to \"\"\n\
        repeat with w in windows\n\
        set wid to id of w\n\
        set ti to 1\n\
        repeat with t in tabs of w\n\
        set out to out & wid & \"|\" & ti & \"|\" & (tty of t) & \"|\" & (busy of t) & linefeed\n\
        set ti to ti + 1\n\
        end repeat\n\
        end repeat\n\
        out\n\
        end tell";
    let out = match Command::new("/usr/bin/osascript").arg("-e").arg(script).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.trim().split('|').collect();
            if f.len() < 4 {
                return None;
            }
            let tty = f[2].trim_start_matches("/dev/").to_string();
            let procs = tty_procs(&tty);
            let claude = procs.to_lowercase().contains("claude");
            Some(TabInfo {
                win: f[0].parse().ok()?,
                tab: f[1].parse().ok()?,
                tty,
                busy: f[3] == "true",
                procs,
                claude,
            })
        })
        .collect()
}

/// Interesting commands on a tty (skips login/shell plumbing), comma-joined.
fn tty_procs(tty: &str) -> String {
    let out = match Command::new("/bin/ps")
        .args(["-o", "command=", "-t", tty])
        .output()
    {
        Ok(o) => o.stdout,
        Err(_) => return String::new(),
    };
    let mut names: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&out).lines() {
        let cmd = line.trim();
        if cmd.is_empty() || cmd.starts_with("login ") || cmd.starts_with('-') {
            continue;
        }
        // First word, basename only, e.g. "/usr/bin/node" -> "node";
        // keep "claude" as typed.
        let first = cmd.split_whitespace().next().unwrap_or("");
        let base = first.rsplit('/').next().unwrap_or(first);
        if base.is_empty() || base == "zsh" || base == "bash" || base == "sh" {
            continue;
        }
        let label = if base == "claude" || cmd.contains("claude") {
            "claude".to_string()
        } else {
            base.to_string()
        };
        if !names.contains(&label) {
            names.push(label);
        }
    }
    names.join(", ")
}
