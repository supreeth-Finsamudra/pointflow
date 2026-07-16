//! Bridges the phone to *already-running* Windows console shells (cmd,
//! PowerShell, pwsh) — the Windows analog of the macOS Terminal.app bridge
//! (`tabs.rs`). Speaks the same `tabs`/`tabscr`/`tabhist` wire messages, so
//! the phone UI works unchanged.
//!
//! Windows has no Apple Events, but it has the console API. A process can be
//! attached to only ONE console at a time — and the agent must keep its own —
//! so every console operation runs in a helper: this same exe relaunched with
//! a hidden flag, created DETACHED (no console of its own) so
//! `AttachConsole(pid)` can join the target shell's console:
//!
//!  • `--console-bridge <pid>` — scrapes the visible screen
//!    (`ReadConsoleOutputW`) ~1.5×/sec plus scrollback once on attach, and
//!    prints JSON lines the agent relays to phones. Plain text, no colors —
//!    the same tradeoff as the macOS bridge. Works for classic conhost
//!    windows AND Windows Terminal tabs (ConPTY keeps a real screen buffer).
//!  • `--console-type <pid>` — types stdin + Enter via `WriteConsoleInputW`:
//!    real console keystrokes, no window focus needed, works behind the lock
//!    screen.
//!  • `--console-focus <pid>` — best-effort raises the console window so the
//!    quick keys (the injection path) land in it. Classic conhost only:
//!    Windows Terminal's ConPTY exposes a hidden pseudo-window, which
//!    `SetForegroundWindow` can't raise.

use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::broadcast;

use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleScreenBufferInfo, GetConsoleWindow, ReadConsoleOutputW,
    WriteConsoleInputW, CHAR_INFO, COMMON_LVB_TRAILING_BYTE, CONSOLE_SCREEN_BUFFER_INFO, COORD,
    INPUT_RECORD, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, LEFT_ALT_PRESSED,
    LEFT_CTRL_PRESSED, SHIFT_PRESSED, SMALL_RECT,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::DETACHED_PROCESS;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, VkKeyScanW, MAPVK_VK_TO_VSC, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};

/// How often the bridge helper re-reads the selected console's screen.
const POLL: Duration = Duration::from_millis(600);
/// Scrollback rows replayed on attach.
const HISTORY_ROWS: i32 = 1000;
/// Console shells worth listing.
const SHELLS: [&str; 3] = ["cmd.exe", "powershell.exe", "pwsh.exe"];

#[derive(Serialize, Clone)]
pub struct TabInfo {
    /// The shell's process id (the "window" the phone addresses).
    pub win: i64,
    /// Always 1 — Windows consoles have no tab index.
    pub tab: i64,
    /// Shell name ("pwsh", "cmd"), shown where macOS shows the tty.
    pub tty: String,
    pub busy: bool,
    /// Processes running under the shell, comma-joined ("claude, node").
    pub procs: String,
    /// True if Claude Code is running in this shell.
    pub claude: bool,
    /// Unknown on Windows (needs PEB spelunking); empty.
    pub cwd: String,
}

struct Bridge {
    child: std::process::Child,
}

pub struct Tabs {
    /// JSON text messages fanned out to phones (shares the events channel).
    out_tx: broadcast::Sender<String>,
    bridge: Arc<Mutex<Option<Bridge>>>,
    /// Bumped on every select/stop so a stale bridge reader can tell it was
    /// replaced (and shouldn't announce "(tab closed)").
    generation: Arc<AtomicU64>,
}

