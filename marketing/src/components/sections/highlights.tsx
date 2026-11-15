import type { ReactNode } from "react";

import { SectionHead } from "@/components/sections/section-head";
import {
  ApprovalPicture,
  ModelsPicture,
  ModesPicture,
  ReviewPicture,
} from "@/components/stageshow/demos/bento";
import { Band, Shell } from "@/components/ui";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

/**
 * The feature grid under the hero. Planned as one picture (Stageshow's bento
 * rules): the approval bar leads at two columns and is the only cursor story;
 * a component, a stat and two more components fill the rest, alternating kind
 * so no two neighbours say the same thing. Copy is page type, outside the
 * stages; each picture is its own Stage at 1:1.
 */
export async function Highlights({ locale }: { locale: Locale }) {
  const t = getT(locale, "highlights");

  return (
    <Shell>
      <Band id="highlights" dashed className="scroll-mt-20 py-20 sm:py-28">
        <SectionHead eyebrow={t("eyebrow")} title={t("title")} copy={t("copy")} />

        <div className="mt-12 grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          <Tile
            className="md:col-span-2"
            lead={t("approve.lead")}
            rest={t("approve.rest")}
            picture={<ApprovalPicture />}
          />
          <Tile lead={t("modes.lead")} rest={t("modes.rest")} picture={<ModesPicture />} />
          <Tile lead={t("review.lead")} rest={t("review.rest")} picture={<ReviewPicture />} />
          <StatTile value={t("stat.value")} label={t("stat.label")} rest={t("stat.rest")} />
          <Tile lead={t("models.lead")} rest={t("models.rest")} picture={<ModelsPicture />} />
        </div>
      </Band>
    </Shell>
  );
}

function Tile({
  lead,
  rest,
  picture,
  className = "",
}: {
  lead: string;
  rest: string;
  picture: ReactNode;
  className?: string;
}) {
  return (
    <article
      className={`flex flex-col overflow-hidden rounded-[18px] border border-edge-default bg-surface-1 ${className}`}
    >
      <p className="m-0 px-7 pt-7 pb-5 text-[16px] leading-[1.55] text-ink-3">
        <strong className="font-semibold text-ink">{lead}</strong> {rest}
      </p>
      <div className="mt-auto">{picture}</div>
    </article>
  );
}

function StatTile({ value, label, rest }: { value: string; label: string; rest: string }) {
  return (
    <article className="flex flex-col justify-center gap-6 overflow-hidden rounded-[18px] border border-edge-default bg-surface-1 p-7">
      <div className="flex items-baseline gap-3 whitespace-nowrap">
        <span className="text-[88px] font-semibold leading-[0.9] tracking-[-0.05em] text-ink">
          {value}
        </span>
        <span className="text-[20px] font-medium tracking-[-0.01em] text-ink-2">{label}</span>
      </div>
      <p className="m-0 max-w-[30ch] text-[16px] leading-[1.55] text-ink-3">{rest}</p>
    </article>
  );
}
