"use client";

import { Moon01Icon, Sun01Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";

/**
 * Flips `<html>` between the dark (default) and light themes and remembers the
 * choice in localStorage. The icon swap is pure CSS via the `light:` variant,
 * so there is nothing to hydrate and no flash of the wrong icon.
 */
export function ThemeToggle({ className = "" }: { className?: string }) {
  function toggle() {
    const root = document.documentElement;
    const next = root.classList.contains("light") ? "dark" : "light";
    root.classList.toggle("light", next === "light");
    root.classList.toggle("dark", next === "dark");
    root.style.colorScheme = next;
    try {
      localStorage.setItem("theme", next);
    } catch {
      /* storage can be unavailable; the class change still applies */
    }
  }

  return (
    <button
      type="button"
      onClick={toggle}
      aria-label="Toggle color theme"
      title="Toggle color theme"
      className={`inline-flex size-8 shrink-0 items-center justify-center rounded-[9px] border border-edge-subtle bg-surface-1 text-ink-2 transition-colors hover:border-edge-strong hover:bg-surface-2 hover:text-ink ${className}`}
    >
      <HugeiconsIcon icon={Moon01Icon} className="size-4 light:hidden" />
      <HugeiconsIcon icon={Sun01Icon} className="hidden size-4 light:block" />
    </button>
  );
}
