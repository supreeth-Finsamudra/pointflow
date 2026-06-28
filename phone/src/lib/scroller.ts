// Inertial scroller shared by the trackpad (two-finger) and the scroll strip.
//
// Callers feed it logical deltas where **positive dy = scroll down**. It emits
// scroll messages immediately while the finger moves, tracks velocity, and on
// release coasts with exponential friction until it falls below a threshold.

import { Send, msg } from "./protocol";

// Flip if a platform scrolls the wrong way for everything at once.
const OUT_SIGN = -1;
const FRICTION = 0.94; // per-frame velocity decay during momentum
const MIN_VELOCITY = 0.02; // px/ms — below this, momentum stops
const RELEASE_MIN = 0.05; // px/ms — minimum flick speed to start momentum

export type ScrollOpts = { speed: number; momentum: boolean };

export type Scroller = {
  begin: () => void;
  move: (dx: number, dy: number, t: number) => void;
  release: () => void;
  cancel: () => void;
};

export function createScroller(send: Send, getOpts: () => ScrollOpts): Scroller {
  let raf = 0;
  let vx = 0; // velocity in output px/ms (smoothed)
  let vy = 0;
  let lastT = 0;
  let moved = false;

  const stopRaf = () => {
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
  };

  const emit = (dx: number, dy: number) => {
    if (dx !== 0 || dy !== 0) send(msg.scroll(dx * OUT_SIGN, dy * OUT_SIGN));
  };

  const begin = () => {
    stopRaf();
    vx = 0;
    vy = 0;
    lastT = 0;
    moved = false;
  };

  const move = (dx: number, dy: number, t: number) => {
    stopRaf(); // a manual scroll cancels any in-flight momentum
    const { speed } = getOpts();
    const ox = dx * speed;
    const oy = dy * speed;
    emit(ox, oy);
    if (lastT) {
      const dt = Math.max(1, t - lastT);
      vx = 0.7 * (ox / dt) + 0.3 * vx;
      vy = 0.7 * (oy / dt) + 0.3 * vy;
    }
    lastT = t;
    moved = true;
  };

  const release = () => {
    const { momentum } = getOpts();
    lastT = 0;
    if (!momentum || !moved || Math.hypot(vx, vy) < RELEASE_MIN) {
      vx = 0;
      vy = 0;
      moved = false;
      return;
    }
    moved = false;
    let last = 0;
    const step = (ts: number) => {
      if (!last) last = ts;
      const dt = Math.min(32, ts - last);
      last = ts;
      emit(vx * dt, vy * dt);
      vx *= FRICTION;
      vy *= FRICTION;
      if (Math.hypot(vx, vy) < MIN_VELOCITY) {
        stopRaf();
        return;
      }
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
  };

  const cancel = () => {
    stopRaf();
    vx = 0;
    vy = 0;
    lastT = 0;
    moved = false;
  };

  return { begin, move, release, cancel };
}
