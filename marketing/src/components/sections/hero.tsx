import Link from "next/link";
import {
  AppleIcon,
  ArrowRight01Icon,
  ComputerIcon,
  GitCompareIcon,
  GithubIcon,
  MicrosoftIcon,
  PlugIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { OrbitIcon } from "@/components/brand";
import { LinuxIcon } from "@/components/brand-icons";
import { OsDownloadButton, OsLink } from "@/components/download-button";
import { Band, ButtonLink, SectionLabel, Shell } from "@/components/ui";
import { getLatestRelease, LATEST_RELEASE_URL } from "@/lib/releases";

export async function Hero() {
  const release = await getLatestRelease();
  const assets = {
    macos: release?.macos,
    windows: release?.windows,
    linux: release?.linux,
  };

  const platforms = [
    {
      icon: <HugeiconsIcon icon={AppleIcon} className="size-3.5 shrink-0" />,
      label: "macOS 13+",
      meta: "dmg",
      href: release?.macos,
    },
    {
      icon: <HugeiconsIcon icon={MicrosoftIcon} className="size-3.5 shrink-0" />,
      label: "Windows",
      meta: "exe",
      href: release?.windows,
    },
    {
      icon: <LinuxIcon className="size-3.5 shrink-0" />,
      label: "Linux",
      meta: "deb",
      href: release?.linux,
    },
  ];

  const surfaces = [
    {
      Icon: ComputerIcon,
      name: "Desktop",
      line: "A native workbench for pi",
      link: release ? `Download ${release.tag}` : "Download",
      assets,
      fallback: LATEST_RELEASE_URL,
      external: true,
    },
    {
      Icon: PlugIcon,
      name: "Providers",
      line: "Ollama, Anthropic, Bedrock",
      link: "See providers",
      assets: {},
      fallback: "#workbench",
      external: false,
    },
    {
      Icon: GitCompareIcon,
      name: "Git",
      line: "Review, commit, and push",
      link: "See the flow",
      assets: {},
      fallback: "#gallery",
      external: false,
    },
  ];

  return (
    <Shell>
      <Band
        dashed
        className="relative overflow-hidden pb-16 pt-16 sm:pb-20 sm:pt-24"
      >
        {/* Ambient glow + halftone texture. */}
        <div
          aria-hidden
          className="pointer-events-none absolute left-1/2 top-[-180px] h-[560px] w-[900px] max-w-none -translate-x-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(91,157,255,0.09),transparent)]"
        />
        <span
          aria-hidden
          className="dither dither-tl left-0 top-0 h-[220px] w-[320px]"
        />
        <span
          aria-hidden
          className="dither dither-br bottom-0 right-0 h-[220px] w-[320px]"
        />

        <div className="relative z-[1] mx-auto flex max-w-[880px] flex-col items-center text-center">
          {release ? (
            <Link
              href={`/changelog/${release.tag}`}
              className="rise group inline-flex items-center gap-2.5 rounded-full border border-hair bg-tint-05 py-1.5 pl-2 pr-3.5 text-[12.5px] no-underline backdrop-blur transition-colors hover:border-tint-26 hover:bg-tint-09"
            >
              <span className="inline-flex items-center gap-1.5 rounded-full bg-brand/12 px-2.5 py-1 font-mono text-[10.5px] uppercase tracking-[0.1em] text-brand">
                <span className="pulse-dot size-1.5 rounded-full bg-brand" />
                {release.tag}
              </span>
              <span className="text-ink-2 transition-colors group-hover:text-ink">
                Release notes
              </span>
              <HugeiconsIcon
                icon={ArrowRight01Icon}
                className="size-3.5 text-ink-3 transition-transform duration-200 group-hover:translate-x-0.5 group-hover:text-ink"
              />
            </Link>
          ) : (
            <SectionLabel className="rise">Sessions · Providers · Git</SectionLabel>
          )}

          <h1
            className="rise mt-7 text-balance text-[clamp(36px,5vw,58px)] font-[250] leading-[1.07] tracking-[-0.03em] text-ink"
            style={{ animationDelay: "80ms" }}
          >
            Get
            <OrbitIcon
              alt=""
              className="mx-[0.16em] inline-block h-[0.82em] w-auto shrink-0 align-[-0.09em]"
            />
            <span className="sr-only">Orbit</span>
            and run pi on{" "}
            <span className="text-ink-2">
              desktop, terminal, and every workspace.
            </span>
          </h1>

          <p
            className="rise mt-6 max-w-[50ch] text-[14.5px] leading-[1.75] text-ink-2"
            style={{ animationDelay: "140ms" }}
          >
            A native workbench for the pi coding agent. Your sessions, your
            models, your machine — chat, review, and commit in one window.
          </p>

          <div
            className="rise mt-9 flex w-full flex-col items-stretch justify-center gap-3.5 sm:w-auto sm:flex-row sm:items-center"
            style={{ animationDelay: "200ms" }}
          >
            <OsDownloadButton
              assets={assets}
              fallback={LATEST_RELEASE_URL}
              className="min-h-11 gap-[9px] px-5 text-[13px] sm:min-w-[196px]"
            />
            <ButtonLink
              href="https://github.com/imrj05/orbit"
              variant="secondary"
              className="min-h-11 gap-[9px] px-5 text-[13px]"
              external
            >
              <HugeiconsIcon icon={GithubIcon} data-icon="inline-start" />
              View on GitHub
            </ButtonLink>
          </div>

          <div
            className="rise mt-7 flex flex-wrap items-center justify-center gap-2.5"
            style={{ animationDelay: "260ms" }}
          >
            {platforms.map(({ icon, label, meta, href }) => {
              const inner = (
                <>
                  <span className="text-ink-3 transition-colors group-hover:text-ink-2">
                    {icon}
                  </span>
                  <span>{label}</span>
                  <span className="font-mono text-[9.5px] uppercase tracking-[0.1em] text-ink-3">
                    {meta}
                  </span>
                </>
              );
              const className =
                "group inline-flex items-center gap-2 rounded-full border border-hair bg-tint-03 px-3 py-1.5 text-[11.5px] text-ink-2 no-underline transition-colors hover:border-tint-26 hover:bg-tint-07 hover:text-ink";

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

        <div
          className="rise relative z-[1] mx-auto mt-14 grid w-full max-w-[980px] grid-cols-1 overflow-hidden rounded-2xl border border-hair bg-tint-02 sm:mt-16 sm:grid-cols-3"
          style={{ animationDelay: "340ms" }}
        >
          {surfaces.map(
            (
              { Icon, name, line, link, assets: linkAssets, fallback, external },
              index,
            ) => (
              <OsLink
                key={name}
                assets={linkAssets}
                fallback={fallback}
                external={external}
                className={`group relative flex flex-col items-center gap-2.5 px-7 py-8 text-center no-underline transition-colors hover:bg-tint-03 ${
                  index > 0
                    ? "border-t border-dashed border-tint-1c sm:border-l sm:border-t-0 sm:border-solid sm:border-hair"
                    : ""
                }`}
              >
                <span className="inline-flex size-11 items-center justify-center rounded-xl bg-well text-ink-3 ring-1 ring-hair transition-colors duration-200 group-hover:text-ink group-hover:ring-tint-26">
                  <HugeiconsIcon icon={Icon} className="size-5" />
                </span>
                <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-ink-3">
                  {name}
                </span>
                <span className="text-[13.5px] text-ink-2 transition-colors duration-200 group-hover:text-ink">
                  {line}
                </span>
                <span className="mt-0.5 inline-flex items-center gap-1 text-[13px] text-ink">
                  {link}
                  <HugeiconsIcon
                    icon={ArrowRight01Icon}
                    className="size-3.5 transition-transform duration-200 group-hover:translate-x-0.5"
                  />
                </span>
              </OsLink>
            ),
          )}
        </div>
      </Band>
    </Shell>
  );
}
