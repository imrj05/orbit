/**
 * Orbit's native subagent tool.
 *
 * Adapted from pi's `examples/extensions/subagent/index.ts` (MIT, see NOTICE).
 * Orbit loads this file with `pi --extension <path>` on every session it
 * spawns. It registers one `subagent` tool that runs child `pi` processes with
 * isolated context windows, so a parent session can delegate recon, planning,
 * review, or implementation without filling its own transcript.
 *
 * Modes:
 *   - single   { agent, task }
 *   - parallel { tasks: [{ agent, task }, ...] }   max 8 tasks, 4 concurrent
 *   - chain    { chain: [{ agent, task }, ...] }   `{previous}` is replaced
 *   - list     { action: "list" }
 *
 * Children are headless (`--mode json -p --no-session`). They do not load
 * Orbit's access guard or other parent-only extensions. The agent's `tools`
 * allowlist is the real boundary: built-in scout/planner/reviewer are
 * read-only, worker gets the default tool set. Plan/Ask sessions propagate
 * their workflow mode to children so a worker cannot bypass read-only scope.
 *
 * Child sessions are ephemeral (`--no-session`): they do not appear in Orbit's
 * sidebar or pi's session store. The parent receives their final text.
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { Type } from "typebox";

import { discoverAgents, formatAgentList } from "./agents.js";
import {
  countToolCalls,
  formatUsage,
  getFinalOutput,
  getResultOutput,
  isFailedResult,
  mapWithConcurrencyLimit,
  selectChildTools,
  summarizeResult,
  truncateOutput,
} from "./lib.js";

const MAX_PARALLEL_TASKS = 8;
const MAX_CONCURRENCY = 4;
const PER_TASK_OUTPUT_CAP = 50 * 1024;
const UPDATE_OUTPUT_CAP = 4 * 1024;

/**
 * Child sessions must never fan out again. Excluding the names used by this
 * extension and the common community subagent packages keeps a delegated
 * worker from recursively spawning its own fleet or waiting on background
 * work that nobody owns.
 */
const CHILD_EXCLUDED_TOOLS = [
  "subagent",
  "subagents_enable",
  "subagent_supervisor",
  "bg_wait",
  "Agent",
  "get_subagent_result",
  "steer_subagent",
].join(",");

/** Orbit materializes the workflow extension here; children inherit its mode. */
function workflowExtensionPath() {
  return path.join(os.homedir(), ".orbit-pi", "workflow-extension", "index.js");
}

/** The parent session's workflow mode, read from Orbit's per-session store. */
function readWorkflowMode(sessionId) {
  if (!sessionId) return undefined;
  try {
    const file = path.join(os.homedir(), ".orbit-pi", "workflow.json");
    const map = JSON.parse(fs.readFileSync(file, "utf8"));
    const mode = map?.[sessionId];
    return mode === "plan" || mode === "ask" || mode === "build"
      ? mode
      : undefined;
  } catch {
    return undefined;
  }
}

/**
 * Resolve how to invoke pi from inside the current pi process. The official
 * implementation handles npm-installed CLIs (node + the CLI script) and Bun's
 * virtual entrypoint; the same logic keeps Orbit's child spawn correct.
 */
function getPiInvocation(args) {
  const currentScript = process.argv[1];
  const isBunVirtualScript = currentScript?.startsWith("/$bunfs/root/");
  if (currentScript && !isBunVirtualScript && fs.existsSync(currentScript)) {
    return { command: process.execPath, args: [currentScript, ...args] };
  }

  const execName = path.basename(process.execPath).toLowerCase();
  const isGenericRuntime = /^(node|bun)(\.exe)?$/.test(execName);
  if (!isGenericRuntime) {
    return { command: process.execPath, args };
  }

  return { command: "pi", args };
}

