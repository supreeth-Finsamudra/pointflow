//! A PointFlow-owned PTY session (ConPTY on Windows, forkpty on unix).
//!
//! Spawns a shell, fans its output out to every connected phone — live plus a
//! scrollback replay on attach — and accepts keystrokes and resizes back.
//! This is deliberately separate from the input-injection path in `input.rs`:
//! the trackpad and keyboard still drive the *computer's* focused app; the
//! terminal drives *this* shell. Nothing here touches `enigo`.
//!
//! On Windows this is the `+ New` terminal backend (`shells.rs` manages
//! several of these) — full color and TUIs, since PointFlow owns the ConPTY.
//! Already-running consoles are bridged separately by `wintabs.rs`.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use tokio::sync::broadcast;

use crate::util::home_dir;

/// Max bytes of scrollback replayed to a phone when it (re)attaches.
const SCROLLBACK_CAP: usize = 256 * 1024;
/// How many output chunks the broadcast buffers before a slow phone lags.
const BROADCAST_CAP: usize = 1024;

/// Handle to the live shell. Cheap to clone (it's all behind the `Arc`).
pub struct Term {
    /// Basename of the spawned shell ("zsh", "pwsh", ...), for labels.
    pub program: String,
    /// PTY master writer — keystrokes go here.
    writer: Mutex<Box<dyn Write + Send>>,
    /// PTY master — kept for `resize()`.
    master: Mutex<Box<dyn MasterPty + Send>>,
    /// Shared scrollback, locked together with broadcast sends so attach is
    /// race-free (no dropped or duplicated boundary bytes).
    inner: Mutex<Shared>,
    /// Live output fan-out; each phone subscribes.
    output_tx: broadcast::Sender<Vec<u8>>,
    /// False once the shell exits (reader hit EOF).
    alive: AtomicBool,
}

struct Shared {
    scrollback: VecDeque<u8>,
}

/// The shell to spawn and its args, per platform.
#[cfg(unix)]
fn default_shell() -> (String, Vec<String>) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    // Login shell, so the user's PATH/aliases are present.
    (shell, vec!["-l".to_string()])
}

#[cfg(windows)]
fn default_shell() -> (String, Vec<String>) {
    fn on_path(name: &str) -> bool {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join(name).exists()))
            .unwrap_or(false)
    }
    // Prefer PowerShell 7+, then Windows PowerShell, then cmd.
    for sh in ["pwsh.exe", "powershell.exe"] {
        if on_path(sh) {
            return (sh.to_string(), vec!["-NoLogo".to_string()]);
        }
    }
    (
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
        Vec::new(),
    )
}

impl Term {
    /// Spawn the platform shell on a PTY and start pumping its output.
    pub fn spawn() -> std::io::Result<Arc<Term>> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(to_io)?;

        let (shell, args) = default_shell();
        let mut cmd = CommandBuilder::new(&shell);
        for a in &args {
            cmd.arg(a);
        }
        #[cfg(unix)]
        cmd.env("TERM", "xterm-256color");
        if let Some(home) = home_dir() {
            cmd.cwd(home);
        }

        // The child runs on the slave side; we keep its handle alive in the
        // reader thread (dropping it early could orphan/reap the shell).
        let child = pair.slave.spawn_command(cmd).map_err(to_io)?;
        drop(pair.slave); // we never read/write the slave directly

        let reader = pair.master.try_clone_reader().map_err(to_io)?;
        let writer = pair.master.take_writer().map_err(to_io)?;
        let (output_tx, _) = broadcast::channel::<Vec<u8>>(BROADCAST_CAP);

        // "C:\...\pwsh.exe" / "/bin/zsh" → "pwsh" / "zsh".
        let program = std::path::Path::new(&shell)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| shell.clone());

        let term = Arc::new(Term {
            program,
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            inner: Mutex::new(Shared {
                scrollback: VecDeque::new(),
            }),
            output_tx,
            alive: AtomicBool::new(true),
        });

        // Reader thread: blocking PTY reads -> scrollback + live broadcast.
        // Holds `child` so the shell lives as long as we're reading it.
        {
            let term = term.clone();
            let mut reader = reader;
            let _child = child;
            std::thread::spawn(move || {
                let _keep_child = _child;
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => {
                            // Shell exited or PTY closed.
                            term.alive.store(false, Ordering::Relaxed);
                            break;
                        }
                        Ok(n) => {
                            let chunk = buf[..n].to_vec();
                            // Append to scrollback and broadcast under one lock
                            // so a concurrently-attaching phone sees a
                            // consistent snapshot+stream boundary.
                            let mut inner = term.inner.lock().unwrap();
                            inner.scrollback.extend(chunk.iter().copied());
                            let over = inner.scrollback.len().saturating_sub(SCROLLBACK_CAP);
                            if over > 0 {
                                inner.scrollback.drain(..over);
                            }
                            let _ = term.output_tx.send(chunk);
                        }
                    }
                }
            });
        }

        Ok(term)
    }

    /// Write raw keystroke bytes to the shell.
    pub fn write_input(&self, bytes: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    /// Resize the PTY to the phone's current viewport (triggers SIGWINCH).
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        if let Ok(master) = self.master.lock() {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    /// Subscribe to live output, atomically capturing the current scrollback.
    /// Returns `(snapshot, live)` — send the snapshot first, then stream `live`.
    pub fn subscribe(&self) -> (Vec<u8>, broadcast::Receiver<Vec<u8>>) {
        let inner = self.inner.lock().unwrap();
        let rx = self.output_tx.subscribe();
        let snapshot: Vec<u8> = inner.scrollback.iter().copied().collect();
        (snapshot, rx)
    }

    /// Whether the shell process is still running.
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

fn to_io<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
}
