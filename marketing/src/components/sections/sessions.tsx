import { SessionsMock } from "@/components/sections/mocks";
import { MockFrame, Story } from "@/components/sections/story";

export function Sessions() {
  return (
    <Story
      id="sessions"
      label="Sessions"
      title="Sessions live where pi put them."
      copy="Your list is read straight from pi and grouped by project — the same sessions the terminal sees. Reopen, clone, or hide a workspace without touching anything on disk; recent sessions stay warm."
      flip
      visual={
        <MockFrame>
          <SessionsMock />
        </MockFrame>
      }
    />
  );
}
