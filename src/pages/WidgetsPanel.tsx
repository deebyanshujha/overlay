/**
 * The widget manager: the catalogue on one side, per-widget settings on the
 * other.
 *
 * Every widget has its own independent switch and its own appearance, which
 * is the point of the module. Position is edited here too, but dragging the
 * widget itself is the primary way — this is for precision and for widgets
 * that have been locked.
 */

import { useEffect, useState } from "react";
import { useApp } from "@/stores/app";
import * as api from "@/services/api";
import { errorMessage } from "@/types/api";
import type { ModuleStatus, WidgetView } from "@/types/api";
import {
  Badge,
  Button,
  Field,
  PlusIcon,
  Segmented,
  Select,
  Slider,
  Toggle,
  Tooltip,
  cx,
  useToast,
} from "@/components/ui";

export function WidgetsPanel({ module }: { module: ModuleStatus }) {
  const { widgetCatalog, capabilities, environment } = useApp();
  const toast = useToast();
  const [widgets, setWidgets] = useState<WidgetView[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = () =>
    api
      .listWidgets()
      .then(setWidgets)
      .catch(() => setWidgets([]));

  useEffect(() => {
    void reload();
  }, [module.config]);

  const run = async (fn: () => Promise<WidgetView[]>, message?: string) => {
    setBusy(true);
    try {
      setWidgets(await fn());
      if (message) toast.push(message, "ok");
    } catch (e) {
      toast.push("Could not update widgets", "danger", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const inProfile = new Set(widgets.map((w) => w.instance.id));
  const current = widgets.find((w) => w.instance.id === selected);

  const sensorNote = (id: string): string | null => {
    if (!capabilities) return null;
    if ((id === "gpu" || id === "gpu_temp") && !capabilities.gpu_usage)
      return capabilities.gpu_detail;
    if (id === "battery" && !capabilities.battery)
      return "No battery detected on this machine.";
    if (id === "media" && !capabilities.media) return capabilities.media_detail;
    return null;
  };

  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-[1fr_auto] gap-5">
        {/* Catalogue */}
        <div>
          <p className="mb-2 text-[12px] font-medium text-[var(--text-secondary)]">
            Widgets in this profile
          </p>
          <div className="flex flex-col gap-1">
            {widgets.length === 0 && (
              <p className="rounded-[8px] bg-[var(--bg-base)] px-3 py-3 text-[11.5px] text-[var(--text-tertiary)]">
                No widgets yet. Add one from the list below.
              </p>
            )}
            {widgets.map((w) => {
              const note = sensorNote(w.instance.id);
              return (
                <div
                  key={w.instance.id}
                  className={cx(
                    "flex items-center gap-3 rounded-[8px] px-3 py-2 transition-colors",
                    selected === w.instance.id
                      ? "bg-[var(--bg-hover)]"
                      : "bg-[var(--bg-base)]",
                  )}
                >
                  <button
                    className="min-w-0 flex-1 text-left"
                    onClick={() =>
                      setSelected((s) => (s === w.instance.id ? null : w.instance.id))
                    }
                  >
                    <span className="flex items-center gap-2">
                      <span className="truncate text-[12.5px]">{w.kind_name}</span>
                      {note && <Badge tone="warn">Sensor unavailable</Badge>}
                      {w.instance.locked && <Badge tone="neutral">Locked</Badge>}
                    </span>
                    <span className="mt-0.5 block text-[10.5px] text-[var(--text-tertiary)]">
                      {Math.round(w.instance.x)}, {Math.round(w.instance.y)} ·{" "}
                      {Math.round(w.instance.width)}×{Math.round(w.instance.height)}
                    </span>
                  </button>
                  <Toggle
                    checked={w.instance.enabled}
                    disabled={busy}
                    srLabel={`Show ${w.kind_name}`}
                    onChange={(v) =>
                      void run(() => api.setWidgetEnabled(w.instance.id, v))
                    }
                  />
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={busy}
                    onClick={() =>
                      void run(() => api.removeWidget(w.instance.id), `${w.kind_name} removed`)
                    }
                  >
                    Remove
                  </Button>
                </div>
              );
            })}
          </div>

          <p className="mb-2 mt-4 text-[12px] font-medium text-[var(--text-secondary)]">
            Add a widget
          </p>
          <div className="flex flex-wrap gap-1.5">
            {widgetCatalog
              .filter((k) => !inProfile.has(k.id))
              .map((k) => {
                const note = sensorNote(k.id);
                return (
                  <Tooltip key={k.id} label={note ?? k.description}>
                    <button
                      disabled={busy}
                      onClick={() => void run(() => api.addWidget(k.id), `${k.name} added`)}
                      className={cx(
                        "inline-flex items-center gap-1 rounded-full border px-3 py-1",
                        "border-[var(--border-default)] text-[11.5px] transition-colors",
                        "hover:border-[var(--accent-500)] hover:text-[var(--accent-400)]",
                        note && "opacity-60",
                      )}
                    >
                      <PlusIcon size={11} />
                      {k.name}
                    </button>
                  </Tooltip>
                );
              })}
          </div>
        </div>

        {/* Per-widget settings */}
        {current && (
          <div className="w-[300px] shrink-0 rounded-[10px] border border-[var(--border-subtle)] bg-[var(--bg-base)] p-3.5">
            <p className="mb-3 text-[12px] font-semibold">{current.kind_name}</p>
            <WidgetSettings
              view={current}
              monitors={environment?.monitors ?? []}
              disabled={busy}
              onPatch={(patch) =>
                void run(() => api.configureWidget(current.instance.id, patch))
              }
              onReset={() =>
                void run(() => api.resetWidget(current.instance.id), "Widget reset")
              }
            />
          </div>
        )}
      </div>
    </div>
  );
}

function WidgetSettings({
  view,
  monitors,
  disabled,
  onPatch,
  onReset,
}: {
  view: WidgetView;
  monitors: Array<{ device: string; primary: boolean; width: number; height: number }>;
  disabled?: boolean;
  onPatch: (patch: Record<string, unknown>) => void;
  onReset: () => void;
}) {
  const w = view.instance;

  return (
    <div className="flex flex-col gap-3.5">
      <Field label="Panel style">
        <Select
          value={w.theme}
          onChange={(v) => onPatch({ theme: v })}
          disabled={disabled}
          options={[
            { value: "glass", label: "Glass" },
            { value: "solid", label: "Solid" },
            { value: "outline", label: "Outline" },
            { value: "bare", label: "No panel" },
          ]}
        />
      </Field>

      <Field label="Typeface">
        <Segmented
          value={w.font}
          onChange={(v) => onPatch({ font: v })}
          disabled={disabled}
          options={[
            { value: "sans", label: "Sans" },
            { value: "mono", label: "Mono" },
            { value: "display", label: "Display" },
          ]}
        />
      </Field>

      <Field label="Text size">
        <Slider
          value={Math.round(w.font_scale * 100)}
          onChange={(v) => onPatch({ font_scale: v / 100 })}
          min={60}
          max={250}
          step={5}
          disabled={disabled}
          format={(v) => `${v}%`}
        />
      </Field>

      <Field label="Opacity">
        <Slider
          value={Math.round(w.opacity * 100)}
          onChange={(v) => onPatch({ opacity: v / 100 })}
          min={10}
          max={100}
          disabled={disabled}
          format={(v) => `${v}%`}
        />
      </Field>

      <div className="grid grid-cols-2 gap-2">
        <Field label="Width">
          <Slider
            value={Math.round(w.width)}
            onChange={(v) => onPatch({ width: v })}
            min={120}
            max={900}
            step={10}
            disabled={disabled}
            format={(v) => `${v}`}
          />
        </Field>
        <Field label="Height">
          <Slider
            value={Math.round(w.height)}
            onChange={(v) => onPatch({ height: v })}
            min={60}
            max={900}
            step={10}
            disabled={disabled}
            format={(v) => `${v}`}
          />
        </Field>
      </div>

      {w.interval_ms > 0 && (
        <Field label="Update every">
          <Slider
            value={w.interval_ms}
            onChange={(v) => onPatch({ interval_ms: v })}
            min={250}
            max={60000}
            step={250}
            disabled={disabled}
            format={(v) => (v >= 1000 ? `${(v / 1000).toFixed(1)}s` : `${v}ms`)}
          />
        </Field>
      )}

      <Field label="Layer" hint="Desktop sits above the wallpaper, below your windows.">
        <Segmented
          value={w.z}
          onChange={(v) => onPatch({ z: v })}
          disabled={disabled}
          options={[
            { value: "desktop", label: "Desktop" },
            { value: "normal", label: "Normal" },
            { value: "top", label: "On top" },
          ]}
        />
      </Field>

      {monitors.length > 1 && (
        <Field label="Display">
          <Select
            value={w.monitor || "auto"}
            onChange={(v) => onPatch({ monitor: v === "auto" ? "" : v })}
            disabled={disabled}
            options={[
              { value: "auto", label: "Wherever it is placed" },
              ...monitors.map((m) => ({
                value: m.device,
                label: `${m.width}×${m.height}${m.primary ? " (primary)" : ""}`,
              })),
            ]}
          />
        </Field>
      )}

      <div className="flex flex-col gap-2 border-t border-[var(--border-subtle)] pt-3">
        <label className="flex cursor-pointer items-center justify-between">
          <span className="text-[11.5px] text-[var(--text-secondary)]">
            Lock position
          </span>
          <Toggle
            checked={w.locked}
            onChange={(v) => onPatch({ locked: v })}
            disabled={disabled}
            srLabel="Lock position"
          />
        </label>
        <label className="flex cursor-pointer items-center justify-between">
          <span className="text-[11.5px] text-[var(--text-secondary)]">
            Click through
          </span>
          <Toggle
            checked={w.click_through}
            onChange={(v) => onPatch({ click_through: v })}
            disabled={disabled}
            srLabel="Click through"
          />
        </label>
      </div>

      <Button size="sm" variant="ghost" onClick={onReset} disabled={disabled}>
        Reset this widget
      </Button>
    </div>
  );
}
