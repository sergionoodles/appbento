import { useEffect, useState, useSyncExternalStore } from "react";
import { createSelfUpdateController, SELF_UPDATE_INTERVAL, type SelfUpdateBackend } from "./self-update";

export function useSelfUpdate(backend: SelfUpdateBackend) {
  const [controller] = useState(() => createSelfUpdateController(backend));
  const state = useSyncExternalStore(controller.subscribe, controller.getState);

  useEffect(() => {
    if (!backend.isTauri) return;
    void controller.check();
    const interval = window.setInterval(() => { void controller.check(); }, SELF_UPDATE_INTERVAL);
    return () => window.clearInterval(interval);
  }, [backend.isTauri, controller]);

  return { ...state, controller };
}
