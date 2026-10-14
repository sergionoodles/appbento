import { RefreshCw } from "lucide-react";
import type { InstalledApp, UpdateInfo, UpdateReport } from "@/lib/types";
import { StageHeader } from "./stage";
import { Button } from "./ui/button";
import { Card } from "./ui/card";

function describeUpdate(update: UpdateInfo): string {
  if (update.status === "unknown-current") return "current version unknown";
  if (update.latest === "Updated file") return "File content changed";
  return `${update.current} → ${update.latest}`;
}

export function UpdatesView({ report, apps, updatingAppIds, isChecking, disabled, onBack, onCheck, onInstall }: {
  report: UpdateReport | null;
  apps: InstalledApp[];
  updatingAppIds: string[];
  disabled: boolean;
  isChecking: boolean;
  onBack: () => void;
  onCheck: () => void;
  onInstall: (id: string) => void;
}) {
  return (
    <div>
      <StageHeader title="Updates" onBack={onBack}>
        <Button variant="soft" disabled={isChecking || disabled || updatingAppIds.length > 0} onClick={onCheck}><RefreshCw size={15} className={isChecking ? "animate-spin" : undefined} />{isChecking ? "Checking…" : "Check again"}</Button>
      </StageHeader>
      <Card className="intro flex flex-col">
        <p className="text-[13px] text-[var(--muted)] mb-3">Checks use each app’s saved update source. Installing downloads a replacement and keeps its launch permissions and settings.</p>
        {!report ? <p role="status">{isChecking ? "Checking releases…" : "No release report yet. Check again to see available updates."}</p> : (
          <>
            <p className="text-[13px] text-[var(--muted)] mb-2">{report.checked_count} checked, {report.unsupported_count} without a supported update source, {report.errors.length} failed</p>
            {report.updates.map((update) => {
              const isUpdating = updatingAppIds.includes(update.app_id);
              const installLabel = update.status === "available" ? "Install update" : "Install latest";

              return (
              <div key={update.app_id} className="flex flex-wrap items-center gap-3 py-3 border-t border-[var(--border)]">
                <span className="truncate flex-1 font-medium">{apps.find((app) => app.id === update.app_id)?.name ?? update.asset_name}</span>
                <span className="text-[var(--muted)] text-[14px]">{describeUpdate(update)}</span>
                <Button size="sm" variant={update.status === "available" ? "accent" : "outline"} disabled={disabled || isChecking || isUpdating} onClick={() => onInstall(update.app_id)}>
                  {isUpdating ? <><RefreshCw size={14} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />Updating…</> : installLabel}
                </Button>
              </div>
              );
            })}
            {report.errors.map((error) => <p key={error.app_id} className="text-[14px] text-[var(--bad)] mt-2">{apps.find((app) => app.id === error.app_id)?.name ?? error.app_id}: {error.message}</p>)}
            {!report.updates.length ? <p className="text-[var(--muted)] mt-2">{report.errors.length || report.unsupported_count ? "No confirmed updates. Some apps could not be checked." : "Everything is up to date."}</p> : null}
          </>
        )}
      </Card>
    </div>
  );
}
