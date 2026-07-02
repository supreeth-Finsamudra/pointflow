//! Installs Claude Code hooks that report to the PointFlow agent.
//!
//! Claude Code fires a `Notification` hook when it needs the user's
//! permission/input and a `Stop` hook when it finishes responding. We register
//! shell commands for both that POST the hook's JSON (stdin) to the agent's
//! `/event` endpoint, tagged with the tmux pane they ran in (`$TMUX_PANE`) and
//! authenticated with the persisted session token. The agent relays these to
//! the phone as notification cards.
//!
//! The install merges into `~/.claude/settings.json` non-destructively (a
//! `.bak-pointflow` backup is written first) and is idempotent — re-running it
//! won't duplicate entries.

use std::path::PathBuf;

use serde_json::{json, Value};

/// Marker present in our hook commands; used to detect existing installs.
const MARKER: &str = ".pointflow/token";

pub fn install(port: u16) -> Result<(), String> {
    let path = settings_path().ok_or("could not resolve $HOME")?;

    let mut root: Value = match std::fs::read_to_string(&path) {
        Ok(s) if !s.trim().is_empty() => {
            serde_json::from_str(&s).map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?
        }
        _ => json!({}),
    };
    if !root.is_object() {
        return Err(format!("{} is not a JSON object", path.display()));
    }

    let mut changed = false;
    for (event, kind) in [("Notification", "notification"), ("Stop", "stop")] {
        let cmd = hook_command(port, kind);
        let hooks = root
            .as_object_mut()
            .unwrap()
            .entry("hooks")
            .or_insert_with(|| json!({}));
        if !hooks.is_object() {
            return Err("settings.json \"hooks\" is not an object".into());
        }
        let entries = hooks
            .as_object_mut()
            .unwrap()
            .entry(event)
            .or_insert_with(|| json!([]));
        let Some(list) = entries.as_array_mut() else {
            return Err(format!("settings.json hooks.{event} is not an array"));
        };

        let already = list.iter().any(|e| {
            e["hooks"]
                .as_array()
                .map(|hs| {
                    hs.iter().any(|h| {
                        h["command"].as_str().is_some_and(|c| c.contains(MARKER))
                    })
                })
                .unwrap_or(false)
        });
        if already {
            println!("[pointflow] {event} hook already installed");
            continue;
        }
        list.push(json!({ "hooks": [{ "type": "command", "command": cmd }] }));
        changed = true;
        println!("[pointflow] installed {event} hook");
    }

    if changed {
        if path.exists() {
            let bak = path.with_extension("json.bak-pointflow");
            std::fs::copy(&path, &bak).map_err(|e| e.to_string())?;
            println!("[pointflow] backup written to {}", bak.display());
        } else if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
        std::fs::write(&path, pretty + "\n").map_err(|e| e.to_string())?;
        println!("[pointflow] wrote {}", path.display());
    } else {
        println!("[pointflow] hooks already present; nothing to do");
    }
    println!(
        "[pointflow] Claude Code will now notify your phone when it needs you.\n  (Events only fire for sessions started after this install.)"
    );
    Ok(())
}

/// The shell command Claude Code runs on each event. `$TMUX_PANE` expands to
/// the pane the session lives in (empty outside tmux); the token is read from
/// the agent's persisted pairing token so the endpoint stays authenticated.
/// Fails silently (agent not running is fine).
fn hook_command(port: u16, kind: &str) -> String {
    format!(
        "curl -s -m 2 -X POST \"http://127.0.0.1:{port}/event?kind={kind}&pane=$TMUX_PANE\" \
         -H \"Authorization: Bearer $(cat \"$HOME/{MARKER}\" 2>/dev/null)\" \
         --data-binary @- >/dev/null 2>&1 || true"
    )
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude").join("settings.json"))
}
