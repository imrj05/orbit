import { ArrowRight02Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

import { SectionHead } from "@/components/sections/section-head";
import { Band, Shell } from "@/components/ui";
import { getT } from "@/i18n/t";
import type { Locale } from "@/i18n/config";

/**
 * How Orbit is built, as a diagram instead of a screenshot: the native app,
 * the newline-delimited JSON it speaks over stdio, one `pi --mode rpc` per open
 * session, and the providers pi calls. Under it, four measured facts.
 */
export async function Architecture({ locale }: { locale: Locale }) {
  const t = getT(locale, "architecture");
  const native = getT(locale, "sections.native");
  const nodes = t.raw("nodes") as { title: string; detail: string; mono?: string }[];
  const stats = t.raw("stats") as { value: string; label: string }[];

  return (
    <Shell>
      <Band id="native" dashed className="scroll-mt-20 py-20 sm:py-28">
        <SectionHead eyebrow={t("eyebrow")} title={native("title")} copy={t("copy")} />

        <ol className="mt-12 grid grid-cols-1 items-stretch gap-3 lg:grid-cols-[1fr_auto_1fr_auto_1fr_auto_1fr]">
          {nodes.map((node, index) => (
            <li key={node.title} className="contents">
              <div
                className={`flex flex-col rounded-[16px] border p-5 ${
                  index === 0
                    ? "border-edge-strong bg-surface-3"
                    : "border-edge-default bg-surface-1"
                }`}
              >
                <span className="font-mono text-[11px] tracking-[0.12em] text-ink-3">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <span className="mt-3 text-[15px] font-semibold tracking-[-0.01em] text-ink">
                  {node.title}
                </span>
                {node.mono ? (
                  <code className="mt-2 self-start rounded-[6px] bg-tint-0a px-1.5 py-0.5 font-mono text-[11.5px] text-ink-2">
                    {node.mono}
                  </code>
                ) : null}
                <span className="mt-2 text-[12.5px] leading-[1.6] text-ink-2">{node.detail}</span>
              </div>
              {index < nodes.length - 1 ? (
                <span aria-hidden className="grid place-items-center text-ink-3">
                  <HugeiconsIcon
                    icon={ArrowRight02Icon}
                    className="size-4 rotate-90 lg:rotate-0"
                    strokeWidth={1.7}
                  />
                </span>
              ) : null}
            </li>
          ))}
        </ol>

        <dl className="mt-10 grid grid-cols-2 gap-px overflow-hidden rounded-[16px] border border-edge-default bg-edge-subtle lg:grid-cols-4">
          {stats.map((stat) => (
            <div key={stat.label} className="flex flex-col-reverse justify-end bg-page px-6 py-6">
              <dt className="mt-2.5 text-[12.5px] leading-[1.5] text-ink-2">{stat.label}</dt>
              <dd className="m-0 whitespace-nowrap text-[34px] font-semibold leading-none tracking-[-0.03em] text-ink">
                {stat.value}
              </dd>
            </div>
          ))}
        </dl>
      </Band>
    </Shell>
  );
}
