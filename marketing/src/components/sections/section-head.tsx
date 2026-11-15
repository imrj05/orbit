/**
 * The heading every feature band opens with: a short eyebrow, one statement,
 * one supporting sentence. Left-aligned by default so it shares the reading
 * edge with what follows; `center` for bands that are one centered composition.
 */
export function SectionHead({
  eyebrow,
  title,
  copy,
  center = false,
}: {
  eyebrow: string;
  title: string;
  copy?: string;
  center?: boolean;
}) {
  return (
    <div className={center ? "mx-auto max-w-[680px] text-center" : "max-w-[680px]"}>
      <p className="m-0 font-mono text-[11.5px] uppercase tracking-[0.14em] text-ink-3">
        {eyebrow}
      </p>
      <h2 className="mt-4 text-balance [word-break:auto-phrase] text-[clamp(30px,3.4vw,44px)] font-semibold leading-[1.12] tracking-[-0.028em] text-ink">
        {title}
      </h2>
      {copy ? (
        <p
          className={`mt-4 max-w-[56ch] text-[15px] leading-[1.75] text-ink-2 ${center ? "mx-auto" : ""}`}
        >
          {copy}
        </p>
      ) : null}
    </div>
  );
}
