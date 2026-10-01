import type { ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { Button } from "./ui/button";

export function StageHeader({ title, onBack, children }: { title: string; onBack: () => void; children?: ReactNode }) {
  return (
    <div className="flex items-center gap-3 mb-5">
      <Button variant="soft" size="icon" aria-label="Back to all apps" onClick={onBack}><ArrowLeft size={17} /></Button>
      <h1 className="text-[28px] font-bold tracking-[-0.035em]">{title}</h1>
      <div className="ml-auto flex items-center gap-2">{children}</div>
    </div>
  );
}
