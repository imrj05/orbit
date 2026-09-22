import Link from "next/link";
import { Download01Icon, GithubIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { OsDownloadButton } from "@/components/download-button";
import { Band, ButtonLink, Shell } from "@/components/ui";
import { getLatestRelease, LATEST_RELEASE_URL } from "@/lib/releases";

export async function Closer() {
  const release = await getLatestRelease();
  const assets = {
    macos: release?.macos,
    windows: release?.windows,
    linux: release?.linux,
  };
  const platforms = [
    { label: "macOS", href: release?.macos ?? LATEST_RELEASE_URL },
    { label: "Windows", href: release?.windows ?? LATEST_RELEASE_URL },
    { label: "Linux", href: release?.linux ?? LATEST_RELEASE_URL },
  ];

  return (
    <Shell>
      <Band dashed className="flex flex-col items-center py-16 text-center sm:py-[88px]">
        <h2 className="mb-4 max-w-[22ch] text-[clamp(30px,3.8vw,44px)] font-[250] leading-[1.14] tracking-[-0.015em] text-ink">
          Start it tonight. Keep your sessions.
        </h2>
        <p className="mb-8 max-w-[52ch] text-[15px] leading-[1.7] text-ink-2">
          Point Orbit at the projects you already run pi on. Same sessions, same
          models, same tools — just drawn on the GPU.
        </p>
        <div className="flex flex-wrap items-center justify-center gap-3.5">
          <OsDownloadButton
            assets={assets}
            fallback={LATEST_RELEASE_URL}
            className="min-w-[184px]"
          />
          <ButtonLink
            href="https://github.com/imrj05/orbit"
            variant="secondary"
            external
          >
            <HugeiconsIcon icon={GithubIcon} data-icon="inline-start" />
            View on GitHub
          </ButtonLink>
        </div>
        <div className="mt-4 flex flex-wrap items-center justify-center gap-x-4 gap-y-2 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
          {platforms.map(({ label, href }) => (
            <a
              key={label}
              href={href}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 no-underline transition-colors hover:text-ink"
            >
              <HugeiconsIcon
                icon={Download01Icon}
                className="size-3.5"
                strokeWidth={1.8}
              />
              {label}
            </a>
          ))}
          {release ? (
            <Link
              href={`/changelog/${release.tag}`}
              className="no-underline transition-colors hover:text-ink"
            >
              Release notes · {release.tag}
            </Link>
          ) : null}
        </div>
      </Band>
    </Shell>
  );
}
