import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Agent({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.agent");

  return (
    <Story
      id="agent"
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      visual={
        <Shot
          src="/screens/agent-detail.png"
          srcLight="/screens/agent-detail-light.png"
          alt={t("shotAlt")}
          width={1200}
          height={900}
          sizes="(max-width: 1024px) 100vw, 560px"
        />
      }
    />
  );
}