async function writePromptToTempFile(agentName, prompt) {
  const dir = await fs.promises.mkdtemp(path.join(os.tmpdir(), "orbit-subagent-"));
  const safeName = agentName.replace(/[^\w.-]+/g, "_");
  const filePath = path.join(dir, `prompt-${safeName}.md`);
  await fs.promises.writeFile(filePath, prompt, { encoding: "utf-8", mode: 0o600 });
  return { dir, filePath };
}

/**
 * Run one child agent and resolve with its collected result. The child's
 * assistant messages are enough for final output, usage, and progress; tool
 * result messages are intentionally not retained.
 */
async function runSingleAgent({
  defaultCwd,
  dispatchDefaults,
  agents,
  agentName,
  task,
  cwd,
  step,
  signal,
  onUpdate,
  makeDetails,
  workflowMode,
}) {
  const agent = agents.find((candidate) => candidate.name === agentName);
  if (!agent) {
    const available = formatAgentList(agents).text;
    return {
      agent: agentName,
      agentSource: "unknown",
      task,
      exitCode: 1,
      messages: [],
      stderr: `Unknown agent: "${agentName}". Available agents: ${available}.`,
      usage: {
        input: 0,
        output: 0,
        cacheRead: 0,
        cacheWrite: 0,
        cost: 0,
        contextTokens: 0,
        turns: 0,
      },
      step,
    };
  }

  const model = agent.model ?? dispatchDefaults.model;
  const args = ["--mode", "json", "-p", "--no-session"];
  if (model) args.push("--model", model);
  const thinking =
    agent.thinking ?? (!agent.model ? dispatchDefaults.thinkingLevel : undefined);
  if (thinking) args.push("--thinking", thinking);
  const readOnly = workflowMode === "plan" || workflowMode === "ask";
  const tools = selectChildTools(agent.tools, workflowMode);
  if (readOnly) {
    if (tools.length > 0) args.push("--tools", tools.join(","));
    else args.push("--no-tools");
  } else if (tools && tools.length > 0) {
    args.push("--tools", tools.join(","));
  }
  args.push("--exclude-tools", CHILD_EXCLUDED_TOOLS);

  const workflowExtension = workflowExtensionPath();
  if (fs.existsSync(workflowExtension)) {
    args.push("--extension", workflowExtension);
  }

  let tempPromptDir = null;
  let tempPromptPath = null;

  const result = {
    agent: agent.name,
    agentSource: agent.source,
    task,
    // -1 until the child exits, so a streaming result reads as running.
    exitCode: -1,
    messages: [],
    stderr: "",
    usage: {
      input: 0,
      output: 0,
      cacheRead: 0,
      cacheWrite: 0,
      cost: 0,
      contextTokens: 0,
      turns: 0,
    },
    model,
    step,
  };

  const emitUpdate = () => {
    if (!onUpdate) return;
    const output = truncateOutput(getFinalOutput(result.messages), UPDATE_OUTPUT_CAP);
    const calls = countToolCalls(result.messages);
    const progress = `Running ${agent.name}...${calls ? ` (${calls} tool calls)` : ""}`;
    onUpdate({
      content: [
        {
          type: "text",
          text: output ? `${progress}\n\n${output}` : progress,
        },
      ],
      details: makeDetails([result]),
    });
  };

  try {
    if (agent.systemPrompt.trim()) {
      const temp = await writePromptToTempFile(agent.name, agent.systemPrompt);
      tempPromptDir = temp.dir;
      tempPromptPath = temp.filePath;
      args.push("--append-system-prompt", tempPromptPath);
    }

    args.push(`Task: ${task}`);

    // Paint the child as running immediately; the first message may be a
    // while away, especially when several children start together.
    emitUpdate();

    let wasAborted = false;
    const invocation = getPiInvocation(args);
    const childCwd = cwd ? path.resolve(defaultCwd, cwd) : defaultCwd;

    const exitCode = await new Promise((resolve) => {
      const proc = spawn(invocation.command, invocation.args, {
        cwd: childCwd,
        shell: false,
        stdio: ["ignore", "pipe", "pipe"],
        env: {
          ...process.env,
          ORBIT_SUBAGENT_CHILD: "1",
          ORBIT_WORKFLOW_MODE: workflowMode || "build",
        },
      });

      let buffer = "";
      let settled = false;
      let killTimer = null;
      let abortHandler = null;

      const finish = (code) => {
        if (settled) return;
        settled = true;
        if (killTimer) clearTimeout(killTimer);
        if (abortHandler) signal?.removeEventListener("abort", abortHandler);
        resolve(code);
      };

      const processLine = (line) => {
        if (!line.trim()) return;
        let event;
        try {
          event = JSON.parse(line);
        } catch {
          return;
        }

        if (event.type !== "message_end" || !event.message) return;
        const message = event.message;
        if (message.role !== "assistant") return;

        result.messages.push(message);
        result.usage.turns += 1;
        const usage = message.usage;
        if (usage) {
          result.usage.input += usage.input || 0;
          result.usage.output += usage.output || 0;
          result.usage.cacheRead += usage.cacheRead || 0;
          result.usage.cacheWrite += usage.cacheWrite || 0;
          result.usage.cost += usage.cost?.total || 0;
          result.usage.contextTokens = usage.totalTokens || 0;
        }
        if (!result.model && message.model) result.model = message.model;
        if (message.stopReason) result.stopReason = message.stopReason;
        if (message.errorMessage) result.errorMessage = message.errorMessage;
        emitUpdate();
      };

      proc.stdout.on("data", (data) => {
        buffer += data.toString();
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";
        for (const line of lines) processLine(line);
      });

      proc.stderr.on("data", (data) => {
        // Keep the tail: a failing child's useful message is at the end.
        result.stderr = (result.stderr + data.toString()).slice(-8 * 1024);
      });

      proc.on("close", (code) => {
        if (buffer.trim()) processLine(buffer);
        finish(code ?? 0);
      });

      proc.on("error", (error) => {
        result.stderr += `\n${error.message}`;
        finish(1);
      });

      if (signal) {
        abortHandler = () => {
          wasAborted = true;
          try {
            proc.kill("SIGTERM");
          } catch {
            /* already exited */
          }
          killTimer = setTimeout(() => {
            if (proc.exitCode === null) {
              try {
                proc.kill("SIGKILL");
              } catch {
                /* already exited */
              }
            }
          }, 5000);
        };
        if (signal.aborted) abortHandler();
        else signal.addEventListener("abort", abortHandler, { once: true });
      }

    });

    result.exitCode = exitCode;
    if (wasAborted) throw new Error("Subagent was aborted");
    return result;
  } finally {
    if (tempPromptPath) {
      try {
        fs.unlinkSync(tempPromptPath);
      } catch {
        /* best-effort temp cleanup */
      }
    }
    if (tempPromptDir) {
      try {
        fs.rmdirSync(tempPromptDir);
      } catch {
        /* best-effort temp cleanup */
      }
    }
  }
}

