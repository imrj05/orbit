"use client";

import { useState, type ReactNode } from "react";

/**
 * The reviews wall moves indefinitely. This wrapper adds the visible pause
 * control the motion needs; hover and keyboard focus still pause it in CSS.
 */
export function ReviewsScroller({
  label,
  pauseLabel,
  resumeLabel,
  children,
}: {
  label: string;
  pauseLabel: string;
  resumeLabel: string;
  children: ReactNode;
}) {
  const [paused, setPaused] = useState(false);

  return (
    <div className="relative mt-12">
      <div className="mb-3 flex justify-end">
        <button
          type="button"
          onClick={() => setPaused((value) => !value)}
          aria-pressed={paused}
          className="min-h-[44px] rounded-[8px] border border-edge-subtle bg-surface-1 px-3.5 py-2 text-[12px] text-ink-2 no-underline transition-colors hover:border-edge-strong hover:bg-surface-2 hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          {paused ? resumeLabel : pauseLabel}
        </button>
      </div>
      <div
        className="reviews-scroller"
        data-paused={paused ? "true" : undefined}
        role="region"
        aria-label={label}
      >
        {children}
      </div>
    </div>
  );
}
