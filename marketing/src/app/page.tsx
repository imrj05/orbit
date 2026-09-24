import Script from "next/script";

import { Agent } from "@/components/sections/agent";
import { Closer } from "@/components/sections/closer";
import { Git } from "@/components/sections/git";
import { Hero } from "@/components/sections/hero";
import { Models } from "@/components/sections/models";
import { Native } from "@/components/sections/native";
import { Sessions } from "@/components/sections/sessions";
import { Usage } from "@/components/sections/usage";
import { SiteFooter } from "@/components/site-footer";
import { SiteNav } from "@/components/site-nav";

export default function Home() {
  return (
    <>
      <SiteNav />
      <main className="flex-1">
        <Hero />
        <Agent />
        <Sessions />
        <Models />
        <Git />
        <Usage />
        <Native />
        <Closer />
      </main>
      <SiteFooter />
      <Script
        src="https://tracking.rajeshwar.tech/api/script.js"
        data-site-id="8c362549e036"
        strategy="afterInteractive"
      />
    </>
  );
}
