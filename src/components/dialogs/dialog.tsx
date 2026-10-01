import { useEffect, useId, useRef, type ReactNode } from "react";

const FOCUSABLE = 'button:not(:disabled), input:not(:disabled), select:not(:disabled), a[href], [tabindex="0"]';

export function Dialog({ title, children, onClose, returnFocusElement, canClose = true }: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  canClose?: boolean;
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
        document.querySelector<HTMLElement>("nav button")?.focus();
      }
    };
  }, [returnFocusElement]);

  return (
    <div className="fixed inset-0 bg-black/60 grid place-items-center z-50 p-4" onClick={(event) => {
      if (event.target === event.currentTarget && canClose) onClose();
    }}>
      <div ref={panel} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}
        className="tui-panel p-5 w-[520px] max-w-full max-h-[90vh] overflow-auto flex flex-col gap-4">
        <h2 id={titleId} className="tui-title">{title}</h2>
        {children}
      </div>
    </div>
  );
}
