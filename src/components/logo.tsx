import { cn } from "@/lib/utils";

/**
 * Four 11-unit cells with a 2-unit gap on a 24-unit grid, so the mark lands on whole pixels at
 * 24px and 48px. Outer corners share one radius so the cells read as a single rounded box;
 * inner corners stay small. src-tauri/icons/icon.svg uses the same geometry.
 */
const CELLS = [
  { path: "M5 0H9A2 2 0 0 1 11 2V9A2 2 0 0 1 9 11H2A2 2 0 0 1 0 9V5A5 5 0 0 1 5 0Z", fill: "var(--accent)", opacity: 1 },
  { path: "M15 0H19A5 5 0 0 1 24 5V9A2 2 0 0 1 22 11H15A2 2 0 0 1 13 9V2A2 2 0 0 1 15 0Z", fill: "var(--fg)", opacity: 0.8 },
  { path: "M2 13H9A2 2 0 0 1 11 15V22A2 2 0 0 1 9 24H5A5 5 0 0 1 0 19V15A2 2 0 0 1 2 13Z", fill: "var(--fg)", opacity: 0.4 },
  { path: "M15 13H22A2 2 0 0 1 24 15V19A5 5 0 0 1 19 24H15A2 2 0 0 1 13 22V15A2 2 0 0 1 15 13Z", fill: "var(--accent)", opacity: 0.6 },
];

export function LogoMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={cn("size-6 shrink-0", className)}>
      {CELLS.map((cell) => <path key={cell.path} d={cell.path} fill={cell.fill} opacity={cell.opacity} />)}
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
