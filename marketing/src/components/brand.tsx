import Image from "next/image";

/**
 * The Orbit Pi wordmark — white ink for dark surfaces. Mirrors
 * `assets/icons/logo.png` in the desktop app.
 */
export function OrbitWordmark({
  className = "",
  alt = "Orbit Pi",
  priority = false,
}: {
  className?: string;
  alt?: string;
  priority?: boolean;
}) {
  return (
    <Image
      src="/logo.png"
      alt={alt}
      width={2172}
      height={724}
      priority={priority}
      className={`${className} light:invert`}
    />
  );
}

/**
 * The Orbit Pi app mark — a rounded tile with the chrome `OP`. Mirrors
 * `assets/icons/logo-icon.png` in the desktop app.
 */
export function OrbitIcon({
  className = "",
  alt = "Orbit Pi",
  priority = false,
}: {
  className?: string;
  alt?: string;
  priority?: boolean;
}) {
  return (
    <Image
      src="/logo-icon.png"
      alt={alt}
      width={1024}
      height={1024}
      priority={priority}
      className={className}
    />
  );
}
