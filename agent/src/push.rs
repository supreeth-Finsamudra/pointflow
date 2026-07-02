//! Web Push: real lock-screen notifications for Copilot events.
//!
//! The phone (installed as a PWA over the HTTPS tunnel) subscribes via
//! `/push/key` + `/push/subscribe`; every Claude Code hook event is then also
//! delivered as an encrypted Web Push message, so the phone buzzes even with
//! the app closed. VAPID keys are generated once (openssl, P-256) and kept in
//! `~/.pointflow/vapid.pem`; subscriptions live in `~/.pointflow/push.json`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use web_push::{
    ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder,
    WebPushClient, WebPushError, WebPushMessageBuilder,
};

pub struct Push {
    pem: String,
    subs: Mutex<Vec<SubscriptionInfo>>,
}

impl Push {
    pub fn init() -> Option<Push> {
        let pem = load_or_create_vapid()?;
        let subs = load_subs();
        Some(Push {
            pem,
            subs: Mutex::new(subs),
        })
    }

    /// The applicationServerKey the browser needs, base64url (no padding).
    pub fn public_key(&self) -> Option<String> {
        let partial =
            VapidSignatureBuilder::from_pem_no_sub(self.pem.as_bytes()).ok()?;
        Some(b64url(&partial.get_public_key()))
    }

    /// Store a browser subscription (deduped by endpoint).
    pub fn subscribe(&self, sub: SubscriptionInfo) {
        let mut subs = self.subs.lock().unwrap();
        subs.retain(|s| s.endpoint != sub.endpoint);
        subs.push(sub);
        save_subs(&subs);
        println!("[pointflow] push: device subscribed ({} total)", subs.len());
    }

    /// Fire a notification to every subscribed device. Expired subscriptions
    /// are pruned. Failures are logged, never fatal.
    pub async fn notify_all(&self, title: &str, body: &str) {
        let subs = self.subs.lock().unwrap().clone();
        if subs.is_empty() {
            return;
        }
        let payload =
            serde_json::json!({ "title": title, "body": body }).to_string();

        let client = match IsahcWebPushClient::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[pointflow] push client failed: {e}");
                return;
            }
        };

        let mut dead: Vec<String> = Vec::new();
        for sub in &subs {
            let sig = match VapidSignatureBuilder::from_pem(
                self.pem.as_bytes(),
                sub,
            )
            .and_then(|mut b| {
                // Apple validates this contact claim; localhost mailto gets a
                // 403 BadJwtToken. A real https URI passes.
                b.add_claim("sub", "https://github.com/supreeth-Finsamudra/pointflow");
                b.build()
            }) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[pointflow] push vapid failed: {e}");
                    continue;
                }
            };

            let mut msg = WebPushMessageBuilder::new(sub);
            msg.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
            msg.set_vapid_signature(sig);
            msg.set_ttl(3600);
            let msg = match msg.build() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[pointflow] push build failed: {e}");
                    continue;
                }
            };

            match client.send(msg).await {
                Ok(()) => {}
                Err(WebPushError::EndpointNotFound(_))
                | Err(WebPushError::EndpointNotValid(_)) => {
                    dead.push(sub.endpoint.clone());
                }
                Err(e) => eprintln!("[pointflow] push send failed: {e}"),
            }
        }

        if !dead.is_empty() {
            let mut subs = self.subs.lock().unwrap();
            subs.retain(|s| !dead.contains(&s.endpoint));
            save_subs(&subs);
        }
    }
}

fn dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".pointflow"))
}

/// P-256 VAPID key, generated once with the system openssl.
fn load_or_create_vapid() -> Option<String> {
    let path = dir()?.join("vapid.pem");
    if let Ok(pem) = std::fs::read_to_string(&path) {
        return Some(pem);
    }
    let out = Command::new("/usr/bin/openssl")
        .args(["ecparam", "-genkey", "-name", "prime256v1", "-noout"])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.is_empty() {
        eprintln!("[pointflow] push disabled: could not generate VAPID key");
        return None;
    }
    let pem = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = std::fs::create_dir_all(dir()?);
    if std::fs::write(&path, &pem).is_err() {
        eprintln!("[pointflow] push: could not persist VAPID key");
    }
    Some(pem)
}

fn subs_path() -> Option<PathBuf> {
    dir().map(|d| d.join("push.json"))
}

fn load_subs() -> Vec<SubscriptionInfo> {
    subs_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_subs(subs: &[SubscriptionInfo]) {
    if let (Some(path), Ok(json)) = (subs_path(), serde_json::to_string_pretty(subs)) {
        let _ = std::fs::write(path, json);
    }
}

fn b64url(bytes: &[u8]) -> String {
    // base64url without padding, per the applicationServerKey convention.
    const CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        out.push(CHARS[(b[0] >> 2) as usize] as char);
        out.push(CHARS[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[(((b[1] & 0x0f) << 2) | (b[2] >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(CHARS[(b[2] & 0x3f) as usize] as char);
        }
    }
    out
}
