import { flushSync } from "react-dom";

export const STAGE_TRANSITION_NAME = "app-stage";

type TransitionDocument = Document & { startViewTransition?: (update: () => void) => { finished: Promise<void> } };

function prefersReducedMotion() {
  return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}

function findElement(selector: string | null) {
  return selector ? document.querySelector<HTMLElement>(selector) : null;
}

/**
 * Runs a state update inside a View Transition so the element matching
 * `fromSelector` (before) morphs into the one matching `toSelector` (after).
 * Falls back to a plain update where the API is missing.
 */
export function transitionTo(update: () => void, { fromSelector = null, toSelector = null }: { fromSelector?: string | null; toSelector?: string | null } = {}) {
  const transitionDocument: TransitionDocument = document;
  if (!transitionDocument.startViewTransition || prefersReducedMotion()) {
    update();
    return;
  }

  const source = findElement(fromSelector);
  if (source) source.style.viewTransitionName = STAGE_TRANSITION_NAME;

  const transition = transitionDocument.startViewTransition(() => {
    if (source) source.style.viewTransitionName = "";
    flushSync(update);
    const target = findElement(toSelector);
    if (target) target.style.viewTransitionName = STAGE_TRANSITION_NAME;
  });

  // A skipped transition rejects `finished`; either way the temporary name must go.
  const clearTargetName = () => {
    const target = findElement(toSelector);
    if (target) target.style.viewTransitionName = "";
  };
  void transition.finished.then(clearTargetName, clearTargetName);
}
