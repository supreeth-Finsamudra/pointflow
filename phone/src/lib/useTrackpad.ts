"use client";

// The trackpad gesture engine.
//
// Recognizes the full vocabulary from one touch surface:
//   1-finger tap            → left click
//   1-finger double-tap     → two clicks (macOS reads as double-click)
//   double-tap + drag       → press-hold drag from the 2nd tap (word select)
//   1-finger hold → drag    → press-hold drag (text select)
//   1-finger hold → lift    → right click
//   2-finger swipe          → scroll (+ momentum)
//   2-finger tap            → right click
//   3-finger swipe          → Mission Control / App Exposé / switch spaces
//
// Cursor moves are accumulated and flushed once per animation frame, with
// speed-based acceleration applied to the per-frame delta.

import { useEffect, useRef } from "react";
import { Send, msg } from "./protocol";
import { createScroller } from "./scroller";
import type { Settings } from "./settings";

const TAP_MS = 250; // max duration for a tap
const TAP_SLOP = 8; // max movement (px) to still count as a tap
const HOLD_MS = 450; // hold-still duration that arms drag / right-click
const DRAG_SLOP = 5; // movement after arming that commits to a drag
const TWO_TAP_MS = 250; // max duration for a two-finger tap
const TWO_SLOP = 10; // max two-finger movement to still count as a tap
const THREE_SWIPE = 55; // centroid travel (px) to trigger a 3-finger swipe
const DOUBLE_SLOP = 28; // max distance between taps to chain a double-tap

function centroid(touches: TouchList) {
  let x = 0;
  let y = 0;
  for (let i = 0; i < touches.length; i++) {
    x += touches[i].clientX;
    y += touches[i].clientY;
  }
  return { x: x / touches.length, y: y / touches.length };
}

