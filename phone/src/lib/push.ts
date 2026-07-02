"use client";

// Web Push client plumbing. Push requires a secure context (the HTTPS tunnel)
// and, on iOS, the app installed to the home screen (16.4+). The flow:
// register the SW → ask permission (user gesture) → subscribe with the
// agent's VAPID key → hand the subscription to the agent.

import { getToken } from "./useViewport";

export type PushState =
  | "unsupported" // no SW/Push API or insecure context
  | "needs-install" // iOS Safari tab — must Add to Home Screen first
  | "off" // supported, not yet enabled
  | "on" // subscribed
  | "denied"; // user blocked notifications

export function isStandalone(): boolean {
  return (
    window.matchMedia("(display-mode: standalone)").matches ||
    // iOS Safari's non-standard flag
    (navigator as unknown as { standalone?: boolean }).standalone === true
  );
}

export async function registerSW(): Promise<ServiceWorkerRegistration | null> {
  if (!("serviceWorker" in navigator)) return null;
  try {
    return await navigator.serviceWorker.register("/sw.js");
  } catch {
    return null;
  }
}

export async function getPushState(): Promise<PushState> {
  if (
    !("serviceWorker" in navigator) ||
    !("PushManager" in window) ||
    !window.isSecureContext
  ) {
    // iOS Safari (not installed) hides PushManager entirely.
    const ios = /iphone|ipad|ipod/i.test(navigator.userAgent);
    if (ios && !isStandalone() && window.isSecureContext) return "needs-install";
    return "unsupported";
  }
  if (Notification.permission === "denied") return "denied";
  const reg = await navigator.serviceWorker.getRegistration();
  const sub = await reg?.pushManager.getSubscription();
  return sub ? "on" : "off";
}

/** Enable push end-to-end. Must be called from a user gesture. */
export async function enablePush(): Promise<PushState> {
  const reg = (await navigator.serviceWorker.getRegistration()) ?? (await registerSW());
  if (!reg) return "unsupported";

  const perm = await Notification.requestPermission();
  if (perm !== "granted") return perm === "denied" ? "denied" : "off";

  const keyRes = await fetch(`/push/key?token=${encodeURIComponent(getToken())}`);
  if (!keyRes.ok) return "off";
  const { key } = await keyRes.json();

  const sub = await reg.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: b64urlToBytes(key),
  });

  const ok = await fetch(`/push/subscribe?token=${encodeURIComponent(getToken())}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(sub.toJSON()),
  });
  return ok.ok ? "on" : "off";
}

function b64urlToBytes(s: string): ArrayBuffer {
  const pad = "=".repeat((4 - (s.length % 4)) % 4);
  const b64 = (s + pad).replace(/-/g, "+").replace(/_/g, "/");
  const raw = atob(b64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out.buffer;
}
