import type { ReactNode } from "react";
import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function Shell({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return <div className={`shell ${className}`}>{children}</div>;
}

export function Band({
  children,
  className = "",
  dashed = false,
  id,
}: {
  children: ReactNode;
  className?: string;
  dashed?: boolean;
  id?: string;
}) {
  return (
    <section
      id={id}
      className={`band ${dashed ? "band-dashed" : ""} ${className}`}
    >
      {children}
    </section>
  );
}

/** Orbit's button intents mapped onto shadcn's `buttonVariants`. */
const buttonIntents = {
  primary: "default",
  secondary: "outline",
  nav: "default",
  text: "ghost",
} as const;

export function ButtonLink({
  href,
  children,
  variant = "primary",
  className = "",
  external = false,
}: {
  href: string;
  children: ReactNode;
  variant?: keyof typeof buttonIntents;
  className?: string;
  external?: boolean;
}) {
  return (
    <a
      href={href}
      className={cn(
        buttonVariants({
          variant: buttonIntents[variant],
          size: variant === "nav" ? "sm" : "lg",
        }),
        variant === "primary" &&
          "min-h-10 gap-[9px] px-[18px] text-[12px]",
        variant === "secondary" && "min-h-10 gap-[9px] px-[18px] text-[12px]",
        variant === "nav" && "rounded-full px-[15px] text-[13.5px]",
        variant === "text" &&
          "px-0 text-[13px] text-ink-2 hover:bg-transparent hover:text-ink",
        className,
      )}
      {...(external ? { target: "_blank", rel: "noreferrer" } : {})}
    >
      {children}
    </a>
  );
}

export function SectionLabel({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <p
      className={`flex items-center gap-[9px] font-mono text-[10px] uppercase leading-[1.4] tracking-[0.065em] text-ink-2 ${className}`}
    >
      <i className="pulse-dot size-[5px] shrink-0 rounded-full bg-brand" />
      {children}
    </p>
  );
}

export function Eyebrow({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <p
      className={`font-mono text-[11.5px] uppercase leading-normal tracking-[0.16em] text-ink-3 ${className}`}
    >
      {children}
    </p>
  );
}
