import { Geist_Mono, IBM_Plex_Sans, Sora } from "next/font/google";

/**
 * Shared font instances. Loaded once here and consumed by both the root layout
 * and the standalone `global-error` document (which replaces the layout, so it
 * cannot inherit the root's font variables).
 */
export const sora = Sora({
  subsets: ["latin"],
  variable: "--font-sora",
  display: "swap",
});

export const geistMono = Geist_Mono({
  subsets: ["latin"],
  variable: "--font-geist-mono",
  display: "swap",
});

/**
 * IBM Plex Sans is the native app's interface face. It is used only inside the
 * product demos, so a faux Orbit window reads in the same type as the real one.
 */
export const plexSans = IBM_Plex_Sans({
  subsets: ["latin"],
  weight: ["400", "500", "600"],
  variable: "--font-plex",
  display: "swap",
});

/** The `className` that exposes every font variable. */
export const FONT_CLASSES = `${sora.variable} ${geistMono.variable} ${plexSans.variable}`;