impl Tabs {
    /// `out_tx` is the agent's phone-facing JSON broadcast (same one Copilot
    /// events use). No background work until a tab is selected.
    pub fn start(out_tx: broadcast::Sender<String>) -> Arc<Tabs> {
        Arc::new(Tabs {
            out_tx,
            bridge: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
        })
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
            tabs: list_shells(),
        })
        .unwrap_or_else(|_| "{\"t\":\"tabs\",\"tabs\":[]}".to_string())
    }

    /// Select a shell: spawn a bridge helper attached to its console; relay
    /// its history + screen frames to phones until deselected or it dies.
    pub fn select(&self, win: i64, _tab: i64) {
        self.stop();
        let my_gen = self.generation.load(Ordering::SeqCst);

        let Some(mut cmd) = helper_cmd("--console-bridge", win as u32) else {
            return;
        };
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[pointflow] console bridge spawn failed: {e}");
                return;
            }
        };
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            return;
        };
        *self.bridge.lock().unwrap() = Some(Bridge { child });
        println!("[pointflow] console bridge attached to pid {win}");

        let out_tx = self.out_tx.clone();
        let generation = self.generation.clone();
        let bridge = self.bridge.clone();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
                match v.get("t").and_then(|t| t.as_str()) {
                    Some("hist") => {
                        let _ = out_tx.send(json_text("tabhist", text));
                    }
                    Some("scr") => {
                        let _ = out_tx.send(json_text("tabscr", text));
                    }
                    _ => {} // heartbeat
                }
            }
            // Helper gone. If we're still the live selection (not replaced by
            // a newer select/stop), the console itself closed — tell the phone.
            if generation.load(Ordering::SeqCst) == my_gen {
                let _ = out_tx.send(json_text("tabscr", "(tab closed)"));
                if let Some(mut b) = bridge.lock().unwrap().take() {
                    let _ = b.child.kill();
                    let _ = b.child.wait();
                }
            }
        });
    }

    /// Type a line (plus Enter) into a shell's console via a one-shot helper —
    /// real console input, no focus needed, works behind the lock screen.
    pub fn type_line(&self, win: i64, _tab: i64, text: &str) {
        let Some(mut cmd) = helper_cmd("--console-type", win as u32) else {
            return;
        };
        cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
        let Ok(mut child) = cmd.spawn() else { return };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        // Don't block the ws task; the helper exits on its own.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }

    /// Best-effort bring the console window frontmost (quick keys go through
    /// the regular injection path, which types into the focused window).
    pub fn focus(&self, win: i64, _tab: i64) {
        let Some(mut cmd) = helper_cmd("--console-focus", win as u32) else {
            return;
        };
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Ok(mut child) = cmd.spawn() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }

    /// Stop streaming (phone closed the tab view). Shells keep running.
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Some(mut b) = self.bridge.lock().unwrap().take() {
            let _ = b.child.kill();
            let _ = b.child.wait();
        }
    }
}

fn json_text(t: &str, text: &str) -> String {
    serde_json::json!({ "t": t, "text": text }).to_string()
}

/// This exe, relaunched in a helper mode, DETACHED so it has no console of
/// its own and `AttachConsole` can join the target's.
fn helper_cmd(flag: &str, pid: u32) -> Option<std::process::Command> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().ok()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(flag).arg(pid.to_string());
    cmd.creation_flags(DETACHED_PROCESS);
    Some(cmd)
}

/// Dispatch `--console-*` helper modes; returns only if none matched.
/// Called first thing in `main` — helpers must not start the server.
pub fn maybe_run_helper() {
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        let Some(pid) = args.get(i + 1).and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        match args[i].as_str() {
            "--console-bridge" => bridge_main(pid),
            "--console-type" => type_main(pid),
            "--console-focus" => focus_main(pid),
            _ => continue,
        }
        std::process::exit(0);
    }
}

// ---------------------------------------------------------------- helpers --

