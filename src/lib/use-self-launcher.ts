import { useEffect, useState } from "react";
import type { AppApi, SelfLauncherStatus } from "./api";

export function useSelfLauncher(backend: AppApi) {
  const [status, setStatus] = useState<SelfLauncherStatus | null>(null);
  const [isRequested, setIsRequested] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!backend.isTauri) return;
    let isActive = true;
    void backend.selfLauncherStatus().then((next) => {
      if (isActive) setStatus(next);
    }).catch((error: unknown) => {
      if (isActive) setError(error instanceof Error ? error.message : String(error));
    });
    return () => { isActive = false; };
  }, [backend]);

  const save = async (install: boolean) => {
    if (isSaving) return;
    setIsSaving(true);
    setError(null);
    try {
      setStatus(await (install ? backend.selfLauncherInstall() : backend.selfLauncherDismiss()));
      setIsRequested(false);
    } catch (error: unknown) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsSaving(false);
    }
  };

  return {
    canAdd: backend.isTauri && !status?.installed,
    shouldPrompt: !import.meta.env.DEV && status !== null && !status.installed && !status.dismissed,
    isRequested, isSaving, error,
    open: () => setIsRequested(true),
    install: () => save(true),
    dismiss: () => save(false),
  };
}
