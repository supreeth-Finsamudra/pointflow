//! PointFlow desktop agent.
//!
//! Serves the static phone UI over the LAN and accepts a single authenticated
//! WebSocket from a phone, translating its messages into real mouse/keyboard
//! input on this Mac. Requires Accessibility permission.

mod hooks;
mod input;
mod protocol;
mod push;
mod tabs;
mod tmux;

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
use tabs::Tabs;
use tmux::Tmux;

/// Default listen port. Override with POINTFLOW_PORT.
const DEFAULT_PORT: u16 = 8742;

#[derive(Clone)]
struct AppState {
    token: String,
    tx: Sender<InputCmd>,
    /// Bridge to the user's tmux panes (list, view, drive).
    tmux: Arc<Tmux>,
    /// Copilot events (from Claude Code hooks) and Terminal-tab streams fanned
    /// out to phones, as ready-to-send JSON strings.
    events: broadcast::Sender<String>,
    /// Bridge to already-open Terminal.app tabs (no tmux required).
    tabs: Arc<Tabs>,
    /// Web Push (lock-screen notifications). None if VAPID setup failed.
    push: Option<Arc<Push>>,
}

#[tokio::main]
async fn main() {
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

    // Dedicated thread owns the (non-Send) input engine; we feed it commands.
    let (tx, rx) = crossbeam_channel::unbounded::<InputCmd>();
    std::thread::spawn(move || input::run(rx));

    // Bridge to tmux for viewing/driving the user's shells.
    let tmux = Tmux::new();
    let (events, _) = broadcast::channel::<String>(64);
    // Bridge to already-open Terminal.app tabs; streams share the events pipe.
    let tabs = Tabs::start(events.clone());

    let web_dir = resolve_web_dir();
    let serve_dir = ServeDir::new(&web_dir).append_index_html_on_directories(true);

    let state = AppState {
        token: token.clone(),
        tx,
        tmux,
        events,
        tabs,
        push: Push::init().map(Arc::new),
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/event", post(event_handler))
        .route("/upload", post(upload_handler))
        .route("/push/key", get(push_key_handler))
        .route("/push/subscribe", post(push_subscribe_handler))
        .route("/manifest.webmanifest", get(manifest_handler))
        .layer(DefaultBodyLimit::max(25 * 1024 * 1024))
        .fallback_service(serve_dir)
        .with_state(state);

    print_banner(&url, &web_dir);

    // `--tunnel`: also expose the agent at a public HTTPS URL via a Cloudflare
    // quick tunnel — works from any network, no VPN on the phone. The session
    // token still gates every connection.
    if std::env::args().any(|a| a == "--tunnel") {
        let token = token.clone();
        tokio::spawn(async move { run_tunnel(port, token).await });
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

    // The hook forwards Claude Code's JSON on stdin; pull out the human text.
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
        .unwrap_or_else(|| match kind {
            "stop" => "Claude finished responding".to_string(),
            _ => "Claude needs your input".to_string(),
        });

    let label = if pane.is_empty() {
        None
    } else {
        let status = if kind == "stop" { "done" } else { "waiting" };
        state.tmux.set_status(&pane, status);
        state.tmux.pane_label(&pane)
    };

    let label = label.unwrap_or_else(|| pane.clone());
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
        tokio::spawn(async move { push.notify_all(&title, &body).await });
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
    match serde_json::from_str::<web_push::SubscriptionInfo>(&body) {
        Ok(sub) => {
            push.subscribe(sub);
            StatusCode::NO_CONTENT
        }
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

/// PWA manifest, served dynamically so the installed app's start_url carries
/// the session token (an installed icon opens pre-authenticated).
async fn manifest_handler(State(state): State<AppState>) -> impl IntoResponse {
    let manifest = serde_json::json!({
        "name": "PointFlow",
        "short_name": "PointFlow",
        "description": "Your phone as trackpad, keyboard and Claude Code remote for your Mac.",
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
        let mut frames = state.tmux.subscribe();
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
    // is tmux control or input that drives the Mac via the engine.
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Binary(b) => state.tmux.write_input(&b),
            Message::Text(t) => {
                if let Ok(m) = serde_json::from_str::<ClientMsg>(&t) {
                    match m {
                        ClientMsg::TmuxList => {
                            let _ = out_tx.send(Message::Text(state.tmux.panes_json()));
                        }
                        ClientMsg::TmuxSelect { id, cols, rows } => {
                            state.tmux.select(&id, cols, rows)
                        }
                        ClientMsg::TmuxResize { cols, rows } => state.tmux.resize(cols, rows),
                        ClientMsg::TmuxKeys { id, hex } => {
                            state.tmux.send_keys_to(&id, &decode_hex(&hex));
                        }
                        ClientMsg::TabList => {
                            let _ = out_tx.send(Message::Text(state.tabs.tabs_json()));
                        }
                        ClientMsg::TabSelect { win, tab } => state.tabs.select(win, tab),
                        ClientMsg::TabStop => state.tabs.stop(),
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

    state.tmux.stop();
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

    let dir = match std::env::var_os("HOME") {
        Some(h) => PathBuf::from(h).join("Downloads").join("PointFlow"),
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
async fn run_tunnel(port: u16, token: String) {
    let bin = ["/opt/homebrew/bin/cloudflared", "/usr/local/bin/cloudflared"]
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
            eprintln!(
                "[pointflow] --tunnel needs cloudflared: brew install cloudflared"
            );
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
                    print_qr(&format!("{url}/?token={token}"));
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
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".pointflow").join("token"))
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

/// Find the built phone UI. Checks POINTFLOW_WEB_DIR, then paths relative to
/// both the binary and the working directory.
fn resolve_web_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("POINTFLOW_WEB_DIR") {
        return PathBuf::from(dir);
    }

    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("phone/out"),
        PathBuf::from("../phone/out"),
        PathBuf::from("../../phone/out"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("phone-ui"));
            candidates.push(dir.join("../phone/out"));
        }
    }

    candidates
        .into_iter()
        .find(|p| p.join("index.html").exists())
        .unwrap_or_else(|| PathBuf::from("phone/out"))
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
    println!("  (Phone and Mac must be on the same WiFi network.)");
}

fn print_banner(url: &str, web_dir: &Path) {
    print_qr(url);

    if !web_dir.join("index.html").exists() {
        eprintln!(
            "\n  ⚠  Phone UI not found at {}\n     Build it first:  (cd phone && pnpm build)\n",
            web_dir.display()
        );
    }
    println!();
}
