import Link from "next/link";
import { ArrowRight01Icon, Download01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { Band, SectionLabel, Shell } from "@/components/ui";
import { getLatestRelease, LATEST_RELEASE_URL } from "@/lib/releases";

const steps = [
  {
    n: "01",
    title: "Connect a provider",
    body: "Sign in or add a key for Anthropic, OpenCode Go, Bedrock, and others — or point Orbit at a local Ollama.",
  },
  {
    n: "02",
    title: "Start a task",
    body: "Pick a workspace from the sidebar and describe what you want in the composer.",
  },
  {
    n: "03",
    title: "Watch it work",
    body: "Thinking, tool calls, and edits stream into the transcript. Steer mid-run or queue a follow-up.",
  },
  {
    n: "04",
    title: "Review the changes",
    body: "Open the Review panel to read every changed file before it lands.",
  },
  {
    n: "05",
    title: "Commit and push",
    body: "Generate a message on the Git page, stage what you want, and commit without leaving Orbit.",
  },
];

export async function Steps() {
  const release = await getLatestRelease();
  const platforms = [
    { label: "macOS", href: release?.macos ?? LATEST_RELEASE_URL },
    { label: "Windows", href: release?.windows ?? LATEST_RELEASE_URL },
    { label: "Linux", href: release?.linux ?? LATEST_RELEASE_URL },
  ];

  return (
    <Shell>
      <Band
        id="install"
        dashed
        className="grid scroll-mt-20 grid-cols-1 items-start gap-7 py-11 sm:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] sm:gap-12 sm:py-[52px]"
      >
        <div>
          <SectionLabel>Get started</SectionLabel>
          <h2 className="mb-3.5 mt-3 text-[clamp(24px,2.4vw,30px)] font-[350] leading-[1.25] tracking-[-0.025em] text-ink">
            From a task to a commit.
          </h2>
          <p className="mb-[18px] text-[13px] leading-[1.8] text-ink-2">
            Orbit runs against your existing pi install. Connect a provider, start
            a task, and the whole loop — chat, review, and commit — stays in one
            window.
          </p>
          <div className="mb-[18px] flex flex-wrap items-center gap-x-3 gap-y-2 font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
            {release ? (
              <Link
                href={`/changelog/${release.tag}`}
                className="rounded-full border border-tint-1c px-2.5 py-1 text-ink-2 no-underline transition-colors hover:text-ink"
              >
                Latest {release.tag}
              </Link>
            ) : null}
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
          </div>
          <a
            href="#features"
            className="inline-flex items-center gap-2 text-[11px] text-ink-2 no-underline transition-colors hover:text-ink"
          >
            See every feature
            <HugeiconsIcon icon={ArrowRight01Icon} className="size-3.5" />
          </a>
        </div>

        <div className="min-w-0 overflow-hidden rounded-2xl bg-window shadow-pop">
          <div className="flex items-center gap-2 bg-winbar px-3 py-[9px] text-[11px] text-ink-3 shadow-[inset_0_-1px_var(--winbar-line)]">
            <span className="mr-2 inline-flex gap-1.5">
              <i className="size-[10px] rounded-full bg-tint-29" />
              <i className="size-[10px] rounded-full bg-tint-29" />
              <i className="size-[10px] rounded-full bg-tint-29" />
            </span>
            <span className="rounded-md bg-tint-14 px-2.5 py-1 text-ink-2">
              Get started
            </span>
          </div>
          <ol className="flex flex-col divide-y divide-tint-0f">
            {steps.map((s) => (
              <li key={s.n} className="flex gap-4 px-4 py-3.5">
                <span className="pt-0.5 font-mono text-[11px] text-ink-3">
                  {s.n}
                </span>
                <div>
                  <p className="text-[13.5px] font-[450] text-ink">{s.title}</p>
                  <p className="mt-1 text-[12.5px] leading-[1.6] text-ink-2">
                    {s.body}
                  </p>
                </div>
              </li>
            ))}
          </ol>
        </div>
      </Band>
    </Shell>
  );
}
