import { useEffect, useRef } from "react";
import { FolderCog, Plus, ScanSearch, Search, Settings, Sparkles, Upload } from "lucide-react";
import { Input } from "./ui/card";
import { Button } from "./ui/button";
import { Menu, MenuItem, MenuSeparator } from "./ui/menu";
import { ThemeSwatches } from "./theme";

function Wordmark({ onHome }: { onHome: () => void }) {
  return (
    <button type="button" onClick={onHome} aria-label="appbento home" className="flex items-center gap-2.5 cursor-pointer rounded-md">
      <span aria-hidden="true" className="grid grid-cols-2 gap-[3px] size-7">
        <i className="rounded-[7px_3px_3px_3px] bg-[var(--accent)]" />
        <i className="rounded-[3px_7px_3px_3px] bg-[var(--fg)] opacity-80" />
        <i className="rounded-[3px_3px_3px_7px] bg-[var(--fg)] opacity-40" />
        <i className="rounded-[3px_3px_7px_3px] bg-[var(--accent)] opacity-60" />
      </span>
      <span className="text-[20px] font-extrabold tracking-[-0.04em]">appbento</span>
    </button>
  );
}

export function Header({ isNative, query, onQuery, disabled, canEditFolders, updateCount, onHome, onUpdates, onFolders, onAdd, onScan }: {
  isNative: boolean;
  query: string;
  onQuery: (query: string) => void;
  disabled: boolean;
  canEditFolders: boolean;
  updateCount: number;
  onHome: () => void;
  onUpdates: () => void;
  onFolders: () => void;
  onAdd: () => void;
  onScan: () => void;
}) {
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focusOnSlash = (event: KeyboardEvent) => {
      const isTyping = event.target instanceof HTMLElement && event.target.matches("input, textarea, select");
      if (event.key !== "/" || isTyping) return;
      event.preventDefault();
      search.current?.focus();
    };
    document.addEventListener("keydown", focusOnSlash);
    return () => document.removeEventListener("keydown", focusOnSlash);
  }, []);

  return (
    <header className="flex items-center gap-2 sm:gap-3 px-4 sm:px-6 py-4 max-w-[1240px] w-full mx-auto">
      <div className="shrink-0"><Wordmark onHome={onHome} /></div>
      {!isNative ? <span className="hidden sm:inline text-[12px] text-[var(--warn)]" title="Launch and file management run in the desktop app.">sample data</span> : null}
      <div className="ml-auto flex items-center gap-2 min-w-0">
        <div className="relative min-w-0 w-28 sm:w-60 focus-within:sm:w-72 transition-[width] duration-300">
          <Search size={15} aria-hidden="true" className="absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--muted)]" />
          <Input ref={search} aria-label="Filter applications and catalog" placeholder="Search apps" value={query} onChange={(event) => onQuery(event.target.value)} className="pl-9" />
        </div>
        <Button variant="soft" aria-label="Updates" onClick={onUpdates} className="relative px-3 sm:px-4">
          <Sparkles size={15} /><span className="hidden sm:inline">Updates</span>
          {updateCount > 0 ? <span className="grid place-items-center min-w-5 h-5 px-1 rounded-md bg-[var(--accent)] text-[var(--on-accent)] text-[11px] font-bold">{updateCount}</span> : null}
        </Button>
        <Menu label="Add apps" variant="soft" icon={<Plus size={17} />} disabled={disabled}>
          <MenuItem icon={<Upload size={15} />} onSelect={onAdd} disabled={!isNative}>Add an AppImage</MenuItem>
          <MenuItem icon={<ScanSearch size={15} />} onSelect={onScan}>Scan folders</MenuItem>
        </Menu>
        <Menu label="Settings" variant="soft" icon={<Settings size={16} />}>
          <MenuItem icon={<FolderCog size={15} />} onSelect={onFolders} disabled={!canEditFolders || disabled}>Folders</MenuItem>
          <MenuSeparator />
          <ThemeSwatches />
        </Menu>
      </div>
    </header>
  );
}