const AgentScopeSchema = Type.Union(
  [Type.Literal("user"), Type.Literal("project"), Type.Literal("both")],
  {
    description: 'Agent directories. Default: "both".',
    default: "both",
  },
);

const TaskItem = Type.Object({
  agent: Type.String({ description: "Name of the agent to invoke" }),
  task: Type.String({ description: "Task to delegate to the agent" }),
  cwd: Type.Optional(
    Type.String({ description: "Working directory for the child process" }),
  ),
});

const ChainItem = Type.Object({
  agent: Type.String({ description: "Name of the agent to invoke" }),
  task: Type.String({
    description: "Task; use {previous} for the prior step's output",
  }),
  cwd: Type.Optional(
    Type.String({ description: "Working directory for the child process" }),
  ),
});

const SubagentParams = Type.Object({
  action: Type.Optional(
    Type.Union([Type.Literal("run"), Type.Literal("list")], {
      description: 'run (default) or list the discovered agents',
      default: "run",
    }),
  ),
  agent: Type.Optional(
    Type.String({ description: "Agent name (single mode)" }),
  ),
  task: Type.Optional(
    Type.String({ description: "Task to delegate (single mode)" }),
  ),
  tasks: Type.Optional(
    Type.Array(TaskItem, {
      description: `Parallel tasks (max ${MAX_PARALLEL_TASKS})`,
    }),
  ),
  chain: Type.Optional(
    Type.Array(ChainItem, {
      description: "Sequential tasks; {previous} receives the prior output",
    }),
  ),
  agentScope: Type.Optional(AgentScopeSchema),
  confirmProjectAgents: Type.Optional(
    Type.Boolean({
      description: "Confirm project-local agents in an untrusted project",
      default: true,
    }),
  ),
  cwd: Type.Optional(
    Type.String({ description: "Working directory (single mode)" }),
  ),
});

