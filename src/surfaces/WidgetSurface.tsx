/**
 * A single widget, rendered into its own transparent window.
 *
 * # Dragging
 *
 * The window is frameless, so dragging is ours to implement. Rather than
 * Tauri's `startDragging` — which hands the whole gesture to the OS and
 * leaves us unable to persist the result — the pointer is captured here and
 * the position written back through `move_widget`, which clamps it to a
 * connected display and saves it to the profile. That is why a widget is
 * still where you put it after a restart.
 *
 * # Click-through
 *
 * A click-through widget still needs to be grabbable, or it could never be
 * moved again. Hovering lifts click-through for as long as the pointer is
 * over it and restores it on leave.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import * as api from "@/services/api";
import { applyTheme } from "@/stores/app";
import { SURFACES_CHANGED } from "@/types/api";
import type { Metrics, WidgetInstance } from "@/types/api";
import { WidgetBody } from "@/widgets/WidgetBody";
import { cx } from "@/components/ui";

export function WidgetSurface({ widgetId }: { widgetId: string }) {
  const [config, setConfig] = useState<WidgetInstance | null>(null);
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [hovered, setHovered] = useState(false);
  const [dragging, setDragging] = useState(false);
  const label = `widget-${widgetId}`;

  // --- configuration -------------------------------------------------
  useEffect(() => {
    let cancelled = false;
    const read = () =>
      api
        .widgetConfig(widgetId)
        .then((c) => {
          if (!cancelled) setConfig(c);
        })
        .catch(() => undefined);
    read();

    void api
      .getSettings()
      .then((s) => applyTheme(s))
      .catch(() => undefined);

    // The studio can change this widget's appearance while it is open. It
    // tells us when that happens rather than us asking on a timer — polling
    // from every widget window put a steady load on the thread that has to
    // stay free to handle clicks.
    const unlisten = listen(SURFACES_CHANGED, () => {
      void read();
      void api.getSettings().then(applyTheme).catch(() => undefined);
    });

    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, [widgetId]);

  // --- metrics -------------------------------------------------------
  //
  // Only widgets that display a live reading poll, and only at their own
  // configured interval. A clock does not ask the backend for CPU load.
  const needsMetrics = METRIC_WIDGETS.has(widgetId);
  useEffect(() => {
    if (!needsMetrics || !config || config.interval_ms === 0) return;
    let cancelled = false;
    const read = () =>
      api
        .getMetrics()
        .then((m) => {
          if (!cancelled) setMetrics(m);
        })
        .catch(() => undefined);
    read();
    const timer = window.setInterval(read, config.interval_ms);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [needsMetrics, config?.interval_ms, config]);

  // --- click-through on hover ----------------------------------------
  useEffect(() => {
    if (!config?.click_through) return;
    void api.setSurfaceClickThrough(label, !hovered).catch(() => undefined);
  }, [hovered, config?.click_through, label]);

  // --- dragging -------------------------------------------------------
  const origin = useRef<{ x: number; y: number; wx: number; wy: number } | null>(null);

  const onPointerDown = useCallback(
    async (e: React.PointerEvent) => {
      if (!config || config.locked || e.button !== 0) return;
      const win = getCurrentWindow();
      const pos = await win.outerPosition();
      const scale = await win.scaleFactor();
      origin.current = {
        x: e.screenX,
        y: e.screenY,
        wx: pos.x / scale,
        wy: pos.y / scale,
      };
      setDragging(true);
      (e.target as Element).setPointerCapture?.(e.pointerId);
    },
    [config],
  );

  const [preview, setPreview] = useState<{ x: number; y: number } | null>(null);

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      const start = origin.current;
      if (!start || !dragging) return;
      // Track the pointer in logical pixels. The window is moved by the
      // effect below; the authoritative clamp and the save happen once, on
      // release, so a drag is one database write rather than hundreds.
      setPreview({
        x: start.wx + (e.screenX - start.x),
        y: start.wy + (e.screenY - start.y),
      });
    },
    [dragging],
  );

  // Mirrors the preview position onto the real window.
  useEffect(() => {
    if (!preview) return;
    void getCurrentWindow()
      .setPosition(new LogicalPosition(preview.x, preview.y))
      .catch(() => undefined);
  }, [preview]);

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (!dragging) return;
      setDragging(false);
      (e.target as Element).releasePointerCapture?.(e.pointerId);
      origin.current = null;
      if (preview) {
        void api.moveWidget(widgetId, preview.x, preview.y).catch(() => undefined);
        setPreview(null);
      }
    },
    [dragging, preview, widgetId],
  );

  if (!config) {
    return <div className="h-full w-full" />;
  }

  const panel = PANEL_STYLES[config.theme];

  return (
    <div
      className="h-full w-full p-[2px]"
      onPointerEnter={() => setHovered(true)}
      onPointerLeave={() => setHovered(false)}
    >
      <div
        onPointerDown={(e) => void onPointerDown(e)}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        className={cx(
          "relative flex h-full w-full flex-col overflow-hidden",
          "transition-[box-shadow,transform] duration-[var(--dur)]",
          panel,
          // Without a filled panel behind it the content sits on the
          // wallpaper, so it needs its own contrast.
          (config.theme === "bare" || config.theme === "outline") &&
            "widget-on-wallpaper",
          !config.locked && "cursor-grab",
          dragging && "cursor-grabbing scale-[1.01]",
          hovered && !config.locked && "shadow-[var(--shadow-lg)]",
        )}
        style={{
          opacity: config.opacity,
          fontFamily:
            config.font === "mono"
              ? "var(--font-mono)"
              : config.font === "display"
                ? "var(--font-display)"
                : "var(--font-sans)",
          fontSize: `${config.font_scale}rem`,
          borderRadius: "var(--radius-lg)",
        }}
      >
        <WidgetBody id={widgetId} config={config} metrics={metrics} />

        {hovered && !config.locked && (
          <span className="pointer-events-none absolute right-2 top-2 rounded-full bg-[var(--bg-overlay)] px-2 py-[2px] text-[9px] uppercase tracking-wider text-[var(--text-tertiary)]">
            drag to move
          </span>
        )}
      </div>
    </div>
  );
}

/** Widgets whose content comes from the metrics sampler. */
const METRIC_WIDGETS = new Set([
  "cpu",
  "ram",
  "gpu",
  "gpu_temp",
  "battery",
  "network",
  "network_status",
  "uptime",
  "storage",
  "media",
]);

const PANEL_STYLES: Record<string, string> = {
  glass:
    "glass border border-[var(--border-default)] shadow-[var(--shadow-md)]",
  solid: "bg-[var(--bg-raised)] border border-[var(--border-subtle)] shadow-[var(--shadow-md)]",
  outline: "bg-transparent border border-[var(--border-strong)]",
  bare: "bg-transparent",
};
