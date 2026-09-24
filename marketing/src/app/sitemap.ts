import type { MetadataRoute } from "next";
import { getChangelog } from "@/lib/changelog";
import { SITE } from "@/lib/site";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const releases = await getChangelog();
  // The newest release is the last time the marketing pages actually changed;
  // using `new Date()` here would claim every URL changed on every regeneration.
  const latestDate = releases.find((entry) => entry.date)?.date;
  const lastModified = latestDate
    ? new Date(`${latestDate}T00:00:00Z`)
    : new Date();

  return [
    {
      url: `${SITE.url}/`,
      lastModified,
      changeFrequency: "weekly",
      priority: 1,
    },
    {
      url: `${SITE.url}/changelog`,
      lastModified,
      changeFrequency: "daily",
      priority: 0.8,
    },
    ...releases.map((entry) => ({
      url: `${SITE.url}/changelog/${entry.tag}`,
      lastModified: entry.date
        ? new Date(`${entry.date}T00:00:00Z`)
        : lastModified,
      changeFrequency: "monthly" as const,
      priority: 0.6,
    })),
  ];
}
