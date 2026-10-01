import { Boxes, FolderSearch, LayoutGrid, Plus, RefreshCw, Settings2, TerminalSquare } from "lucide-react";
import { cn } from "@/lib/utils";
import { Badge, Input } from "./ui/card";
import { Button } from "./ui/button";
import { ThemeSelect } from "./theme";

export type Tab = "bento" | "library" | "catalog" | "updates";

const TABS = [
  { id: "bento", label: "bento", icon: LayoutGrid },
  { id: "library", label: "library", icon: Boxes },
  { id: "catalog", label: "catalog", icon: FolderSearch },
  { id: "updates", label: "updates", icon: RefreshCw },
] as const;

export function Header({ isNative, tab, onTab, query, onQuery, disabled, canEditFolders, isChecking, onFolders, onAdd, onScan, onCheck }: {
  isNative: boolean;
  tab: Tab;
  onTab: (tab: Tab) => void;
  query: string;
  onQuery: (query: string) => void;
  disabled: boolean;
  canEditFolders: boolean;
  isChecking: boolean;
  onFolders: () => void;
  onAdd: () => void;
  onScan: () => void;
  onCheck: () => void;
}) {
  return (
    <header className="flex flex-wrap items-center gap-3 px-4 py-2.5 border-b border-[var(--border)]">
      <div className="flex items-center gap-2 font-bold"><TerminalSquare size={18} className="text-[var(--accent)]" /><span>appbento</span><Badge>v0.1</Badge></div>
      <nav className="flex gap-1" aria-label="Main navigation">
        {TABS.map(({ id, label, icon: Icon }) => (
          <button key={id} type="button" onClick={() => onTab(id)} aria-current={tab === id ? "page" : undefined}
            className={cn("flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[13px] border", tab === id ? "border-[var(--accent)] text-[var(--fg)] bg-[var(--panel)]" : "border-transparent text-[var(--muted)] hover:text-[var(--fg)]")}>
            <Icon size={15} />{label}
          </button>
        ))}
      </nav>
      <div className="ml-auto flex flex-wrap items-center gap-2">
        <Input aria-label="Filter applications and catalog" placeholder="filter // type to search" value={query} onChange={(event) => onQuery(event.target.value)} className="w-56" />
        <ThemeSelect />
        <Button onClick={onFolders} disabled={!canEditFolders || disabled}><Settings2 size={14} />folders</Button>
        <Button onClick={onAdd} disabled={disabled || !isNative}><Plus size={14} />add .AppImage</Button>
        <Button onClick={onScan} disabled={disabled}><FolderSearch size={14} />scan</Button>
        <Button variant="default" onClick={onCheck} disabled={isChecking || disabled}><RefreshCw size={14} />{isChecking ? "checking…" : "check releases"}</Button>
      </div>
    </header>
  );
}