export default function activate(pi) {
  // A child process launched by this extension never needs to register it.
  // (The parent excludes this file from child args; this is a second fence.)
  if (process.env.ORBIT_SUBAGENT_CHILD === "1") return;

  pi.registerCommand("subagents", {
    description: "List subagent roles (built-in, user, project)",
    handler: async (_args, ctx) => {
      const { agents, projectAgentsDir } = discoverAgents(
        ctx.cwd ?? process.cwd(),
        "both",
      );
      const lines = agents.map(
        (agent) => `${agent.name} (${agent.source}) — ${agent.description}`,
      );
      const suffix = projectAgentsDir
        ? `\n\nProject agents: ${projectAgentsDir}`
        : "";
      ctx.ui.notify(
        `Subagents (${agents.length})\n${lines.join("\n") || "None found"}${suffix}`,
        "info",
      );
    },
  });

  pi.registerTool({
    name: "subagent",
    label: "Subagent",
    promptSnippet:
      "Delegate a task to an isolated child agent (scout, planner, reviewer, worker).",
    promptGuidelines: [
      "Use subagent for read-only recon (scout), planning (planner), independent review (reviewer), or isolated implementation (worker) when delegation saves context.",
      "Do not run parallel writers on the same files; give each worker a disjoint scope or run them at different times.",
    ],
    description: [
      "Delegate work to a child pi session with an isolated context window.",
      "Modes: single (agent + task), parallel (tasks array), chain (sequential; {previous} receives the prior step's output), or action:'list' to discover agents.",
      "Built-in agents: scout (read-only recon), planner (read-only plan), reviewer (read-only review), worker (implementation).",
      "User agents live in ~/.pi/agent/agents; project agents live in .pi/agents and override built-ins by name.",
      "Use for recon, independent review, context isolation, or parallel read-only work. Parallel writers share the workspace: give each writer disjoint files or run them at different times.",
    ].join(" "),
    parameters: SubagentParams,

    async execute(_toolCallId, params, signal, onUpdate, ctx) {
      const defaultCwd = ctx.cwd ?? process.cwd();
      const agentScope = params.agentScope ?? "both";
      const discovery = discoverAgents(defaultCwd, agentScope);
      const agents = discovery.agents;
      const action = params.action ?? "run";

      const makeDetails = (mode) => (results) => ({
        mode,
        agentScope,
        projectAgentsDir: discovery.projectAgentsDir,
        results: results.map(summarizeResult),
      });

      if (action === "list") {
        const list = agents
          .map(
            (agent) =>
              `- ${agent.name} (${agent.source})${agent.tools ? ` — tools: ${agent.tools.join(", ")}` : ""}: ${agent.description}`,
          )
          .join("\n");
        return {
          content: [
            {
              type: "text",
              text: `Available agents (${agents.length}):\n\n${list}`,
            },
          ],
          details: {
            mode: "list",
            agentScope,
            projectAgentsDir: discovery.projectAgentsDir,
            results: [],
          },
        };
      }

      const hasChain = Array.isArray(params.chain) && params.chain.length > 0;
      const hasTasks = Array.isArray(params.tasks) && params.tasks.length > 0;
      const hasSingle = Boolean(params.agent && params.task);
      const modeCount = Number(hasChain) + Number(hasTasks) + Number(hasSingle);

      if (modeCount !== 1) {
        const available = formatAgentList(agents).text;
        return {
          content: [
            {
              type: "text",
              text: `Invalid parameters. Provide exactly one of single (agent + task), parallel (tasks), or chain (chain).\nAvailable agents: ${available}`,
            },
          ],
          details: makeDetails("single")([]),
        };
      }

      const requestedNames = new Set();
      if (params.chain) for (const step of params.chain) requestedNames.add(step.agent);
      if (params.tasks) for (const task of params.tasks) requestedNames.add(task.agent);
      if (params.agent) requestedNames.add(params.agent);

      const projectAgentsRequested = [...requestedNames]
        .map((name) => agents.find((agent) => agent.name === name))
        .filter((agent) => agent?.source === "project");

      const confirmProjectAgents = params.confirmProjectAgents ?? true;
      const trusted =
        typeof ctx.isProjectTrusted === "function" ? ctx.isProjectTrusted() : false;
      if (
        projectAgentsRequested.length > 0 &&
        confirmProjectAgents &&
        ctx.hasUI &&
        !trusted
      ) {
        const names = projectAgentsRequested.map((agent) => agent.name).join(", ");
        const dir = discovery.projectAgentsDir ?? "(unknown)";
        const ok = await ctx.ui.confirm(
          "Run project-local agents?",
          `Agents: ${names}\nSource: ${dir}\n\nProject agents are repo-controlled. Only continue for trusted repositories.`,
        );
        if (!ok) {
          return {
            content: [
              {
                type: "text",
                text: "Canceled: project-local agents not approved.",
              },
            ],
            details: makeDetails(
              hasChain ? "chain" : hasTasks ? "parallel" : "single",
            )([]),
          };
        }
      }

      const dispatchDefaults = {
        model: ctx.model ? `${ctx.model.provider}/${ctx.model.id}` : undefined,
        thinkingLevel: ctx.thinkingLevel,
      };
      const sessionId = ctx.sessionManager?.getSessionId?.();
      const workflowMode =
        readWorkflowMode(sessionId) ??
        (process.env.ORBIT_WORKFLOW_MODE === "plan" ||
        process.env.ORBIT_WORKFLOW_MODE === "ask" ||
        process.env.ORBIT_WORKFLOW_MODE === "build"
          ? process.env.ORBIT_WORKFLOW_MODE
          : "build");

      if (hasChain) {
        const results = [];
        let previousOutput = "";

        for (let index = 0; index < params.chain.length; index++) {
          const step = params.chain[index];
          const taskWithContext = step.task.replace(
            /\{previous\}/g,
            previousOutput,
          );
          const chainUpdate = onUpdate
            ? (partial) => {
                const current = partial.details?.results?.[0];
                if (!current) return;
                onUpdate({
                  content: partial.content,
                  details: makeDetails("chain")([...results, current]),
                });
              }
            : undefined;

          const result = await runSingleAgent({
            defaultCwd,
            dispatchDefaults,
            agents,
            agentName: step.agent,
            task: taskWithContext,
            cwd: step.cwd,
            step: index + 1,
            signal,
            onUpdate: chainUpdate,
            makeDetails: makeDetails("chain"),
            workflowMode,
          });
          results.push(result);

          if (isFailedResult(result)) {
            return {
              content: [
                {
                  type: "text",
                  text: `Chain stopped at step ${index + 1} (${step.agent}): ${getResultOutput(result)}`,
                },
              ],
              details: makeDetails("chain")(results),
              isError: true,
            };
          }
          previousOutput = getFinalOutput(result.messages);
        }

        const usage = formatUsage(aggregateUsage(results));
        return {
          content: [
            {
              type: "text",
              text: `${getFinalOutput(results[results.length - 1].messages) || "(no output)"}${usage ? `\n\n[${usage}]` : ""}`,
            },
          ],
          details: makeDetails("chain")(results),
        };
      }

      if (hasTasks) {
        if (params.tasks.length > MAX_PARALLEL_TASKS) {
          return {
            content: [
              {
                type: "text",
                text: `Too many parallel tasks (${params.tasks.length}). Max is ${MAX_PARALLEL_TASKS}.`,
              },
            ],
            details: makeDetails("parallel")([]),
          };
        }

        const allResults = params.tasks.map((task) => ({
          agent: task.agent,
          agentSource: "unknown",
          task: task.task,
          exitCode: -1,
          messages: [],
          stderr: "",
          usage: {
            input: 0,
            output: 0,
            cacheRead: 0,
            cacheWrite: 0,
            cost: 0,
            contextTokens: 0,
            turns: 0,
          },
        }));

        const emitParallelUpdate = () => {
          if (!onUpdate) return;
          const running = allResults.filter((result) => result.exitCode === -1).length;
          const done = allResults.length - running;
          const rows = allResults
            .map((result) => {
              const status =
                result.exitCode === -1
                  ? "running"
                  : isFailedResult(result)
                    ? "failed"
                    : "done";
              return `- ${result.agent}: ${status}`;
            })
            .join("\n");
          onUpdate({
            content: [
              {
                type: "text",
                text: `Parallel: ${done}/${allResults.length} done, ${running} running...\n\n${rows}`,
              },
            ],
            details: makeDetails("parallel")(allResults),
          });
        };

        emitParallelUpdate();

        const results = await mapWithConcurrencyLimit(
          params.tasks,
          MAX_CONCURRENCY,
          async (task, index) => {
            const result = await runSingleAgent({
              defaultCwd,
              dispatchDefaults,
              agents,
              agentName: task.agent,
              task: task.task,
              cwd: task.cwd,
              signal,
              onUpdate: () => emitParallelUpdate(),
              makeDetails: makeDetails("parallel"),
              workflowMode,
            });
            allResults[index] = result;
            emitParallelUpdate();
            return result;
          },
        );

        const successCount = results.filter(
          (result) => !isFailedResult(result),
        ).length;
        const summaries = results.map((result) => {
          const status = isFailedResult(result) ? "failed" : "completed";
          const usage = formatUsage(result.usage, result.model);
          return `### ${result.agent} ${status}\n\n${truncateOutput(
            getResultOutput(result),
            PER_TASK_OUTPUT_CAP,
          )}${usage ? `\n\n[${usage}]` : ""}`;
        });
        return {
          content: [
            {
              type: "text",
              text: `Parallel: ${successCount}/${results.length} succeeded\n\n${summaries.join(
                "\n\n---\n\n",
              )}`,
            },
          ],
          details: makeDetails("parallel")(results),
        };
      }

      const result = await runSingleAgent({
        defaultCwd,
        dispatchDefaults,
        agents,
        agentName: params.agent,
        task: params.task,
        cwd: params.cwd,
        signal,
        onUpdate,
        makeDetails: makeDetails("single"),
        workflowMode,
      });

      const failed = isFailedResult(result);
      const usage = formatUsage(result.usage, result.model);
      return {
        content: [
          {
            type: "text",
            text: `### ${result.agent} ${failed ? "failed" : "completed"}\n\n${truncateOutput(
              getResultOutput(result),
            )}${usage ? `\n\n[${usage}]` : ""}`,
          },
        ],
        details: makeDetails("single")([result]),
        isError: failed,
      };
    },
  });
}

function aggregateUsage(results) {
  const total = {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    cost: 0,
    turns: 0,
  };
  for (const result of results) {
    total.input += result.usage.input;
    total.output += result.usage.output;
    total.cacheRead += result.usage.cacheRead;
    total.cacheWrite += result.usage.cacheWrite;
    total.cost += result.usage.cost;
    total.turns += result.usage.turns;
  }
  return total;
}
