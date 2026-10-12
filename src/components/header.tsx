import { useEffect, useRef } from "react";
import { FolderCog, Plus, ScanSearch, Search, Settings, Upload, Archive } from "lucide-react";
import { Input } from "./ui/card";
import { Menu, MenuItem, MenuSeparator } from "./ui/menu";
import { ThemeSwatches } from "./theme";

export function Header({ isNative, query, onQuery, disabled, canEditFolders, onFolders, onConfiguration, onAdd, onScan }: {
  isNative: boolean;
  query: string;
  onQuery: (query: string) => void;
  disabled: boolean;
  canEditFolders: boolean;
  onFolders: () => void;
  onConfiguration: () => void;
  onAdd: () => void;
  onScan: () => void;
}) {
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      const isTyping = event.target instanceof HTMLElement && event.target.matches("input, textarea, select");
      const isFindShortcut = event.ctrlKey && event.key.toLowerCase() === "f";
      if (!isFindShortcut && (event.key !== "/" || isTyping)) return;
      event.preventDefault();
      search.current?.focus();
      search.current?.select();
    };
    document.addEventListener("keydown", focusSearch);
    return () => document.removeEventListener("keydown", focusSearch);
  }, []);

  return (
    <header className="h-[52px] shrink-0 flex items-center gap-2 px-4 border-b border-[var(--hairline)]">
      <div className="relative min-w-0 w-72">
        <Search size={15} aria-hidden="true" className="absolute left-3 top-1/2 -translate-y-1/2 text-[var(--muted)]" />
        <Input ref={search} aria-label="Filter applications" placeholder="Search apps (Ctrl+F)" value={query} onChange={(event) => onQuery(event.target.value)} className="h-8 pl-8 text-[13px]" />
      </div>
      {!isNative ? <span className="text-[12px] text-[var(--warn)]" title="Launch and file management run in the desktop app.">sample data</span> : null}
      <div className="ml-auto flex items-center gap-1">
        <Menu label="Add apps" icon={<Plus size={17} />} disabled={disabled}>
          <MenuItem icon={<Upload size={15} />} onSelect={onAdd} disabled={!isNative}>Add an AppImage</MenuItem>
          <MenuItem icon={<ScanSearch size={15} />} onSelect={onScan}>Scan folders</MenuItem>
        </Menu>
        <Menu label="Settings" icon={<Settings size={16} />}>
          <MenuItem icon={<FolderCog size={15} />} onSelect={onFolders} disabled={!canEditFolders || disabled}>Folders</MenuItem>
          <MenuItem icon={<Archive size={15} />} onSelect={onConfiguration} disabled={!isNative || disabled}>Export / restore settings</MenuItem>
          <MenuSeparator />
          <ThemeSwatches />
        </Menu>
      </div>
    </header>
  );
}
