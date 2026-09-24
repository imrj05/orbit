/**
 * Renders a JSON-LD block. Valid anywhere in the document — crawlers read it
 * from the body — so pages can drop one in without touching `<head>`.
 */
export function JsonLd({ data }: { data: unknown }) {
  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{ __html: JSON.stringify(data) }}
    />
  );
}
