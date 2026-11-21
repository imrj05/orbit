/**
 * A child's live event log: every JSONL line the child pi prints, appended
 * verbatim to `~/.orbit-pi/subagent-logs/<run>-<child>.jsonl`.
 *
 * Orbit tails this file and feeds each line to the same transcript the main
 * session streams into, so the side pane shows what the child read, ran,
 * thought and wrote, live, from pi's own events. The file is display-only:
 * the delegation never depends on it, so any failure to open or write it
 * leaves the run untouched and the card falls back to the `details` summary.
 *
 * Logs older than {@link LOG_MAX_AGE_MS} are removed when a new run starts.
 */

import fs from "node:fs";
import path from "node:path";

/** Logs live for a week; a run older than that is not reopened in practice. */
export const LOG_MAX_AGE_MS = 7 * 24 * 60 * 60 * 1000;

/** The directory every child log is written to. */
export function logDir(home) {
  return path.join(home, ".orbit-pi", "subagent-logs");
}

/**
 * Open the log for one child, or `null` when it cannot be written. `run` and
 * `child` name the file; both are reduced to `[A-Za-z0-9_-]` so a value can
 * never step outside the directory.
 */
export function openChildLog({ home, run, child, io = fs }) {
  if (typeof home !== "string" || home === "") return null;
  const safe = (value) => String(value ?? "").replace(/[^A-Za-z0-9_-]/g, "") || "x";
  const dir = logDir(home);
  const file = path.join(dir, `${safe(run)}-${safe(child)}.jsonl`);
  let fd;
  try {
    io.mkdirSync(dir, { recursive: true, mode: 0o700 });
    // `wx`: a run id is unique, so an existing file is someone else's.
    fd = io.openSync(file, "wx", 0o600);
  } catch {
    return null;
  }
  let open = true;
  return {
    path: file,
    /** Append complete lines; a write failure closes the log quietly. */
    append(lines) {
      if (!open || lines.length === 0) return;
      try {
        io.writeSync(fd, `${lines.join("\n")}\n`);
      } catch {
        this.close();
      }
    },
    close() {
      if (!open) return;
      open = false;
      try {
        io.closeSync(fd);
      } catch {
        // Already gone; nothing to release.
      }
    },
  };
}

/** Remove child logs last written more than `maxAgeMs` ago. Best effort. */
export function pruneLogs({ home, now, maxAgeMs = LOG_MAX_AGE_MS, io = fs }) {
  if (typeof home !== "string" || home === "") return 0;
  const dir = logDir(home);
  let names;
  try {
    names = io.readdirSync(dir);
  } catch {
    return 0;
  }
  let removed = 0;
  for (const name of names) {
    if (!name.endsWith(".jsonl")) continue;
    const file = path.join(dir, name);
    try {
      if (now - io.statSync(file).mtimeMs > maxAgeMs) {
        io.unlinkSync(file);
        removed += 1;
      }
    } catch {
      // Raced with another run's prune, or unreadable: leave it.
    }
  }
  return removed;
}
