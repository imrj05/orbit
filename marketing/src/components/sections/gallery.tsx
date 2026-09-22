import { Shot } from "@/components/shot";
import { Band, Eyebrow, Shell } from "@/components/ui";

const shots = [
  {
    src: "/screens/new-task.png",
    lightSrc: "/screens/new-task-light.png",
    kicker: "New task",
    caption: "Pick a workspace, choose an access mode, then describe the task.",
    alt: "Orbit's new-task page with a workspace picker, the composer, and the access-mode menu open on Supervised, Auto-accept edits, and Full access.",
    wide: true,
  },
  {
    src: "/screens/git.png",
    lightSrc: "/screens/git-light.png",
    kicker: "Git",
    caption: "Stage, review, and commit — with History and Graph.",
    alt: "Orbit's Git page listing staged and changed files with line counts and a commit box.",
    wide: false,
  },
  {
    src: "/screens/providers.png",
    lightSrc: "/screens/providers-light.png",
    kicker: "Providers",
    caption: "pi's live catalog plus custom providers, with login and usage.",
    alt: "Orbit's Providers page showing provider cards for Ollama, OpenCode Go, Bedrock, and Anthropic with usage meters.",
    wide: false,
  },
  {
    src: "/screens/plugins.png",
    lightSrc: "/screens/plugins-light.png",
    kicker: "Plugins",
    caption: "pi packages for this machine and this project.",
    alt: "Orbit's Plugins page listing installed pi packages with update and remove actions.",
    wide: false,
  },
  {
    src: "/screens/skills.png",
    lightSrc: "/screens/skills-light.png",
    kicker: "Skills",
    caption: "Project and global pi skills, with the SKILL.md in view.",
    alt: "Orbit's Skills page listing project and global skills with the selected skill's SKILL.md rendered on the right.",
    wide: false,
  },
  {
    src: "/screens/models.png",
    lightSrc: "/screens/models-light.png",
    kicker: "Models",
    caption: "Every model pi reports, grouped by provider, with favorites.",
    alt: "Orbit's Models page listing models grouped by provider with context sizes and favorite toggles.",
    wide: false,
  },
  {
    src: "/screens/appearance.png",
    lightSrc: "/screens/appearance-light.png",
    kicker: "Appearance",
    caption: "Themes, background, fonts, sizes, and spacing density.",
    alt: "Orbit's Appearance settings with theme swatches, type and density previews, and font pickers.",
    wide: false,
  },
  {
    src: "/screens/usage.png",
    lightSrc: "/screens/usage-light.png",
    kicker: "Usage",
    caption: "Requests, tokens, cost, and cache — read from your own sessions.",
    alt: "Orbit's Usage page with request, token, cost, and cache metrics above a tokens-over-time chart.",
    wide: true,
  },
  {
    src: "/screens/general.png",
    lightSrc: "/screens/general-light.png",
    kicker: "General",
    caption: "Local by default — sessions, workspace, and notifications.",
    alt: "Orbit's General settings showing the connected pi agent, the local session store, the workspace, and notification toggles.",
    wide: false,
  },
  {
    src: "/screens/agent.png",
    lightSrc: "/screens/agent-light.png",
    kicker: "Agent",
    caption: "Follow-up delivery, compaction, retries, and session naming.",
    alt: "Orbit's Agent settings with follow-up delivery, auto-compaction, auto-retry, compact-now, and session rename.",
    wide: false,
  },
];

export function Gallery() {
  return (
    <Shell>
      <Band id="gallery" dashed className="scroll-mt-20 py-16 sm:py-[88px]">
        <Eyebrow className="mb-3.5">Surfaces</Eyebrow>
        <h2 className="mb-4 max-w-[22ch] text-[clamp(28px,3.2vw,38px)] font-[300] leading-[1.2] tracking-[-0.015em] text-ink">
          Every surface, one window.
        </h2>
        <p className="mb-10 max-w-[52ch] text-[14.5px] leading-[1.7] text-ink-2">
          Sessions, Git, providers, plugins, skills, models, usage, and settings
          live in the same app — no context switching, no web views.
        </p>

        <div className="grid grid-cols-1 gap-5 md:grid-cols-2">
          {shots.map((s) => (
            <figure key={s.kicker} className={s.wide ? "md:col-span-2" : ""}>
              <Shot
                src={s.src}
                srcLight={s.lightSrc}
                alt={s.alt}
                sizes={
                  s.wide
                    ? "(max-width: 768px) 100vw, 1180px"
                    : "(max-width: 768px) 100vw, 590px"
                }
                className="transition-shadow duration-200"
              />
              <figcaption className="mt-3 flex flex-wrap items-baseline gap-x-3 gap-y-1">
                <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-ink-3">
                  {s.kicker}
                </span>
                <span className="text-[13.5px] text-ink-2">{s.caption}</span>
              </figcaption>
            </figure>
          ))}
        </div>
      </Band>
    </Shell>
  );
}
