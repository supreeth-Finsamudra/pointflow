//! Bridges the phone to the user's tmux panes.
//!
//! tmux is the one place macOS lets us cleanly read *and* drive already-running
//! shells. Viewing works by **attaching a real tmux client on a PTY**: on
//! attach, tmux repaints the entire screen (correct state, cursor, colors) and
//! keeps it in sync — including full-screen TUIs like Claude Code. Before
//! attaching we replay the pane's *history* (`capture-pane` up to the visible
//! screen) so the phone can scroll back through everything.
//!
//! Typing goes through the attached client's PTY. `send-keys -H` is kept for
//! the Copilot cards, which poke a pane without attaching to it.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
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
    /// Basename of the pane's working directory ("point-flow").
    pub cwd: String,
    /// Copilot status from Claude Code hooks: "waiting" | "done" | absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// The live attachment: a tmux client running inside a PTY we own.
struct Attach {
    pane: String,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

pub struct Tmux {
    out_tx: broadcast::Sender<Vec<u8>>,
    attach: Mutex<Option<Attach>>,
    /// Copilot status per pane id, driven by Claude Code hook events.
    statuses: Mutex<HashMap<String, String>>,
}

impl Tmux {
    pub fn new() -> Arc<Tmux> {
        let (out_tx, _) = broadcast::channel(BROADCAST_CAP);
        Arc::new(Tmux {
            out_tx,
            attach: Mutex::new(None),
            statuses: Mutex::new(HashMap::new()),
        })
    }

    /// Subscribe to the attached pane's output (history replay, repaint, live).
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
        let fmt = "#{pane_id}\t#{session_name}\t#{window_index}\t#{window_name}\t#{pane_current_command}\t#{pane_active}\t#{pane_width}\t#{pane_height}\t#{pane_current_path}";
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
                let path = f.get(8).copied().unwrap_or("");
                Some(PaneInfo {
                    id: f[0].to_string(),
                    label: format!("{}:{} {}", f[1], f[2], f[3]),
                    cmd: f[4].to_string(),
                    active: f[5] == "1",
                    w: f[6].parse().unwrap_or(80),
                    h: f[7].parse().unwrap_or(24),
                    cwd: path.rsplit('/').next().unwrap_or(path).to_string(),
                    status: statuses.get(f[0]).cloned(),
                })
            })
            .collect()
    }

    /// Create a fresh shell in a new tmux session (starting the tmux server if
    /// needed) and return its `(pane_id, label)`. Sessions are created in the
    /// user's home directory.
    pub fn create_session(&self) -> Option<(String, String)> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let out = Command::new(tmux_bin())
            .args([
                "new-session",
                "-d",
                "-P",
                "-F",
                "#{pane_id}\t#{session_name}:#{window_index} #{window_name}",
                "-c",
                &home,
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let line = String::from_utf8_lossy(&out.stdout);
        let mut f = line.trim().split('\t');
        Some((f.next()?.to_string(), f.next().unwrap_or("shell").to_string()))
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

    /// View a pane at the phone's size: replay its history for scrollback,
    /// make it the session's active pane, then attach a tmux client on a PTY —
    /// tmux repaints the full screen into it and streams every update.
    pub fn select(&self, pane: &str, cols: u16, rows: u16) {
        let mut slot = self.attach.lock().unwrap();
        stop_attach(&mut slot);

        // History only (everything *above* the visible screen), so the phone
        // can scroll back; the attach below repaints the visible screen itself.
        if let Ok(o) = Command::new(tmux_bin())
            .args(["capture-pane", "-t", pane, "-p", "-e", "-S", "-", "-E", "-1"])
            .output()
        {
            if o.status.success() && !o.stdout.is_empty() {
                let mut history = Vec::with_capacity(o.stdout.len() + 8);
                // capture-pane emits \n line endings; a fresh xterm needs \r\n.
                for line in o.stdout.split(|&b| b == b'\n') {
                    history.extend_from_slice(line);
                    history.extend_from_slice(b"\r\n");
                }
                let _ = self.out_tx.send(history);
            }
        }

        // Focus the pane so the attached client shows it.
        let _ = Command::new(tmux_bin())
            .args(["select-window", "-t", pane])
            .status();
        let _ = Command::new(tmux_bin())
            .args(["select-pane", "-t", pane])
            .status();

        // Attach as a real client inside a PTY sized to the phone.
        let pty = match native_pty_system().openpty(PtySize {
            rows: rows.max(4),
            cols: cols.max(20),
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[pointflow] tmux attach: openpty failed: {e}");
                return;
            }
        };
        let mut cmd = CommandBuilder::new(tmux_bin());
        cmd.args(["attach-session", "-t", pane]);
        cmd.env("TERM", "xterm-256color");
        // A leftover $TMUX would make tmux refuse ("sessions should be nested
        // with care"); we are not inside tmux, but be explicit.
        cmd.env_remove("TMUX");

        let child = match pty.slave.spawn_command(cmd) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[pointflow] tmux attach failed: {e}");
                return;
            }
        };
        drop(pty.slave);

        let reader = match pty.master.try_clone_reader() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[pointflow] tmux attach: reader failed: {e}");
                return;
            }
        };
        let writer = match pty.master.take_writer() {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[pointflow] tmux attach: writer failed: {e}");
                return;
            }
        };

        // Pump the attached client's screen to the phones.
        {
            let tx = self.out_tx.clone();
            let mut reader = reader;
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = reader.read(&mut buf) {
                    if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            });
        }

        println!("[pointflow] attached to pane {pane} at {cols}x{rows}");
        *slot = Some(Attach {
            pane: pane.to_string(),
            master: pty.master,
            writer,
            child,
        });
    }

    /// Raw keystrokes from the phone → the attached tmux client's PTY.
    pub fn write_input(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut slot = self.attach.lock().unwrap();
        if let Some(a) = slot.as_mut() {
            let _ = a.writer.write_all(bytes);
            let _ = a.writer.flush();
            self.statuses.lock().unwrap().remove(&a.pane);
        }
    }

    /// Resize the attached client to the phone's viewport; tmux reflows
    /// (window-size=latest means the most recent client wins).
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols < 20 || rows < 4 {
            return;
        }
        let slot = self.attach.lock().unwrap();
        if let Some(a) = slot.as_ref() {
            let _ = a.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    /// Send raw key bytes to a specific pane *without* attaching (Copilot
    /// cards). Responding to a pane clears its badge.
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

    /// Detach (e.g. phone closed the pane view or disconnected).
    pub fn stop(&self) {
        let mut slot = self.attach.lock().unwrap();
        stop_attach(&mut slot);
    }
}

fn stop_attach(slot: &mut Option<Attach>) {
    if let Some(mut a) = slot.take() {
        // Killing the client detaches it; the session (and Claude Code in it)
        // keeps running untouched.
        let _ = a.child.kill();
        let _ = a.child.wait();
        println!("[pointflow] detached from pane {}", a.pane);
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
