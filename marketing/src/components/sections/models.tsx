import { Shot } from "@/components/shot";
import { Story } from "@/components/sections/story";

function ModelCard() {
  const rows = [
    { name: "Claude", meta: "Anthropic" },
    { name: "GPT", meta: "OpenAI" },
    { name: "DeepSeek", meta: "Ollama · local" },
  ];
  return (
    <div className="absolute -left-5 top-8 hidden w-[188px] rounded-[12px] border border-edge-default bg-window/95 p-3 shadow-card backdrop-blur md:block">
      <p className="mb-2 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-3">
        Model
      </p>
      <div className="flex flex-col gap-1">
        {rows.map((r, i) => (
          <div
            key={r.name}
            className={`flex items-baseline justify-between gap-2 rounded-[7px] px-2 py-1.5 text-[12px] ${
              i === 0 ? "bg-surface-2 text-ink" : "text-ink-2"
            }`}
          >
            <span>{r.name}</span>
            <span className="font-mono text-[10px] text-ink-3">{r.meta}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function Models() {
  return (
    <Story
      id="providers"
      label="Models & providers"
      title="Every provider. Every model. One window."
      copy="pi's live catalog plus custom providers — sign in with a key or OAuth, then pick any model per task, with thinking effort set alongside it. Local models from Ollama sit next to the hosted ones."
      points={[
        "Anthropic, OpenAI, Bedrock, OpenCode Go, and your own endpoints.",
        "Point Orbit at a local Ollama and everything stays on your machine.",
        "Favorites and context sizes, grouped by provider.",
      ]}
      visual={
        <div className="relative">
          <Shot
            src="/screens/models.png"
            srcLight="/screens/models-light.png"
            alt="Orbit's Models page listing models grouped by provider with context sizes and favorite toggles."
            sizes="(max-width: 1024px) 100vw, 560px"
          />
          <ModelCard />
        </div>
      }
    />
  );
}
