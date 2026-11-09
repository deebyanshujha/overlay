/**
 * Floating dock.
 *
 * Magnification is computed from the pointer's distance to each icon's
 * centre, which is what makes the effect feel continuous rather than a
 * stepped hover state. Launching goes through `launch_pinned`, which only
 * accepts a path already present in the validated profile config — the dock
 * cannot be talked into starting something else.
 */

import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "@/services/api";
import { applyTheme } from "@/stores/app";
import { errorMessage, SURFACES_CHANGED } from "@/types/api";
import { cx } from "@/components/ui";

interface Pinned {
  id: string;
  name: string;
  path: string;
}

interface DockConfig {
  edge: "bottom" | "top" | "left" | "right";
  icon_size: number;
  magnification: number;
  show_running_indicators: boolean;
  opacity: number;
  pinned: Pinned[];
}

const DEFAULTS: DockConfig = {
  edge: "bottom",
  icon_size: 48,
  magnification: 1.6,
  show_running_indicators: true,
  opacity: 0.92,
  pinned: [],
};

export function DockSurface() {
  const [config, setConfig] = useState<DockConfig>(DEFAULTS);
  const [pointer, setPointer] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const strip = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void api.getSettings().then(applyTheme).catch(() => undefined);
    const read = () =>
      api
        .listModules()
        .then((r) => {
          const dock = r.modules.find((m) => m.id === "dock");
          if (dock) setConfig({ ...DEFAULTS, ...(dock.config as unknown as DockConfig) });
        })
        .catch(() => undefined);
    void read();
    // Told, not polled — see HudSurface.
    const unlisten = listen(SURFACES_CHANGED, () => void read());
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  const vertical = config.edge === "left" || config.edge === "right";

  return (
    <div className="flex h-full w-full items-center justify-center p-1.5">
      <div
        ref={strip}
        onPointerMove={(e) => {
          const box = strip.current?.getBoundingClientRect();
          if (!box) return;
          setPointer(vertical ? e.clientY - box.top : e.clientX - box.left);
        }}
        onPointerLeave={() => setPointer(null)}
        className={cx(
          "glass flex items-end gap-1.5 border border-[var(--border-default)]",
          "px-3 shadow-[var(--shadow-lg)]",
          vertical ? "h-full flex-col justify-center py-3" : "w-full justify-center py-2",
        )}
        style={{ borderRadius: 18, opacity: config.opacity }}
      >
        {config.pinned.length === 0 ? (
          <p className="px-3 py-2 text-[11px] text-[var(--text-tertiary)]">
            Pin applications in OVERLAY → Customize → Floating Dock
          </p>
        ) : (
          config.pinned.map((app, index) => (
            <DockIcon
              key={app.id}
              app={app}
              index={index}
              count={config.pinned.length}
              size={config.icon_size}
              magnification={config.magnification}
              pointer={pointer}
              vertical={vertical}
              onLaunch={async () => {
                try {
                  await api.launchPinned(app.path);
                  setError(null);
                } catch (e) {
                  setError(errorMessage(e));
                  window.setTimeout(() => setError(null), 4000);
                }
              }}
            />
          ))
        )}
      </div>

      {error && (
        <div className="absolute bottom-full mb-2 rounded-[7px] bg-[var(--danger-dim)] px-2.5 py-1 text-[11px] text-[var(--danger)]">
          {error}
        </div>
      )}
    </div>
  );
}

function DockIcon({
  app,
  index,
  count,
  size,
  magnification,
  pointer,
  vertical,
  onLaunch,
}: {
  app: Pinned;
  index: number;
  count: number;
  size: number;
  magnification: number;
  pointer: number | null;
  vertical: boolean;
  onLaunch: () => void;
}) {
  const cell = size + 6;
  const centre = index * cell + cell / 2 + 12;

  // Gaussian falloff: icons near the pointer grow, and the growth tapers
  // smoothly instead of ending at a hard edge.
  let scale = 1;
  if (pointer !== null && magnification > 1) {
    const distance = Math.abs(pointer - centre);
    const influence = cell * 1.6;
    scale = 1 + (magnification - 1) * Math.exp(-((distance / influence) ** 2));
  }

  const [hovered, setHovered] = useState(false);
  void count;

  return (
    <button
      onClick={onLaunch}
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
      title={app.name}
      className="relative flex shrink-0 flex-col items-center justify-end"
      style={{
        width: size,
        height: size,
        transform: `scale(${scale.toFixed(3)}) translateY(${vertical ? 0 : -(scale - 1) * size * 0.3}px)`,
        transformOrigin: vertical ? "center left" : "center bottom",
        transition: pointer === null ? "transform var(--dur) var(--ease)" : "none",
      }}
    >
      <span
        className={cx(
          "flex h-full w-full items-center justify-center rounded-[22%]",
          "border border-[var(--border-default)] bg-[var(--bg-raised)]",
          "text-[var(--text-secondary)] transition-colors",
          hovered && "border-[var(--accent-500)] text-[var(--text-primary)]",
        )}
      >
        <span style={{ fontSize: size * 0.4, fontWeight: 500 }}>
          {app.name.charAt(0).toUpperCase()}
        </span>
      </span>

      {hovered && (
        <span
          className={cx(
            "pointer-events-none absolute whitespace-nowrap rounded-[6px]",
            "border border-[var(--border-default)] bg-[var(--bg-overlay)] px-2 py-1",
            "text-[11px] text-[var(--text-primary)] shadow-[var(--shadow-md)]",
            vertical ? "left-full ml-2" : "bottom-full mb-2",
          )}
          style={{ transform: `scale(${(1 / scale).toFixed(3)})` }}
        >
          {app.name}
        </span>
      )}
    </button>
  );
}
