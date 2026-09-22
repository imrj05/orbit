import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { ImageResponse } from "next/og";

export const OG_SIZE = { width: 1200, height: 630 };

let logoPromise: Promise<string> | null = null;

/** The Orbit wordmark, inlined so the card renders without a network hop. */
function logoDataUrl(): Promise<string> {
  logoPromise ??= readFile(join(process.cwd(), "public/logo.png")).then(
    (buffer) => `data:image/png;base64,${buffer.toString("base64")}`,
  );
  return logoPromise;
}

const CHIPS = ["Chat", "Tools", "Review", "Git", "Providers"];

/**
 * The shared dark OG card: wordmark, eyebrow, a headline, and the product
 * chips. Sourced from the same palette as the site.
 */
export async function renderOgCard({
  eyebrow,
  title,
  meta,
}: {
  eyebrow: string;
  title: string;
  meta?: string;
}) {
  const logo = await logoDataUrl();

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          background: "#141518",
          padding: "68px 72px",
          color: "#f4f4f5",
          fontFamily: "sans-serif",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={logo} width={116} height={39} alt="" />
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 12,
              fontSize: 20,
              letterSpacing: 3,
              color: "#a2a5ab",
            }}
          >
            <div
              style={{
                width: 9,
                height: 9,
                borderRadius: 9,
                background: "#5b9dff",
              }}
            />
            {eyebrow.toUpperCase()}
          </div>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
          <div
            style={{
              fontSize: 68,
              lineHeight: 1.1,
              letterSpacing: -1.6,
              maxWidth: 950,
              color: "#f4f4f5",
            }}
          >
            {title}
          </div>
          {meta ? (
            <div style={{ fontSize: 27, color: "#a2a5ab" }}>{meta}</div>
          ) : null}
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            borderTop: "1px solid rgba(255,255,255,0.09)",
            paddingTop: 28,
          }}
        >
          <div style={{ display: "flex", gap: 12 }}>
            {CHIPS.map((chip) => (
              <div
                key={chip}
                style={{
                  display: "flex",
                  fontSize: 20,
                  color: "#a2a5ab",
                  border: "1px solid rgba(255,255,255,0.13)",
                  borderRadius: 999,
                  padding: "9px 17px",
                }}
              >
                {chip}
              </div>
            ))}
          </div>
          <div style={{ display: "flex", fontSize: 20, color: "#5b9dff" }}>
            github.com/imrj05/orbit
          </div>
        </div>
      </div>
    ),
    OG_SIZE,
  );
}
