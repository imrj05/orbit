import { GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { LinuxIcon, MacosIcon, WindowsIcon } from "@/components/brand-icons";
import { OrbitField } from "@/components/orbit-field";
import { OsDownloadButton } from "@/components/download-button";
import { Shot } from "@/components/shot";
import { Band, ButtonLink, Shell } from "@/components/ui";
import { getLatestRelease, LATEST_RELEASE_URL } from "@/lib/releases";
import { downloadLabels } from "@/i18n/labels";
import { NO_DOWNLOADS, preferredDownload } from "@/lib/downloads";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

export async function Hero({ locale }: { locale: Locale }) {
  const t = getT(locale, "hero");
  const release = await getLatestRelease();
  const downloads = release?.downloads ?? NO_DOWNLOADS;

  const platforms = [
    {
      icon: <MacosIcon className="size-3.5" />,
      label: t("platforms.macos"),
      href: preferredDownload(downloads, "macos")?.url,
    },
    {
      icon: <WindowsIcon className="size-3.5" />,
      label: t("platforms.windows"),
      href: preferredDownload(downloads, "windows")?.url,
    },
    {
      icon: <LinuxIcon className="size-3.5" />,
      label: t("platforms.linux"),
      href: preferredDownload(downloads, "linux")?.url,
    },
  ];

  return (
    <Shell>
      <Band className="relative overflow-hidden pb-24 pt-16 sm:pb-32 sm:pt-24">
        <OrbitField />
        <div className="relative z-[1] mx-auto flex max-w-[760px] flex-col items-center text-center">

          <h1
            className="rise mt-0 text-balance text-[clamp(36px,5vw,56px)] font-semibold leading-[1.08] tracking-[-0.032em] text-ink"
            style={{ animationDelay: "80ms" }}
          >
            {t("heading")}
          </h1>

          <p
            className="rise mt-6 max-w-[52ch] text-[15px] leading-[1.75] text-ink-2"
            style={{ animationDelay: "140ms" }}
          >
            {t("body")}
          </p>

          <div
            className="rise mt-10 flex w-full flex-col items-stretch justify-center gap-3 sm:w-auto sm:flex-row sm:items-center"
            style={{ animationDelay: "200ms" }}
          >
            <OsDownloadButton
              downloads={downloads}
              fallback={LATEST_RELEASE_URL}
              className="min-h-12 px-6 text-sm sm:min-w-[208px]"
              labels={downloadLabels(locale)}
            />
            <ButtonLink
              href="https://github.com/imrj05/orbit"
              variant="secondary"
              className="min-h-12 px-6 text-sm"
              external
            >
              <HugeiconsIcon icon={GithubIcon} data-icon="inline-start" />
              {t("github")}
            </ButtonLink>
          </div>

          <div
            className="rise mt-6 flex flex-wrap items-center justify-center gap-x-5 gap-y-2"
            style={{ animationDelay: "260ms" }}
          >
            {platforms.map(({ icon, label, href }) => {
              const inner = (
                <>
                  <span className="text-ink-3 transition-colors group-hover:text-ink-2">
                    {icon}
                  </span>
                  <span>{label}</span>
                </>
              );
              const className =
                "group inline-flex items-center gap-1.5 text-[12px] text-ink-3 no-underline transition-colors hover:text-ink-2";

              return href ? (
                <a
                  key={label}
                  href={href}
                  target="_blank"
                  rel="noreferrer"
                  className={className}
                >
                  {inner}
                </a>
              ) : (
                <span key={label} className={className}>
                  {inner}
                </span>
              );
            })}
          </div>
        </div>

        {/* The product is the hero; the frame carries the only deep shadow
         * in the whole design. */}
        <div className="relative z-[1] mt-12 sm:mt-14">
          <div className="rise-window relative mx-auto w-full max-w-[1180px]">
            <Shot
              src="/screens/session.png"
              srcLight="/screens/session-light.png"
              priority
              alt={t("shotAlt")}
            />
          </div>
        </div>
      </Band>
    </Shell>
  );
}
