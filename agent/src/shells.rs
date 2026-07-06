//! PointFlow-owned shells: the terminal backend where tmux doesn't exist
//! (Windows today; a no-tmux fallback elsewhere later — see docs/ROADMAP.md).
//!
//! Manages several `term::Term` PTY sessions ("+ New" on the phone spawns
//! one) behind the same method surface `main.rs` uses on `tmux::Tmux`, so the
//! wire protocol — `tlist`/`tnew`/`tsel`/`tresize`/`tkeys`, binary keystrokes
//! and output — and therefore the whole phone UI work unchanged.
//!
//! One pane is "selected" per agent (last-writer-wins across phones, same as
//! the tmux bridge): its output is fanned out on the shared broadcast, and
//! binary frames from phones are written to it. `send_keys_to` pokes any
//! session without selecting it (Copilot cards' Approve/Deny).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::{broadcast, watch};

use crate::term::Term;
use crate::util::home_dir;

/// Buffered output messages before a slow phone is dropped to the latest.
const BROADCAST_CAP: usize = 256;

#[derive(Serialize, Clone)]
struct PaneInfo {
    id: String,
    label: String,
    cmd: String,
    active: bool,
    w: u32,
    h: u32,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

struct Session {
    term: Arc<Term>,
    label: String,
    w: u16,
    h: u16,
}

/// The live selection: which session streams to phones, plus the switch that
/// stops its pump task when another pane is selected.
struct Selected {
    id: String,
    stop: watch::Sender<bool>,
}

pub struct Shells {
    out_tx: broadcast::Sender<Vec<u8>>,
    sessions: Mutex<HashMap<String, Session>>,
    selected: Mutex<Option<Selected>>,
    next_id: Mutex<u32>,
    /// Copilot status per pane id, driven by Claude Code hook events.
    statuses: Mutex<HashMap<String, String>>,
}

impl Shells {
    pub fn new() -> Arc<Shells> {
        let (out_tx, _) = broadcast::channel(BROADCAST_CAP);
        Arc::new(Shells {
            out_tx,
            sessions: Mutex::new(HashMap::new()),
            selected: Mutex::new(None),
            next_id: Mutex::new(0),
            statuses: Mutex::new(HashMap::new()),
        })
    }

    /// Subscribe to the selected pane's output (snapshot replay, then live).
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
        self.prune_dead();
        let sessions = self.sessions.lock().unwrap();
        let selected = self.selected.lock().unwrap();
        let statuses = self.statuses.lock().unwrap();
        let cwd = home_dir()
            .and_then(|h| h.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();

        let mut panes: Vec<PaneInfo> = sessions
            .iter()
            .map(|(id, s)| PaneInfo {
                id: id.clone(),
                label: s.label.clone(),
                cmd: s.term.program.clone(),
                active: selected.as_ref().is_some_and(|sel| &sel.id == id),
                w: s.w as u32,
                h: s.h as u32,
                cwd: cwd.clone(),
                status: statuses.get(id).cloned(),
            })
            .collect();
        // Stable order (HashMap iteration isn't): by numeric id.
        panes.sort_by_key(|p| p.id.trim_start_matches('%').parse::<u32>().unwrap_or(0));

        serde_json::to_string(&Msg { t: "panes", panes })
            .unwrap_or_else(|_| "{\"t\":\"panes\",\"panes\":[]}".to_string())
    }

    /// Spawn a fresh shell session and return its `(pane_id, label)`.
    pub fn create_session(&self) -> Option<(String, String)> {
        let term = match Term::spawn() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("[pointflow] could not spawn shell: {e}");
                return None;
            }
        };
        let mut next = self.next_id.lock().unwrap();
        *next += 1;
        let id = format!("%{}", *next);
        let label = format!("shell {} ({})", *next, term.program);
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Session {
                term,
                label: label.clone(),
                w: 80,
                h: 24,
            },
        );
        Some((id, label))
    }

    /// Human label for a pane id, if it still exists.
    pub fn pane_label(&self, id: &str) -> Option<String> {
        self.sessions.lock().unwrap().get(id).map(|s| s.label.clone())
    }

    /// Record a Copilot status ("waiting"/"done") for a pane.
    pub fn set_status(&self, pane: &str, status: &str) {
        self.statuses
            .lock()
            .unwrap()
            .insert(pane.to_string(), status.to_string());
    }

    /// View a pane at the phone's size: replay its scrollback, then stream
    /// its live output on the shared broadcast until another pane is selected.
    pub fn select(&self, pane: &str, cols: u16, rows: u16) {
        self.stop();

        let (snapshot, mut live) = {
            let mut sessions = self.sessions.lock().unwrap();
            let Some(s) = sessions.get_mut(pane) else {
                return;
            };
            s.w = cols;
            s.h = rows;
            s.term.resize(cols, rows);
            s.term.subscribe()
        };

        if !snapshot.is_empty() {
            let _ = self.out_tx.send(snapshot);
        }

        let (stop_tx, mut stop_rx) = watch::channel(false);
        *self.selected.lock().unwrap() = Some(Selected {
            id: pane.to_string(),
            stop: stop_tx,
        });

        // Pump: session output → shared broadcast, until deselected/replaced.
        let out_tx = self.out_tx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => break,
                    chunk = live.recv() => match chunk {
                        Ok(bytes) => {
                            let _ = out_tx.send(bytes);
                        }
                        // Slow consumer: resume from the latest output.
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        });
    }

    /// Raw keystrokes for the selected pane.
    // Never hold `selected` while taking `sessions`: panes_json/prune_dead
    // lock sessions → selected, so the reverse order can ABBA-deadlock.
    pub fn write_input(&self, bytes: &[u8]) {
        let Some(id) = self.selected.lock().unwrap().as_ref().map(|sel| sel.id.clone()) else {
            return;
        };
        if let Some(s) = self.sessions.lock().unwrap().get(&id) {
            s.term.write_input(bytes);
        }
    }

    /// Resize the selected pane to the phone's viewport.
    pub fn resize(&self, cols: u16, rows: u16) {
        let Some(id) = self.selected.lock().unwrap().as_ref().map(|sel| sel.id.clone()) else {
            return;
        };
        if let Some(s) = self.sessions.lock().unwrap().get_mut(&id) {
            s.w = cols;
            s.h = rows;
            s.term.resize(cols, rows);
        }
    }

    /// Send key bytes to a *specific* pane without selecting it (used by
    /// notification cards: Approve/Deny from anywhere).
    pub fn send_keys_to(&self, pane: &str, bytes: &[u8]) {
        if let Some(s) = self.sessions.lock().unwrap().get(pane) {
            s.term.write_input(bytes);
        }
    }

    /// Stop streaming (keeps every shell running for the next attach).
    pub fn stop(&self) {
        if let Some(sel) = self.selected.lock().unwrap().take() {
            let _ = sel.stop.send(true);
        }
    }

    /// Drop sessions whose shell has exited (and their stale statuses).
    fn prune_dead(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        let dead: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| !s.term.alive())
            .map(|(id, _)| id.clone())
            .collect();
        if dead.is_empty() {
            return;
        }
        let mut statuses = self.statuses.lock().unwrap();
        let mut selected = self.selected.lock().unwrap();
        for id in dead {
            sessions.remove(&id);
            statuses.remove(&id);
            if selected.as_ref().is_some_and(|sel| sel.id == id) {
                if let Some(sel) = selected.take() {
                    let _ = sel.stop.send(true);
                }
            }
        }
    }
}
