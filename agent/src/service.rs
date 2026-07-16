//! `--install-service`: keep the agent alive across crashes, reboots and
//! power loss.
//!
//! macOS: a LaunchAgent (`~/Library/LaunchAgents/com.pointflow.agent.plist`)
//! with RunAtLoad + KeepAlive — launchd starts the agent at login and
//! restarts it if it ever dies. Combined with `pmset autorestart on` (auto
//! power-on after an outage) and automatic login, a Mac mini comes back from
//! a power cut with the agent serving, the tmux world rebuilt (see
//! `tmux::restore_if_needed`) and a push telling the phone where to
//! reconnect (`POINTFLOW_SERVICE` marks service-managed runs so manual
//! restarts don't buzz the phone).

#[cfg(target_os = "macos")]
pub fn install(tunnel: bool) {
    use std::process::Command;

    let Ok(exe) = std::env::current_exe() else {
        eprintln!("[pointflow] cannot resolve this binary's path");
        return;
    };
    let Some(home) = crate::util::home_dir() else {
        eprintln!("[pointflow] cannot resolve the home directory");
        return;
    };
    let Some(state) = crate::util::state_dir() else { return };
    let _ = std::fs::create_dir_all(&state);
    let log = state.join("agent.log");

    let agents = home.join("Library/LaunchAgents");
    let _ = std::fs::create_dir_all(&agents);
    let plist_path = agents.join("com.pointflow.agent.plist");

    let tunnel_arg = if tunnel {
        "\n        <string>--tunnel</string>"
    } else {
        ""
    };
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.pointflow.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>{tunnel_arg}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>WorkingDirectory</key>
    <string>{home}</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>POINTFLOW_SERVICE</key>
        <string>1</string>
    </dict>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        exe = exe.display(),
        home = home.display(),
        log = log.display(),
    );
    if let Err(e) = std::fs::write(&plist_path, plist) {
        eprintln!("[pointflow] could not write {}: {e}", plist_path.display());
        return;
    }

    let uid = user_id();
    // Reload cleanly if a previous version is already registered.
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{uid}/com.pointflow.agent")])
        .output();
    let plist_str = plist_path.to_string_lossy();
    let loaded = Command::new("/bin/launchctl")
        .args(["bootstrap", &format!("gui/{uid}"), &plist_str])
        .status()
        .is_ok_and(|s| s.success())
        || Command::new("/bin/launchctl")
            .args(["load", "-w", &plist_str])
            .status()
            .is_ok_and(|s| s.success());
    if !loaded {
        eprintln!("[pointflow] launchctl could not load the service (plist written to {plist_str})");
        return;
    }

    println!(
        "\n  ✓ Service installed — the agent now starts at login and restarts if it dies.\n\
         \n\
         \x20   service   com.pointflow.agent{}\n\
         \x20   logs      {}\n\
         \x20   remove    pointflow-agent --uninstall-service\n\
         \n\
         \x20 To survive POWER LOSS end-to-end (Mac mini / desktop):\n\
         \x20   1. Power back on after an outage:  sudo pmset autorestart on\n\
         \x20   2. Log in without a keyboard: System Settings → Users & Groups →\n\
         \x20      \"Automatically log in as…\" (LaunchAgents run at login; FileVault\n\
         \x20      must be off for this)\n\
         \x20   3. Trackpad/keyboard injection under the service needs Accessibility\n\
         \x20      granted to the binary itself: System Settings → Privacy &\n\
         \x20      Security → Accessibility → + → {}\n\
         \x20      (terminal streaming and Claude cards work without it)\n\
         \n\
         \x20 Reconnecting after a restart:\n\
         \x20   • LAN: the pairing token persists, so with a fixed IP (router DHCP\n\
         \x20     reservation) the same QR link keeps working forever.\n\
         \x20   • --tunnel: quick tunnels mint a NEW public URL each start — phones\n\
         \x20     that enabled notifications get a \"back online\" push carrying the\n\
         \x20     fresh link. For a permanent URL, use a named Cloudflare tunnel or\n\
         \x20     Tailscale instead.\n\
         \x20   • Shells: the tmux layout is snapshotted every 30 s and rebuilt on\n\
         \x20     the next start; panes that were running Claude Code are resumed\n\
         \x20     with `claude --continue`.\n",
        if tunnel { "  (with --tunnel)" } else { "" },
        log.display(),
        exe.display(),
    );
}

#[cfg(target_os = "macos")]
pub fn uninstall() {
    use std::process::Command;

    let uid = user_id();
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &format!("gui/{uid}/com.pointflow.agent")])
        .output();
    if let Some(home) = crate::util::home_dir() {
        let plist = home.join("Library/LaunchAgents/com.pointflow.agent.plist");
        let _ = std::fs::remove_file(&plist);
    }
    println!("[pointflow] service removed (the running agent, if any, was stopped)");
}

#[cfg(target_os = "macos")]
fn user_id() -> String {
    std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "501".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn install(_tunnel: bool) {
    eprintln!(
        "[pointflow] --install-service is macOS-only for now (Windows: create a \
         Scheduled Task running this binary at logon; see docs/ROADMAP.md)"
    );
}

#[cfg(not(target_os = "macos"))]
pub fn uninstall() {
    eprintln!("[pointflow] --uninstall-service is macOS-only for now");
}
