import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 rounded-md font-medium border transition-[background-color,color,border-color,transform] duration-200 disabled:opacity-45 disabled:cursor-not-allowed",
  {
    variants: {
      variant: {
        default: "bg-[var(--fg)] text-[var(--bg)] border-transparent hover:opacity-90",
        accent: "bg-[var(--accent)] text-[var(--on-accent)] border-transparent hover:brightness-110",
        soft: "bg-[var(--panel2)] text-[var(--fg)] border-[var(--border)] hover:border-[var(--muted)]",
        outline: "bg-transparent border-[var(--border)] text-[var(--fg)] hover:border-[var(--muted)]",
        ghost: "border-transparent text-[var(--muted)] hover:text-[var(--fg)] hover:bg-[var(--panel2)]",
        danger: "bg-transparent border-[var(--bad)] text-[var(--bad)] hover:bg-[var(--bad)] hover:text-[var(--bg)]",
      },
      size: {
        sm: "px-3 h-8 text-[13px]",
        md: "px-4 h-10 text-[14px]",
        lg: "px-6 h-12 text-[16px]",
        icon: "size-9 text-[14px]",
      },
    },
    defaultVariants: { variant: "soft", size: "md" },
  }
);

export type ButtonProps = React.ComponentProps<"button"> & VariantProps<typeof buttonVariants>;

export function Button({ className, variant, size, type = "button", ...props }: ButtonProps) {
  return <button type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
