import { cn } from "@/lib/utils";

/**
 * One 11-unit cell rotated into each quadrant of a 24-unit grid (2-unit gap), so the four cells are
 * congruent by construction and the mark lands on whole pixels at 24px and 48px. The outer corner
 * has the large radius so the cells read as a single rounded box; inner corners stay small.
 * src-tauri/icons/icon.svg uses the same geometry.
 */
const CELL_PATH = "M5 0H9A2 2 0 0 1 11 2V9A2 2 0 0 1 9 11H2A2 2 0 0 1 0 9V5A5 5 0 0 1 5 0Z";

const CELLS = [
  { rotation: 0, fill: "var(--accent)", opacity: 1 },
  { rotation: 90, fill: "var(--fg)", opacity: 0.8 },
  { rotation: 180, fill: "var(--accent)", opacity: 0.6 },
  { rotation: 270, fill: "var(--fg)", opacity: 0.4 },
];

export function LogoMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={cn("size-6 shrink-0", className)}>
      {CELLS.map((cell) => (
        <path key={cell.rotation} d={CELL_PATH} transform={`rotate(${cell.rotation} 12 12)`} fill={cell.fill} opacity={cell.opacity} />
      ))}
    </svg>
  );
}

export function Wordmark({ onHome }: { onHome: () => void }) {
  return (
    <button type="button" onClick={onHome} aria-label="appbento home" className="flex items-center gap-2 rounded-md px-1 -mx-1">
      <LogoMark />
      <span className="font-display text-[17px] font-bold leading-none tracking-[-0.03em]">appbento</span>
    </button>
  );
}
