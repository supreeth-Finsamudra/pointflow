"use client";

// User-tunable touch settings, persisted to localStorage.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
} from "react";

export type Settings = {
  pointerSpeed: number; // base cursor gain
  acceleration: number; // extra gain per px/frame of speed
  scrollSpeed: number; // scroll multiplier
  naturalScroll: boolean; // two-finger direction (content follows fingers)
  momentum: boolean; // inertial scrolling
  tapToClick: boolean; // tap the pad to click
  doubleClickMs: number; // max gap between taps for double-tap-drag
};

export const DEFAULTS: Settings = {
  pointerSpeed: 1.7,
  acceleration: 0.35,
  scrollSpeed: 1.0,
  naturalScroll: true,
  momentum: true,
  tapToClick: true,
  doubleClickMs: 300,
};

const STORAGE_KEY = "pointflow.settings.v1";

type Ctx = {
  settings: Settings;
  update: (patch: Partial<Settings>) => void;
  reset: () => void;
};

const SettingsContext = createContext<Ctx>({
  settings: DEFAULTS,
  update: () => {},
  reset: () => {},
});

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [settings, setSettings] = useState<Settings>(DEFAULTS);

  // Load once on mount (client-only; avoids hydration mismatch).
  useEffect(() => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      // Load persisted settings after mount to avoid an SSR/hydration mismatch.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      if (raw) setSettings({ ...DEFAULTS, ...JSON.parse(raw) });
    } catch {
      /* ignore corrupt/unavailable storage */
    }
  }, []);

  const update = useCallback((patch: Partial<Settings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        /* ignore */
      }
      return next;
    });
  }, []);

  const reset = useCallback(() => {
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      /* ignore */
    }
    setSettings(DEFAULTS);
  }, []);

  return (
    <SettingsContext.Provider value={{ settings, update, reset }}>
      {children}
    </SettingsContext.Provider>
  );
}

export const useSettings = () => useContext(SettingsContext);
