import type { ReactNode } from "react";
import { Band, SectionLabel, Shell } from "@/components/ui";

/**
 * A storytelling band: one clear statement and one large visual, alternating
 * which side each sits on. No feature grids — spacing and scale do the work.
 */
export function Story({
  id,
  label,
  title,
  copy,
  points,
  visual,
  flip = false,
}: {
  id: string;
  label: string;
  title: string;
  copy: string;
  points?: string[];
  visual: ReactNode;
  flip?: boolean;
}) {
  return (
    <Shell>
      <Band
        id={id}
        dashed
        className="grid scroll-mt-20 grid-cols-1 items-center gap-12 py-20 sm:py-28 lg:grid-cols-2 lg:gap-16"
      >
        <div className={flip ? "lg:order-2" : ""}>
          <SectionLabel>{label}</SectionLabel>
          <h2 className="mt-4 max-w-[24ch] text-balance text-[clamp(26px,2.8vw,34px)] font-semibold leading-[1.18] tracking-[-0.024em] text-ink">
            {title}
          </h2>
          <p className="mt-4 max-w-[48ch] text-[14.5px] leading-[1.75] text-ink-2">
            {copy}
          </p>
          {points ? (
            <ul className="mt-7 flex flex-col">
              {points.map((p) => (
                <li
                  key={p}
                  className="border-t border-edge-subtle py-3.5 text-[13px] leading-[1.65] text-ink-2 last:border-b"
                >
                  {p}
                </li>
              ))}
            </ul>
          ) : null}
        </div>
        <div className={flip ? "lg:order-1" : ""}>{visual}</div>
      </Band>
    </Shell>
  );
}

/** A framed product surface for UI mocks — card elevation, nothing more. */
export function MockFrame({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-[14px] border border-edge-default bg-surface-1 p-1.5 shadow-card sm:p-2">
      <div className="rounded-[10px] bg-window px-5 py-6 sm:px-6">
        {children}
      </div>
    </div>
  );
}
