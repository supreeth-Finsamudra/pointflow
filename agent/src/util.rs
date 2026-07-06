//! Small cross-platform helpers shared by the agent modules.

use std::path::PathBuf;

/// The user's home directory: `$HOME` on unix; on Windows, `%USERPROFILE%`
/// (HOME is usually unset outside of Git Bash/MSYS).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// `~/.pointflow` — the agent's state directory (token, VAPID key, push subs).
pub fn state_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".pointflow"))
}
