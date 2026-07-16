//! Web Push: real lock-screen notifications for Copilot events.
//!
//! The phone (installed as a PWA over the HTTPS tunnel) subscribes via
//! `/push/key` + `/push/subscribe`; every Claude Code hook event is then also
//! delivered as an encrypted Web Push message, so the phone buzzes even with
//! the app closed. The VAPID key is generated once in pure Rust (p256, SEC1
//! PEM) and kept in `~/.pointflow/vapid.pem`; subscriptions live in
//! `~/.pointflow/push.json`.
//!
//! Windows: the `web-push` crate's crypto (ece) requires openssl, which is not
//! sanely buildable for Windows targets, so push is a disabled stub there for
//! now — same API, always off, reason printed once. Tracked in
//! docs/ROADMAP.md Phase 4 (move to a RustCrypto web-push implementation).

#[cfg(not(windows))]
use std::path::PathBuf;

#[cfg(not(windows))]
use crate::util::state_dir;

#[cfg(not(windows))]
mod real {
    use std::sync::Mutex;

    use web_push::{
        ContentEncoding, IsahcWebPushClient, SubscriptionInfo, VapidSignatureBuilder,
        WebPushClient, WebPushError, WebPushMessageBuilder,
    };

    use super::{load_or_create_vapid, load_subs, save_subs};

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
            Some(super::b64url(&partial.get_public_key()))
        }

        /// Parse and store a browser PushSubscription (standard JSON shape).
        /// Returns false when the body isn't a valid subscription.
        pub fn subscribe_json(&self, body: &str) -> bool {
            let Ok(sub) = serde_json::from_str::<SubscriptionInfo>(body) else {
                return false;
            };
            let mut subs = self.subs.lock().unwrap();
            subs.retain(|s| s.endpoint != sub.endpoint);
            subs.push(sub);
            save_subs(&subs);
            println!("[pointflow] push: device subscribed ({} total)", subs.len());
            true
        }

        /// Fire a notification to every subscribed device. Expired subscriptions
        /// are pruned. Failures are logged, never fatal. `url`, when given, is
        /// opened on tap (the "back online after a restart" reconnect path —
        /// it may be a brand-new tunnel origin).
        pub async fn notify_all(&self, title: &str, body: &str, url: Option<&str>) {
            let subs = self.subs.lock().unwrap().clone();
            if subs.is_empty() {
                return;
            }
            let mut payload = serde_json::json!({ "title": title, "body": body });
            if let Some(u) = url {
                payload["url"] = serde_json::Value::String(u.to_string());
            }
            let payload = payload.to_string();

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
}

#[cfg(windows)]
mod real {
    /// Disabled push backend (see module docs). Same surface as the real one
    /// so `main.rs` is platform-agnostic; `init` declines, so the handlers
    /// answer 503 and the phone's 🔔 stays in its unsupported state.
    pub struct Push;

    impl Push {
        pub fn init() -> Option<Push> {
            eprintln!(
                "[pointflow] push notifications are not yet supported on Windows \
                 (see docs/ROADMAP.md)"
            );
            None
        }

        pub fn public_key(&self) -> Option<String> {
            None
        }

        pub fn subscribe_json(&self, _body: &str) -> bool {
            false
        }

        pub async fn notify_all(&self, _title: &str, _body: &str, _url: Option<&str>) {}
    }
}

pub use real::Push;

/// P-256 VAPID key in SEC1 PEM ("EC PRIVATE KEY"), generated once in pure
/// Rust — byte-compatible with what `openssl ecparam -genkey` produced, so
/// existing `vapid.pem` files (and their live browser subscriptions) keep
/// working. Note: web-push's `sec1_decode` requires the named-curve OID and
/// public-key blocks openssl emits (p256's own `to_sec1_pem` omits the OID),
/// so the DER is assembled explicitly below.
#[cfg(not(windows))]
fn load_or_create_vapid() -> Option<String> {
    let path = state_dir()?.join("vapid.pem");
    if let Ok(pem) = std::fs::read_to_string(&path) {
        return Some(pem);
    }
    let key = p256::SecretKey::random(&mut rand::rngs::OsRng);
    let pem = sec1_pem(&key);
    let _ = std::fs::create_dir_all(state_dir()?);
    if std::fs::write(&path, &pem).is_err() {
        eprintln!("[pointflow] push: could not persist VAPID key");
    }
    Some(pem)
}

/// SEC1 `ECPrivateKey` DER for P-256, openssl-shaped, wrapped as PEM:
/// SEQUENCE { INTEGER 1, OCTET STRING scalar,
///            [0] OID prime256v1, [1] BIT STRING uncompressed-pubkey }.
/// All lengths are fixed for this curve, so the encoding is deterministic.
#[cfg(not(windows))]
fn sec1_pem(key: &p256::SecretKey) -> String {
    use p256::elliptic_curve::sec1::ToEncodedPoint;

    let scalar = key.to_bytes(); // 32 bytes
    let public = key.public_key().to_encoded_point(false); // 65 bytes, 0x04-led

    let mut der: Vec<u8> = Vec::with_capacity(121);
    der.extend_from_slice(&[0x30, 0x77]); // SEQUENCE, 119 bytes
    der.extend_from_slice(&[0x02, 0x01, 0x01]); // INTEGER 1
    der.extend_from_slice(&[0x04, 0x20]); // OCTET STRING, 32 bytes
    der.extend_from_slice(&scalar);
    // [0] { OID 1.2.840.10045.3.1.7 (prime256v1) }
    der.extend_from_slice(&[
        0xa0, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
    ]);
    // [1] { BIT STRING, no unused bits, 65-byte point }
    der.extend_from_slice(&[0xa1, 0x44, 0x03, 0x42, 0x00]);
    der.extend_from_slice(public.as_bytes());

    let b64 = b64std(&der);
    let mut pem = String::from("-----BEGIN EC PRIVATE KEY-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).unwrap());
        pem.push('\n');
    }
    pem.push_str("-----END EC PRIVATE KEY-----\n");
    pem
}

/// Standard base64 with padding (PEM body).
#[cfg(not(windows))]
fn b64std(bytes: &[u8]) -> String {
    const CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        out.push(CHARS[(b[0] >> 2) as usize] as char);
        out.push(CHARS[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[(((b[1] & 0x0f) << 2) | (b[2] >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(b[2] & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(not(windows))]
fn subs_path() -> Option<PathBuf> {
    state_dir().map(|d| d.join("push.json"))
}

#[cfg(not(windows))]
fn load_subs() -> Vec<web_push::SubscriptionInfo> {
    subs_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(not(windows))]
fn save_subs(subs: &[web_push::SubscriptionInfo]) {
    if let (Some(path), Ok(json)) = (subs_path(), serde_json::to_string_pretty(subs)) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(not(windows))]
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
