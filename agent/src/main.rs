//! PointFlow desktop agent.
//!
//! Serves the static phone UI over the LAN and accepts a single authenticated
//! WebSocket from a phone, translating its messages into real mouse/keyboard
//! input on this computer (macOS: requires Accessibility permission).
//!
//! Terminal backend per platform: macOS/Linux bridge to the user's tmux panes
//! (`tmux.rs`, plus macOS Terminal.app tabs via `tabs.rs`); Windows manages
//! PointFlow-owned ConPTY shells (`shells.rs`) and bridges to already-running
//! console shells (`wintabs.rs`) — same wire protocol either way.

mod hooks;
mod input;
mod protocol;
mod push;
mod service;
#[cfg(windows)]
mod shells;
#[cfg(target_os = "macos")]
mod tabs;
#[cfg(windows)]
mod term;
#[cfg(unix)]
mod tmux;
mod util;
#[cfg(windows)]
mod wintabs;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

use input::InputCmd;
use protocol::ClientMsg;
use push::Push;
#[cfg(windows)]
use shells::Shells as ShellBackend;
#[cfg(target_os = "macos")]
use tabs::Tabs;
#[cfg(unix)]
use tmux::Tmux as ShellBackend;
use util::home_dir;
#[cfg(windows)]
use wintabs::Tabs;

/// Default listen port. Override with POINTFLOW_PORT.
const DEFAULT_PORT: u16 = 8742;

/// The built phone UI, baked into the binary so the agent ships as a single
/// file. Release builds embed `phone/out`; debug builds read it live from
/// disk (so UI rebuilds don't need a cargo rebuild). `POINTFLOW_WEB_DIR`
/// still overrides with an on-disk directory.
#[derive(rust_embed::Embed)]
#[folder = "../phone/out"]
struct WebAssets;

#[derive(Clone)]
struct AppState {
    token: String,
    tx: Sender<InputCmd>,
    /// Terminal backend: tmux panes (unix) or owned ConPTY shells (Windows).
    shells: Arc<ShellBackend>,
    /// Copilot events (from Claude Code hooks) and Terminal-tab streams fanned
    /// out to phones, as ready-to-send JSON strings.
    events: broadcast::Sender<String>,
    /// Bridge to already-open terminals: Terminal.app tabs on macOS, running
    /// console shells on Windows (no tmux required either way).
    #[cfg(any(target_os = "macos", windows))]
    tabs: Arc<Tabs>,
    /// Web Push (lock-screen notifications). None if VAPID setup failed or
    /// unsupported on this OS.
    push: Option<Arc<Push>>,
}

