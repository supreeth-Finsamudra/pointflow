//! Live mirror of the user's frontmost terminal window.
//!
//! A background thread grabs the active terminal window, JPEG-encodes it, and
//! fans the frames out to attached phones — but only while at least one phone
//! is actually viewing (tracked by `viewers`), so we don't capture/encode for
//! nothing. Typing is *not* handled here: the phone drives the real terminal
//! through the existing input-injection path (`input.rs`), exactly as the
//! trackpad does. This just provides the picture.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageEncoder, RgbImage};
use tokio::sync::broadcast;
use xcap::Window;

/// Cap the streamed width; terminal text stays readable and frames stay small.
const MAX_WIDTH: u32 = 1280;
const JPEG_QUALITY: u8 = 70;
/// ~10 fps while someone is watching.
const FRAME_INTERVAL: Duration = Duration::from_millis(100);
/// Idle poll when nobody is viewing.
const IDLE_POLL: Duration = Duration::from_millis(150);
/// Buffered frames before a slow phone is dropped to the latest.
const BROADCAST_CAP: usize = 4;

pub struct Mirror {
    frame_tx: broadcast::Sender<Vec<u8>>,
    viewers: AtomicUsize,
    /// App name of the most recently captured window, for `focus()`.
    last_app: Mutex<Option<String>>,
}

impl Mirror {
    /// Spawn the capture thread and return a handle. Capture is gated on
    /// `viewers > 0`, so this idles cheaply until a phone opens the mirror.
    pub fn start() -> Arc<Mirror> {
        let (frame_tx, _) = broadcast::channel(BROADCAST_CAP);
        let mirror = Arc::new(Mirror {
            frame_tx,
            viewers: AtomicUsize::new(0),
            last_app: Mutex::new(None),
        });
        {
            let mirror = mirror.clone();
            std::thread::spawn(move || mirror.capture_loop());
        }
        mirror
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.frame_tx.subscribe()
    }

    pub fn add_viewer(&self) {
        self.viewers.fetch_add(1, Ordering::SeqCst);
    }

    pub fn remove_viewer(&self) {
        let _ = self
            .viewers
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                Some(v.saturating_sub(1))
            });
    }

    /// Bring the mirrored terminal app to the front so injected keystrokes
    /// land in it. Best-effort (AppleScript); silently ignored if it fails.
    pub fn focus(&self) {
        let app = self.last_app.lock().ok().and_then(|g| g.clone());
        let Some(app) = app else { return };
        // xcap reports iTerm as "iTerm2"; AppleScript wants "iTerm".
        let app = if app.to_lowercase().contains("iterm") {
            "iTerm".to_string()
        } else {
            app
        };
        let _ = std::process::Command::new("osascript")
            .args(["-e", &format!("tell application \"{app}\" to activate")])
            .output();
    }

    fn capture_loop(&self) {
        let mut warned = false;
        loop {
            if self.viewers.load(Ordering::SeqCst) == 0 {
                std::thread::sleep(IDLE_POLL);
                continue;
            }
            match self.capture_frame() {
                Ok(Some(jpeg)) => {
                    warned = false;
                    let _ = self.frame_tx.send(jpeg);
                }
                Ok(None) => { /* no terminal window found right now */ }
                Err(e) => {
                    if !warned {
                        eprintln!(
                            "[pointflow] screen capture failed ({e}).\n  Grant Screen Recording to your terminal: System Settings → Privacy &\n  Security → Screen Recording → enable it, then restart the agent."
                        );
                        warned = true;
                    }
                }
            }
            std::thread::sleep(FRAME_INTERVAL);
        }
    }

    fn capture_frame(&self) -> Result<Option<Vec<u8>>, String> {
        let target = match pick_terminal_window().map_err(|e| e.to_string())? {
            Some(w) => w,
            None => return Ok(None),
        };
        if let (Ok(app), Ok(mut last)) = (target.app_name(), self.last_app.lock()) {
            *last = Some(app);
        }

        let rgba = target.capture_image().map_err(|e| e.to_string())?;
        let rgb = image::DynamicImage::ImageRgba8(rgba).to_rgb8();
        let rgb = downscale(rgb, MAX_WIDTH);
        let (w, h) = (rgb.width(), rgb.height());

        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
            .write_image(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
        Ok(Some(out))
    }
}

/// Choose which window to mirror: the focused terminal if there is one, else
/// the front-most terminal; failing that, fall back to the focused/front-most
/// window of any app so the phone still shows something useful.
fn pick_terminal_window() -> xcap::XCapResult<Option<Window>> {
    let mut terminals: Vec<Window> = Vec::new();
    let mut others: Vec<Window> = Vec::new();

    for w in Window::all()? {
        if w.is_minimized().unwrap_or(false) {
            continue;
        }
        if w.width().unwrap_or(0) < 80 || w.height().unwrap_or(0) < 60 {
            continue; // skip tiny utility/menubar windows
        }
        let app = w.app_name().unwrap_or_default().to_lowercase();
        if is_terminal_app(&app) {
            terminals.push(w);
        } else {
            others.push(w);
        }
    }

    let pool = if !terminals.is_empty() { terminals } else { others };
    Ok(front_or_focused(pool))
}

/// Prefer a focused window; otherwise the highest z-order (front-most).
fn front_or_focused(pool: Vec<Window>) -> Option<Window> {
    let mut best: Option<Window> = None;
    let mut best_z = i32::MIN;
    for w in pool {
        if w.is_focused().unwrap_or(false) {
            return Some(w);
        }
        let z = w.z().unwrap_or(i32::MIN);
        if z >= best_z {
            best_z = z;
            best = Some(w);
        }
    }
    best
}

fn is_terminal_app(app_lower: &str) -> bool {
    const APPS: &[&str] = &[
        "alacritty", "kitty", "ghostty", "hyper", "warp", "tabby", "rio",
    ];
    app_lower.contains("term") || APPS.iter().any(|a| app_lower.contains(a))
}

fn downscale(img: RgbImage, max_w: u32) -> RgbImage {
    let w = img.width();
    if w <= max_w {
        return img;
    }
    let h = img.height();
    let nh = ((h as u64 * max_w as u64) / w as u64).max(1) as u32;
    image::imageops::resize(&img, max_w, nh, image::imageops::FilterType::Triangle)
}
