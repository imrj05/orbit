import {
  ArrowDown01Icon,
  ArrowRight01Icon,
  ArrowUp01Icon,
  BrainIcon,
  ChartBarLineIcon,
  CpuIcon,
  KeyboardIcon,
  ListPlusIcon,
  LockIcon,
  Search01Icon,
  ShieldCheckIcon,
  SparklesIcon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

export function TranscriptMock() {
  return (
    <div className="flex flex-col gap-3 text-[11.5px] leading-[1.55]">
      <p className="self-end max-w-[80%] rounded-[9px] bg-slab px-3 py-1.5 text-ink">
        Port the theme switcher.
      </p>
      <div className="flex items-center gap-2 text-ink-3">
        <HugeiconsIcon icon={BrainIcon} className="size-3.5" />
        <span className="shimmer-text font-mono text-[10.5px]">
          thinking · 8.1s
        </span>
      </div>
      <div className="flex items-center gap-2 rounded-md bg-tint-0a px-2.5 py-1.5 font-mono text-[10.5px] text-ink-2">
        Run bun test
        <HugeiconsIcon icon={Tick02Icon} className="size-3 text-ink-3" />
      </div>
      <p className="text-ink-2">
        Ported the switcher; 412 tests pass. Restyling the controls now
        <span className="caret" />
      </p>
    </div>
  );
}

export function SessionsMock() {
  const groups = [
    {
      project: "orbit",
      sessions: [
        { title: "Port the theme switcher", meta: "now", active: true },
        { title: "Review the diff flow", meta: "3h" },
      ],
    },
    {
      project: "sample-store",
      sessions: [
        { title: "Flag checkout risks", meta: "yd" },
        { title: "Map the codebase", meta: "2d" },
      ],
    },
  ];
  return (
    <div className="flex flex-col gap-3 text-[11.5px]">
      {groups.map((g) => (
        <div key={g.project} className="flex flex-col gap-1">
          <span className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-3">
            {g.project}
          </span>
          {g.sessions.map((s) => (
            <div
              key={s.title}
              className={`flex items-center gap-2 rounded-[5px] px-2 py-1 ${
                s.active ? "bg-tint-12 text-ink" : "text-ink-2"
              }`}
            >
              <span
                className={`size-[5px] rounded-full ${
                  s.active ? "bg-brand" : "bg-ink-3"
                }`}
              />
              <span className="truncate">{s.title}</span>
              <span className="ml-auto font-mono text-[10px] text-ink-3">
                {s.meta}
              </span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

export function ToolsMock() {
  const tools = [
    { verb: "Edit", target: "settings.rs", meta: "1.2s", open: true },
    { verb: "Bash", target: "bun test", meta: "41s" },
    { verb: "Read", target: "auth/session.ts", meta: "0.4s" },
  ];
  return (
    <div className="flex flex-col gap-2 font-mono text-[10.5px]">
      {tools.map((t) => (
        <div key={t.target} className="flex flex-col gap-1.5">
          <div className="flex items-baseline gap-2">
            <HugeiconsIcon
              icon={ArrowRight01Icon}
              className={`size-3 text-ink-3 ${t.open ? "rotate-90" : ""}`}
            />
            <span className="text-ink-2">{t.verb}</span>
            <span className="truncate text-ink-3">{t.target}</span>
            <span className="ml-auto text-ink-3">{t.meta}</span>
          </div>
          {t.open ? (
            <div className="ml-4 rounded-md bg-tint-0a px-2.5 py-1.5 text-ink-2">
              {"{ density: 1.0, motion: false }"}
            </div>
          ) : null}
        </div>
      ))}
    </div>
  );
}

export function ComposerMock() {
  return (
    <div className="flex flex-col gap-2.5 text-[11.5px]">
      <div className="rounded-[9px] bg-tint-0a px-3 py-2.5 text-ink shadow-[inset_0_0_0_1px_var(--tint-0f)]">
        Refactor the parser, then run the suite
        <span className="caret" />
      </div>
      <div className="flex flex-wrap items-center gap-1.5 font-mono text-[10px] text-ink-2">
        <span className="inline-flex items-center gap-1.5 rounded-md bg-tint-0a px-2 py-1 shadow-[inset_0_0_0_1px_var(--tint-0f)]">
          <HugeiconsIcon icon={CpuIcon} className="size-3 text-ink-3" />
          deepseek-v4-flash
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-md bg-tint-0a px-2 py-1 shadow-[inset_0_0_0_1px_var(--tint-0f)]">
          <HugeiconsIcon icon={BrainIcon} className="size-3 text-ink-3" />
          High
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-md bg-tint-0a px-2 py-1 shadow-[inset_0_0_0_1px_var(--tint-0f)]">
          <HugeiconsIcon icon={LockIcon} className="size-3 text-ink-3" />
          Full access
        </span>
      </div>
      <div className="flex items-center gap-2 text-[10.5px] text-ink-3">
        <HugeiconsIcon icon={ListPlusIcon} className="size-3.5" />
        <span>follow-up queued — “then run the tests”</span>
      </div>
    </div>
  );
}

export function ApprovalMock() {
  return (
    <div className="flex flex-col gap-3 text-[11.5px]">
      <p className="self-end max-w-[80%] rounded-[9px] bg-slab px-3 py-1.5 text-ink">
        Refactor the parser, then run the suite.
      </p>

      <div className="rounded-xl bg-tint-0a p-3 shadow-[inset_0_0_0_1px_var(--tint-12)]">
        <div className="flex flex-wrap items-center gap-2 text-[11.5px] text-ink-2">
          <HugeiconsIcon icon={ShieldCheckIcon} className="size-3.5 text-brand" />
          <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-3">
            orbit-guard
          </span>
          <span className="text-ink">Agent wants to run</span>
          <code className="rounded bg-tint-0f px-1.5 py-px font-mono text-[10.5px] text-ink">
            bun test
          </code>
        </div>
        <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
          <span className="rounded-md bg-ink px-2.5 py-1 text-[10.5px] font-medium text-page">
            Allow once
          </span>
          <span className="rounded-md bg-tint-0f px-2.5 py-1 text-[10.5px] text-ink-2 shadow-[inset_0_0_0_1px_var(--tint-12)]">
            Always allow this tool
          </span>
          <span className="rounded-md bg-tint-0f px-2.5 py-1 text-[10.5px] text-ink-2 shadow-[inset_0_0_0_1px_var(--tint-12)]">
            Deny
          </span>
          <span className="ml-auto inline-flex items-center gap-1 font-mono text-[9.5px] text-ink-3">
            <HugeiconsIcon icon={KeyboardIcon} className="size-3" />
            ↑↓ ⏎ esc
          </span>
        </div>
      </div>

      <div className="flex items-center gap-2 text-[10.5px] text-ink-3">
        <HugeiconsIcon icon={LockIcon} className="size-3.5" />
        <span>access mode</span>
        <span className="rounded-md bg-tint-0a px-2 py-0.5 font-mono text-ink-2 shadow-[inset_0_0_0_1px_var(--tint-0f)]">
          Supervised
        </span>
      </div>
    </div>
  );
}
