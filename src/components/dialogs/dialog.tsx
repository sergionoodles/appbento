import { useEffect, useId, useRef, type ReactNode } from "react";
import { cn } from "@/lib/utils";

const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), select:not(:disabled), a[href], [tabindex="0"]';

/** `isBare` drops the padded card chrome so children can lay out the whole panel; the title stays as the accessible name. */
export function Dialog({ title, titleAction, children, onClose, returnFocusElement, canClose = true, isSheet = false, isBare = false, className }: {
  title: string;
  titleAction?: ReactNode;
  children: ReactNode;
  onClose: () => void;
  canClose?: boolean;
  isSheet?: boolean;
  isBare?: boolean;
  className?: string;
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
          "bg-[var(--panel)] border border-[var(--border)] shadow-2xl max-w-full flex flex-col outline-none",
          isBare ? "overflow-hidden" : "p-7 overflow-auto gap-5",
          isSheet ? "sheet w-[480px] h-full rounded-[10px]" : "dialog-card w-[520px] max-h-[90vh] rounded-[10px]",
          className
        )}>
        {titleAction ? <div className="flex items-center justify-between gap-4">
          <h2 id={titleId} className="text-[18px] font-semibold leading-tight">{title}</h2>
          {titleAction}
        </div> : <h2 id={titleId} className={isBare ? "sr-only" : "text-[18px] font-semibold leading-tight"}>{title}</h2>}
        {children}
      </div>
    </div>
  );
}
