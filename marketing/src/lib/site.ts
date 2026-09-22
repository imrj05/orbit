/**
 * Single source of truth for site-wide metadata. `SITE_URL` resolves from an
 * explicit env var first (set `NEXT_PUBLIC_SITE_URL` when self-hosting), then
 * Vercel's production URL, then localhost for development.
 */
function resolveSiteUrl(): string {
  const explicit = process.env.NEXT_PUBLIC_SITE_URL?.trim();
  if (explicit) {
    return (explicit.startsWith("http") ? explicit : `https://${explicit}`).replace(/\/$/, "");
  }

  const vercel =
    process.env.VERCEL_PROJECT_PRODUCTION_URL ?? process.env.VERCEL_URL;
  if (vercel) return `https://${vercel.replace(/\/$/, "")}`;

  return "http://localhost:3000";
}

export const SITE_URL = resolveSiteUrl();

export const SITE = {
  name: "Orbit",
  title: "Orbit — a native workbench for the pi coding agent",
  description:
    "A native desktop client for the pi coding agent. Chat, tools, review, Git, providers, and usage in one window — same sessions as the terminal.",
  ogDescription:
    "A native workbench for the pi coding agent — chat, review, and commit in one window.",
  url: SITE_URL,
  github: "https://github.com/imrj05/orbit",
  keywords: [
    "Orbit",
    "Orbit Pi",
    "pi coding agent",
    "pi agent",
    "coding agent",
    "AI coding agent",
    "desktop app",
    "GPUI",
    "Rust desktop app",
    "developer tools",
    "workbench",
    "AI pair programmer",
    "code review",
    "git client",
    "model providers",
    "local first",
  ],
} as const;
