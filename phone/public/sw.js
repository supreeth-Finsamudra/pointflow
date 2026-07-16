/* PointFlow service worker: receives Web Push (Copilot events) and opens the
 * app on tap. Kept minimal on purpose — no offline caching yet, so a plain
 * reload always gets the newest UI. */

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));

self.addEventListener("push", (e) => {
  let data = {};
  try {
    data = e.data ? e.data.json() : {};
  } catch {
    data = { body: e.data && e.data.text() };
  }
  e.waitUntil(
    self.registration.showNotification(data.title || "PointFlow", {
      body: data.body || "",
      icon: "/icon-192.png",
      badge: "/icon-192.png",
      tag: "pointflow",
      renotify: true,
      data,
    }),
  );
});

self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  // "Back online" pushes carry the agent's current URL — after a reboot the
  // tunnel hostname is brand new, so open it instead of the (dead) old app.
  const url = e.notification.data && e.notification.data.url;
  e.waitUntil(
    url
      ? self.clients.openWindow(url)
      : self.clients
          .matchAll({ type: "window", includeUncontrolled: true })
          .then((list) => {
            for (const c of list) {
              if ("focus" in c) return c.focus();
            }
            return self.clients.openWindow("/");
          }),
  );
});
