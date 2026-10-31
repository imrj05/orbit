import { renderOgHome } from "@/lib/og";

export const runtime = "nodejs";
export const alt = "Orbit — a native workbench for the pi coding agent";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

/**
 * The home OG card for the landing pages under `[locale]`. The landing's
 * `generateMetadata` sets its own `openGraph`, which replaces the root
 * segment's metadata wholesale — including the og:image the root
 * `opengraph-image.tsx` would otherwise contribute — so this segment carries
 * its own card, the way the changelog does. The art is shared
 * (`renderOgHome`): the copy stays English because the card's satori fonts
 * are Latin-only, so translated headlines would render broken glyphs.
 */
export default function Image() {
  return renderOgHome();
}