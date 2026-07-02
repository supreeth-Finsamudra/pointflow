"use client";

// Prompt history for the compose box (the terminal's own ↑ recall fills the
// *terminal's* input, which we can't read back — so the app keeps its own).
// Newest first, deduped, capped, persisted.

const KEY = "pf.hist";
const CAP = 50;

export function loadHistory(): string[] {
  try {
    const raw = localStorage.getItem(KEY);
    const arr = raw ? JSON.parse(raw) : [];
    return Array.isArray(arr) ? arr : [];
  } catch {
    return [];
  }
}

export function pushHistory(entry: string) {
  const text = entry.trim();
  if (!text) return;
  try {
    const hist = loadHistory().filter((h) => h !== text);
    hist.unshift(text);
    localStorage.setItem(KEY, JSON.stringify(hist.slice(0, CAP)));
  } catch {
    /* private mode */
  }
}
