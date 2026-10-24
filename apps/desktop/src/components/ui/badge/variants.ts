import { cva, type VariantProps } from "class-variance-authority";

export const badgeVariants = cva("inline-flex w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-md border px-2 py-0.5 text-xs font-medium whitespace-nowrap transition-[color,box-shadow]", {
  variants: {
    variant: {
      default: "border-transparent bg-primary text-primary-foreground",
      secondary: "border-transparent bg-secondary text-secondary-foreground",
      destructive: "border-transparent bg-destructive text-white",
      outline: "border-border text-foreground",
      success: "border-transparent bg-[color:var(--success)] text-white",
      warning: "border-transparent bg-[color:var(--warning)] text-white",
    },
  },
  defaultVariants: { variant: "default" },
});

export type BadgeVariants = VariantProps<typeof badgeVariants>;
