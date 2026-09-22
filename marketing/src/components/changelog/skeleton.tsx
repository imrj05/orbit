import { Skeleton } from "@/components/ui/skeleton";

/** Mirrors the feed's featured panel + timeline while the notes stream in. */
export function ChangelogSkeleton() {
  return (
    <div className="grid gap-12 lg:grid-cols-[200px_minmax(0,1fr)] lg:gap-16">
      <div className="hidden flex-col gap-1 lg:flex">
        <Skeleton className="mb-3 ml-3 h-3 w-20" />
        {Array.from({ length: 7 }).map((_, index) => (
          <Skeleton key={index} className="h-8 w-full" />
        ))}
      </div>

      <div className="min-w-0 max-w-[48rem]">
        <div className="flex flex-col gap-4 rounded-2xl bg-brand/[0.03] p-6 ring-1 ring-brand/10 sm:p-7">
          <div className="flex items-center justify-between gap-4">
            <Skeleton className="h-5 w-24" />
            <Skeleton className="h-3 w-28" />
          </div>
          <Skeleton className="h-3 w-16" />
          <div className="flex flex-col gap-2.5">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-[92%]" />
            <Skeleton className="h-4 w-[78%]" />
          </div>
        </div>

        <div className="mt-8 flex flex-col divide-y divide-hair">
          {Array.from({ length: 3 }).map((_, index) => (
            <div key={index} className="flex flex-col gap-4 py-10 first:pt-0">
              <div className="flex items-center justify-between gap-4">
                <Skeleton className="h-5 w-24" />
                <Skeleton className="h-3 w-28" />
              </div>
              <Skeleton className="h-3 w-16" />
              <div className="flex flex-col gap-2.5">
                <Skeleton className="h-4 w-full" />
                <Skeleton className="h-4 w-[90%]" />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
