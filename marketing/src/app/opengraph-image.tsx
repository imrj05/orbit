import { renderOgCard } from "@/lib/og";

export const runtime = "nodejs";
export const alt = "Orbit — a native workbench for the pi coding agent";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function Image() {
  return renderOgCard({
    eyebrow: "Native workbench",
    title: "A native workbench for the pi coding agent.",
    meta: "Chat, tools, review, and Git in one window — same sessions as the terminal.",
  });
}
