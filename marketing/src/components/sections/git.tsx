import { GitCompareIcon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";

function GitCard() {
  return (
    <div className="absolute -bottom-5 right-6 hidden rounded-[12px] border border-edge-default bg-window/95 px-3.5 py-3 shadow-card backdrop-blur md:block">
      <p className="mb-1 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
        <HugeiconsIcon icon={GitCompareIcon} className="size-3" />
        Git
      </p>
      <p className="text-[12px] text-ink-2">
        <span className="text-ink">3 files changed</span>
        {" · "}
        <span className="font-mono text-[11px]">+184 −61</span>
      </p>
    </div>
  );
}

export function Git() {
  return (
    <Story
      id="git"
      label="Git"
      title="Read the diff before you switch apps."
      copy="The agent edits files behind the transcript. Orbit keeps a live diff in a right-hand panel and a full Git page beside it — stage, review, and commit without leaving the window."
      points={[
        "A live git diff of the workspace, refreshed when a run settles.",
        "Generate a commit message, stage what you want, and push in place.",
        "History and graph views for the branch you are standing on.",
      ]}
      flip
      visual={
        <div className="relative">
          <Shot
            src="/screens/review.png"
            srcLight="/screens/review-light.png"
            alt="The Orbit Review panel open beside a session, showing a selected file's diff on the left and the changed-file tree on the right."
            sizes="(max-width: 1024px) 100vw, 560px"
          />
          <GitCard />
        </div>
      }
    />
  );
}
