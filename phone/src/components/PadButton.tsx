"use client";

export function PadButton({
  children,
  onPress,
  accent = false,
  className = "",
}: {
  children: React.ReactNode;
  onPress: () => void;
  accent?: boolean;
  className?: string;
}) {
  const theme = accent
    ? "border-emerald-400/30 bg-emerald-500/20 text-emerald-200 active:bg-emerald-500/35"
    : "border-white/10 bg-white/5 text-white/90 active:bg-white/15";
  return (
    <button
      type="button"
      // Fire on press for snappy, repeatable taps.
      onPointerDown={(e) => {
        e.preventDefault();
        onPress();
      }}
      className={`select-none rounded-xl border px-3 py-3 text-base font-medium ${theme} ${className}`}
    >
      {children}
    </button>
  );
}
