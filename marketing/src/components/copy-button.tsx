"use client";

import { Copy01Icon, Tick02Icon } from "@hugeicons/core-free-icons";
import { HugeiconsIcon } from "@hugeicons/react";
import { useState } from "react";
import { Button } from "@/components/ui/button";

export function CopyButton({
  value,
  label = "Copy",
}: {
  value: string;
  label?: string;
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  }

  return (
    <Button
      type="button"
      variant="outline"
      size="xs"
      onClick={copy}
      aria-live="polite"
      className="font-mono text-[10.5px]"
    >
      {copied ? (
        <HugeiconsIcon icon={Tick02Icon} data-icon="inline-start" />
      ) : (
        <HugeiconsIcon icon={Copy01Icon} data-icon="inline-start" />
      )}
      {copied ? "Copied" : label}
    </Button>
  );
}
