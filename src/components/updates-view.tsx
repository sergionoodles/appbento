import { RefreshCw } from "lucide-react";
import type { UpdateReport } from "@/lib/types";
import { StageHeader } from "./stage";
import { Button } from "./ui/button";
import { Card } from "./ui/card";

export function UpdatesView({ report, isChecking, onBack, onCheck, onOpen }: {
  report: UpdateReport | null;
  isChecking: boolean;
  onBack: () => void;
  onCheck: () => void;
  onOpen: (url: string) => void;
}) {
  return (
    <div>
      <StageHeader title="Updates" onBack={onBack}>
        <Button variant="soft" disabled={isChecking} onClick={onCheck}><RefreshCw size={15} className={isChecking ? "animate-spin" : undefined} />{isChecking ? "Checking…" : "Check again"}</Button>
      </StageHeader>
      <Card className="intro flex flex-col">
        <p className="text-[13px] text-[var(--muted)] mb-3">Downloads open in your browser. Add the new AppImage afterwards; installed files are not changed automatically.</p>
        {!report ? <p role="status">{isChecking ? "Checking releases…" : "No release report yet. Check again to see available downloads."}</p> : (
          <>
            <p className="text-[13px] text-[var(--muted)] mb-2">{report.checked_count} checked, {report.unsupported_count} without a supported update source, {report.errors.length} failed</p>
            {report.updates.map((update) => (
              <div key={update.app_id} className="flex flex-wrap items-center gap-3 py-3 border-t border-[var(--border)]">
                <span className="truncate flex-1 font-medium">{update.asset_name}</span>
                <span className="text-[var(--muted)] text-[14px]">{update.status === "unknown-current" ? "current version unknown" : `${update.current} → ${update.latest}`}</span>
                <Button size="sm" variant={update.status === "available" ? "accent" : "outline"} onClick={() => onOpen(update.download_url)}>{update.status === "available" ? "download update" : "download latest"}</Button>
              </div>
            ))}
            {report.errors.map((error) => <p key={error.app_id} className="text-[14px] text-[var(--bad)] mt-2">{error.app_id}: {error.message}</p>)}
            {!report.updates.length ? <p className="text-[var(--muted)] mt-2">{report.errors.length || report.unsupported_count ? "No confirmed updates. Some apps could not be checked." : "Everything is up to date."}</p> : null}
          </>
        )}
      </Card>
    </div>
  );
}
