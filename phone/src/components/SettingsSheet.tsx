"use client";

import { Settings, useSettings } from "../lib/settings";

function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="flex justify-between text-sm text-white/70">
        {label}
        <span className="tabular-nums text-white/40">{value.toFixed(2)}</span>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        className="w-full accent-emerald-400"
      />
    </label>
  );
}

function Toggle({
  label,
  value,
  onChange,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onChange(!value)}
      className="flex items-center justify-between rounded-xl border border-white/10 bg-white/5 px-4 py-3 text-left text-sm"
    >
      <span className="text-white/80">{label}</span>
      <span
        className={`relative h-6 w-10 rounded-full transition-colors ${
          value ? "bg-emerald-500/70" : "bg-white/15"
        }`}
      >
        <span
          className={`absolute top-0.5 h-5 w-5 rounded-full bg-white transition-transform ${
            value ? "translate-x-4" : "translate-x-0.5"
          }`}
        />
      </span>
    </button>
  );
}

export function SettingsSheet({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const { settings, update, reset } = useSettings();
  if (!open) return null;

  const set = (patch: Partial<Settings>) => update(patch);

  return (
    <div
      className="fixed inset-0 z-50 flex flex-col justify-end bg-black/50"
      onClick={onClose}
    >
      <div
        className="flex max-h-[85dvh] flex-col gap-4 overflow-y-auto rounded-t-2xl border-t border-white/10 bg-zinc-950 p-5 pb-8"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Settings</h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg px-2 py-1 text-white/50 active:text-white"
          >
            Done
          </button>
        </header>

        <Slider
          label="Pointer speed"
          value={settings.pointerSpeed}
          min={0.5}
          max={3.5}
          step={0.05}
          onChange={(v) => set({ pointerSpeed: v })}
        />
        <Slider
          label="Acceleration"
          value={settings.acceleration}
          min={0}
          max={1}
          step={0.05}
          onChange={(v) => set({ acceleration: v })}
        />
        <Slider
          label="Scroll speed"
          value={settings.scrollSpeed}
          min={0.3}
          max={3}
          step={0.05}
          onChange={(v) => set({ scrollSpeed: v })}
        />

        <Toggle
          label="Natural scrolling (two-finger)"
          value={settings.naturalScroll}
          onChange={(v) => set({ naturalScroll: v })}
        />
        <Toggle
          label="Momentum scrolling"
          value={settings.momentum}
          onChange={(v) => set({ momentum: v })}
        />
        <Toggle
          label="Tap to click"
          value={settings.tapToClick}
          onChange={(v) => set({ tapToClick: v })}
        />

        <button
          type="button"
          onClick={reset}
          className="mt-1 rounded-xl border border-white/10 px-4 py-2 text-sm text-white/50 active:bg-white/10"
        >
          Reset to defaults
        </button>
      </div>
    </div>
  );
}
