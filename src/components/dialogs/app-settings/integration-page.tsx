import type { InstalledApp } from "@/lib/types";
import { Button } from "../../ui/button";
import { FieldGroup } from "../../ui/field";

export function IntegrationPage({ app, isSaving, onSetMenuEntry }: {
  app: InstalledApp;
  isSaving: boolean;
  onSetMenuEntry: (enabled: boolean) => Promise<boolean>;
}) {
  const hasMenuEntry = app.desktop_id !== null;

  return (
    <FieldGroup title="System app menu">
      <div className="flex flex-col gap-4 p-4">
        <p className="text-[14px]">{hasMenuEntry ? "This app has a shortcut in your system app menu." : "This app has no shortcut in your system app menu."}</p>
        <p className="text-[13px] text-[var(--muted)]">Add a shortcut to launch this app using its saved permissions, arguments, and environment variables. Changes to launch settings update the shortcut automatically.</p>
        <p className="text-[13px] text-[var(--muted)]">Removing the shortcut keeps the AppImage and its library entry.</p>
        <Button variant="outline" className="self-start" disabled={isSaving} onClick={() => void onSetMenuEntry(!hasMenuEntry)}>
          {hasMenuEntry ? "Remove from app menu" : "Add to app menu"}
        </Button>
      </div>
    </FieldGroup>
  );
}
