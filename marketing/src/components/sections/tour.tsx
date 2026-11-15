import { TourTabs, type TourItem } from "@/components/sections/tour-tabs";
import { SectionHead } from "@/components/sections/section-head";
import { Band, Shell } from "@/components/ui";
import { getT } from "@/i18n/t";
import { messagesFor } from "@/i18n/messages";
import type { Locale } from "@/i18n/config";

/**
 * Each tab: the message namespace under `sections.*`, its anchor, and the real
 * screenshot pair. Order follows a first session: the agent, where sessions
 * live, starting a task, then the workbench around it.
 */
const TABS: {
  key: string;
  id: string;
  src: string;
  width?: number;
  height?: number;
  narrow?: boolean;
}[] = [
  { key: "agent", id: "agent", src: "agent-detail", width: 1200, height: 900 },
  { key: "sessions", id: "sessions", src: "sessions-detail", width: 500, height: 1150, narrow: true },
  { key: "newTask", id: "new-task", src: "new-task" },
  { key: "git", id: "git", src: "review" },
  { key: "providers", id: "providers", src: "models" },
  { key: "skills", id: "skills", src: "skills" },
  { key: "mcp", id: "mcp", src: "mcp" },
  { key: "plugins", id: "plugins", src: "plugins" },
  { key: "usage", id: "usage", src: "usage" },
  { key: "appearance", id: "appearance", src: "appearance" },
];

export async function Tour({ locale }: { locale: Locale }) {
  const t = getT(locale, "tour");
  const sections = getT(locale, "sections");
  // Not every section has points (Sessions is copy only); read the merged
  // dictionary rather than asking the translator for a key that may not exist.
  const raw = messagesFor(locale).sections as Record<string, { points?: string[] }>;

  const items: TourItem[] = TABS.map(({ key, id, src, width = 3164, height = 2068, narrow }) => ({
    id,
    label: sections(`${key}.label`),
    title: sections(`${key}.title`),
    copy: sections(`${key}.copy`),
    points: raw[key]?.points ?? [],
    shot: {
      src: `/screens/${src}.png`,
      srcLight: `/screens/${src}-light.png`,
      alt: sections(`${key}.shotAlt`),
      width,
      height,
      narrow,
    },
  }));

  return (
    <Shell>
      <Band id="tour" dashed className="scroll-mt-20 py-20 sm:py-28">
        <SectionHead eyebrow={t("eyebrow")} title={t("title")} copy={t("copy")} />
        <div className="mt-12">
          <TourTabs items={items} label={t("tablist")} />
        </div>
      </Band>
    </Shell>
  );
}