export function useTrackpad(send: Send, settings: Settings) {
  const ref = useRef<HTMLDivElement>(null);
  const settingsRef = useRef(settings);
  // Keep the latest settings reachable from the long-lived touch handlers
  // without re-binding them on every settings change.
  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const scroller = createScroller(send, () => ({
      speed: settingsRef.current.scrollSpeed,
      momentum: settingsRef.current.momentum,
    }));

    const g = {
      // single-finger
      active: false,
      id: -1,
      x: 0,
      y: 0,
      startX: 0,
      startY: 0,
      startT: 0,
      moved: false,
      armed: false,
      dragging: false,
      secondTap: false,
      holdTimer: undefined as ReturnType<typeof setTimeout> | undefined,
      // double-tap chaining
      lastTapT: 0,
      lastTapX: 0,
      lastTapY: 0,
      // multi-finger
      mode: "none" as "none" | "two" | "three",
      suppress: false, // a multi-touch happened → no single-finger click
      twoStartT: 0,
      twoMoved: false,
      twoX: 0, // last centroid
      twoY: 0,
      twoSX: 0, // start centroid (for tap detection)
      twoSY: 0,
      threeStartX: 0,
      threeStartY: 0,
      threeFired: false,
      // rAF batching
      pdx: 0,
      pdy: 0,
      raf: 0,
    };

    const clearHold = () => {
      if (g.holdTimer) clearTimeout(g.holdTimer);
      g.holdTimer = undefined;
    };

    const flushMove = () => {
      g.raf = 0;
      if (g.pdx === 0 && g.pdy === 0) return;
      const { pointerSpeed, acceleration } = settingsRef.current;
      const speed = Math.hypot(g.pdx, g.pdy); // px this frame ~ velocity
      const gain = pointerSpeed + acceleration * speed;
      send(msg.move(g.pdx * gain, g.pdy * gain));
      g.pdx = 0;
      g.pdy = 0;
    };

    const queueMove = (dx: number, dy: number) => {
      g.pdx += dx;
      g.pdy += dy;
      if (!g.raf) g.raf = requestAnimationFrame(flushMove);
    };

    const startDrag = () => {
      g.dragging = true;
      send(msg.down("left"));
      navigator.vibrate?.(15);
    };

    const buzz = (ms: number) => navigator.vibrate?.(ms);

    // ---- touch start ----
    const onStart = (e: TouchEvent) => {
      e.preventDefault();
      const n = e.touches.length;

      if (n >= 3) {
        // Three fingers → spatial navigation.
        clearHold();
        g.active = false;
        g.suppress = true;
        g.mode = "three";
        const c = centroid(e.touches);
        g.threeStartX = c.x;
        g.threeStartY = c.y;
        g.threeFired = false;
        return;
      }

      if (n === 2) {
        // Two fingers → scroll / two-finger tap.
        clearHold();
        g.active = false;
        g.suppress = true;
        g.mode = "two";
        const c = centroid(e.touches);
        g.twoX = c.x;
        g.twoY = c.y;
        g.twoSX = c.x;
        g.twoSY = c.y;
        g.twoStartT = e.timeStamp;
        g.twoMoved = false;
        scroller.begin();
        return;
      }

      // Single finger.
      scroller.cancel();
      const t = e.changedTouches[0];
      const now = e.timeStamp;
      g.active = true;
      g.mode = "none";
      g.suppress = false;
      g.id = t.identifier;
      g.x = t.clientX;
      g.y = t.clientY;
      g.startX = t.clientX;
      g.startY = t.clientY;
      g.startT = now;
      g.moved = false;
      g.armed = false;
      g.dragging = false;

      // Is this the second tap of a double-tap (→ drag-on-move)?
      g.secondTap =
        now - g.lastTapT < settingsRef.current.doubleClickMs &&
        Math.hypot(t.clientX - g.lastTapX, t.clientY - g.lastTapY) < DOUBLE_SLOP;

      // First taps arm hold→drag/right-click; second taps drag on move instead.
      clearHold();
      if (!g.secondTap) {
        g.holdTimer = setTimeout(() => {
          if (g.active && !g.moved) {
            g.armed = true;
            buzz(18);
          }
        }, HOLD_MS);
      }
    };

    // ---- touch move ----
    const onMove = (e: TouchEvent) => {
      e.preventDefault();

      if (g.mode === "three") {
        if (e.touches.length < 3 || g.threeFired) return;
        const c = centroid(e.touches);
        const dx = c.x - g.threeStartX;
        const dy = c.y - g.threeStartY;
        if (Math.max(Math.abs(dx), Math.abs(dy)) < THREE_SWIPE) return;
        g.threeFired = true;
        buzz(25);
        if (Math.abs(dx) > Math.abs(dy)) {
          // swipe left → next space, swipe right → previous space
          send(msg.chord(["ctrl"], dx < 0 ? "right" : "left"));
        } else {
          // up → Mission Control, down → App Exposé
          send(msg.chord(["ctrl"], dy < 0 ? "up" : "down"));
        }
        return;
      }

      if (g.mode === "two") {
        if (e.touches.length < 2) return;
        const c = centroid(e.touches);
        const dx = c.x - g.twoX;
        const dy = c.y - g.twoY;
        g.twoX = c.x;
        g.twoY = c.y;
        if (!g.twoMoved && Math.hypot(c.x - g.twoSX, c.y - g.twoSY) > TWO_SLOP) {
          g.twoMoved = true;
        }
        // Natural scroll: content follows fingers (down = scroll up).
        const natural = settingsRef.current.naturalScroll;
        scroller.move(natural ? -dx : dx, natural ? -dy : dy, e.timeStamp);
        return;
      }

      if (!g.active) return;
      const t = Array.from(e.changedTouches).find((c) => c.identifier === g.id);
      if (!t) return;
      const dx = t.clientX - g.x;
      const dy = t.clientY - g.y;
      g.x = t.clientX;
      g.y = t.clientY;
      const dist = Math.hypot(t.clientX - g.startX, t.clientY - g.startY);

      if (dist > TAP_SLOP && !g.moved) {
        g.moved = true;
        if (!g.armed) clearHold();
      }

      // Commit to a drag: from an armed hold, or a second-tap drag.
      if (!g.dragging && g.moved && (g.armed || g.secondTap) && dist > DRAG_SLOP) {
        startDrag();
      }

      queueMove(dx, dy);
    };

    // ---- touch end / cancel ----
    const onEnd = (e: TouchEvent) => {
      e.preventDefault();
      const remaining = e.touches.length;

      // Multi-finger gesture winding down.
      if (g.mode === "two") {
        const dt = e.timeStamp - g.twoStartT;
        if (!g.twoMoved && dt < TWO_TAP_MS) {
          send(msg.click("right"));
        } else {
          scroller.release();
        }
        if (remaining === 0) {
          g.mode = "none";
          g.suppress = false;
        }
        return;
      }
      if (g.mode === "three") {
        if (remaining === 0) {
          g.mode = "none";
          g.suppress = false;
        }
        return;
      }

      if (!g.active) return;
      clearHold();
      const dt = e.timeStamp - g.startT;

      if (g.dragging) {
        send(msg.up("left"));
      } else if (g.armed) {
        send(msg.click("right"));
      } else if (
        !g.suppress &&
        !g.moved &&
        dt < TAP_MS &&
        settingsRef.current.tapToClick
      ) {
        send(msg.click("left"));
        g.lastTapT = e.timeStamp;
        g.lastTapX = g.startX;
        g.lastTapY = g.startY;
      }

      g.active = false;
      g.armed = false;
      g.dragging = false;
      g.secondTap = false;
    };

    el.addEventListener("touchstart", onStart, { passive: false });
    el.addEventListener("touchmove", onMove, { passive: false });
    el.addEventListener("touchend", onEnd, { passive: false });
    el.addEventListener("touchcancel", onEnd, { passive: false });
    return () => {
      el.removeEventListener("touchstart", onStart);
      el.removeEventListener("touchmove", onMove);
      el.removeEventListener("touchend", onEnd);
      el.removeEventListener("touchcancel", onEnd);
      clearHold();
      scroller.cancel();
      if (g.raf) cancelAnimationFrame(g.raf);
    };
  }, [send]);

  return ref;
}
