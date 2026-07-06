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
    ? "pf-accent border-transparent font-semibold"
    : "border-white/10 bg-white/[0.06] text-white/90 active:bg-white/15";
  return (
    <button
      type="button"
      // Fire on press for snappy, repeatable taps.
      onPointerDown={(e) => {
        e.preventDefault();
        onPress();
      }}
      className={`pf-press select-none rounded-xl border px-3 py-3 text-base font-medium ${theme} ${className}`}
    >
      {children}
    </button>
  );
}
