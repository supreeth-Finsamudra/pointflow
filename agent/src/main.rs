//! PointFlow desktop agent.
//!
//! Serves the static phone UI over the LAN and accepts a single authenticated
//! WebSocket from a phone, translating its messages into real mouse/keyboard
//! input on this Mac. Requires Accessibility permission.

mod input;
mod protocol;
mod tmux;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;

use input::InputCmd;
use protocol::ClientMsg;
use tmux::Tmux;

/// Default listen port. Override with POINTFLOW_PORT.
const DEFAULT_PORT: u16 = 8742;

#[derive(Clone)]
struct AppState {
    token: String,
    tx: Sender<InputCmd>,
    /// Bridge to the user's tmux panes (list, view, drive).
    tmux: Arc<Tmux>,
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

    // Dedicated thread owns the (non-Send) input engine; we feed it commands.
    let (tx, rx) = crossbeam_channel::unbounded::<InputCmd>();
    std::thread::spawn(move || input::run(rx));

    // Bridge to tmux for viewing/driving the user's shells.
    let tmux = Tmux::new();

    let web_dir = resolve_web_dir();
    let serve_dir = ServeDir::new(&web_dir).append_index_html_on_directories(true);

    let state = AppState {
        token: token.clone(),
        tx,
        tmux,
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(serve_dir)
        .with_state(state);

    print_banner(&url, &web_dir);

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

    // Drain the outgoing queue to the socket.
    let send_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Recv half: binary frames are keystrokes for the selected pane; JSON text
    // is tmux control or input that drives the Mac via the engine.
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Binary(b) => state.tmux.send_keys(&b),
            Message::Text(t) => {
                if let Ok(m) = serde_json::from_str::<ClientMsg>(&t) {
                    match m {
                        ClientMsg::TmuxList => {
                            let _ = out_tx.send(Message::Text(state.tmux.panes_json()));
                        }
                        ClientMsg::TmuxSelect { id } => state.tmux.select(&id),
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
    pump.abort();
    send_task.abort();
    println!("[pointflow] phone disconnected");
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
