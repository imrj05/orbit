import Script from "next/script";

import { Architecture } from "@/components/sections/architecture";
import { Capabilities } from "@/components/sections/capabilities";
import { Closer } from "@/components/sections/closer";
import { Contributors } from "@/components/sections/contributors";
import { Faq } from "@/components/sections/faq";
import { Hero } from "@/components/sections/hero";
import { Highlights } from "@/components/sections/highlights";
import { Reviews } from "@/components/sections/reviews";
import { Tour } from "@/components/sections/tour";
import { Trust } from "@/components/sections/trust";
import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";
import type { Locale } from "@/i18n/config";

/**
 * The landing page, shared by `/` (English) and `/{locale}`. The language of the
 * subtree is declared by `[locale]/layout`, so nothing here needs a `lang`.
 */
export async function Landing({ locale }: { locale: Locale }) {
  // Analytics is loaded only in production builds, never in `next dev`, so
  // local browsing does not pollute the dashboard.
  const analyticsEnabled = process.env.NODE_ENV === "production";

  return (
    <>
      <SiteNav locale={locale} path="/" />
      <main className="flex-1">
        <Hero locale={locale} />
        <Trust locale={locale} />
        <Highlights locale={locale} />
        <Tour locale={locale} />
        <Capabilities locale={locale} />
        <Architecture locale={locale} />
        <Contributors locale={locale} />
        <Reviews locale={locale} />
        <Faq locale={locale} />
        <Closer locale={locale} />
      </main>
      <SiteFooter locale={locale} />
      {analyticsEnabled ? (
        <Script
          src="https://tracking.rajeshwar.tech/api/script.js"
          data-site-id="f08dd17d42ca"
          strategy="afterInteractive"
        />
      ) : null}
    </>
  );
}
