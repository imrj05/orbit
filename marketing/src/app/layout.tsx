import type { Metadata, Viewport } from "next";
import { Geist_Mono, Sora } from "next/font/google";
import Script from "next/script";
import { SITE } from "@/lib/site";
import "./globals.css";

const sora = Sora({
  subsets: ["latin"],
  variable: "--font-sora",
  display: "swap",
});

const geistMono = Geist_Mono({
  subsets: ["latin"],
  variable: "--font-geist-mono",
  display: "swap",
});

export const metadata: Metadata = {
  metadataBase: new URL(SITE.url),
  title: {
    default: SITE.title,
    template: "%s · Orbit",
  },
  description: SITE.description,
  applicationName: SITE.name,
  generator: "Next.js",
  keywords: [...SITE.keywords],
  authors: [{ name: SITE.name, url: SITE.github }],
  creator: SITE.name,
  publisher: SITE.name,
  category: "Developer Tools",
  referrer: "origin-when-cross-origin",
  alternates: { canonical: "/" },
  openGraph: {
    type: "website",
    siteName: SITE.name,
    url: "/",
    title: SITE.title,
    description: SITE.ogDescription,
    locale: "en_US",
  },
  twitter: {
    card: "summary_large_image",
    title: SITE.title,
    description: SITE.ogDescription,
  },
  robots: {
    index: true,
    follow: true,
    googleBot: {
      index: true,
      follow: true,
      "max-image-preview": "large",
      "max-snippet": -1,
      "max-video-preview": -1,
    },
  },
  formatDetection: { email: false, address: false, telephone: false },
  manifest: "/manifest.webmanifest",
};

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f6f6f7" },
    { media: "(prefers-color-scheme: dark)", color: "#141518" },
  ],
  colorScheme: "dark light",
  width: "device-width",
  initialScale: 1,
};

/** Runs before paint so the stored system/preference theme applies without a flash. */
const THEME_SCRIPT = `(function(){try{var s=localStorage.getItem('theme');var light=s?s==='light':window.matchMedia('(prefers-color-scheme: light)').matches;var r=document.documentElement;r.classList.toggle('light',light);r.classList.toggle('dark',!light);r.style.colorScheme=light?'light':'dark';}catch(e){}})();`;

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="en"
      data-scroll-behavior="smooth"
      suppressHydrationWarning
      className={`dark ${sora.variable} ${geistMono.variable} h-full`}
    >
      <body className="min-h-full flex flex-col bg-page text-ink">
        <Script
          id="theme-init"
          strategy="beforeInteractive"
          dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }}
        />
        {children}
      </body>
    </html>
  );
}
