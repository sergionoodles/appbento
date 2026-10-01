import { useEffect, useState, useSyncExternalStore } from "react";
import { api, type AppApi } from "./api";
import { createLibraryController } from "./library-controller";

export function useLibrary(backend: AppApi = api) {
  const [controller] = useState(() => createLibraryController(backend));
  const state = useSyncExternalStore(controller.subscribe, controller.getState);

  useEffect(() => { void controller.initialize(); }, [controller]);

  return { ...state, controller };
}
