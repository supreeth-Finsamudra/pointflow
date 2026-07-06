"use client";

// The on-screen keyboard shrinks the *visual* viewport, not the layout one.
// Sheets track {h, top} so their content stays inside the visible area while
// an opaque backdrop keeps covering the whole screen (no bleed-through of the
// page underneath — the "two input boxes" bug).

import { useEffect, useState } from "react";

export function useVisibleViewport(): { h: number | null; top: number } {
  const [h, setH] = useState<number | null>(null);
  const [top, setTop] = useState(0);

  useEffect(() => {
    const vv = window.visualViewport;
    const update = () => {
      setH(vv ? Math.round(vv.height) : window.innerHeight);
      setTop(vv ? Math.round(vv.offsetTop) : 0);
    };
    update();
    vv?.addEventListener("resize", update);
    vv?.addEventListener("scroll", update);
    window.addEventListener("resize", update);
    return () => {
      vv?.removeEventListener("resize", update);
      vv?.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, []);

  return { h, top };
}

/** Session token from the pairing URL, persisted so reloads and the
 *  installed-PWA launch (whose URL may lose the query) stay authenticated. */
export function getToken(): string {
  if (typeof window === "undefined") return "";
  const fromUrl = new URLSearchParams(window.location.search).get("token");
  try {
    if (fromUrl) {
      localStorage.setItem("pf.token", fromUrl);
      return fromUrl;
    }
    return localStorage.getItem("pf.token") ?? "";
  } catch {
    return fromUrl ?? "";
  }
}