#[tokio::main]
async fn main() {
    // Hidden `--console-*` helper modes (this exe relaunched to attach to
    // another shell's console — see wintabs.rs). Never returns if one matched.
    #[cfg(windows)]
    wintabs::maybe_run_helper();

    let port: u16 = std::env::var("POINTFLOW_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let token = load_or_create_token();

    let ip = local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "localhost".to_string());
    let url = format!("http://{ip}:{port}/?token={token}");

    // `--qr` just reprints the pairing QR for the saved token and exits, so you
    // can re-pair a device without restarting (or disturbing) a running agent.
    if std::env::args().any(|a| a == "--qr") {
        print_qr(&url);
        return;
    }

    // `--install-hooks` registers Claude Code Notification/Stop hooks that
    // report to this agent, then exits.
    if std::env::args().any(|a| a == "--install-hooks") {
        if let Err(e) = hooks::install(port) {
            eprintln!("[pointflow] hook install failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    // `--install-service` keeps the agent alive across reboots and power
    // loss (auto-start + restart-on-crash; see service.rs), then exits.
    if std::env::args().any(|a| a == "--install-service") {
        service::install(std::env::args().any(|a| a == "--tunnel"));
        return;
    }
    if std::env::args().any(|a| a == "--uninstall-service") {
        service::uninstall();
        return;
    }

    // Dedicated thread owns the (non-Send) input engine; we feed it commands.
    let (tx, rx) = crossbeam_channel::unbounded::<InputCmd>();
    std::thread::spawn(move || input::run(rx));

    // Keep the *system* awake while the agent runs so shells (and Claude
    // Code) stay reachable with the display off/locked.
    keep_awake();

    // After a reboot/power loss, rebuild the user's tmux world (same
    // sessions, same cwds, Claude conversations resumed) before phones
    // connect — then keep snapshotting it for the next outage.
    #[cfg(unix)]
    {
        tmux::restore_if_needed();
        std::thread::spawn(tmux::snapshot_loop);
    }

    // Terminal backend: tmux bridge (unix) or owned ConPTY shells (Windows).
    let shells = ShellBackend::new();
    let (events, _) = broadcast::channel::<String>(64);
    // Bridge to already-open terminals; streams share the events pipe.
    #[cfg(any(target_os = "macos", windows))]
    let tabs = Tabs::start(events.clone());

    let web_dir = std::env::var("POINTFLOW_WEB_DIR").ok().map(PathBuf::from);

    let state = AppState {
        token: token.clone(),
        tx,
        shells,
        events,
        #[cfg(any(target_os = "macos", windows))]
        tabs,
        push: Push::init().map(Arc::new),
    };
    // Kept for the startup "back online" pushes below; the router takes `state`.
    let push_handle = state.push.clone();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/event", post(event_handler))
        .route("/upload", post(upload_handler))
        .route("/push/key", get(push_key_handler))
        .route("/push/subscribe", post(push_subscribe_handler))
        .route("/manifest.webmanifest", get(manifest_handler))
        .layer(DefaultBodyLimit::max(25 * 1024 * 1024))
        .with_state(state);
    // Phone UI: an explicit POINTFLOW_WEB_DIR serves from disk (dev override);
    // otherwise the UI embedded in the binary answers.
    let app = match &web_dir {
        Some(dir) => app.fallback_service(
            ServeDir::new(dir).append_index_html_on_directories(true),
        ),
        None => app.fallback(get(embedded_ui_handler)),
    };

    print_banner(&url, web_dir.as_deref());

    // `--tunnel`: also expose the agent at a public HTTPS URL via a Cloudflare
    // quick tunnel — works from any network, no VPN on the phone. The session
    // token still gates every connection.
    let tunnel_requested = std::env::args().any(|a| a == "--tunnel");
    if tunnel_requested {
        let token = token.clone();
        let push = push_handle.clone();
        tokio::spawn(async move { run_tunnel(port, token, push).await });
    }

    // Service-managed (re)start — a reboot, power back after an outage, or a
    // crash restart: tell subscribed phones the agent is reachable again.
    // Tunnel runs push their own (freshly-minted) URL from run_tunnel instead.
    if !tunnel_requested && std::env::var_os("POINTFLOW_SERVICE").is_some() {
        if let Some(push) = push_handle {
            let url = url.clone();
            tokio::spawn(async move {
                push.notify_all(
                    "✦ PointFlow is back online",
                    "Tap to reconnect to your shells",
                    Some(&url),
                )
                .await;
            });
        }
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[pointflow] could not bind 0.0.0.0:{port}: {e}");
            std::process::exit(1);
        }
    };
    // Note: axum 0.7's `serve` takes a concrete TcpListener, so we can't set
    // TCP_NODELAY per connection here. Pointer latency is kept low instead by
    // rAF-batching sends on the phone (≤1 move per frame).
    axum::serve(listener, app).await.expect("server crashed");
}

/// Keep the *system* awake while the agent runs (the display may sleep).
#[cfg(target_os = "macos")]
fn keep_awake() {
    // `-w` ties the assertion to our lifetime.
    if std::process::Command::new("/usr/bin/caffeinate")
        .args(["-i", "-s", "-w", &std::process::id().to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
    {
        println!("[pointflow] keeping the Mac awake while running (display may sleep)");
    }
}

#[cfg(windows)]
fn keep_awake() {
    use windows_sys::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    // The assertion is tied to the calling thread, so park one for our
    // lifetime; it clears automatically when the process exits.
    std::thread::spawn(|| {
        let ok = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) } != 0;
        if ok {
            println!("[pointflow] keeping the PC awake while running (display may sleep)");
        }
        loop {
            std::thread::park();
        }
    });
}

#[cfg(not(any(target_os = "macos", windows)))]
fn keep_awake() {
    // TODO(roadmap Phase 4): systemd-inhibit on Linux.
}

/// Serve the embedded phone UI: exact path first, then `<dir>/index.html`
/// (Next static-export routes), 404 otherwise.
async fn embedded_ui_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let key = if path.is_empty() { "index.html" } else { path };
    let (key, file) = match WebAssets::get(key) {
        Some(f) => (key.to_string(), Some(f)),
        None => {
            let idx = format!("{}/index.html", key.trim_end_matches('/'));
            let f = WebAssets::get(&idx);
            (idx, f)
        }
    };
    match file {
        Some(f) => (
            [(
                axum::http::header::CONTENT_TYPE,
                mime_guess::from_path(&key).first_or_octet_stream().to_string(),
            )],
            f.data,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Receives Claude Code hook events (see `hooks.rs`) and relays them to phones.
/// Authenticated with the same session token as the WebSocket (Bearer header or
/// `?token=`), so nothing on the network can spoof notifications.
async fn event_handler(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: String,
) -> StatusCode {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::trim);
    let token = bearer.or(q.get("token").map(String::as_str));
    if token != Some(state.token.as_str()) {
        return StatusCode::UNAUTHORIZED;
    }

    let kind = q.get("kind").map(String::as_str).unwrap_or("notification");
    let pane = q.get("pane").cloned().unwrap_or_default();

    // The hook forwards Claude Code's JSON on stdin; pull out the human text
    // and the session's working directory (identifies WHICH agent this is).
    let parsed = serde_json::from_str::<serde_json::Value>(&body).ok();
    let message = parsed
        .as_ref()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| match kind {
            "stop" => "Claude finished responding".to_string(),
            _ => "Claude needs your input".to_string(),
        });
    let cwd_label = parsed
        .as_ref()
        .and_then(|v| v.get("cwd").and_then(|c| c.as_str()))
        .map(|p| format!("claude · {}", p.rsplit('/').next().unwrap_or(p)));

    let label = if pane.is_empty() {
        // Session outside tmux — identify it by its project folder instead.
        cwd_label.unwrap_or_default()
    } else {
        let status = if kind == "stop" { "done" } else { "waiting" };
        state.shells.set_status(&pane, status);
        state
            .shells
            .pane_label(&pane)
            .or(cwd_label)
            .unwrap_or_else(|| pane.clone())
    };
    let event = serde_json::json!({
        "t": "event",
        "kind": kind,
        "pane": pane,
        "label": label,
        "message": message,
    });
    println!("[pointflow] copilot event: {kind} pane={pane}");
    let _ = state.events.send(event.to_string());

    // Also deliver as a lock-screen push (PWA over HTTPS); fire-and-forget.
    if let Some(push) = state.push.clone() {
        let title = if kind == "stop" {
            "✓ Claude finished".to_string()
        } else {
            "✳ Claude needs you".to_string()
        };
        let body = if label.is_empty() {
            message.clone()
        } else {
            format!("{message} — {label}")
        };
        tokio::spawn(async move { push.notify_all(&title, &body, None).await });
    }
    StatusCode::NO_CONTENT
}

/// The applicationServerKey the phone needs to subscribe to push.
async fn push_key_handler(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if q.get("token") != Some(&state.token) {
        return (StatusCode::UNAUTHORIZED, String::new());
    }
    match state.push.as_ref().and_then(|p| p.public_key()) {
        Some(key) => (
            StatusCode::OK,
            serde_json::json!({ "key": key }).to_string(),
        ),
        None => (StatusCode::SERVICE_UNAVAILABLE, String::new()),
    }
}

/// Stores the browser's PushSubscription (standard JSON shape).
async fn push_subscribe_handler(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
    body: String,
) -> StatusCode {
    if q.get("token") != Some(&state.token) {
        return StatusCode::UNAUTHORIZED;
    }
    let Some(push) = &state.push else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    if push.subscribe_json(&body) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::BAD_REQUEST
    }
}

/// PWA manifest, served dynamically so the installed app's start_url carries
/// the session token (an installed icon opens pre-authenticated).
async fn manifest_handler(State(state): State<AppState>) -> impl IntoResponse {
    let manifest = serde_json::json!({
        "name": "PointFlow",
        "short_name": "PointFlow",
        "description": "Your phone as trackpad, keyboard and Claude Code remote for your computer.",
        "start_url": format!("/?token={}", state.token),
        "display": "standalone",
        "background_color": "#050508",
        "theme_color": "#050508",
        "icons": [
            { "src": "/icon-192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "/icon-512.png", "sizes": "512x512", "type": "image/png" }
        ]
    });
    (
        [(axum::http::header::CONTENT_TYPE, "application/manifest+json")],
        manifest.to_string(),
    )
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    // First message must authenticate with the session token.
    let authed = matches!(
        socket.recv().await,
        Some(Ok(Message::Text(t)))
            if matches!(
                serde_json::from_str::<ClientMsg>(&t),
                Ok(ClientMsg::Auth { token }) if token == state.token
            )
    );

    if !authed {
        let _ = socket
            .send(Message::Text("{\"t\":\"denied\"}".to_string()))
            .await;
        eprintln!("[pointflow] rejected unauthenticated connection");
        return;
    }

    let _ = socket.send(Message::Text("{\"t\":\"ok\"}".to_string())).await;
    println!("[pointflow] phone connected");

    // Bidirectional from here. All agent→phone messages funnel through one
    // mpsc so both the tmux output stream (binary) and the panes list (text)
    // share the single sink.
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    // Pump the selected pane's output (snapshot + live) to the phone as binary.
    let pump = {
        let mut frames = state.shells.subscribe();
        let out_tx = out_tx.clone();
        tokio::spawn(async move {
            loop {
                match frames.recv().await {
                    Ok(bytes) => {
                        if out_tx.send(Message::Binary(bytes)).is_err() {
                            break;
                        }
                    }
                    // A slow phone fell behind; resume from the latest output.
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    };

    // Relay Copilot events (Claude Code hooks) to this phone as JSON text.
    let events_task = {
        let mut evs = state.events.subscribe();
        let out_tx = out_tx.clone();
        tokio::spawn(async move {
            loop {
                match evs.recv().await {
                    Ok(json) => {
                        if out_tx.send(Message::Text(json)).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    };

    // Drain the outgoing queue to the socket.
    let send_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Recv half: binary frames are keystrokes for the attached pane; JSON text
    // is shell control or input that drives the computer via the engine.
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Binary(b) => state.shells.write_input(&b),
            Message::Text(t) => {
                if let Ok(m) = serde_json::from_str::<ClientMsg>(&t) {
                    match m {
                        ClientMsg::TmuxList => {
                            let _ = out_tx.send(Message::Text(state.shells.panes_json()));
                        }
                        ClientMsg::TmuxSelect { id, cols, rows } => {
                            state.shells.select(&id, cols, rows)
                        }
                        ClientMsg::TmuxResize { cols, rows } => state.shells.resize(cols, rows),
                        ClientMsg::TmuxKeys { id, hex } => {
                            state.shells.send_keys_to(&id, &decode_hex(&hex));
                        }
                        ClientMsg::TmuxNew => {
                            if let Some((id, label)) = state.shells.create_session() {
                                let _ = out_tx.send(Message::Text(
                                    serde_json::json!({
                                        "t": "tcreated", "id": id, "label": label
                                    })
                                    .to_string(),
                                ));
                                let _ =
                                    out_tx.send(Message::Text(state.shells.panes_json()));
                            }
                        }
                        // Already-open terminals: Terminal.app tabs (macOS) or
                        // console shells (Windows); elsewhere the phone gets an
                        // empty list and the section stays quiet.
                        #[cfg(any(target_os = "macos", windows))]
                        ClientMsg::TabList => {
                            let _ = out_tx.send(Message::Text(state.tabs.tabs_json()));
                        }
                        #[cfg(not(any(target_os = "macos", windows)))]
                        ClientMsg::TabList => {
                            let _ = out_tx.send(Message::Text(
                                "{\"t\":\"tabs\",\"tabs\":[]}".to_string(),
                            ));
                        }
                        #[cfg(any(target_os = "macos", windows))]
                        ClientMsg::TabSelect { win, tab } => state.tabs.select(win, tab),
                        #[cfg(any(target_os = "macos", windows))]
                        ClientMsg::TabStop => state.tabs.stop(),
                        #[cfg(any(target_os = "macos", windows))]
                        ClientMsg::TabType { win, tab, text } => {
                            state.tabs.type_line(win, tab, &text)
                        }
                        #[cfg(any(target_os = "macos", windows))]
                        ClientMsg::TabFocus { win, tab } => state.tabs.focus(win, tab),
                        #[cfg(not(any(target_os = "macos", windows)))]
                        ClientMsg::TabSelect { .. }
                        | ClientMsg::TabStop
                        | ClientMsg::TabType { .. }
                        | ClientMsg::TabFocus { .. } => {}
                        // Channel send only fails if the input thread died.
                        other => {
                            if let Some(cmd) = other.into_cmd() {
                                let _ = state.tx.send(cmd);
                            }
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    state.shells.stop();
    #[cfg(any(target_os = "macos", windows))]
    state.tabs.stop();
    pump.abort();
    events_task.abort();
    send_task.abort();
    println!("[pointflow] phone disconnected");
}

/// Receives a photo/file from the phone and saves it under ~/Downloads/PointFlow,
/// returning the absolute path — the phone then inserts that path into the
/// terminal prompt so Claude Code can read the image. Token-authenticated.
async fn upload_handler(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if q.get("token") != Some(&state.token) {
        return (StatusCode::UNAUTHORIZED, String::new());
    }
    if body.is_empty() {
        return (StatusCode::BAD_REQUEST, String::new());
    }

    let name = q.get("name").map(String::as_str).unwrap_or("photo.jpg");
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let dir = match home_dir() {
        Some(h) => h.join("Downloads").join("PointFlow"),
        None => std::env::temp_dir().join("pointflow-uploads"),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    let path = dir.join(format!("{stamp}-{safe}"));
    if let Err(e) = std::fs::write(&path, &body) {
        return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    }
    println!("[pointflow] photo saved: {} ({} KB)", path.display(), body.len() / 1024);
    (
        StatusCode::OK,
        serde_json::json!({ "path": path.to_string_lossy() }).to_string(),
    )
}

/// Spawn `cloudflared` as a quick tunnel to this agent and print the public
/// pairing QR/URL once the edge assigns one. Runs for the agent's lifetime.
/// Quick tunnels mint a new hostname every start, so once the URL is known
/// it's also pushed to subscribed phones (the old link died with the reboot).
async fn run_tunnel(port: u16, token: String, push: Option<Arc<Push>>) {
    // Well-known install spots first (a minimal PATH is common under launchd);
    // otherwise trust PATH — winget/choco/apt all put cloudflared there.
    #[cfg(target_os = "macos")]
    let candidates = ["/opt/homebrew/bin/cloudflared", "/usr/local/bin/cloudflared"];
    #[cfg(not(target_os = "macos"))]
    let candidates: [&str; 0] = [];
    let bin = candidates
        .iter()
        .find(|p| Path::new(p).exists())
        .copied()
        .unwrap_or("cloudflared");

    let mut child = match tokio::process::Command::new(bin)
        .args([
            "tunnel",
            "--url",
            &format!("http://127.0.0.1:{port}"),
            "--no-autoupdate",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(_) => {
            #[cfg(target_os = "macos")]
            eprintln!("[pointflow] --tunnel needs cloudflared: brew install cloudflared");
            #[cfg(windows)]
            eprintln!(
                "[pointflow] --tunnel needs cloudflared: winget install Cloudflare.cloudflared"
            );
            #[cfg(not(any(target_os = "macos", windows)))]
            eprintln!("[pointflow] --tunnel needs cloudflared on PATH");
            return;
        }
    };

    let Some(stderr) = child.stderr.take() else { return };
    let mut lines = tokio::io::BufReader::new(stderr);
    let mut announced = false;
    let mut line = String::new();
    use tokio::io::AsyncBufReadExt;
    loop {
        line.clear();
        match lines.read_line(&mut line).await {
            Ok(0) => break, // cloudflared exited
            Ok(_) => {
                if announced {
                    continue;
                }
                if let Some(url) = extract_tunnel_url(&line) {
                    announced = true;
                    println!("\n  ✦ Public tunnel ready — works from ANY network:\n");
                    let link = format!("{url}/?token={token}");
                    print_qr(&link);
                    if let Some(push) = &push {
                        push.notify_all(
                            "✦ PointFlow is back online",
                            "New public link after a restart — tap to reconnect",
                            Some(&link),
                        )
                        .await;
                    }
                }
            }
            Err(_) => break,
        }
    }
    eprintln!("[pointflow] tunnel closed (cloudflared exited)");
}

/// Pull "https://xxx.trycloudflare.com" out of a cloudflared log line.
fn extract_tunnel_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    let end = rest.find(".trycloudflare.com")? + ".trycloudflare.com".len();
    Some(rest[..end].to_string())
}

/// Decode a hex string ("0d0a") to bytes; invalid input yields an empty vec.
fn decode_hex(s: &str) -> Vec<u8> {
    if s.len() % 2 != 0 {
        return Vec::new();
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .unwrap_or_default()
}

/// 16 hex chars of randomness — embedded in the pairing QR/URL.
fn gen_token() -> String {
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| format!("{:x}", rng.gen_range(0..16)))
        .collect()
}

/// Where the persistent pairing token lives: `~/.pointflow/token`.
fn token_path() -> Option<PathBuf> {
    util::state_dir().map(|d| d.join("token"))
}

/// Load the saved pairing token, creating (and persisting) one on first run.
/// A stable token means restarting the agent — or running `--qr` — reuses the
/// same QR, so paired phones keep working across restarts.
fn load_or_create_token() -> String {
    if let Some(path) = token_path() {
        if let Ok(saved) = std::fs::read_to_string(&path) {
            let saved = saved.trim();
            if !saved.is_empty() {
                return saved.to_string();
            }
        }
        let token = gen_token();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, &token) {
            eprintln!("[pointflow] could not persist token ({e}); using a session-only one");
        }
        return token;
    }
    gen_token()
}

/// Render the pairing QR + URL to the terminal.
fn print_qr(url: &str) {
    use qrcode::render::unicode;
    use qrcode::QrCode;

    println!("\n  PointFlow agent\n  ===============\n");

    if let Ok(code) = QrCode::new(url.as_bytes()) {
        let qr = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .quiet_zone(true)
            .build();
        println!("{qr}");
    }

    println!("  Scan the QR with your phone, or open this on your phone:\n");
    println!("    {url}\n");
    println!("  (Phone and computer must be on the same WiFi network.)");
}

fn print_banner(url: &str, web_dir: Option<&Path>) {
    print_qr(url);

    match web_dir {
        Some(dir) if !dir.join("index.html").exists() => eprintln!(
            "\n  ⚠  Phone UI not found at {}\n     Build it first:  (cd phone && pnpm build)\n",
            dir.display()
        ),
        None if WebAssets::get("index.html").is_none() => eprintln!(
            "\n  ⚠  Phone UI missing from this build.\n     Debug builds read phone/out from disk: (cd phone && pnpm build)\n"
        ),
        _ => {}
    }
    println!();
}
