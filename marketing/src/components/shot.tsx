import Image from "next/image";

export function Shot({
  src,
  srcLight,
  alt,
  width = 3164,
  height = 2068,
  priority = false,
  variant = "card",
  sizes = "(max-width: 768px) 100vw, (max-width: 1280px) 92vw, 1180px",
  className = "",
}: {
  src: string;
  srcLight?: string;
  alt: string;
  width?: number;
  height?: number;
  priority?: boolean;
  /** "product" is reserved for the single hero showcase — the strongest
   * elevation on the page. Everywhere else stays at card elevation. */
  variant?: "card" | "product";
  sizes?: string;
  className?: string;
}) {
  const frame =
    variant === "product"
      ? "rounded-[16px] border border-edge-default bg-surface-1 p-1.5 shadow-product sm:p-2"
      : "rounded-[14px] border border-edge-default bg-surface-1 p-1 shadow-card";

  return (
    <div className={`${frame} ${className}`}>
      <Image
        src={src}
        alt={alt}
        width={width}
        height={height}
        priority={priority}
        sizes={sizes}
        className={`h-auto w-full rounded-[10px]${srcLight ? " light:hidden" : ""}`}
      />
      {srcLight ? (
        <Image
          src={srcLight}
          alt={alt}
          width={width}
          height={height}
          priority={priority}
          sizes={sizes}
          className="hidden h-auto w-full rounded-[10px] light:block"
        />
      ) : null}
    </div>
  );
}
