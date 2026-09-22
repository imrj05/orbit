import type { MetadataRoute } from "next";
import { getChangelog } from "@/lib/changelog";
import { SITE } from "@/lib/site";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const lastModified = new Date();
  const releases = await getChangelog();

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
