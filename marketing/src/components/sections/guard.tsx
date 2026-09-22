import { ApprovalMock } from "@/components/sections/mocks";
import { Band, SectionLabel, Shell } from "@/components/ui";

const modes = [
  {
    name: "Supervised",
    body: "Asks before every edit and every command the agent wants to run.",
  },
  {
    name: "Auto-accept edits",
    body: "Applies file edits on its own, and still asks before commands.",
  },
  {
    name: "Full access",
    body: "Runs without prompting. Pick it deliberately, per task.",
  },
];

export function Guard() {
  return (
    <Shell>
      <Band
        id="guard"
        dashed
        className="relative scroll-mt-20 overflow-hidden bg-missions py-16 sm:py-[88px]"
      >
        <div
          aria-hidden="true"
          className="dither dither-tl left-0 top-0 h-[220px] w-[320px]"
        />
        <div
          aria-hidden="true"
          className="dither dither-br bottom-0 right-0 h-[220px] w-[320px]"
        />
        <div className="relative z-[1] grid grid-cols-1 items-center gap-12 lg:grid-cols-2 lg:gap-16">
          <div>
            <SectionLabel>Safeguards</SectionLabel>
            <h2 className="mb-3.5 mt-3 text-[clamp(26px,3vw,36px)] font-[300] leading-[1.2] tracking-[-0.02em] text-ink">
              The guard holds the keys.
            </h2>
            <p className="mb-8 max-w-[46ch] text-[13.5px] leading-[1.8] text-ink-2">
              Every mutating tool call passes through an access mode you set per
              task. Orbit asks before the agent edits a file or runs a command, and
              remembers what you allow.
            </p>

            <ul className="flex flex-col">
              {modes.map((m) => (
                <li
                  key={m.name}
                  className="flex gap-5 border-t border-tint-0f py-4"
                >
                  <div>
                    <p className="text-[14px] font-[450] text-ink">{m.name}</p>
                    <p className="mt-1 max-w-[44ch] text-[13px] leading-[1.7] text-ink-2">
                      {m.body}
                    </p>
                  </div>
                </li>
              ))}
            </ul>

            <p className="mt-7 max-w-[48ch] text-[12px] leading-[1.7] text-ink-3">
              The bar offers <span className="text-ink-2">Allow once</span>,{" "}
              <span className="text-ink-2">Always allow this tool</span>, or{" "}
              <span className="text-ink-2">Deny</span> — recorded per mode and
              driven with ↑ ↓ ⏎ esc. It is a confirmation guard, not a sandbox: pi
              ships no sandbox, and Orbit does not add one.
            </p>
          </div>

          <ApprovalMock />
        </div>
      </Band>
    </Shell>
  );
}
