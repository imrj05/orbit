import { ComposerMock, ToolsMock, TranscriptMock } from "@/components/sections/mocks";
import { MockFrame, Story } from "@/components/sections/story";

export function Agent() {
  return (
    <Story
      id="agent"
      label="The agent"
      title="Watch it think, call tools, and edit."
      copy="Thinking, tool calls, and file edits stream into a GPU-rendered transcript that never reflows. Steer mid-run or queue a follow-up from the composer — model and thinking effort are a keystroke away."
      points={[
        "Coalesced streaming, GFM markdown, and paint-only syntax highlighting.",
        "Prompt, steer, queue, or cancel — all against the live pi process.",
        "Every tool call expands into arguments, output, and inline diffs.",
      ]}
      visual={
        <MockFrame>
          <div className="flex flex-col gap-5">
            <ComposerMock />
            <div className="h-px bg-edge-subtle" />
            <TranscriptMock />
            <div className="h-px bg-edge-subtle" />
            <ToolsMock />
          </div>
        </MockFrame>
      }
    />
  );
}
