const TOKEN = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)\s]+\))/g;

/**
 * Renders the small inline markdown used across the release notes —
 * `**bold**`, `` `code` ``, and `[label](url)` — without pulling in a full
 * markdown pipeline. Everything else is emitted verbatim.
 */
export function Inline({ text }: { text: string }) {
  const parts = text.split(TOKEN).filter((part) => part !== "");

  return (
    <>
      {parts.map((part, index) => {
        if (part.startsWith("**") && part.endsWith("**")) {
          return (
            <strong key={index} className="font-medium text-ink">
              {part.slice(2, -2)}
            </strong>
          );
        }

        if (part.startsWith("`") && part.endsWith("`")) {
          return (
            <code
              key={index}
              className="rounded-[4px] bg-well px-1 py-px font-mono text-[0.84em] text-ink"
            >
              {part.slice(1, -1)}
            </code>
          );
        }

        const link = part.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
        if (link) {
          return (
            <a
              key={index}
              href={link[2]}
              target="_blank"
              rel="noreferrer"
              className="text-ink underline decoration-hair underline-offset-[3px] transition-colors hover:decoration-ink-2"
            >
              {link[1]}
            </a>
          );
        }

        return <span key={index}>{part}</span>;
      })}
    </>
  );
}
