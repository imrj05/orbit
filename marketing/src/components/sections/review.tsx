import { Tick02Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { Shot } from "@/components/shot";
import { Band, SectionLabel, Shell } from "@/components/ui";

const points = [
  "A live git diff HEAD of the workspace, in a right-hand panel.",
  "Refreshed automatically when a run settles — no manual reload.",
  "Open it from the transcript's changed-files card, the Git page, or ⌘P; the transcript stays put.",
];

export function Review() {
  return (
    <Shell>
      <Band
        dashed
        className="relative overflow-hidden bg-well py-16 sm:py-[88px]"
      >
        <div className="relative z-[1] grid grid-cols-1 items-center gap-12 lg:grid-cols-2 lg:gap-16">
          <div>
            <SectionLabel>Review</SectionLabel>
            <h2 className="mb-3.5 mt-3 text-[clamp(26px,3vw,36px)] font-[300] leading-[1.2] tracking-[-0.02em] text-ink">
              Read the diff before you switch apps.
            </h2>
            <p className="mb-7 max-w-[46ch] text-[13.5px] leading-[1.8] text-ink-2">
              The agent edits files behind the transcript. Orbit keeps the diff in
              view so you can judge the change in the place it happened.
            </p>
            <ul className="flex flex-col gap-3">
              {points.map((p) => (
                <li key={p} className="flex items-start gap-2.5">
                  <HugeiconsIcon
                    icon={Tick02Icon}
                    className="mt-0.5 size-3.5 shrink-0 text-ink-3"
                  />
                  <span className="max-w-[44ch] text-[13px] leading-[1.7] text-ink-2">
                    {p}
                  </span>
                </li>
              ))}
            </ul>
          </div>

          <Shot
            src="/screens/review.png"
            srcLight="/screens/review-light.png"
            alt="The Orbit Review panel open beside a session, showing a selected file's diff on the left and the changed-file tree on the right."
          />
        </div>
      </Band>
    </Shell>
  );
}
