import { fireEvent, screen } from "@testing-library/react";

/** Far enough to pass the drag threshold. Moving left of jsdom's zero-sized tiles drops before them. */
const POINTER_TRAVEL = -10;

/** Drags an app's tile and drops it on `target`: before a tile, or at the end of a drop zone. */
export function dragTile(appName: string, target: Element) {
  const tile = tileOf(appName);
  const original = document.elementsFromPoint;
  document.elementsFromPoint = () => [target];
  try {
    fireEvent.pointerDown(tile, { button: 0, isPrimary: true, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(window, { clientX: POINTER_TRAVEL, clientY: 0 });
    fireEvent.pointerUp(window);
  } finally {
    document.elementsFromPoint = original;
  }
}

export function tileOf(appName: string) {
  return screen.getByRole("article", { name: appName });
}
