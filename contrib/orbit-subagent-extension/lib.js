/**
 * Pure helpers for Orbit's subagent tool.
 *
 * Kept free of pi imports so `node --test` can exercise them directly.
 */

/** Compact token counts: 999 → "999", 1200 → "1.2k", 2_400_000 → "2.4M". */
export function formatTokens(count) {
  if (count < 1000) return String(count);
  if (count < 10_000) return `${(count / 1000).toFixed(1)}k`;
  if (count < 1_000_000) return `${Math.round(count / 1000)}k`;
  return `${(count / 1_000_000).toFixed(1)}M`;
}

/** One line of usage facts for a tool result, or "" when nothing is known. */
export function formatUsage(usage, model) {
  if (!usage) return "";
  const parts = [];
  if (usage.turns) parts.push(`${usage.turns} turn${usage.turns === 1 ? "" : "s"}`);
  if (usage.input) parts.push(`↑${formatTokens(usage.input)}`);
  if (usage.output) parts.push(`↓${formatTokens(usage.output)}`);
  if (usage.cacheRead) parts.push(`R${formatTokens(usage.cacheRead)}`);
  if (usage.cacheWrite) parts.push(`W${formatTokens(usage.cacheWrite)}`);
  if (usage.cost) parts.push(`$${usage.cost.toFixed(4)}`);
  if (model) parts.push(model);
  return parts.join(" · ");
}

/** The text blocks of one pi message, joined by newlines. */
function messageText(message) {
  const content = message?.content;
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .filter((part) => part?.type === "text" && typeof part.text === "string")
    .map((part) => part.text)
    .join("\n");
}

/** The last assistant text in a child session, or "". */
export function getFinalOutput(messages) {
  for (let i = messages.length - 1; i >= 0; i--) {
    const message = messages[i];
    if (message?.role !== "assistant") continue;
    const text = messageText(message).trim();
    if (text) return text;
  }
  return "";
}

/** Number of tool calls the child has emitted so far (for progress updates). */
export function countToolCalls(messages) {
  let count = 0;
  for (const message of messages) {
    if (message?.role !== "assistant" || !Array.isArray(message.content)) continue;
    for (const part of message.content) {
      if (part?.type === "toolCall" || part?.type === "tool_call") count += 1;
    }
  }
  return count;
}

/** Whether a child ended in failure (exit code, provider error, or abort). */
export function isFailedResult(result) {
  return (
    result.exitCode !== 0 ||
    result.stopReason === "error" ||
    result.stopReason === "aborted"
  );
}

/** The model-facing output for a child: error detail first, then its answer. */
export function getResultOutput(result) {
  if (isFailedResult(result)) {
    return (
      result.errorMessage ||
      result.stderr ||
      getFinalOutput(result.messages) ||
      "(no output)"
    );
  }
  return getFinalOutput(result.messages) || "(no output)";
}

/**
 * Cap a child's output in bytes, without slicing a UTF-8 sequence in half.
 * The full result remains available in the tool details for a future native
 * renderer; only the model-facing text is capped.
 */
export function truncateOutput(output, cap = 50 * 1024) {
  const text = String(output ?? "");
  const bytes = Buffer.byteLength(text, "utf8");
  if (bytes <= cap) return text;
  let truncated = text.slice(0, cap);
  while (Buffer.byteLength(truncated, "utf8") > cap) {
    truncated = truncated.slice(0, -1);
  }
  return `${truncated}\n\n[Output truncated: ${bytes - Buffer.byteLength(truncated, "utf8")} bytes omitted.]`;
}

/** A compact tool-details record; never carries the child's full transcript. */
export function summarizeResult(result) {
  const status =
    result.exitCode === -1
      ? "running"
      : isFailedResult(result)
        ? "failed"
        : "completed";
  return {
    agent: result.agent,
    agentSource: result.agentSource,
    task: result.task,
    step: result.step,
    status,
    exitCode: result.exitCode,
    model: result.model,
    stopReason: result.stopReason,
    errorMessage: result.errorMessage,
    usage: result.usage,
    output: truncateOutput(getResultOutput(result)),
  };
}

/** Tools a Plan/Ask child may keep. Deliberately excludes bash and mutation. */
export const READ_ONLY_TOOLS = ["read", "grep", "find", "ls"];

/**
 * The child tool set for one agent under a workflow mode.
 *
 * Build returns the agent's own allowlist unchanged (undefined means "pi's
 * defaults"). Plan/Ask always returns an explicit read-only subset — even for
 * an agent with no allowlist — so delegation cannot widen a read-only session.
 */
export function selectChildTools(agentTools, workflowMode) {
  if (workflowMode !== "plan" && workflowMode !== "ask") return agentTools;
  if (!agentTools) return [...READ_ONLY_TOOLS];
  return agentTools.filter((tool) => READ_ONLY_TOOLS.includes(tool));
}

/** Run `fn` over `items` with a bounded number in flight, preserving order. */
export async function mapWithConcurrencyLimit(items, concurrency, fn) {
  if (items.length === 0) return [];
  const limit = Math.max(1, Math.min(concurrency, items.length));
  const results = new Array(items.length);
  let nextIndex = 0;
  const workers = new Array(limit).fill(null).map(async () => {
    while (true) {
      const current = nextIndex++;
      if (current >= items.length) return;
      results[current] = await fn(items[current], current);
    }
  });
  await Promise.all(workers);
  return results;
}
