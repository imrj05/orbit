import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";

export function Usage() {
  return (
    <Story
      id="usage"
      label="Usage"
      title="Know what every run costs."
      copy="Requests, tokens, cost, and cache — read from pi's own meter, across the sessions on your machine. No third-party analytics, no guesswork about where the context went."
      points={[
        "Per-run and per-provider figures, with cached and context tokens.",
        "A 30-day view of tokens over time, kept entirely local.",
      ]}
      visual={
        <Shot
          src="/screens/usage.png"
          srcLight="/screens/usage-light.png"
          alt="Orbit's Usage page with request, token, cost, and cache metrics above a tokens-over-time chart."
          sizes="(max-width: 1024px) 100vw, 560px"
        />
      }
    />
  );
}
