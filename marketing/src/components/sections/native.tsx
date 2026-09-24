import { ApprovalMock } from "@/components/sections/mocks";
import { MockFrame, Story } from "@/components/sections/story";

export function Native() {
  return (
    <Story
      id="native"
      label="Native"
      title="A real desktop app. You hold the keys."
      copy="Orbit is a native macOS, Windows, and Linux app — Rust and GPUI, keyboard-first, no web views. And every mutating tool call passes through an access mode you set per task, so the agent never runs ahead of you."
      points={[
        "Supervised — asks before every edit and every command.",
        "Auto-accept edits — applies file edits, still asks before commands.",
        "Full access — runs without prompting, picked deliberately per task.",
      ]}
      visual={
        <MockFrame>
          <ApprovalMock />
        </MockFrame>
      }
    />
  );
}
