import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 rounded-md font-mono text-[13px] px-3 py-1.5 border transition-colors cursor-pointer disabled:opacity-50",
  {
    variants: {
      variant: {
        default: "bg-[var(--accent)] text-black border-transparent hover:brightness-110",
        outline: "bg-transparent border-[var(--border)] text-[var(--fg)] hover:border-[var(--accent)]",
        ghost: "border-transparent text-[var(--muted)] hover:text-[var(--fg)]",
        danger: "bg-transparent border-[var(--bad)] text-[var(--bad)] hover:bg-[var(--bad)] hover:text-black",
      },
      size: {
        sm: "px-2 py-1 text-[12px]",
        md: "px-3 py-1.5",
      },
    },
    defaultVariants: { variant: "outline", size: "md" },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export function Button({ className, variant, size, ...props }: ButtonProps) {
  return <button className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
