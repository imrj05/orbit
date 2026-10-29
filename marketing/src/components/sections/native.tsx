import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Native({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.native");

  return (
    <Story
      id="native"
      title={t("title")}
      copy={t("copy")}
      points={t.raw("points") as string[]}
      visual={
        <Shot
          src="/screens/native-access.png"
          srcLight="/screens/native-access-light.png"
          alt={t("shotAlt")}
          width={2439}
          height={1368}
          sizes="(max-width: 1024px) 100vw, 560px"
        />
      }
    />
  );
}
