import { useEffect, useState } from "react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp } from "@/lib/types";

type ReviewResult = { key: string; token: string | null; error: string | null };

export function useAppReview(app: InstalledApp, backend: AppApi) {
  const needsReview = !app.launch_config || app.reviewed_digest === null;
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<ReviewResult | null>(null);
  const key = JSON.stringify([app.id, app.file_path, app.kind, app.reviewed_digest, app.launch_config, attempt]);
  const id = app.id;

  useEffect(() => {
    if (!needsReview) return;
    let isActive = true;
    backend.libraryPrepareReview(id).then(
      (token) => { if (isActive) setResult({ key, token, error: null }); },
      (failure: unknown) => {
        if (isActive) setResult({ key, token: null, error: failure instanceof Error ? failure.message : String(failure) });
      },
    );
    return () => { isActive = false; };
  }, [backend, id, key, needsReview]);

  const current = needsReview && result?.key === key ? result : null;
  return {
    needsReview,
    token: current?.token ?? null,
    error: current?.error ?? null,
    isVerifying: needsReview && !current,
    retry: () => setAttempt((previous) => previous + 1),
  };
}
