"use client";

import Image from "next/image";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowLeft01Icon,
  ArrowRight01Icon,
  Cancel01Icon,
  FullScreenIcon,
  MinimizeScreenIcon,
  ZoomInAreaIcon,
  ZoomOutAreaIcon,
} from "@hugeicons/core-free-icons";

export type LightboxItem = {
  id: string;
  src: string;
  srcLight?: string;
  alt: string;
  width: number;
  height: number;
};

type LightboxContextValue = {
  register: (item: LightboxItem, el: HTMLElement) => () => void;
  open: (id: string) => void;
};

const LightboxContext = createContext<LightboxContextValue | null>(null);

export function useLightbox() {
  const context = useContext(LightboxContext);
  if (!context) {
    throw new Error("useLightbox must be used within a LightboxProvider");
  }
  return context;
}

const MIN_ZOOM = 1;
const MAX_ZOOM = 4;
const ZOOM_STEP = 1.5;

/**
 * A single gallery viewer shared by every `<Shot>` on the page. Shots register
 * themselves on mount; opening one sorts the registry into document order so
 * next/previous walks the images the way the reader sees them.
 */
export function LightboxProvider({ children }: { children: ReactNode }) {
  const registry = useRef(
    new Map<string, { item: LightboxItem; el: HTMLElement }>(),
  );
  const dialogRef = useRef<HTMLDialogElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const [items, setItems] = useState<LightboxItem[]>([]);
  const [index, setIndex] = useState<number | null>(null);
  const [zoom, setZoom] = useState(MIN_ZOOM);
  const [fullscreen, setFullscreen] = useState(false);

  const register = useCallback<LightboxContextValue["register"]>((item, el) => {
    registry.current.set(item.id, { item, el });
    return () => registry.current.delete(item.id);
  }, []);

  const open = useCallback((id: string) => {
    const entries = [...registry.current.values()];
    entries.sort((a, b) => {
      const position = a.el.compareDocumentPosition(b.el);
      if (position & Node.DOCUMENT_POSITION_FOLLOWING) return -1;
      if (position & Node.DOCUMENT_POSITION_PRECEDING) return 1;
      return 0;
    });
    const list = entries.map((entry) => entry.item);
    setItems(list);
    const start = list.findIndex((item) => item.id === id);
    setIndex(start === -1 ? 0 : start);
    setZoom(MIN_ZOOM);
  }, []);

  const close = useCallback(() => {
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
    setIndex(null);
  }, []);

  const move = useCallback(
    (delta: number) => {
      if (items.length < 2) return;
      setIndex((current) => {
        if (current === null) return current;
        return (current + delta + items.length) % items.length;
      });
      setZoom(MIN_ZOOM);
    },
    [items.length],
  );

  const toggleFullscreen = useCallback(() => {
    const stage = stageRef.current;
    if (!stage || typeof stage.requestFullscreen !== "function") return;
    if (document.fullscreenElement) {
      void document.exitFullscreen().catch(() => {});
    } else {
      void stage.requestFullscreen().catch(() => {});
    }
  }, []);

  // Drive the native modal from `index`.
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (index !== null && !dialog.open) dialog.showModal();
    if (index === null && dialog.open) dialog.close();
  }, [index]);

  // Reflect browser fullscreen changes (covers Esc and OS toggles).
  useEffect(() => {
    const onChange = () => setFullscreen(Boolean(document.fullscreenElement));
    document.addEventListener("fullscreenchange", onChange);
    document.addEventListener("webkitfullscreenchange", onChange);
    return () => {
      document.removeEventListener("fullscreenchange", onChange);
      document.removeEventListener("webkitfullscreenchange", onChange);
    };
  }, []);

  // Keep the focal point centered when the zoom level changes.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const frame = requestAnimationFrame(() => {
      el.scrollLeft = (el.scrollWidth - el.clientWidth) / 2;
      el.scrollTop = (el.scrollHeight - el.clientHeight) / 2;
    });
    return () => cancelAnimationFrame(frame);
  }, [zoom, index]);

  const item = index === null ? null : items[index];
  const multiple = items.length > 1;

  return (
    <LightboxContext.Provider value={{ register, open }}>
      {children}

      <dialog
        ref={dialogRef}
        onClose={() => setIndex(null)}
        onKeyDown={(event) => {
          if (event.key === "ArrowLeft") {
            event.preventDefault();
            move(-1);
          } else if (event.key === "ArrowRight") {
            event.preventDefault();
            move(1);
          }
        }}
        aria-label="Image viewer"
        className="inset-0 m-0 h-full max-h-none w-full max-w-none border-0 bg-transparent p-0 backdrop:bg-black/90 backdrop:backdrop-blur-sm"
      >
        {item ? (
          <div ref={stageRef} className="relative h-full w-full">
            <div className="pointer-events-none absolute inset-x-0 top-0 z-10 flex items-start justify-between gap-2 p-3 sm:p-4">
              <span
                aria-live="polite"
                className="pointer-events-auto rounded-full bg-black/55 px-2.5 py-1 text-xs font-medium tabular-nums text-white/80 backdrop-blur-sm"
              >
                {(index ?? 0) + 1} / {items.length}
              </span>

              <div className="pointer-events-auto flex items-center gap-1.5">
                <Control
                  label="Zoom out"
                  onClick={() => setZoom((z) => Math.max(z / ZOOM_STEP, MIN_ZOOM))}
                  disabled={zoom <= MIN_ZOOM}
                >
                  <HugeiconsIcon icon={ZoomOutAreaIcon} className="size-4" />
                </Control>
                <Control
                  label="Zoom in"
                  onClick={() => setZoom((z) => Math.min(z * ZOOM_STEP, MAX_ZOOM))}
                  disabled={zoom >= MAX_ZOOM}
                >
                  <HugeiconsIcon icon={ZoomInAreaIcon} className="size-4" />
                </Control>
                <Control
                  label={fullscreen ? "Exit full screen" : "Full screen"}
                  onClick={toggleFullscreen}
                >
                  <HugeiconsIcon
                    icon={fullscreen ? MinimizeScreenIcon : FullScreenIcon}
                    className="size-4"
                  />
                </Control>
                <Control label="Close" onClick={close}>
                  <HugeiconsIcon icon={Cancel01Icon} className="size-4" />
                </Control>
              </div>
            </div>

            {multiple ? (
              <>
                <Control
                  label="Previous image"
                  onClick={() => move(-1)}
                  className="absolute left-2 top-1/2 z-10 -translate-y-1/2 sm:left-4"
                >
                  <HugeiconsIcon icon={ArrowLeft01Icon} className="size-5" />
                </Control>
                <Control
                  label="Next image"
                  onClick={() => move(1)}
                  className="absolute right-2 top-1/2 z-10 -translate-y-1/2 sm:right-4"
                >
                  <HugeiconsIcon icon={ArrowRight01Icon} className="size-5" />
                </Control>
              </>
            ) : null}

            <div
              ref={scrollRef}
              className="h-full w-full overflow-auto overscroll-contain p-4 sm:p-8"
              onClick={(event) => {
                if (event.target === event.currentTarget) close();
              }}
            >
              <div
                className="flex items-center justify-center"
                style={{ width: `${zoom * 100}%`, height: `${zoom * 100}%` }}
              >
                <Image
                  src={item.src}
                  alt={item.alt}
                  width={item.width}
                  height={item.height}
                  sizes="200vw"
                  draggable={false}
                  className={`h-full w-full object-contain rounded-lg shadow-product${item.srcLight ? " light:hidden" : ""}`}
                />
                {item.srcLight ? (
                  <Image
                    src={item.srcLight}
                    alt={item.alt}
                    width={item.width}
                    height={item.height}
                    sizes="200vw"
                    draggable={false}
                    className="hidden h-full w-full object-contain rounded-lg shadow-product light:block"
                  />
                ) : null}
              </div>
            </div>
          </div>
        ) : null}
      </dialog>
    </LightboxContext.Provider>
  );
}

function Control({
  label,
  onClick,
  disabled,
  className = "",
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className={`inline-flex size-9 items-center justify-center rounded-full bg-black/55 text-white/90 backdrop-blur-sm transition-colors hover:bg-black/75 hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/70 disabled:pointer-events-none disabled:opacity-35 ${className}`}
    >
      {children}
    </button>
  );
}
