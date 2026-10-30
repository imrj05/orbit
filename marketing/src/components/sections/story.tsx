import type { ReactNode } from "react";
import { Band, Shell } from "@/components/ui";

/**
 * A storytelling band: one clear statement and one large visual, alternating
 * which side each sits on. No feature grids — spacing and scale do the work.
 */
export function Story({
  id,
  title,
  copy,
  points,
  visual,
  flip = false,
}: {
  id: string;
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
          <h2 className="mt-0 max-w-[24ch] text-balance text-[clamp(30px,3.2vw,40px)] font-semibold leading-[1.18] tracking-[-0.024em] text-ink">
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
