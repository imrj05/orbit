import { OrbitIcon } from "@/components/brand";
import { Band, Shell } from "@/components/ui";
import { Shot } from "@/components/shot";

export function Stage() {
  return (
    <Shell>
      <Band dashed className="relative overflow-hidden bg-well py-11 sm:py-14">
        <OrbitIcon
          aria-hidden="true"
          alt=""
          className="watermark left-1/2 top-1/2 h-[520px] w-[520px] max-w-none -translate-x-1/2 -translate-y-1/2 opacity-[0.05]"
        />
        <div
          aria-hidden="true"
          className="pointer-events-none absolute left-1/2 top-1/2 h-[24rem] w-[52rem] max-w-none -translate-x-1/2 -translate-y-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(91,157,255,0.06),transparent)]"
        />
        <div
          aria-hidden="true"
          className="dither dither-tl left-0 top-0 h-[240px] w-[360px]"
        />
        <div
          aria-hidden="true"
          className="dither dither-br bottom-0 right-0 h-[240px] w-[360px]"
        />
        <div className="rise-window relative z-[1] mx-auto w-full max-w-[1180px]">
          <Shot
            src="/screens/session.png"
            srcLight="/screens/session-light.png"
            priority
            alt="A live Orbit session: a transcript answering a question about the API with a findings table, a list of changed files, and the composer below."
          />
        </div>
      </Band>
    </Shell>
  );
}
