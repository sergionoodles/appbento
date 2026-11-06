import { useState } from "react";
import { TriangleAlert, X } from "lucide-react";
import type { DbusRule, DesktopIntegration, SandboxPermissions, SandboxStatus } from "@/lib/types";
import { isDesktopIntegrationEnabled, validateDbusName } from "@/lib/sandbox";
import { Button } from "../../ui/button";
import { Input } from "../../ui/card";
import { FieldGroup, Notice } from "../../ui/field";
import { Select, type SelectOption } from "../../ui/select";
import { Switch } from "../../ui/switch";

type DbusAccess = DbusRule["access"];
type IntegrationKey = Exclude<keyof DesktopIntegration, "custom">;

const INTEGRATIONS: { key: IntegrationKey; label: string; description: string }[] = [
  { key: "portals", label: "File dialogs, links and screen sharing", description: "Use your desktop's file picker, open links in your browser, and request screen sharing. Your desktop asks before sharing files or your screen." },
  { key: "notifications", label: "Notifications", description: "Show desktop notifications." },
  { key: "tray", label: "Tray icon", description: "Show an icon in your panel's system tray." },
  { key: "media_controls", label: "Media controls", description: "Appear in your desktop's media controls and respond to media keys." },
  { key: "secrets", label: "Saved passwords (keyring)", description: "Read and store passwords in your system keyring." },
];

const DBUS_ACCESS_OPTIONS: SelectOption<DbusAccess>[] = [
  { value: "talk", label: "Talk to service" },
  { value: "own", label: "Own name" },
];

function DbusAccessSelect({ value, onChange, ...props }: { value: DbusAccess; onChange: (access: DbusAccess) => void; "aria-label": string; disabled: boolean; className?: string }) {
  return <Select value={value} options={DBUS_ACCESS_OPTIONS} onChange={onChange} {...props} />;
}

export function DesktopIntegrationSettings({ permissions, status, isSaving, onChange, onError }: {
  permissions: SandboxPermissions;
  status: SandboxStatus | null;
  isSaving: boolean;
  onChange: (permissions: SandboxPermissions) => void;
  onError: (error: string | null) => void;
}) {
  const integration = permissions.desktop_integration;
  const setIntegration = (next: DesktopIntegration) => onChange({ ...permissions, desktop_integration: next });
  const isUnavailable = status !== null && !status.desktop_integration_available && isDesktopIntegrationEnabled(integration);

  return (
    <>
      {isUnavailable ? <UnavailableNotice error={status.desktop_integration_error} /> : null}
      <FieldGroup title="Desktop integration" description="Lets the app use desktop services through a filtered connection. Everything else on the session bus stays hidden.">
        {INTEGRATIONS.map(({ key, label, description }) => (
          <div key={key}>
            <label className="flex items-center gap-4 px-4 py-3">
              <span className="flex-1 min-w-0">
                <span className="block text-[14px]">{label}</span>
                <span id={`desktop-${key}`} className="block text-[12.5px] leading-snug text-[var(--muted)]">{description}</span>
              </span>
              <Switch aria-label={label} aria-describedby={`desktop-${key}`} disabled={isSaving} checked={integration[key]}
                onChange={(event) => setIntegration({ ...integration, [key]: event.target.checked })} />
            </label>
            {key === "secrets" && integration.secrets
              ? <div className="px-4 pb-3"><Notice tone="warn" icon={<TriangleAlert size={16} />}>This app can read every password in your keyring, including those saved by other apps.</Notice></div>
              : null}
          </div>
        ))}
        <CustomDbusNames integration={integration} isSaving={isSaving} onChange={setIntegration} onError={onError} />
      </FieldGroup>
    </>
  );
}

function UnavailableNotice({ error }: { error: string | null }) {
  return (
    <Notice tone="warn" icon={<TriangleAlert size={16} />}>
      <p>Desktop integration is unavailable{error ? `: ${error}` : "."}</p>
      <div className="mt-1.5 text-[var(--muted)]">
        <p>The app won't launch while these are on. On Arch Linux, install xdg-dbus-proxy:</p>
        <code className="block my-2 select-all rounded-md bg-[var(--bg)] px-3 py-2 text-[var(--fg)]">sudo pacman -S xdg-dbus-proxy</code>
      </div>
    </Notice>
  );
}

function CustomDbusNames({ integration, isSaving, onChange, onError }: {
  integration: DesktopIntegration;
  isSaving: boolean;
  onChange: (integration: DesktopIntegration) => void;
  onError: (error: string | null) => void;
}) {
  const [name, setName] = useState("");
  const [access, setAccess] = useState<DbusAccess>("talk");
  const { custom } = integration;

  const addName = () => {
    const trimmed = name.trim();
    const validation = validateDbusName(trimmed, custom, access);
    onError(validation);
    if (validation) return;
    onChange({ ...integration, custom: [...custom, { name: trimmed, access }] });
    setName("");
  };

  return (
    <details open={custom.length > 0 || undefined}>
      <summary className="cursor-pointer px-4 py-3 text-[13px] text-[var(--muted)]">Advanced: custom D-Bus names{custom.length > 0 ? ` (${custom.length})` : ""}</summary>
      {custom.map((rule, index) => (
        <div key={`${rule.name}:${rule.access}`} className="flex items-center gap-3 px-4 py-2">
          <span className="flex-1 min-w-0 truncate font-mono text-[13px]" title={rule.name}>{rule.name}</span>
          <DbusAccessSelect aria-label={`Access for ${rule.name}`} className="w-40" value={rule.access} disabled={isSaving}
            onChange={(selected) => onChange({ ...integration, custom: custom.map((entry, entryIndex) => entryIndex === index ? { ...entry, access: selected } : entry) })} />
          <Button variant="ghost" size="icon" aria-label={`Remove D-Bus name ${rule.name}`} disabled={isSaving}
            onClick={() => onChange({ ...integration, custom: custom.filter((_, entryIndex) => entryIndex !== index) })}><X size={15} /></Button>
        </div>
      ))}
      <form className="flex items-center gap-2 p-3" onSubmit={(event) => {
        event.preventDefault();
        if (name.trim()) addName();
      }}>
        <Input aria-label="D-Bus name" className="flex-1 font-mono text-[13px]" value={name} disabled={isSaving} placeholder="org.example.Service" onChange={(event) => setName(event.target.value)} />
        <DbusAccessSelect aria-label="New D-Bus name access" className="w-40" value={access} disabled={isSaving} onChange={setAccess} />
        <Button type="submit" disabled={isSaving || !name.trim()}>Add name</Button>
      </form>
    </details>
  );
}
