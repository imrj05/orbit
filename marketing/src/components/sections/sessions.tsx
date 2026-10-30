import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Sessions({ locale }: { locale: Locale }) {
  const t = getT(locale, "sections.sessions");

  return (
    <Story
      id="sessions"
      title={t("title")}
      copy={t("copy")}
      flip
      visual={
        <Shot
          src="/screens/sessions-detail.png"
          srcLight="/screens/sessions-detail-light.png"
          alt={t("shotAlt")}
          width={500}
          height={1150}
          className="mx-auto w-full max-w-[360px]"
          sizes="(max-width: 1024px) 100vw, 360px"
        />
      }
    />
  );
}
