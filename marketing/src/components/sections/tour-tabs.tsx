"use client";

import { useRef, useState, useSyncExternalStore } from "react";

import { Shot } from "@/components/shot";

export type TourItem = {
  /** The section's old anchor (`#agent`, `#git`…), kept so nav and footer links still land. */
  id: string;
  label: string;
  title: string;
  copy: string;
  points: string[];
  shot: {
    src: string;
    srcLight: string;
    alt: string;
    width: number;
    height: number;
    /** A tall, narrow capture (the session list) is shown at a column's width. */
    narrow?: boolean;
  };
};

/**
 * The feature tour: one band, one tab strip, the real screenshots. It replaces
 * a stack of eleven alternating story bands, so a reader scans every surface
 * from one place and opens only what they care about.
 *
 * Every old section id still exists as an anchor at the top of the band; a
 * `/#git` link scrolls here and selects the Git tab.
 */
export function TourTabs({ items, label }: { items: TourItem[]; label: string }) {
  // The URL's hash is an external store: a nav link to `/#git` changes it and
  // the tab follows. A tab picked by hand wins until the hash changes again.
  const hash = useSyncExternalStore(subscribeHash, readHash, () => "");
  const [choice, setChoice] = useState<{ index: number; hash: string } | null>(null);
  const fromHash = items.findIndex((item) => item.id === hash);
  const active =
    choice && choice.hash === hash ? choice.index : fromHash >= 0 ? fromHash : (choice?.index ?? 0);
  const setActive = (index: number) => setChoice({ index, hash });
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);

  function onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    const last = items.length - 1;
    const next =
      event.key === "ArrowRight"
        ? index === last ? 0 : index + 1
        : event.key === "ArrowLeft"
          ? index === 0 ? last : index - 1
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? last
              : null;
    if (next === null) return;
    event.preventDefault();
    setActive(next);
    tabs.current[next]?.focus();
  }

  const item = items[active] ?? items[0];
  if (!item) return null;

  return (
    <div>
      {items.map((entry) => (
        <span key={entry.id} id={entry.id} className="block scroll-mt-24" aria-hidden />
      ))}

      <div
        role="tablist"
        aria-label={label}
        className="tour-tabs -mx-5 flex gap-1 overflow-x-auto px-5 pb-1 sm:mx-0 sm:flex-wrap sm:px-0"
      >
        {items.map((entry, index) => {
          const selected = index === active;
          return (
            <button
              key={entry.id}
              ref={(el) => {
                tabs.current[index] = el;
              }}
              id={`tour-tab-${entry.id}`}
              type="button"
              role="tab"
              aria-selected={selected}
              aria-controls="tour-panel"
              tabIndex={selected ? 0 : -1}
              onClick={() => setActive(index)}
              onKeyDown={(event) => onKeyDown(event, index)}
              className={`h-9 shrink-0 rounded-full px-4 text-[13px] whitespace-nowrap transition-colors ${
                selected
                  ? "bg-ink text-page"
                  : "text-ink-2 hover:bg-tint-0a hover:text-ink"
              }`}
            >
              {entry.label}
            </button>
          );
        })}
      </div>

      <div
        id="tour-panel"
        role="tabpanel"
        aria-labelledby={`tour-tab-${item.id}`}
        className="mt-10 grid grid-cols-1 items-center gap-10 lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)] lg:gap-14"
      >
        <div key={`copy-${item.id}`} className="tour-in">
          <h3 className="m-0 max-w-[22ch] text-balance [word-break:auto-phrase] text-[clamp(26px,2.6vw,34px)] font-semibold leading-[1.18] tracking-[-0.022em] text-ink">
            {item.title}
          </h3>
          <p className="mt-4 max-w-[46ch] text-[14.5px] leading-[1.75] text-ink-2">{item.copy}</p>
          {item.points.length ? (
            <ul className="mt-7 flex flex-col">
              {item.points.map((point) => (
                <li
                  key={point}
                  className="flex gap-3 border-t border-edge-subtle py-3.5 text-[13px] leading-[1.65] text-ink-2 last:border-b"
                >
                  <span aria-hidden className="mt-[9px] size-1 shrink-0 rounded-full bg-ink-3" />
                  {point}
                </li>
              ))}
            </ul>
          ) : null}
        </div>

        <div key={`shot-${item.id}`} className="tour-in tour-in--late">
          <Shot
            src={item.shot.src}
            srcLight={item.shot.srcLight}
            alt={item.shot.alt}
            width={item.shot.width}
            height={item.shot.height}
            className={item.shot.narrow ? "mx-auto w-full max-w-[340px]" : ""}
            sizes={item.shot.narrow ? "340px" : "(max-width: 1024px) 100vw, 680px"}
          />
        </div>
      </div>
    </div>
  );
}

function subscribeHash(onChange: () => void) {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

function readHash() {
  return window.location.hash.slice(1);
}
