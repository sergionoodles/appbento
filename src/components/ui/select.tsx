import { useEffect, useId, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

export interface SelectOption<T extends string> {
  value: T;
  label: string;
}

const POPUP_GAP = 6;
const POPUP_MAX_HEIGHT = 288;

/**
 * Themed replacement for a native select. WebKitGTK draws the open list of a native select with the
 * desktop GTK theme and ignores page styles, so the list is rendered here instead.
 */
export function Select<T extends string>({ id, value, options, onChange, disabled, className, "aria-label": ariaLabel }: {
  id?: string;
  value: T;
  options: readonly SelectOption<T>[];
  onChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  const [activeIndex, setActiveIndex] = useState<number | null>(null);
  const [position, setPosition] = useState<CSSProperties>({});
  const root = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const listId = useId();
  const isOpen = activeIndex !== null;
  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value));

  useEffect(() => {
    if (!isOpen) return;

    const close = () => setActiveIndex(null);
    const closeOnOutsidePress = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) close();
    };

    // The list is placed against the viewport, so it closes instead of drifting when anything scrolls.
    const closeOnScroll = (event: Event) => {
      if (event.target instanceof Node && root.current?.contains(event.target)) return;
      close();
    };

    document.addEventListener("pointerdown", closeOnOutsidePress);
    window.addEventListener("scroll", closeOnScroll, true);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePress);
      window.removeEventListener("scroll", closeOnScroll, true);
      window.removeEventListener("resize", close);
    };
  }, [isOpen]);

  useEffect(() => {
    if (activeIndex === null) return;
    root.current?.querySelector(`[data-index="${activeIndex}"]`)?.scrollIntoView?.({ block: "nearest" });
  }, [activeIndex]);

  const open = (index: number) => {
    const bounds = trigger.current?.getBoundingClientRect();
    if (bounds) setPosition(placePopup(bounds));
    setActiveIndex(index);
  };

  const choose = (index: number) => {
    const option = options[index];
    setActiveIndex(null);
    if (option && option.value !== value) onChange(option.value);
  };

  const handleKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (activeIndex === null) {
      if (!OPENING_KEYS.includes(event.key)) return;
      event.preventDefault();
      open(selectedIndex);
      return;
    }

    if (event.key === "Tab") {
      setActiveIndex(null);
      return;
    }

    if (event.key === "Escape") {
      // Keep the surrounding dialog open: Escape only dismisses the list.
      event.preventDefault();
      event.stopPropagation();
      setActiveIndex(null);
      return;
    }

    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(activeIndex);
      return;
    }

    const next = nextIndex(event.key, activeIndex, options.length);
    if (next === null) return;
    event.preventDefault();
    setActiveIndex(next);
  };

  return (
    <span ref={root} className={cn("relative inline-flex w-full", className)}>
      <button ref={trigger} id={id} type="button" role="combobox" aria-label={ariaLabel} aria-haspopup="listbox"
        aria-expanded={isOpen} aria-controls={isOpen ? listId : undefined}
        aria-activedescendant={isOpen ? `${listId}-${activeIndex}` : undefined}
        disabled={disabled} onClick={() => (isOpen ? setActiveIndex(null) : open(selectedIndex))} onKeyDown={handleKey}
        className="peer flex items-center w-full h-10 rounded-md border border-[var(--border)] bg-[var(--panel2)] pl-3.5 pr-9 text-left text-[14px] text-[var(--fg)] outline-none transition-colors hover:border-[var(--muted)] focus:border-[var(--accent)] aria-expanded:border-[var(--accent)] disabled:opacity-45 disabled:cursor-not-allowed">
        <span className="truncate">{options[selectedIndex]?.label}</span>
      </button>
      <ChevronDown aria-hidden="true" size={16} className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[var(--muted)] peer-focus:text-[var(--accent)]" />
      {isOpen ? (
        <ul id={listId} role="listbox" aria-label={ariaLabel} style={position}
          className="menu fixed z-50 overflow-y-auto rounded-lg p-1 flex flex-col">
          {options.map((option, index) => (
            <li key={option.value} id={`${listId}-${index}`} role="option" data-index={index} aria-selected={index === selectedIndex}
              data-active={index === activeIndex || undefined}
              onPointerDown={(event) => event.preventDefault()} onPointerMove={() => setActiveIndex(index)} onClick={() => choose(index)}
              className="flex items-center gap-2.5 rounded-md px-2.5 h-8 shrink-0 text-[13.5px] text-[var(--fg)] data-[active]:bg-[var(--border)]">
              <span className="flex-1 truncate">{option.label}</span>
              {index === selectedIndex ? <Check aria-hidden="true" size={15} className="text-[var(--accent)]" /> : null}
            </li>
          ))}
        </ul>
      ) : null}
    </span>
  );
}

const OPENING_KEYS = ["ArrowDown", "ArrowUp", "Enter", " "];

function nextIndex(key: string, current: number, count: number): number | null {
  if (key === "ArrowDown") return Math.min(current + 1, count - 1);
  if (key === "ArrowUp") return Math.max(current - 1, 0);
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  return null;
}

/** Opens below the trigger, or above it when the viewport has more room there. */
function placePopup(bounds: DOMRect): CSSProperties {
  const spaceBelow = window.innerHeight - bounds.bottom - POPUP_GAP;
  const spaceAbove = bounds.top - POPUP_GAP;
  const isAbove = spaceBelow < POPUP_MAX_HEIGHT && spaceAbove > spaceBelow;
  const vertical = isAbove
    ? { bottom: window.innerHeight - bounds.top + POPUP_GAP }
    : { top: bounds.bottom + POPUP_GAP };

  return {
    ...vertical,
    left: bounds.left,
    width: bounds.width,
    maxHeight: Math.min(POPUP_MAX_HEIGHT, isAbove ? spaceAbove : spaceBelow),
  };
}
