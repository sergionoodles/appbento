import { createContext, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Button } from "./button";

const MenuContext = createContext<() => void>(() => undefined);

export function Menu({ label, icon, children, disabled, className, variant = "ghost" }: {
  label: string;
  icon: ReactNode;
  children: ReactNode;
  disabled?: boolean;
  className?: string;
  variant?: "ghost" | "soft";
}) {
  const [isOpen, setIsOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuId = useId();

  // Focus returns to the trigger first so callers capturing focus get an element that stays mounted.
  const close = () => {
    setIsOpen(false);
    trigger.current?.focus();
  };

  useEffect(() => {
    if (!isOpen) return;
    root.current?.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus();
    const closeOnOutsidePress = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setIsOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePress);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePress);
  }, [isOpen]);

  const handleKey = (event: React.KeyboardEvent) => {
    if (event.key === "Escape" && isOpen) {
      event.stopPropagation();
      setIsOpen(false);
      trigger.current?.focus();
      return;
    }
    if (!isOpen || (event.key !== "ArrowDown" && event.key !== "ArrowUp")) return;
    event.preventDefault();
    const items = [...(root.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ?? [])];
    const index = items.indexOf(document.activeElement as HTMLElement);
    const step = event.key === "ArrowDown" ? 1 : -1;
    items[(index + step + items.length) % items.length]?.focus();
  };

  return (
    <div ref={root} className={cn("relative", className)} onKeyDown={handleKey}>
      <Button ref={trigger} variant={variant} size="icon" aria-label={label} aria-haspopup="menu" aria-expanded={isOpen}
        aria-controls={isOpen ? menuId : undefined} disabled={disabled} onClick={() => setIsOpen(!isOpen)}>
        {icon}
      </Button>
      {isOpen ? (
        <div id={menuId} role="menu" aria-label={label} className="menu absolute right-0 top-full mt-2 z-30 w-max min-w-52 max-w-[min(22rem,calc(100vw-2rem))] rounded-lg p-1.5 flex flex-col">
          <MenuContext.Provider value={close}>{children}</MenuContext.Provider>
        </div>
      ) : null}
    </div>
  );
}

export function MenuItem({ icon, children, label, onSelect, isDanger, disabled }: {
  icon?: ReactNode;
  /** Accessible name when the visible text is shorter than the full action. */
  label?: string;
  children: ReactNode;
  onSelect: () => void;
  isDanger?: boolean;
  disabled?: boolean;
}) {
  const close = useContext(MenuContext);
  return (
    <button type="button" role="menuitem" aria-label={label} disabled={disabled}
      className={cn(
        "flex items-center gap-2.5 whitespace-nowrap rounded-md px-3 h-9 text-left text-[14px] cursor-pointer disabled:opacity-45 hover:bg-[var(--border)] focus-visible:bg-[var(--border)] focus-visible:outline-none",
        isDanger ? "text-[var(--bad)]" : "text-[var(--fg)]"
      )}
      onClick={() => {
        close();
        onSelect();
      }}>
      {icon}<span className="truncate">{children}</span>
    </button>
  );
}

export function MenuSeparator() {
  return <div role="separator" className="my-1 h-px bg-[var(--border)]" />;
}
