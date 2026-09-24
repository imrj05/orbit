import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { ImageResponse } from "next/og";

export const OG_SIZE = { width: 1200, height: 630 };

/* Site palette, mirrored from globals.css (dark). */
const PAGE = "#09090b";
const INK = "rgba(255,255,255,0.94)";
const INK_2 = "rgba(255,255,255,0.62)";
const INK_3 = "rgba(255,255,255,0.42)";
const BRAND = "#5b9dff";

let logoPromise: Promise<string> | null = null;

/** The Orbit wordmark, inlined so the card renders without a network hop. */
function logoDataUrl(): Promise<string> {
  logoPromise ??= readFile(join(process.cwd(), "public/logo.png")).then(
    (buffer) => `data:image/png;base64,${buffer.toString("base64")}`,
  );
  return logoPromise;
}

let shotPromise: Promise<string> | null = null;

/** The hero session screenshot (public/screens/session.png), inlined. */
function shotDataUrl(): Promise<string> {
  shotPromise ??= readFile(
    join(process.cwd(), "public/screens/session.png"),
  ).then((buffer) => `data:image/png;base64,${buffer.toString("base64")}`);
  return shotPromise;
}

type OgFont = {
  name: string;
  data: ArrayBuffer;
  weight: 400 | 500 | 600 | 700;
  style: "normal";
};

/** Same fonts as the site. Requests the Google Fonts CSS with a legacy UA so
 * the API returns plain TTFs (satori can't parse woff2, and the subset woff
 * files trip its font parser). */
async function fetchGoogleFont(
  family: string,
  weight: OgFont["weight"],
): Promise<OgFont | null> {
  try {
    const css = await fetch(
      `https://fonts.googleapis.com/css2?family=${encodeURIComponent(family)}:wght@${weight}&display=swap`,
      { headers: { "User-Agent": "Mozilla/4.0" } },
    ).then((r) => (r.ok ? r.text() : null));
    const url = css?.match(/url\((https:[^)]+)\) format\('truetype'\)/)?.[1];
    if (!url) return null;
    const data = await fetch(url).then((r) =>
      r.ok ? r.arrayBuffer() : null,
    );
    return data ? { name: family, data, weight, style: "normal" } : null;
  } catch {
    return null;
  }
}

async function loadFonts(): Promise<OgFont[]> {
  const fonts = await Promise.all([
    fetchGoogleFont("Sora", 400),
    fetchGoogleFont("Sora", 600),
    fetchGoogleFont("Geist Mono", 400),
  ]);
  // next/og bundles Geist Regular as the default sans, so a failed fetch
  // just means slightly different letterforms — never a broken image.
  return fonts.filter((f): f is OgFont => f != null);
}

const CHIPS = ["Chat", "Tools", "Review", "Git", "Providers"];

/**
 * The shared OG card for inner pages (changelog, releases): wordmark,
 * eyebrow, headline, product chips. Sourced from the site palette.
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
  const [logo, fonts] = await Promise.all([logoDataUrl(), loadFonts()]);

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          background: PAGE,
          padding: "68px 72px",
          color: INK,
          fontFamily: "Sora",
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
              color: INK_2,
              fontFamily: "Geist Mono",
            }}
          >
            <div
              style={{
                width: 9,
                height: 9,
                borderRadius: 9,
                background: BRAND,
              }}
            />
            {eyebrow.toUpperCase()}
          </div>
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
          <div
            style={{
              fontSize: 64,
              fontWeight: 600,
              lineHeight: 1.1,
              letterSpacing: -2,
              maxWidth: 950,
              color: INK,
            }}
          >
            {title}
          </div>
          {meta ? (
            <div style={{ fontSize: 27, color: INK_2 }}>{meta}</div>
          ) : null}
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            borderTop: "1px solid rgba(255,255,255,0.08)",
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
                  color: INK_2,
                  border: "1px solid rgba(255,255,255,0.12)",
                  borderRadius: 999,
                  padding: "9px 17px",
                }}
              >
                {chip}
              </div>
            ))}
          </div>
          <div
            style={{
              display: "flex",
              fontSize: 20,
              color: BRAND,
              fontFamily: "Geist Mono",
            }}
          >
            github.com/imrj05/orbit
          </div>
        </div>
      </div>
    ),
    { ...OG_SIZE, fonts },
  );
}

/**
 * The home OG card: headline block on the left, the application peeking in
 * from the right and bleeding off both edges — the product is the proof.
 */
export async function renderOgHome() {
  const [logo, shot, fonts] = await Promise.all([
    logoDataUrl(),
    shotDataUrl(),
    loadFonts(),
  ]);

  return new ImageResponse(
    (
      <div
        style={{
          width: "100%",
          height: "100%",
          display: "flex",
          background: PAGE,
          color: INK,
          fontFamily: "Sora",
          position: "relative",
          overflow: "hidden",
        }}
      >
        {/* Ambient light behind the application. */}
        <div
          style={{
            position: "absolute",
            right: -160,
            top: 60,
            width: 860,
            height: 620,
            borderRadius: 999,
            background:
              "radial-gradient(closest-side, rgba(91,157,255,0.10), transparent)",
          }}
        />

        {/* The application, cropped off the right and bottom edges. */}
        <div
          style={{
            position: "absolute",
            left: 600,
            top: 68,
            display: "flex",
            width: 1060,
            borderRadius: 20,
            border: "1px solid rgba(255,255,255,0.10)",
            background: "rgba(255,255,255,0.025)",
            padding: 8,
            boxShadow: "0 20px 60px rgba(0,0,0,0.35)",
            overflow: "hidden",
          }}
        >
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={shot}
            width={1044}
            height={682}
            alt=""
            style={{ borderRadius: 12 }}
          />
        </div>

        {/* Copy column. */}
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            justifyContent: "space-between",
            width: 540,
            padding: "64px 0 64px 72px",
          }}
        >
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src={logo} width={150} height={50} alt="Orbit Pi" />

          <div style={{ display: "flex", flexDirection: "column", gap: 26 }}>
            <div
              style={{
                fontSize: 52,
                fontWeight: 600,
                lineHeight: 1.12,
                letterSpacing: -1.8,
                color: INK,
              }}
            >
              A native workbench for the pi coding agent.
            </div>
            <div style={{ fontSize: 24, lineHeight: 1.55, color: INK_2 }}>
              Chat, review, and commit in one window — the same sessions as the
              terminal.
            </div>
          </div>

          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 12,
              fontSize: 19,
              letterSpacing: 1,
              color: INK_3,
              fontFamily: "Geist Mono",
            }}
          >
            <div
              style={{
                width: 8,
                height: 8,
                borderRadius: 8,
                background: BRAND,
              }}
            />
            macOS · Windows · Linux
          </div>
        </div>
      </div>
    ),
    { ...OG_SIZE, fonts },
  );
}
