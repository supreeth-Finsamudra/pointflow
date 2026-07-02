//! Bridges the phone to the user's tmux panes.
//!
//! tmux is the one place macOS lets us cleanly read *and* drive already-running
//! shells: `list-panes` enumerates every pane, `capture-pane` dumps the full
//! colored scrollback, `pipe-pane` streams live output, and `send-keys` types
//! into any pane (focused or not) — no Screen Recording or Accessibility needed.
//!
//! One pane is "selected" at a time. Selecting it broadcasts a scrollback
//! snapshot, then streams the pane's live output (via `pipe-pane` → a temp file
//! we `tail`). Keystrokes go back through `send-keys -H` (raw hex bytes), so any
//! key — text, arrows, Ctrl-C — round-trips exactly.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

/// Buffered output messages before a slow phone is dropped to the latest.
const BROADCAST_CAP: usize = 256;

#[derive(Serialize, Clone)]
pub struct PaneInfo {
    /// tmux pane id, e.g. "%3".
    pub id: String,
    /// Human label: "session:window name".
    pub label: String,
    /// Foreground command, e.g. "claude", "zsh".
    pub cmd: String,
    pub active: bool,
    pub w: u32,
    pub h: u32,
    /// Copilot status from Claude Code hooks: "waiting" | "done" | absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

struct Active {
    pane: Option<String>,
    tail: Option<Child>,
    file: Option<PathBuf>,
}

pub struct Tmux {
    out_tx: broadcast::Sender<Vec<u8>>,
    active: Mutex<Active>,
    /// Copilot status per pane id, driven by Claude Code hook events.
    statuses: Mutex<HashMap<String, String>>,
}

impl Tmux {
    pub fn new() -> Arc<Tmux> {
        let (out_tx, _) = broadcast::channel(BROADCAST_CAP);
        Arc::new(Tmux {
            out_tx,
            active: Mutex::new(Active {
                pane: None,
                tail: None,
                file: None,
            }),
            statuses: Mutex::new(HashMap::new()),
        })
    }

    /// Subscribe to the selected pane's output (snapshot first, then live).
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.out_tx.subscribe()
    }

    /// `{"t":"panes","panes":[…]}` for the phone's picker.
    pub fn panes_json(&self) -> String {
        #[derive(Serialize)]
        struct Msg {
            t: &'static str,
            panes: Vec<PaneInfo>,
        }
        serde_json::to_string(&Msg {
            t: "panes",
            panes: self.list_panes(),
        })
        .unwrap_or_else(|_| "{\"t\":\"panes\",\"panes\":[]}".to_string())
    }

    fn list_panes(&self) -> Vec<PaneInfo> {
        let fmt = "#{pane_id}\t#{session_name}\t#{window_index}\t#{window_name}\t#{pane_current_command}\t#{pane_active}\t#{pane_width}\t#{pane_height}";
        let out = match Command::new(tmux_bin())
            .args(["list-panes", "-a", "-F", fmt])
            .output()
        {
            Ok(o) if o.status.success() => o.stdout,
            _ => return Vec::new(), // no server / tmux not running
        };
        let statuses = self.statuses.lock().unwrap();
        String::from_utf8_lossy(&out)
            .lines()
            .filter_map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() < 8 {
                    return None;
                }
                Some(PaneInfo {
                    id: f[0].to_string(),
                    label: format!("{}:{} {}", f[1], f[2], f[3]),
                    cmd: f[4].to_string(),
                    active: f[5] == "1",
                    w: f[6].parse().unwrap_or(80),
                    h: f[7].parse().unwrap_or(24),
                    status: statuses.get(f[0]).cloned(),
                })
            })
            .collect()
    }

    /// Human label for a pane id, if it still exists.
    pub fn pane_label(&self, id: &str) -> Option<String> {
        self.list_panes()
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.label)
    }

    /// Record a Copilot status ("waiting"/"done") for a pane.
    pub fn set_status(&self, pane: &str, status: &str) {
        self.statuses
            .lock()
            .unwrap()
            .insert(pane.to_string(), status.to_string());
    }

    /// Select a pane: tear down any previous stream, push a scrollback snapshot,
    /// then start streaming live output.
    pub fn select(&self, pane: &str) {
        let mut active = self.active.lock().unwrap();
        stop_stream(&mut active);
        active.pane = Some(pane.to_string());

        // Full scrollback + current screen, with colors.
        if let Ok(o) = Command::new(tmux_bin())
            .args(["capture-pane", "-t", pane, "-p", "-e", "-S", "-"])
            .output()
        {
            if o.status.success() && !o.stdout.is_empty() {
                let _ = self.out_tx.send(o.stdout);
            }
        }

        // Live: pipe the pane's output to a temp file, then tail it.
        let file = std::env::temp_dir().join(format!("pointflow-tmux-{}.log", sanitize(pane)));
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::write(&file, b"");
        let _ = Command::new(tmux_bin())
            .args([
                "pipe-pane",
                "-O",
                "-t",
                pane,
                &format!("cat >> '{}'", file.display()),
            ])
            .status();

        if let Ok(mut child) = Command::new("/usr/bin/tail")
            .args(["-c", "+1", "-F", &file.to_string_lossy()])
            .stdout(Stdio::piped())
            .spawn()
        {
            if let Some(mut stdout) = child.stdout.take() {
                let tx = self.out_tx.clone();
                std::thread::spawn(move || {
                    let mut buf = [0u8; 8192];
                    while let Ok(n) = stdout.read(&mut buf) {
                        if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                });
            }
            active.tail = Some(child);
            active.file = Some(file);
        }
    }

    /// Send raw key bytes to the selected pane (verbatim, via hex).
    pub fn send_keys(&self, bytes: &[u8]) {
        let pane = self.active.lock().unwrap().pane.clone();
        let Some(pane) = pane else { return };
        self.send_keys_to(&pane, bytes);
    }

    /// Send raw key bytes to a specific pane (used by notification cards).
    /// Responding to a pane means the user has handled it — clear its badge.
    pub fn send_keys_to(&self, pane: &str, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.statuses.lock().unwrap().remove(pane);
        let mut args: Vec<String> =
            vec!["send-keys".into(), "-t".into(), pane.to_string(), "-H".into()];
        for b in bytes {
            args.push(format!("{b:02x}"));
        }
        let _ = Command::new(tmux_bin()).args(&args).status();
    }

    /// Stop streaming (e.g. on disconnect): toggle pipe-pane off, kill the tail.
    pub fn stop(&self) {
        let mut active = self.active.lock().unwrap();
        stop_stream(&mut active);
        active.pane = None;
    }
}

fn stop_stream(active: &mut Active) {
    if let Some(pane) = active.pane.clone() {
        let _ = Command::new(tmux_bin())
            .args(["pipe-pane", "-t", &pane]) // no command toggles piping off
            .status();
    }
    if let Some(mut child) = active.tail.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(file) = active.file.take() {
        let _ = std::fs::remove_file(file);
    }
}

/// tmux may not be on the agent's PATH (e.g. launched with a minimal env), so
/// look in the usual Homebrew/local spots before falling back to PATH.
fn tmux_bin() -> &'static str {
    for p in ["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux"] {
        if Path::new(p).exists() {
            return p;
        }
    }
    "tmux"
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}
