import { CircleCheck, Loader2, ShieldCheck, TriangleAlert } from "lucide-react";
import { Button } from "../../ui/button";

type SaveStatusProps = {
  isVerifying: boolean;
  needsReview: boolean;
  verificationError: string | null;
  isSaving: boolean;
  isUpdating: boolean;
  operation: string | null;
  isLoadingDefaults: boolean;
  hasChanges: boolean;
  onRetry: () => void;
};

const OPERATION_STATUSES: Record<string, { title: string; detail: string }> = {
  "Verifying app file and starting app": { title: "Verifying and starting app…", detail: "Settings saved. Checking the approved app file before launch. Large apps can take a few seconds." },
  "Preparing app launch": { title: "Preparing app launch…", detail: "Settings saved. Checking that the app is ready to run." },
  "Save update source": { title: "Saving update source…", detail: "Writing your update settings to disk." },
  "Save icon": { title: "Saving app icon…", detail: "Loading and saving the image. Remote images can take a few seconds." },
  "Add to app menu": { title: "Updating app menu…", detail: "Creating the system app shortcut." },
  "Remove from app menu": { title: "Updating app menu…", detail: "Removing the system app shortcut." },
  "Check app updates": { title: "Checking for app updates…", detail: "Contacting the update source. This can take a few seconds." },
};

function statusContent(props: SaveStatusProps) {
  if (props.isSaving) {
    const status = OPERATION_STATUSES[props.operation ?? ""] ?? { title: "Saving settings…", detail: "Writing your changes to disk." };
    return { ...status, icon: Loader2, isBusy: true };
  }
  if (props.isUpdating) {
    return { title: "Installing app update…", detail: "Downloading and verifying the new app file. Settings can be saved when the update finishes.", icon: Loader2, isBusy: true };
  }
  if (props.isVerifying) {
    return { title: "Verifying app file…", detail: "Checking the file you’re approving. Large apps can take a few seconds. You can keep editing.", icon: Loader2, isBusy: true };
  }
  if (props.verificationError) {
    return { title: "File verification failed", detail: "Approval is blocked. Check that the app file is accessible, then retry.", icon: TriangleAlert, isBusy: false };
  }
  if (props.isLoadingDefaults) {
    return { title: "Loading default permissions…", detail: "Preparing the app’s initial settings.", icon: Loader2, isBusy: true };
  }
  if (props.needsReview) {
    return { title: "Ready to approve", detail: "File verification complete. Save to confirm these permissions.", icon: ShieldCheck, isBusy: false };
  }
  return { title: props.hasChanges ? "Ready to save" : "No unsaved changes", detail: "File approval is preserved. The app file is checked again before launch.", icon: CircleCheck, isBusy: false };
}

export function SaveStatus(props: SaveStatusProps) {
  const status = statusContent(props);
  const Icon = status.icon;
  const color = props.verificationError ? "text-[var(--bad)]" : "text-[var(--muted)]";
  const iconClass = status.isBusy ? "motion-safe:animate-spin text-[var(--accent)]" : color;
  return (
    <div id="app-settings-save-status" role="status" aria-live="polite" aria-atomic="true" className="flex min-w-0 items-start gap-2.5">
      <Icon aria-hidden="true" size={16} className={`mt-0.5 shrink-0 ${iconClass}`} />
      <div className="min-w-0">
        <p className="text-[13px] font-medium text-[var(--fg)]">{status.title}</p>
        <p className="mt-0.5 text-[12px] leading-relaxed text-[var(--muted)]">{status.detail}</p>
        {props.verificationError ? <Button variant="outline" size="sm" className="mt-2" onClick={props.onRetry}>Retry verification</Button> : null}
      </div>
    </div>
  );
}
