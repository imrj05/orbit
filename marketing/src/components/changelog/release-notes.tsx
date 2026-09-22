import { Inline } from "@/components/changelog/inline";
import type { ChangelogEntry } from "@/lib/changelog";
import { cn } from "@/lib/utils";

const KIND_DOT: Record<string, string> = {
  Added: "bg-brand",
  Changed: "bg-ink-2",
  Fixed: "bg-ink-2",
};

/** The change groups for one release, shared by the index and release pages. */
export function ReleaseNotes({
  groups,
  className,
}: {
  groups: ChangelogEntry["groups"];
  className?: string;
}) {
  const populated = groups.filter((group) => group.items.length > 0);

  return (
    <div className={cn("flex flex-col gap-6", className)}>
      {populated.length === 0 ? (
        <p className="text-[14px] italic text-ink-3">
          Maintenance release — no notes were published.
        </p>
      ) : (
        populated.map((group, index) => (
          <section key={`${group.kind}-${index}`}>
            {group.kind ? (
              <h4 className="mb-3 flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
                <span
                  aria-hidden
                  className={cn(
                    "size-[5px] rounded-full",
                    KIND_DOT[group.kind] ?? "bg-ink-3",
                  )}
                />
                {group.kind}
              </h4>
            ) : null}
            <ul className="flex flex-col gap-2.5">
              {group.items.map((item, itemIndex) => (
                <li
                  key={itemIndex}
                  className="relative pl-[18px] text-[14.5px] leading-[1.72] text-ink-2"
                >
                  <span
                    aria-hidden
                    className="absolute left-0 top-[0.72em] size-[3px] rounded-full bg-ink-3/80"
                  />
                  <Inline text={item} />
                </li>
              ))}
            </ul>
          </section>
        ))
      )}
    </div>
  );
}