/// Emit one JSON line to the parent agent; Err means the agent died.
fn emit(kind: &str, text: &str) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(json_text(kind, text).as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

/// Scrape loop: history once, then the visible screen on every change.
fn bridge_main(pid: u32) {
    let hout = match attach_conout(pid) {
        Some(h) => h,
        None => {
            let _ = emit("err", "attach failed");
            std::process::exit(1);
        }
    };

    // Scrollback above the current viewport, replayed once for phone scroll.
    if let Some(info) = buffer_info(hout) {
        let top = info.srWindow.Top as i32;
        if top > 0 {
            let from = (top - HISTORY_ROWS).max(0) as i16;
            let lines = read_rows(hout, 0, info.dwSize.X - 1, from, (top - 1) as i16);
            let text = lines.join("\n");
            let text = text.trim_matches('\n');
            if !text.is_empty() && emit("hist", text).is_err() {
                std::process::exit(0);
            }
        }
    }

    let mut last = String::new();
    let mut quiet = 0u32;
    loop {
        let Some(info) = buffer_info(hout) else {
            // Console gone — the shell (or its window) closed.
            std::process::exit(0);
        };
        let w = info.srWindow;
        let screen = read_rows(hout, w.Left, w.Right, w.Top, w.Bottom).join("\n");
        if screen != last {
            quiet = 0;
            if emit("scr", &screen).is_err() {
                std::process::exit(0); // agent died
            }
            last = screen;
        } else {
            quiet += 1;
            // Idle console: heartbeat ~every 10s so an orphaned helper
            // notices its agent is gone and exits.
            if quiet >= 16 {
                quiet = 0;
                if emit("ping", "").is_err() {
                    std::process::exit(0);
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Type stdin (+ Enter) into the console as real key events.
fn type_main(pid: u32) {
    let mut text = String::new();
    let _ = std::io::stdin().read_to_string(&mut text);
    let hin = match attach_conin(pid) {
        Some(h) => h,
        None => std::process::exit(1),
    };

    let mut records: Vec<INPUT_RECORD> = Vec::new();
    for ch in text.chars() {
        match ch {
            '\r' => {}
            '\n' => push_enter(&mut records),
            _ => push_char(&mut records, ch),
        }
    }
    push_enter(&mut records);

    // The console input buffer is small; feed it in chunks.
    let mut i = 0;
    while i < records.len() {
        let n = (records.len() - i).min(64) as u32;
        let mut written = 0u32;
        let ok = unsafe { WriteConsoleInputW(hin, records.as_ptr().add(i), n, &mut written) };
        if ok == 0 {
            break;
        }
        if written == 0 {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        i += written as usize;
    }
    std::process::exit(0);
}

/// Raise the console's window (classic conhost; ConPTY's is a hidden stub).
fn focus_main(pid: u32) {
    unsafe {
        FreeConsole();
        if AttachConsole(pid) == 0 {
            std::process::exit(1);
        }
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
    std::process::exit(0);
}

/// Attach to `pid`'s console and open its screen buffer.
fn attach_conout(pid: u32) -> Option<HANDLE> {
    attach_open(pid, "CONOUT$")
}

/// Attach to `pid`'s console and open its input buffer.
fn attach_conin(pid: u32) -> Option<HANDLE> {
    attach_open(pid, "CONIN$")
}

fn attach_open(pid: u32, dev: &str) -> Option<HANDLE> {
    let name: Vec<u16> = dev.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        FreeConsole(); // defensive: DETACHED helpers have none anyway
        if AttachConsole(pid) == 0 {
            return None;
        }
        let h = CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        (h != INVALID_HANDLE_VALUE).then_some(h)
    }
}

fn buffer_info(hout: HANDLE) -> Option<CONSOLE_SCREEN_BUFFER_INFO> {
    unsafe {
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
        (GetConsoleScreenBufferInfo(hout, &mut info) != 0).then_some(info)
    }
}

/// Read buffer rows [top..=bottom] × [left..=right] as right-trimmed text.
/// Chunked in bands: `ReadConsoleOutputW` rejects large regions (~64 KB).
fn read_rows(hout: HANDLE, left: i16, right: i16, top: i16, bottom: i16) -> Vec<String> {
    if right < left || bottom < top {
        return Vec::new();
    }
    let width = (right - left + 1) as usize;
    let band = ((16_000 / width.max(1)).max(1) as i16).min(bottom - top + 1);
    let mut out = Vec::with_capacity((bottom - top + 1) as usize);

    let mut y = top;
    while y <= bottom {
        let yend = (y + band - 1).min(bottom);
        let rows = (yend - y + 1) as usize;
        let mut buf: Vec<CHAR_INFO> =
            vec![unsafe { std::mem::zeroed() }; width * rows];
        let mut region = SMALL_RECT {
            Left: left,
            Top: y,
            Right: right,
            Bottom: yend,
        };
        let ok = unsafe {
            ReadConsoleOutputW(
                hout,
                buf.as_mut_ptr(),
                COORD {
                    X: width as i16,
                    Y: rows as i16,
                },
                COORD { X: 0, Y: 0 },
                &mut region,
            )
        };
        if ok == 0 {
            break;
        }
        for r in 0..rows {
            let mut line = String::with_capacity(width);
            for cell in &buf[r * width..(r + 1) * width] {
                // Wide (CJK) glyphs occupy two cells; skip the trailing one.
                if cell.Attributes & COMMON_LVB_TRAILING_BYTE != 0 {
                    continue;
                }
                let u = unsafe { cell.Char.UnicodeChar };
                line.push(char::from_u32(u as u32).unwrap_or(' '));
            }
            out.push(line.trim_end().to_string());
        }
        y = yend + 1;
    }
    out
}

/// Key-down + key-up records for one character, with the VK/shift state a
/// real keyboard would produce (PSReadLine and friends key off the VK).
fn push_char(records: &mut Vec<INPUT_RECORD>, ch: char) {
    let mut units = [0u16; 2];
    for &unit in ch.encode_utf16(&mut units).iter() {
        let scan = unsafe { VkKeyScanW(unit) };
        let (vk, state) = if scan == -1 {
            (0u16, 0u32) // no key for this char; UnicodeChar carries it
        } else {
            let vk = (scan & 0xff) as u16;
            let mods = ((scan >> 8) & 0xff) as u32;
            let mut state = 0u32;
            if mods & 1 != 0 {
                state |= SHIFT_PRESSED;
            }
            // AltGr characters report Ctrl+Alt together.
            if mods & 2 != 0 {
                state |= LEFT_CTRL_PRESSED;
            }
            if mods & 4 != 0 {
                state |= LEFT_ALT_PRESSED;
            }
            (vk, state)
        };
        push_key(records, unit, vk, state);
    }
}

fn push_enter(records: &mut Vec<INPUT_RECORD>) {
    push_key(records, b'\r' as u16, VK_RETURN, 0);
}

fn push_key(records: &mut Vec<INPUT_RECORD>, unicode: u16, vk: u16, state: u32) {
    let scan = if vk != 0 {
        unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) as u16 }
    } else {
        0
    };
    for down in [1, 0] {
        let mut rec: INPUT_RECORD = unsafe { std::mem::zeroed() };
        rec.EventType = KEY_EVENT as u16;
        rec.Event.KeyEvent = KEY_EVENT_RECORD {
            bKeyDown: down,
            wRepeatCount: 1,
            wVirtualKeyCode: vk,
            wVirtualScanCode: scan,
            uChar: KEY_EVENT_RECORD_0 { UnicodeChar: unicode },
            dwControlKeyState: state,
        };
        records.push(rec);
    }
}

// ----------------------------------------------------------- enumeration --

struct Proc {
    pid: u32,
    ppid: u32,
    /// Lowercased exe basename ("pwsh.exe").
    name: String,
}

/// Every running console shell that PointFlow didn't spawn itself (those are
/// already listed in the shells section), with what's running inside it.
fn list_shells() -> Vec<TabInfo> {
    let procs = snapshot_processes();
    let me = std::process::id();
    let parent_of: HashMap<u32, u32> = procs.iter().map(|p| (p.pid, p.ppid)).collect();
    let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for p in &procs {
        children.entry(p.ppid).or_default().push(p);
    }

    // Walk up the parent chain (bounded: PID reuse can create cycles).
    let descends_from_agent = |mut pid: u32| -> bool {
        for _ in 0..64 {
            if pid == me {
                return true;
            }
            match parent_of.get(&pid) {
                Some(&pp) if pp != 0 && pp != pid => pid = pp,
                _ => return false,
            }
        }
        false
    };

    let mut tabs: Vec<TabInfo> = procs
        .iter()
        .filter(|p| SHELLS.contains(&p.name.as_str()))
        .filter(|p| !descends_from_agent(p.pid))
        .map(|shell| {
            let mut names: Vec<String> = Vec::new();
            collect_descendants(shell.pid, &children, &mut names, 0);
            let claude = names.iter().any(|n| n.starts_with("claude"));
            TabInfo {
                win: shell.pid as i64,
                tab: 1,
                tty: shell.name.trim_end_matches(".exe").to_string(),
                busy: !names.is_empty(),
                procs: names.join(", "),
                claude,
                cwd: String::new(),
            }
        })
        .collect();
    tabs.sort_by_key(|t| t.win);
    tabs
}

/// Exe basenames (sans .exe) under `pid`, skipping console plumbing.
fn collect_descendants(
    pid: u32,
    children: &HashMap<u32, Vec<&Proc>>,
    names: &mut Vec<String>,
    depth: u32,
) {
    if depth > 8 {
        return;
    }
    for child in children.get(&pid).into_iter().flatten() {
        if !matches!(child.name.as_str(), "conhost.exe" | "openconsole.exe") {
            let label = child.name.trim_end_matches(".exe").to_string();
            if !names.contains(&label) {
                names.push(label);
            }
        }
        collect_descendants(child.pid, children, names, depth + 1);
    }
}

fn snapshot_processes() -> Vec<Proc> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                out.push(Proc {
                    pid: entry.th32ProcessID,
                    ppid: entry.th32ParentProcessID,
                    name: String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase(),
                });
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        windows_sys::Win32::Foundation::CloseHandle(snap);
    }
    out
}
