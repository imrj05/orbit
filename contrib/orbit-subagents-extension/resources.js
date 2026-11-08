/**
 * The guardrail paths a child session must load.
 *
 * Orbit sets `ORBIT_BUNDLED_EXTENSIONS` to a JSON array of absolute paths on
 * every session it spawns (`BundledExtensions::spawn`). A child is a new
 * session, so it does not inherit anything implicitly: the tool passes each
 * path on as `--extension`, which is what keeps the access guard and the
 * workflow mode in force inside the child.
 */

/** Parse the inherited list. Anything that is not a string array is empty. */
export function parseBundledExtensions(raw) {
  if (typeof raw !== "string" || raw.trim() === "") return [];
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const seen = new Set();
  const out = [];
  for (const entry of parsed) {
    if (typeof entry !== "string" || entry.trim() === "") continue;
    const value = entry.trim();
    if (seen.has(value)) continue;
    seen.add(value);
    out.push(value);
  }
  return out;
}
