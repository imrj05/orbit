"use client";

import {
  Add01Icon,
  ArrowDown01Icon,
  ArrowUp02Icon,
  BulbIcon,
  CommandLineIcon,
  FileEditIcon,
  FileSearchIcon,
  Folder01Icon,
  GitBranchIcon,
  GitCompareIcon,
  LockIcon,
  Rocket01Icon,
  Search01Icon,
  Settings01Icon,
  StopIcon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import { useState, type ReactNode } from "react";

import {
  Bar,
  Collapse,
  Count,
  Spinner,
  Stage,
  Stream,
  Window,
  type Act,
} from "@/components/stageshow/kit";
import "@/components/stageshow/orbit-look.css";
import "./hero.css";

// The hero: one Orbit session doing one real thing. Beats: the task is typed
// into the composer and sent (the send key turns into Stop on itself); pi
// reads the client, edits it and runs the tests, each tool row arriving with
// a spinner that settles into a tick; the header's diff chip rolls as the
// edit lands; the answer streams in place; the "Changed 2 files" card arrives
// as the payoff and the run settles. Plays once; the still is the payoff.
//
// Real strings: the prompt, three tool calls, the answer, the two files and
// their counts, the session list's titles. The earlier turn and the rest of
// the chrome are placeholder at real line heights.

const PROMPT = "Back off before the RPC client respawns pi";
const ANSWER =
  "Respawns now wait 250 ms, then 1 s, then 4 s, and stop after three tries. The counter only refills once a run stays up, so a crash loop can't spin forever. Both paths have tests.";
const WORDS = ANSWER.split(" ").length;

type Tool = 0 | 1 | 2; // 0 hidden · 1 running · 2 done

type S = {
  draft: string;
  sent: boolean;
  running: boolean;
  tools: [Tool, Tool, Tool];
  words: number;
  added: number;
  removed: number;
  card: boolean;
  seconds: number;
};

const initial: S = {
  draft: "",
  sent: false,
  running: false,
  tools: [0, 0, 0],
  words: 0,
  added: 0,
  removed: 0,
  card: false,
  seconds: 0,
};

const poster: S = {
  draft: "",
  sent: true,
  running: false,
  tools: [2, 2, 2],
  words: WORDS,
  added: 79,
  removed: 6,
  card: true,
  seconds: 21,
};

async function script(a: Act<S>) {
  await a.wait(700);
  a.mark("type");
  await a.type("draft", PROMPT, { interval: 34 });
  await a.wait(300);
  a.mark("send");
  await a.key("Enter");
  a.put({ sent: true, draft: "", running: true, seconds: 1 });

  const tick = (n: number) => a.put({ seconds: n });
  await a.wait(650);
  a.put({ tools: [1, 0, 0] });
  await a.wait(800);
  tick(4);
  a.put({ tools: [2, 1, 0] });
  await a.wait(1000);
  tick(9);
  a.put({ tools: [2, 2, 1], added: 38, removed: 6 });
  await a.wait(1200);
  tick(15);
  a.put({ tools: [2, 2, 2], added: 79 });
  await a.wait(450);

  a.mark("stream");
  for (let i = 1; i <= WORDS; i++) {
    a.put({ words: i });
    await a.wait(42);
  }
  await a.wait(250);
  a.mark("card");
  a.put({ card: true, running: false, seconds: 21 });
  await a.wait(2600);
}

export default function HeroDemo() {
  return (
    <Stage<S>
      width={1120}
      height={660}
      compact={{ below: 760, width: 358, height: 560 }}
      theme="dark"
      fidelity="standard"
      initial={initial}
      poster={poster}
      script={script}
      play="once"
      keys={{ wide: "bottom-right", compact: "bottom" }}
      className="orbit-look oh"
    >
      {(s, _rect, layout) => (
        <Window
          className="oh-win"
          sidebarWidth={248}
          sidebar={layout === "compact" ? undefined : <Sidebar running={s.running} />}
          title={layout === "compact" ? "RPC respawn backoff" : undefined}
          lead={layout === "compact" ? undefined : <span className="oh-title">RPC respawn backoff</span>}
          toolbar={<DiffChip added={s.added} removed={s.removed} />}
          body={{ className: "oh-main" }}
        >
          <div className="oh-thread">
            {/* The earlier turn: its question, answer and changed-files card
                as placeholder, so the transcript opens full and the eye goes
                to the turn that is about to happen. */}
            <div className="oh-past">
              <Bar w={180} h={30} r={10} tone={1} className="oh-past-q" />
              <Bar w="94%" h={7} r={3.5} tone={2} />
              <Bar w="88%" h={7} r={3.5} tone={2} />
              <Bar w="72%" h={7} r={3.5} tone={2} />
              <Bar w="100%" h={64} r={12} tone={1} className="oh-past-card" />
              <Bar w={240} h={30} r={10} tone={1} className="oh-past-q" />
              <Bar w="90%" h={7} r={3.5} tone={2} />
              <Bar w="64%" h={7} r={3.5} tone={2} />
            </div>

            <Collapse open={s.sent} reveal="drop" from="below">
              <div className="oh-q">
                <div className="oh-bubble">{PROMPT}</div>
              </div>
            </Collapse>

            <Collapse open={s.sent}>
              <div className="oh-status">
                <span className="oh-status__mark" data-done={!s.running || undefined}>
                  <Spinner size={12} />
                  <HugeiconsIcon icon={BulbIcon} size={14} strokeWidth={1.6} />
                </span>
                <span>
                  {s.running ? "Working for " : "Worked for "}
                  <Count value={s.seconds} duration={360} />s
                </span>
              </div>
            </Collapse>

            <div className="oh-tools">
              <ToolRow
                state={s.tools[0]}
                icon={FileSearchIcon}
                verb="Read"
                target={layout === "compact" ? "src/client.rs" : "crates/orbit-rpc/src/client.rs"}
              />
              <ToolRow
                state={s.tools[1]}
                icon={FileEditIcon}
                verb="Edit"
                target="client.rs"
                meta={
                  <>
                    <span className="o-add">+38</span> <span className="o-del">−6</span>
                  </>
                }
              />
              <ToolRow
                state={s.tools[2]}
                icon={CommandLineIcon}
                verb="Bash"
                target="cargo test -p orbit-rpc"
                meta={s.tools[2] === 2 ? <span className="o-add">14 passed</span> : <span className="oh-faint">running</span>}
              />
            </div>

            <Collapse open={s.words > 0}>
              <Stream className="oh-answer" text={ANSWER} count={s.words} />
            </Collapse>

            <Collapse open={s.card} reveal="drop" from="below">
              <ChangedCard compact={layout === "compact"} />
            </Collapse>
          </div>

          <Composer draft={s.draft} running={s.running} sent={s.sent} compact={layout === "compact"} />
        </Window>
      )}
    </Stage>
  );
}

function Icon({ icon, size = 14 }: { icon: IconSvgElement; size?: number }) {
  return <HugeiconsIcon icon={icon} size={size} strokeWidth={1.6} className="o-icon" />;
}

function ToolRow({
  state,
  icon,
  verb,
  target,
  meta,
}: {
  state: Tool;
  icon: IconSvgElement;
  verb: string;
  target: string;
  meta?: ReactNode;
}) {
  return (
    <Collapse open={state > 0}>
      <div className="oh-tool">
        <Icon icon={icon} />
        <span className="oh-tool__verb">{verb}</span>
        <span className="oh-tool__target o-mono">{target}</span>
        <span className="oh-tool__meta">{meta}</span>
        <span className="oh-tool__mark" data-done={state === 2 || undefined}>
          <Spinner size={11} />
          <HugeiconsIcon icon={Tick02Icon} size={13} strokeWidth={2} />
        </span>
      </div>
    </Collapse>
  );
}

function DiffChip({ added, removed }: { added: number; removed: number }) {
  return (
    <span className="oh-diff" data-on={added > 0 || undefined}>
      <span className="o-add">
        +<Count value={added} duration={520} />
      </span>
      <span className="o-del">
        −<Count value={removed} duration={520} />
      </span>
    </span>
  );
}

const FILES = [
  { path: "crates/orbit-rpc/src/client.rs", add: 38, del: 6 },
  { path: "crates/orbit-rpc/tests/restart.rs", add: 41, del: 0 },
];

function ChangedCard({ compact }: { compact: boolean }) {
  return (
    <div className="oh-card">
      <div className="oh-card__head">
        <span className="oh-card__glyph">
          <Icon icon={GitCompareIcon} size={15} />
        </span>
        <span className="oh-card__titles">
          <span className="oh-card__title">Changed 2 files</span>
          <span className="oh-card__sum">
            <span className="o-add">+79</span> <span className="o-del">−6</span>
          </span>
        </span>
        <span className="oh-card__review">
          <Icon icon={GitCompareIcon} size={13} />
          Review
        </span>
      </div>
      {FILES.map((f) => (
        <div key={f.path} className="oh-card__file">
          <span className="oh-card__rs" aria-hidden>
            rs
          </span>
          <span className="oh-card__path o-mono">{compact ? f.path.split("/").pop() : f.path}</span>
          <span className="oh-card__count o-mono">
            <span className="o-add">+{f.add}</span> <span className="o-del">−{f.del}</span>
          </span>
        </div>
      ))}
    </div>
  );
}

function Composer({
  draft,
  running,
  sent,
  compact,
}: {
  draft: string;
  running: boolean;
  sent: boolean;
  compact: boolean;
}) {
  return (
    <div className="oh-composer-wrap">
      <div className="oh-composer" data-focus={!sent || undefined}>
        <div className="oh-composer__text">
          {draft ? (
            <span className="oh-composer__draft">
              {draft}
              <i className="oh-caret" />
            </span>
          ) : (
            <span className="oh-composer__ph">
              <i className="oh-caret" data-blink />
              {running ? "Steer or queue a follow-up…" : "Describe the task…"}
            </span>
          )}
        </div>
        <div className="oh-composer__row">
          {!compact && (
            <>
              <span className="o-chip">
                <Icon icon={Add01Icon} />
              </span>
              <span className="o-chip">
                <Icon icon={LockIcon} />
              </span>
            </>
          )}
          <span className="o-chip">
            <Icon icon={Rocket01Icon} />
            Build
            <Icon icon={ArrowDown01Icon} size={12} />
          </span>
          <span className="oh-spacer" />
          <span className="o-chip">
            <span className="oh-model-dot" />
            Sonnet 5.5
            {!compact && <Icon icon={ArrowDown01Icon} size={12} />}
          </span>
          {!compact && (
            <span className="o-chip">
              <Icon icon={BulbIcon} />
              High
            </span>
          )}
          <span className="oh-send" data-running={running || undefined} data-ready={(!!draft && !running) || undefined}>
            <HugeiconsIcon icon={ArrowUp02Icon} size={15} strokeWidth={2} className="oh-send__go" />
            <HugeiconsIcon icon={StopIcon} size={13} strokeWidth={2} className="oh-send__stop" />
          </span>
        </div>
      </div>
      <div className="oh-foot">
        <span>
          <Icon icon={Folder01Icon} size={13} />
          orbit
        </span>
        <span>
          <Icon icon={GitBranchIcon} size={13} />
          main
        </span>
      </div>
    </div>
  );
}

const SESSIONS = [
  { title: "Accent color picker", preview: "Can you add an option to change the accent", age: "38m" },
  { title: "Usage summary KPIs", preview: "Check the usage page summary cards", age: "3h" },
  { title: "Fix issue 79 layout", preview: "Can you check the issue below and find", age: "5h" },
];

function Sidebar({ running }: { running: boolean }) {
  return (
    <div className="oh-side">
      <div className="oh-brand">ORBIT PI</div>
      <div className="oh-new">
        <Icon icon={FileEditIcon} />
        <span>New Task</span>
        <span className="oh-kbd">⌘N</span>
      </div>
      <div className="oh-side__label">
        All sessions
      </div>
      <div className="oh-session" data-active>
        <div className="oh-session__top">
          <span className="oh-session__title">RPC respawn backoff</span>
          <span className="oh-session__state" data-done={!running || undefined}>
            <Spinner size={10} />
            <span className="oh-session__now">now</span>
          </span>
        </div>
        <div className="oh-session__meta">
          <Icon icon={Folder01Icon} size={12} />
          orbit
          <Icon icon={GitBranchIcon} size={12} />
          main
        </div>
      </div>
      {SESSIONS.map((r) => (
        <div key={r.title} className="oh-session">
          <div className="oh-session__top">
            <span className="oh-session__title">{r.title}</span>
            <span className="oh-session__age">{r.age}</span>
          </div>
          <div className="oh-session__preview">{r.preview}</div>
        </div>
      ))}
      <div className="oh-session oh-session--filler">
        <Bar w={132} h={7} r={3.5} tone={2} />
        <Bar w={168} h={6} r={3} tone={1} />
      </div>
      <div className="oh-side__foot">
        <span className="oh-connected">
          <i />
          Connected
        </span>
        <span className="oh-side__tools">
          <Icon icon={Search01Icon} size={13} />
          <Icon icon={Settings01Icon} size={13} />
        </span>
      </div>
    </div>
  );
}

/**
 * The hero with an accessible Replay: the demo plays once as it scrolls into
 * view and ends on its payoff, so a remount is the explained way back to the
 * opening frame (`references/kit.md`, Explicit replay).
 */
export function HeroDemoWithReplay({ label, replayLabel }: { label: string; replayLabel: string }) {
  const [run, setRun] = useState(0);
  return (
    <div className="oh-host">
      <div role="img" aria-label={label}>
        <HeroDemo key={run} />
      </div>
      <button type="button" className="oh-replay" onClick={() => setRun((n) => n + 1)}>
        {replayLabel}
      </button>
    </div>
  );
}
