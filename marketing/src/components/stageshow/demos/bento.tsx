"use client";

import {
  CommandLineIcon,
  FileEditIcon,
  FileSearchIcon,
  GitCompareIcon,
  LockIcon,
  ArrowDown01Icon,
  Search01Icon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";

import {
  Code,
  Collapse,
  Roll,
  Segmented,
  Spinner,
  Stage,
  Toggle,
  type Act,
} from "@/components/stageshow/kit";
import "@/components/stageshow/orbit-look.css";
import "./bento.css";

// The feature grid's pictures, one Stage per tile (bento rule 12). The copy
// lives in the page around them; these hold only the product. Each tile shows
// one component at real size, never a whole window: the hero above already
// spends the grid's one set of window chrome.

function Glyph({ icon, size = 14 }: { icon: IconSvgElement; size?: number }) {
  return <HugeiconsIcon icon={icon} size={size} strokeWidth={1.6} className="o-icon" />;
}

/* ── Approvals: the lead tile, the one cursor story in the grid ─────── */

// Beats: a bash call waits on the access guard; the cursor goes to Allow
// once and presses it; the bar folds away; the call's status rolls Waiting →
// Running → Done. Plays once as the grid scrolls in.

const COMMAND = "rm -rf target/ && cargo build --release";

type Ap = { phase: "ask" | "run" | "done"; lines: number };

const OUTPUT = {
  wide: [
    "   Compiling orbit-rpc v0.2.19",
    "   Compiling orbit-pi v0.2.19",
    "    Finished `release` profile [optimized] in 38.2s",
  ],
  compact: ["Compiling orbit-rpc", "Compiling orbit-pi", "Finished release in 38.2s"],
};

async function approvals(a: Act<Ap>) {
  await a.wait(900);
  await a.move("allow");
  await a.wait(250);
  await a.click("allow");
  // The output opens with its first line in it, never as an empty box.
  a.put({ phase: "run", lines: 1 });
  await a.wait(650);
  a.put({ lines: 2 });
  await a.wait(900);
  a.put({ phase: "done", lines: 3 });
  await a.wait(1600);
}

export function ApprovalPicture() {
  return (
    <Stage<Ap>
      width={760}
      height={250}
      compact={{ below: 620, width: 358, height: 236 }}
      theme="dark"
      initial={{ phase: "ask", lines: 0 }}
      poster={{ phase: "ask", lines: 0 }}
      hover={["allow"]}
      script={approvals}
      play="once"
      cursor={{ at: { wide: { x: 700, y: 40 }, compact: { x: 320, y: 40 } }, style: "real" }}
      className="orbit-look ob ob-approve"
    >
      {(s, _rect, layout) => (
        <div className="ob-ap">
          <div className="ob-ap__tool">
            <Glyph icon={CommandLineIcon} />
            <span className="ob-ap__verb">Bash</span>
            <span className="ob-ap__cmd o-mono">{layout === "wide" ? COMMAND : "cargo build --release"}</span>
            <span className="ob-ap__status" data-phase={s.phase}>
              <span className="ob-ap__mark" data-done={s.phase === "done" || undefined}>
                <Spinner size={11} />
                <HugeiconsIcon icon={Tick02Icon} size={13} strokeWidth={2} />
              </span>
              <Roll>{s.phase === "ask" ? "Waiting for approval" : s.phase === "run" ? "Running" : "Done in 38s"}</Roll>
            </span>
          </div>

          <Collapse open={s.phase === "ask"}>
            <div className="ob-ap__bar">
              <div className="ob-ap__ask">
                <span className="ob-ap__lock">
                  <Glyph icon={LockIcon} size={13} />
                </span>
                <span className="ob-ap__q">
                  Allow <b>bash</b> to run this command?
                </span>
                {layout === "wide" && <span className="ob-ap__mode">Supervised</span>}
              </div>
              <div className="ob-ap__choices">
                <span className="ob-ap__btn" data-primary data-target="allow" data-cursor="pointer">
                  Allow once
                  <kbd>⏎</kbd>
                </span>
                <span className="ob-ap__btn">{layout === "wide" ? "Always allow bash" : "Always"}</span>
                <span className="ob-ap__btn">
                  Deny
                  <kbd>esc</kbd>
                </span>
              </div>
            </div>
          </Collapse>

          <Collapse open={s.phase !== "ask"} solid>
            <div className="ob-ap__out o-mono">
              {OUTPUT[layout].map((line, i) => (
                <div key={line} className="ob-ap__line" data-on={i < s.lines || undefined} data-ok={i === 2 || undefined}>
                  {line}
                </div>
              ))}
            </div>
          </Collapse>

          <div className="ob-ap__composer">
            <span className="ob-ap__ph">Steer or queue a follow-up…</span>
          </div>
        </div>
      )}
    </Stage>
  );
}

/* ── Plan / Build / Ask ─────────────────────────────────────────────── */

// No cursor: the mode changes and the tool list answers it, write tools
// switching off one after another. Ends on Plan, the read-only state.

type Md = { mode: 0 | 1 | 2; off: number };

const TOOLS: { name: string; icon: IconSvgElement; write?: boolean; note?: string }[] = [
  { name: "read", icon: FileSearchIcon },
  { name: "grep", icon: Search01Icon },
  { name: "edit", icon: FileEditIcon, write: true },
  { name: "write", icon: FileEditIcon, write: true },
  { name: "bash", icon: CommandLineIcon, note: "read-only" },
];

async function modes(a: Act<Md>) {
  await a.wait(1100);
  a.put({ mode: 0 });
  await a.wait(220);
  for (let i = 1; i <= 3; i++) {
    a.put({ off: i });
    await a.wait(110);
  }
  await a.wait(1600);
}

export function ModesPicture() {
  return (
    <Stage<Md>
      width={380}
      height={258}
      theme="dark"
      initial={{ mode: 1, off: 0 }}
      poster={{ mode: 0, off: 3 }}
      script={modes}
      play="once"
      className="orbit-look ob ob-modes"
    >
      {(s) => (
        <div className="ob-md">
          <Segmented items={["Plan", "Build", "Ask"]} value={s.mode} className="ob-md__seg" />
          <div className="ob-md__list">
            {TOOLS.map((t, i) => {
              const gated = t.write ? s.off >= (t.name === "edit" ? 1 : 2) : false;
              const limited = t.note ? s.off >= 3 : false;
              return (
                <div key={t.name} className="ob-md__row" data-off={gated || undefined}>
                  <Glyph icon={t.icon} />
                  <span className="o-mono ob-md__name">{t.name}</span>
                  <span className="ob-md__note" data-on={limited || undefined}>
                    {t.note}
                  </span>
                  {i < 4 ? <Toggle on={!gated} className="ob-md__toggle" /> : <span className="ob-md__gate" data-on={limited || undefined}><Glyph icon={LockIcon} size={13} /></span>}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </Stage>
  );
}

/* ── Last Turn review ───────────────────────────────────────────────── */

const DIFF = [
  { text: "fn restart_delay(attempt: u32) -> Duration {" },
  { text: "    Duration::from_millis(250)", kind: "del" as const },
  { text: "    let ms = 250 * 4u64.pow(attempt.min(2));", kind: "add" as const },
  { text: "    Duration::from_millis(ms)", kind: "add" as const },
  { text: "}" },
  { text: "" },
  { text: "const AUTO_RESTART_LIMIT: u32 = 3;", kind: "add" as const },
];

export function ReviewPicture() {
  return (
    <Stage width={380} height={280} theme="dark" className="orbit-look ob ob-review">
      {() => (
        <div className="ob-rv">
          <div className="ob-rv__head">
            <span className="ob-rv__source">
              <Glyph icon={GitCompareIcon} size={13} />
              Last Turn
              <Glyph icon={ArrowDown01Icon} size={12} />
            </span>
            <span className="ob-rv__nav">1 of 2</span>
          </div>
          <div className="ob-rv__file">
            <span className="ob-rv__rs" aria-hidden>
              rs
            </span>
            <span className="o-mono">client.rs</span>
            <span className="ob-rv__count o-mono">
              <span className="o-add">+38</span> <span className="o-del">−6</span>
            </span>
          </div>
          <Code lines={DIFF} start={212} className="ob-rv__code" />
        </div>
      )}
    </Stage>
  );
}

/* ── Models ─────────────────────────────────────────────────────────── */

const MODELS = [
  { name: "Claude Opus 5.5", provider: "Anthropic", ctx: "200K" },
  { name: "GPT-5", provider: "OpenAI", ctx: "400K", on: true },
  { name: "Gemini 2.5 Pro", provider: "Google", ctx: "1M" },
  { name: "qwen3-coder:30b", provider: "Ollama · local", ctx: "256K" },
];

export function ModelsPicture() {
  return (
    <Stage width={380} height={280} theme="dark" className="orbit-look ob ob-models">
      {() => (
        <div className="ob-ml">
          <div className="ob-ml__search">
            <Glyph icon={Search01Icon} size={13} />
            <span>Search models</span>
          </div>
          {MODELS.map((m) => (
            <div key={m.name} className="ob-ml__row" data-on={m.on || undefined}>
              <span className="ob-ml__name">{m.name}</span>
              <span className="ob-ml__provider">{m.provider}</span>
              <span className="ob-ml__ctx o-mono">{m.ctx}</span>
              <span className="ob-ml__check">{m.on && <HugeiconsIcon icon={Tick02Icon} size={13} strokeWidth={2} />}</span>
            </div>
          ))}
          <div className="ob-ml__effort">
            <span>Thinking</span>
            <Segmented items={["Off", "Low", "Medium", "High"]} value={2} className="ob-ml__seg" />
          </div>
        </div>
      )}
    </Stage>
  );
}
