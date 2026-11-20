import {
  BubbleChatIcon,
  ComputerTerminal01Icon,
  DashboardSquare01Icon,
  GitBranchIcon,
  Layers01Icon,
  Shield01Icon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { SectionHead } from "@/components/sections/section-head";
import { Band, Shell } from "@/components/ui";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

/**
 * The detailed index: everything the tour can't show in one screenshot, as a
 * ruled grid of six groups. Every line is a shipped behaviour from the README's
 * feature list — nothing here is roadmap.
 */
const ICONS = [
  BubbleChatIcon,
  Layers01Icon,
  Shield01Icon,
  GitBranchIcon,
  ComputerTerminal01Icon,
  DashboardSquare01Icon,
];

export async function Capabilities({ locale }: { locale: Locale }) {
  const t = getT(locale, "capabilities");
  const groups = t.raw("groups") as { title: string; items: string[] }[];

  return (
    <Shell>
      <Band id="capabilities" dashed className="scroll-mt-20 py-20 sm:py-28">
        <SectionHead eyebrow={t("eyebrow")} title={t("title")} copy={t("copy")} />

        <div className="mt-12 grid grid-cols-1 gap-px overflow-hidden rounded-[18px] border border-edge-default bg-edge-subtle sm:grid-cols-2 lg:grid-cols-3">
          {groups.map((group, index) => {
            const icon = ICONS[index] ?? Layers01Icon;
            return (
              <section key={group.title} className="bg-page p-7">
                <h3 className="m-0 flex items-center gap-2.5 text-[15px] font-semibold tracking-[-0.01em] text-ink">
                  <span className="grid size-8 place-items-center rounded-[9px] border border-edge-default bg-surface-2">
                    <HugeiconsIcon icon={icon} className="size-4 text-ink-2" strokeWidth={1.7} />
                  </span>
                  {group.title}
                </h3>
                <ul className="mt-5 flex flex-col gap-2.5">
                  {group.items.map((item) => (
                    <li key={item} className="flex gap-3 text-[13px] leading-[1.6] text-ink-2">
                      <span aria-hidden className="mt-[8px] size-1 shrink-0 rounded-full bg-ink-3" />
                      {item}
                    </li>
                  ))}
                </ul>
              </section>
            );
          })}
        </div>
      </Band>
    </Shell>
  );
}
