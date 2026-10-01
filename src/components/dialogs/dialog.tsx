import { useEffect, useId, useRef, type ReactNode } from "react";
import { cn } from "@/lib/utils";

const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), select:not(:disabled), a[href], [tabindex="0"]';

export function Dialog({ title, children, onClose, returnFocusElement, canClose = true, isSheet = false }: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  canClose?: boolean;
  isSheet?: boolean;
  returnFocusElement?: HTMLElement | null;
}) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  const closable = useRef(canClose);

  useEffect(() => {
    close.current = onClose;
    closable.current = canClose;
  }, [onClose, canClose]);

  useEffect(() => {
    const previouslyFocused = returnFocusElement ?? document.activeElement;
    const element = panel.current;
    const firstControl = element?.querySelector<HTMLElement>(FOCUSABLE);
    (firstControl ?? element)?.focus();
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (closable.current) close.current();
      }
      if (event.key !== "Tab" || !element) return;
      const controls = [...element.querySelectorAll<HTMLElement>(FOCUSABLE)];
      const first = controls[0];
      const last = controls[controls.length - 1];
      if (!first || !last) {
        event.preventDefault();
        element.focus();
        return;
      }
      const isOutside = !element.contains(document.activeElement);
      if (event.shiftKey && (document.activeElement === first || isOutside)) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (document.activeElement === last || isOutside)) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("keydown", handleKey);
      if (previouslyFocused instanceof HTMLElement && previouslyFocused.isConnected) {
        previouslyFocused.focus();
      } else {
        document.querySelector<HTMLElement>("header button")?.focus();
      }
    };
  }, [returnFocusElement]);

  return (
    <div className={cn("sheet-backdrop fixed inset-0 bg-black/55 backdrop-blur-sm z-50 flex p-3", isSheet ? "justify-end" : "items-center justify-center")} onClick={(event) => {
      if (event.target === event.currentTarget && canClose) onClose();
    }}>
      <div ref={panel} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}
        className={cn(
          "bg-[var(--panel)] border border-[var(--border)] shadow-2xl p-7 max-w-full overflow-auto flex flex-col gap-5 outline-none",
          isSheet ? "sheet w-[480px] h-full rounded-[10px]" : "dialog-card w-[520px] max-h-[90vh] rounded-[10px]"
        )}>
        <h2 id={titleId} className="text-[24px] font-bold tracking-[-0.035em] leading-tight">{title}</h2>
        {children}
      </div>
    </div>
  );
}
