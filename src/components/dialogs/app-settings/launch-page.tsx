import type { LaunchDraft } from "@/lib/launch-draft";
import { Field } from "../../ui/field";

const TEXTAREA_CLASS = "w-full rounded-md border border-[var(--border)] bg-[var(--panel2)] px-4 py-3 text-[14px] text-[var(--fg)] placeholder:text-[var(--muted)] outline-none focus:border-[var(--accent)] font-mono resize-y";

export function LaunchPage({ draft, onChange, isSaving }: {
  draft: LaunchDraft;
  onChange: (draft: LaunchDraft) => void;
  isSaving: boolean;
}) {
  return (
    <div className="flex flex-col gap-7">
      <Field label="Command-line arguments" htmlFor="launch-arguments" hint="One argument per line. Spaces are preserved; do not add shell quotes. These are passed before any files or URLs opened with the app.">
        <textarea id="launch-arguments" rows={5} className={TEXTAREA_CLASS} placeholder={"--disable-gpu\n--profile\nMy profile"}
          value={draft.arguments} disabled={isSaving} onChange={(event) => onChange({ ...draft, arguments: event.target.value })} />
      </Field>
      <Field label="Environment variables" htmlFor="launch-environment" hint="One NAME=value per line. Values are literal, including spaces and equals signs. An empty value is allowed. These override the app’s environment in both launch modes.">
        <textarea id="launch-environment" rows={5} className={TEXTAREA_CLASS} placeholder={"GTK_THEME=Adwaita:dark\nLANG=en_US.UTF-8"}
          value={draft.environment} disabled={isSaving} onChange={(event) => onChange({ ...draft, environment: event.target.value })} />
      </Field>
    </div>
  );
}
