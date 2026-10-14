import { LogoMark } from "../logo";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";

export function SelfLauncherDialog({ isSaving, error, onAdd, onClose, returnFocusElement }: {
  isSaving: boolean;
  error: string | null;
  onAdd: () => void;
  onClose: () => void;
  returnFocusElement: HTMLElement | null;
}) {
  return <Dialog title="Add AppBento to your system launcher?" onClose={onClose} canClose={!isSaving} returnFocusElement={returnFocusElement}>
    <div className="flex items-start gap-4">
      <LogoMark className="size-12 shrink-0" />
      <p className="text-[14px] text-[var(--muted)]">Open AppBento from your system’s app menu without finding the AppImage file. You can also add it later from Settings.</p>
    </div>
    {error ? <p role="alert" className="text-[14px] text-[var(--bad)] selectable">{error}</p> : null}
    <div className="flex justify-end gap-2">
      <Button variant="ghost" disabled={isSaving} onClick={onClose}>Not now</Button>
      <Button disabled={isSaving} onClick={onAdd}>{isSaving ? "Saving…" : "Add to launcher"}</Button>
    </div>
  </Dialog>;
}
