import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Models({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.providers");

  return (
    <Story
      id="providers"
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      flip
      visual={
        <Shot
          src="/screens/models.png"
          srcLight="/screens/models-light.png"
          alt={t("shotAlt")}
          sizes="(max-width: 1024px) 100vw, 560px"
        />
      }
    />
  );
}
