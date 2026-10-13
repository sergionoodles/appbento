import { useState, type CSSProperties, type ReactNode } from "react";
import { ArrowUpRight, Github, Scale, Tag, X } from "lucide-react";
import { version } from "../../../package.json";
import { cn } from "@/lib/utils";
import { LogoMark } from "../logo";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";

export const REPOSITORY_URL = "https://github.com/sergionoodles/appbento";
const REPOSITORY_NAME = "sergionoodles/appbento";
const RELEASE_NOTES_URL = `${REPOSITORY_URL}/releases/tag/v${version}`;
const LICENSE_NAME = "MIT";

/** Wide tiles sit in one row; stacked tiles put the icon above the text so half-width tiles keep their labels on one line. */
function Tile({ icon, label, value, onOpen, isStacked = false, className, index }: {
  icon: ReactNode;
  label: string;
  value: string;
  onOpen?: () => void;
  isStacked?: boolean;
  className?: string;
  index: number;
}) {
  const badge = <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-[color-mix(in_oklab,var(--accent)_14%,transparent)] text-[var(--accent)]">{icon}</span>;
  const arrow = onOpen ? <ArrowUpRight size={16} aria-hidden="true" className="about-tile-arrow shrink-0 text-[var(--muted)]" /> : null;
  const text = <span className="min-w-0 flex-1 flex flex-col gap-0.5">
    <span className="whitespace-nowrap text-[11.5px] uppercase tracking-[0.08em] text-[var(--muted)]">{label}</span>
    <span className="truncate text-[14px] font-medium">{value}</span>
  </span>;
  const content = isStacked
    ? <><span className="flex w-full items-start justify-between">{badge}{arrow}</span>{text}</>
    : <>{badge}{text}{arrow}</>;
  const style = { "--i": index } as CSSProperties;
  const tileClass = cn("about-tile flex gap-3 rounded-[10px] border p-3 text-left", isStacked ? "flex-col" : "items-center", className);

  if (!onOpen) return <div className={tileClass} style={style}>{content}</div>;
  return <button type="button" onClick={onOpen} className={cn(tileClass, "is-link")} style={style}>{content}</button>;
}

export function AboutDialog({ onOpenExternal, onClose, returnFocusElement }: {
  onOpenExternal: (url: string) => Promise<void>;
  onClose: () => void;
  returnFocusElement: HTMLElement | null;
}) {
  const [openError, setOpenError] = useState<string | null>(null);

  const open = (url: string) => {
    setOpenError(null);
    void onOpenExternal(url).catch((error: unknown) => setOpenError(error instanceof Error ? error.message : String(error)));
  };

  return <Dialog title="About AppBento" onClose={onClose} returnFocusElement={returnFocusElement} isBare className="w-[440px]">
    <div className="about-hero relative flex flex-col items-center px-8 pt-10 pb-7 text-center">
      <Button variant="ghost" size="icon" aria-label="Close" onClick={onClose} className="absolute right-3 top-3 size-8"><X size={16} /></Button>
      <LogoMark className="about-mark size-20" />
      <p className="mt-5 font-display text-[34px] font-bold leading-none tracking-[-0.04em]">appbento</p>
      <p className="mt-2 text-[13.5px] text-[var(--muted)]">Your AppImages, neatly packed.</p>
      <span className="about-version mt-4 rounded-full border px-3 py-1 text-[12.5px] font-medium tabular-nums" aria-label={`Version ${version}`}>v{version}</span>
    </div>
    <div className="settle grid grid-cols-2 gap-2.5 px-5 pb-5">
      <Tile index={0} className="col-span-2" icon={<Github size={18} />} label="Source code" value={REPOSITORY_NAME} onOpen={() => open(REPOSITORY_URL)} />
      <Tile index={1} isStacked icon={<Tag size={17} />} label="Release notes" value={`What's new in ${version}`} onOpen={() => open(RELEASE_NOTES_URL)} />
      <Tile index={2} isStacked icon={<Scale size={17} />} label="License" value={`${LICENSE_NAME}, free & open`} />
    </div>
    {openError ? <p role="alert" className="px-5 pb-4 -mt-1 text-[13px] text-[var(--bad)] selectable">{openError}</p> : null}
    <p className="border-t border-[var(--hairline)] bg-[var(--chrome)] px-5 py-3 text-center text-[12px] text-[var(--muted)]">
      Local AppImage manager for Linux · Built with Tauri &amp; React
    </p>
  </Dialog>;
}
