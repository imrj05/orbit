import Image from "next/image";

export function Shot({
  src,
  srcLight,
  alt,
  width = 3164,
  height = 2068,
  priority = false,
  sizes = "(max-width: 768px) 100vw, (max-width: 1280px) 92vw, 1180px",
  className = "",
}: {
  src: string;
  srcLight?: string;
  alt: string;
  width?: number;
  height?: number;
  priority?: boolean;
  sizes?: string;
  className?: string;
}) {
  return (
    <div
      className={`overflow-hidden rounded-2xl bg-window shadow-pop ${className}`}
    >
      <Image
        src={src}
        alt={alt}
        width={width}
        height={height}
        priority={priority}
        sizes={sizes}
        className={`h-auto w-full${srcLight ? " light:hidden" : ""}`}
      />
      {srcLight ? (
        <Image
          src={srcLight}
          alt={alt}
          width={width}
          height={height}
          priority={priority}
          sizes={sizes}
          className="hidden h-auto w-full light:block"
        />
      ) : null}
    </div>
  );
}
