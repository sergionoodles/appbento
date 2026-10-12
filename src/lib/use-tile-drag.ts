import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { dropIndex, sectionAppIds, sectionOf, type AppLayout } from "./app-layout";

/** Pixels the pointer must travel before a press becomes a drag, to avoid dragging on a plain click. */
const DRAG_THRESHOLD = 6;
const SCROLL_EDGE = 56;
const SCROLL_STEP = 16;

export interface DropTarget {
  sectionId: string;
  index: number;
}

export interface TileDrag {
  appId: string;
  x: number;
  y: number;
  offsetX: number;
  offsetY: number;
  width: number;
  height: number;
  target: DropTarget;
  /** Over a drop zone (group header, sidebar entry) rather than a tile, so the ghost shrinks out of the way. */
  isOverZone: boolean;
}

interface PointerTarget {
  target: DropTarget;
  isZone: boolean;
}

/**
 * Resolves what the pointer is over: a tile (insert before or after it), or a section drop zone
 * such as a group header or sidebar entry (append). Gaps and the dragged tile itself return
 * undefined so the current target holds steady while tiles reflow.
 */
function findTarget(layout: AppLayout, appId: string, x: number, y: number): PointerTarget | undefined {
  for (const element of document.elementsFromPoint(x, y)) {
    const tile = element.closest<HTMLElement>("[data-tile-id]");
    const tileSection = tile?.closest<HTMLElement>("[data-section-id]")?.dataset.sectionId;
    if (tile && tileSection) {
      const overId = tile.dataset.tileId ?? "";
      if (overId === appId) return undefined;
      const rect = tile.getBoundingClientRect();
      const isAfter = x > rect.left + rect.width / 2;
      return { target: { sectionId: tileSection, index: dropIndex(layout, appId, tileSection, overId, isAfter) }, isZone: false };
    }

    const zone = element.closest<HTMLElement>("[data-drop-section]")?.dataset.dropSection;
    if (zone) return { target: { sectionId: zone, index: sectionAppIds(layout, zone).filter((id) => id !== appId).length }, isZone: true };
  }
  return undefined;
}

function scrollNearEdge(container: HTMLElement | null, y: number) {
  if (!container) return;
  const bounds = container.getBoundingClientRect();
  if (y < bounds.top + SCROLL_EDGE) container.scrollTop -= SCROLL_STEP;
  else if (y > bounds.bottom - SCROLL_EDGE) container.scrollTop += SCROLL_STEP;
}

function initialTarget(layout: AppLayout, appId: string): DropTarget | null {
  const sectionId = sectionOf(layout, appId);
  if (!sectionId) return null;
  return { sectionId, index: sectionAppIds(layout, sectionId).indexOf(appId) };
}

export function useTileDrag(layout: AppLayout, onDrop: (appId: string, target: DropTarget) => void) {
  const [drag, setDrag] = useState<TileDrag | null>(null);
  const latest = useRef({ layout, onDrop });
  const stopListening = useRef<(() => void) | null>(null);
  const shouldSuppressClick = useRef(false);

  useLayoutEffect(() => {
    latest.current = { layout, onDrop };
  });

  useEffect(() => () => stopListening.current?.(), []);

  const startDrag = (event: React.PointerEvent<HTMLElement>, appId: string) => {
    const isFromAction = event.target instanceof Element && event.target.closest(".tile-action");
    if (event.button !== 0 || !event.isPrimary || isFromAction) return;
    const startTarget = initialTarget(latest.current.layout, appId);
    if (!startTarget) return;

    const tile = event.currentTarget;
    const rect = tile.getBoundingClientRect();
    const scroller = tile.closest<HTMLElement>("[data-scroll-container]");
    const start = { x: event.clientX, y: event.clientY };
    let current: TileDrag | null = null;

    const move = (moveEvent: PointerEvent) => {
      const distance = Math.hypot(moveEvent.clientX - start.x, moveEvent.clientY - start.y);
      if (!current && distance < DRAG_THRESHOLD) return;
      const base: TileDrag = current ?? {
        appId, x: start.x, y: start.y, offsetX: start.x - rect.left, offsetY: start.y - rect.top,
        width: rect.width, height: rect.height, target: startTarget, isOverZone: false,
      };
      scrollNearEdge(scroller, moveEvent.clientY);
      const found = findTarget(latest.current.layout, appId, moveEvent.clientX, moveEvent.clientY);
      current = {
        ...base, x: moveEvent.clientX, y: moveEvent.clientY,
        target: found?.target ?? base.target, isOverZone: found?.isZone ?? base.isOverZone,
      };
      setDrag(current);
    };

    const finish = (shouldDrop: boolean) => {
      stopListening.current?.();
      if (!current) return;
      setDrag(null);
      shouldSuppressClick.current = true;
      window.setTimeout(() => { shouldSuppressClick.current = false; }, 0);
      if (shouldDrop) latest.current.onDrop(appId, current.target);
    };
    const drop = () => finish(true);
    const cancel = () => finish(false);
    const cancelOnEscape = (keyEvent: KeyboardEvent) => {
      if (keyEvent.key !== "Escape" || !current) return;
      keyEvent.preventDefault();
      keyEvent.stopPropagation();
      finish(false);
    };

    stopListening.current = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", drop);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", cancelOnEscape, true);
      stopListening.current = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", drop);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", cancelOnEscape, true);
  };

  /** A finished drag ends with a click on whatever is under the pointer; swallow it so nothing opens. */
  const suppressClickAfterDrag = (event: React.MouseEvent) => {
    if (!shouldSuppressClick.current) return;
    event.preventDefault();
    event.stopPropagation();
    shouldSuppressClick.current = false;
  };

  return { drag, startDrag, suppressClickAfterDrag };
}
