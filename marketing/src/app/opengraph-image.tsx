import { renderOgHome } from "@/lib/og";

export const runtime = "nodejs";
export const alt = "Orbit — a native workbench for the pi coding agent";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function Image() {
  return renderOgHome();
}
