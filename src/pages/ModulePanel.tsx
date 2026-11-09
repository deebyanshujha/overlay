/**
 * Per-module configuration panels.
 *
 * Each panel edits a typed slice of its module's config and sends the whole
 * object back, where the Rust side validates and clamps it. The frontend
 * never assumes its own values are legal — it renders what comes back.
 */

import { useCallback, useState } from "react";
import { useApp } from "@/stores/app";
import * as api from "@/services/api";
import { errorMessage } from "@/types/api";
import type { ModuleStatus } from "@/types/api";
import {
  Badge,
  Button,
  ColorSwatches,
  Dot,
  Field,
  Segmented,
  Select,
  Slider,
  TextInput,
  Toggle,
  useToast,
} from "@/components/ui";
import { WidgetsPanel } from "@/pages/WidgetsPanel";

type Config = Record<string, unknown>;

/** Debounced config writer shared by every panel. */
function useConfigWriter(module: ModuleStatus) {
  const { configureModule } = useApp();
  const toast = useToast();
  const [saving, setSaving] = useState(false);

  const write = useCallback(
    async (next: Config) => {
      setSaving(true);
      try {
        const { warnings } = await configureModule(module.id, next);
        warnings.forEach((w) => toast.push(w, "warn"));
      } catch (e) {
        toast.push(`Could not update ${module.name}`, "danger", errorMessage(e));
      } finally {
        setSaving(false);
      }
    },
    [configureModule, module.id, module.name, toast],
  );

  return { write, saving };
}

export function ModulePanel({ module }: { module: ModuleStatus }) {
  return (
    <div className="flex flex-col gap-5">
      {module.notes.length > 0 && (
        <ul className="flex flex-col gap-1.5">
          {module.notes.map((n, i) => (
            <li key={i} className="flex gap-2">
              <span className="mt-[6px]">
                <Dot />
              </span>
              <span className="text-[11.5px] leading-relaxed text-[var(--text-tertiary)]">
                {n}
              </span>
            </li>
          ))}
        </ul>
      )}

      {module.id === "taskbar" && <TaskbarPanel module={module} />}
      {module.id === "desktop_icons" && <DesktopIconsPanel module={module} />}
      {module.id === "widgets" && <WidgetsPanel module={module} />}
      {module.id === "dock" && <DockPanel module={module} />}
      {module.id === "hud" && <HudPanel module={module} />}
      {module.id === "control_center" && <ControlCenterPanel module={module} />}
      {module.id === "dropzone" && <DropzonePanel module={module} />}
      {(module.id === "wallflow" || module.id === "quickdock") && (
        <CompanionPanel module={module} />
      )}

      <ResetRow module={module} />
    </div>
  );
}

function ResetRow({ module }: { module: ModuleStatus }) {
  const { refreshModules } = useApp();
  const toast = useToast();
  return (
    <div className="flex items-center justify-between border-t border-[var(--border-subtle)] pt-3">
      <span className="text-[11px] text-[var(--text-tertiary)]">
        v{module.version}
      </span>
      <Button
        size="sm"
        variant="ghost"
        onClick={async () => {
          try {
            await api.resetModuleConfig(module.id);
            await refreshModules();
            toast.push(`${module.name} settings reset`, "ok");
          } catch (e) {
            toast.push("Could not reset", "danger", errorMessage(e));
          }
        }}
      >
        Reset to defaults
      </Button>
    </div>
  );
}

// --------------------------------------------------------------------------
// Taskbar
// --------------------------------------------------------------------------

const TINTS = [
  { value: 0x0b0c10, label: "Graphite" },
  { value: 0x161220, label: "Midnight" },
  { value: 0x0a1418, label: "Deep teal" },
  { value: 0x1a1014, label: "Plum" },
  { value: 0x000000, label: "Black" },
  { value: 0xf2f4f8, label: "Light" },
];

function TaskbarPanel({ module }: { module: ModuleStatus }) {
  const { environment } = useApp();
  const { write, saving } = useConfigWriter(module);
  const cfg = module.config as Config;

  const set = (patch: Config) => void write({ ...cfg, ...patch });
  const isWin11 = environment?.is_windows_11 ?? false;
  const accentOk = environment?.taskbar_accent_supported ?? false;

  return (
    <div className="grid grid-cols-2 gap-x-6 gap-y-4">
      <Field label="Finish" hint={accentOk ? undefined : "Not available on this Windows build."}>
        <Select
          value={String(cfg.finish ?? "blurred")}
          onChange={(v) => set({ finish: v })}
          disabled={!accentOk || saving}
          options={[
            { value: "system", label: "Leave as Windows has it" },
            { value: "tinted", label: "Tinted (no blur)" },
            { value: "blurred", label: "Blurred" },
            { value: "acrylic", label: "Acrylic" },
            { value: "clear", label: "Fully clear" },
          ]}
        />
      </Field>

      <Field label="Opacity">
        <Slider
          value={Math.round(Number(cfg.opacity ?? 0.35) * 100)}
          onChange={(v) => set({ opacity: v / 100 })}
          disabled={!accentOk || cfg.finish === "system" || saving}
          format={(v) => `${v}%`}
        />
      </Field>

      <Field label="Tint" hint="Only used by tinted, blurred and acrylic finishes.">
        <ColorSwatches
          value={Number(cfg.tint ?? 0x0e1016)}
          onChange={(v) => set({ tint: v })}
          colors={TINTS}
        />
      </Field>

      <div className="flex flex-col gap-4">
        <Field
          label="Alignment"
          hint={isWin11 ? undefined : "Windows 11 only."}
        >
          <Segmented
            value={String(cfg.alignment ?? "unset")}
            onChange={(v) => set({ alignment: v === "unset" ? null : v })}
            disabled={!isWin11 || saving}
            options={[
              { value: "unset", label: "Leave" },
              { value: "left", label: "Left" },
              { value: "center", label: "Centre" },
            ]}
          />
        </Field>

        <Field label="Icon size" hint={isWin11 ? undefined : "Windows 11 only."}>
          <Segmented
            value={String(cfg.icon_size ?? "unset")}
            onChange={(v) => set({ icon_size: v === "unset" ? null : v })}
            disabled={!isWin11 || saving}
            options={[
              { value: "unset", label: "Leave" },
              { value: "small", label: "Small" },
              { value: "medium", label: "Medium" },
              { value: "large", label: "Large" },
            ]}
          />
        </Field>
      </div>

      <TriStateRow
        label="Auto-hide the taskbar"
        hint="Stored inside Windows' own taskbar settings; every other preference in that record is left untouched."
        value={cfg.auto_hide as boolean | null}
        onChange={(v) => set({ auto_hide: v })}
        disabled={saving}
      />

      <TriStateRow
        label="Windows transparency effects"
        hint="The same switch as Settings → Personalisation → Colours."
        value={cfg.system_transparency as boolean | null}
        onChange={(v) => set({ system_transparency: v })}
        disabled={saving}
      />

      {isWin11 && (
        <>
          <TriStateRow
            label="Widgets button"
            value={cfg.show_widgets_button as boolean | null}
            onChange={(v) => set({ show_widgets_button: v })}
            disabled={saving}
          />
          <TriStateRow
            label="Task view button"
            value={cfg.show_task_view as boolean | null}
            onChange={(v) => set({ show_task_view: v })}
            disabled={saving}
          />
        </>
      )}
    </div>
  );
}

/**
 * Three-state control for a setting OVERLAY can leave alone.
 *
 * "Leave" is not the same as "off": it means OVERLAY never touches the
 * setting at all, so it is never recorded as owned and never restored. That
 * distinction is the difference between a customiser that claims your whole
 * taskbar and one that claims only what you asked for.
 */
function TriStateRow({
  label,
  hint,
  value,
  onChange,
  disabled,
}: {
  label: string;
  hint?: string;
  value: boolean | null | undefined;
  onChange: (next: boolean | null) => void;
  disabled?: boolean;
}) {
  const current = value === null || value === undefined ? "leave" : value ? "on" : "off";
  return (
    <Field label={label} hint={hint}>
      <Segmented
        value={current}
        onChange={(v) => onChange(v === "leave" ? null : v === "on")}
        disabled={disabled}
        options={[
          { value: "leave", label: "Leave" },
          { value: "on", label: "On" },
          { value: "off", label: "Off" },
        ]}
      />
    </Field>
  );
}

// --------------------------------------------------------------------------
// Desktop icons
// --------------------------------------------------------------------------

function DesktopIconsPanel({ module }: { module: ModuleStatus }) {
  const { write, saving } = useConfigWriter(module);
  const cfg = module.config as Config;

  return (
    <div className="flex items-center justify-between gap-6">
      <div>
        <p className="text-[12.5px] font-medium">Show desktop icons</p>
        <p className="mt-0.5 max-w-[56ch] text-[11.5px] leading-relaxed text-[var(--text-tertiary)]">
          Hiding icons changes visibility only. Positions, names and files are
          untouched, and nothing on your Desktop is ever moved or deleted.
        </p>
      </div>
      <Toggle
        checked={Boolean(cfg.show_icons)}
        onChange={(v) => void write({ ...cfg, show_icons: v })}
        disabled={saving}
        srLabel="Show desktop icons"
      />
    </div>
  );
}

// --------------------------------------------------------------------------
// Dock
// --------------------------------------------------------------------------

function DockPanel({ module }: { module: ModuleStatus }) {
  const { write, saving } = useConfigWriter(module);
  const cfg = module.config as Config;
  const set = (patch: Config) => void write({ ...cfg, ...patch });
  const pinned = (cfg.pinned as Array<{ id: string; name: string; path: string }>) ?? [];

  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-2 gap-x-6 gap-y-4">
        <Field label="Edge">
          <Segmented
            value={String(cfg.edge ?? "bottom")}
            onChange={(v) => set({ edge: v })}
            disabled={saving}
            options={[
              { value: "bottom", label: "Bottom" },
              { value: "top", label: "Top" },
              { value: "left", label: "Left" },
              { value: "right", label: "Right" },
            ]}
          />
        </Field>
        <Field label="Icon size">
          <Slider
            value={Number(cfg.icon_size ?? 48)}
            onChange={(v) => set({ icon_size: v })}
            min={24}
            max={128}
            disabled={saving}
            format={(v) => `${v}px`}
          />
        </Field>
        <Field label="Magnification" hint="1.0 turns it off.">
          <Slider
            value={Math.round(Number(cfg.magnification ?? 1.6) * 10)}
            onChange={(v) => set({ magnification: v / 10 })}
            min={10}
            max={25}
            disabled={saving}
            format={(v) => `${(v / 10).toFixed(1)}×`}
          />
        </Field>
        <Field label="Opacity">
          <Slider
            value={Math.round(Number(cfg.opacity ?? 0.92) * 100)}
            onChange={(v) => set({ opacity: v / 100 })}
            min={20}
            max={100}
            disabled={saving}
            format={(v) => `${v}%`}
          />
        </Field>
      </div>

      <div className="flex gap-6">
        <InlineToggle
          label="Running indicators"
          value={Boolean(cfg.show_running_indicators ?? true)}
          onChange={(v) => set({ show_running_indicators: v })}
          disabled={saving}
        />
        <InlineToggle
          label="Auto-hide"
          value={Boolean(cfg.auto_hide)}
          onChange={(v) => set({ auto_hide: v })}
          disabled={saving}
        />
      </div>

      <PinnedApps
        pinned={pinned}
        onChange={(next) => set({ pinned: next })}
        disabled={saving}
      />
    </div>
  );
}

function PinnedApps({
  pinned,
  onChange,
  disabled,
}: {
  pinned: Array<{ id: string; name: string; path: string }>;
  onChange: (next: Array<{ id: string; name: string; path: string; args: string[] }>) => void;
  disabled?: boolean;
}) {
  const toast = useToast();

  const add = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: true,
      title: "Pin applications to the dock",
      filters: [{ name: "Programs and shortcuts", extensions: ["exe", "lnk"] }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];

    const additions = paths.map((path) => {
      const name =
        path.split(/[\\/]/).pop()?.replace(/\.(exe|lnk)$/i, "") ?? "Application";
      return { id: path.toLowerCase(), name, path, args: [] };
    });

    const merged = [
      ...pinned.map((p) => ({ ...p, args: [] as string[] })),
      ...additions.filter((a) => !pinned.some((p) => p.id === a.id)),
    ];
    onChange(merged);
    toast.push(`Pinned ${additions.length} item${additions.length === 1 ? "" : "s"}`, "ok");
  };

  return (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[12px] font-medium text-[var(--text-secondary)]">
          Pinned applications
        </span>
        <Button size="sm" variant="secondary" onClick={() => void add()} disabled={disabled}>
          Add…
        </Button>
      </div>
      {pinned.length === 0 ? (
        <p className="rounded-[8px] bg-[var(--bg-base)] px-3 py-3 text-[11.5px] text-[var(--text-tertiary)]">
          Nothing pinned yet. The dock appears once it has something to show.
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {pinned.map((p) => (
            <li
              key={p.id}
              className="flex items-center justify-between gap-3 rounded-[8px] bg-[var(--bg-base)] px-3 py-2"
            >
              <div className="min-w-0">
                <p className="truncate text-[12px]">{p.name}</p>
                <p className="truncate text-[10.5px] text-[var(--text-tertiary)]">
                  {p.path}
                </p>
              </div>
              <Button
                size="sm"
                variant="ghost"
                disabled={disabled}
                onClick={() =>
                  onChange(
                    pinned
                      .filter((x) => x.id !== p.id)
                      .map((x) => ({ ...x, args: [] as string[] })),
                  )
                }
              >
                Remove
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// --------------------------------------------------------------------------
// HUD
// --------------------------------------------------------------------------

const HUD_METRICS: Array<{ id: string; label: string }> = [
  { id: "cpu", label: "CPU" },
  { id: "ram", label: "Memory" },
  { id: "gpu", label: "GPU" },
  { id: "gpu_temp", label: "GPU temp" },
  { id: "battery", label: "Battery" },
  { id: "network", label: "Network" },
  { id: "storage", label: "Storage" },
  { id: "uptime", label: "Uptime" },
];

function HudPanel({ module }: { module: ModuleStatus }) {
  const { capabilities } = useApp();
  const { write, saving } = useConfigWriter(module);
  const cfg = module.config as Config;
  const set = (patch: Config) => void write({ ...cfg, ...patch });
  const metrics = (cfg.metrics as string[]) ?? [];

  const unavailableReason = (id: string): string | null => {
    if (!capabilities) return null;
    if ((id === "gpu" || id === "gpu_temp") && !capabilities.gpu_usage)
      return capabilities.gpu_detail;
    if (id === "gpu_temp" && !capabilities.gpu_temperature)
      return "This GPU does not report temperature.";
    if (id === "battery" && !capabilities.battery)
      return "No battery detected on this machine.";
    return null;
  };

  const toggleMetric = (id: string) => {
    const next = metrics.includes(id)
      ? metrics.filter((m) => m !== id)
      : [...metrics, id];
    if (next.length === 0) return; // the backend requires at least one
    set({ metrics: next });
  };

  return (
    <div className="flex flex-col gap-5">
      <div>
        <p className="mb-2 text-[12px] font-medium text-[var(--text-secondary)]">
          Metrics shown
        </p>
        <div className="flex flex-wrap gap-1.5">
          {HUD_METRICS.map((m) => {
            const on = metrics.includes(m.id);
            const reason = unavailableReason(m.id);
            return (
              <button
                key={m.id}
                onClick={() => toggleMetric(m.id)}
                disabled={saving}
                title={reason ?? undefined}
                className={[
                  "rounded-full border px-3 py-1 text-[11.5px] transition-all",
                  on
                    ? "border-[var(--accent-500)] bg-[rgb(var(--accent-rgb)/0.14)] text-[var(--accent-400)]"
                    : "border-[var(--border-default)] text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]",
                ].join(" ")}
              >
                {m.label}
                {reason && on && " ·  unavailable"}
              </button>
            );
          })}
        </div>
        {metrics.some((m) => unavailableReason(m)) && (
          <p className="mt-2 text-[11px] leading-relaxed text-[var(--text-tertiary)]">
            Metrics this machine cannot report are shown as unavailable in the
            HUD rather than estimated.
          </p>
        )}
      </div>

      <div className="grid grid-cols-2 gap-x-6 gap-y-4">
        <Field label="Corner">
          <Select
            value={String(cfg.anchor ?? "top_right")}
            onChange={(v) => set({ anchor: v })}
            disabled={saving}
            options={[
              { value: "top_left", label: "Top left" },
              { value: "top_right", label: "Top right" },
              { value: "bottom_left", label: "Bottom left" },
              { value: "bottom_right", label: "Bottom right" },
            ]}
          />
        </Field>
        <Field label="Layout">
          <Segmented
            value={String(cfg.layout ?? "horizontal")}
            onChange={(v) => set({ layout: v })}
            disabled={saving}
            options={[
              { value: "horizontal", label: "Row" },
              { value: "vertical", label: "Column" },
            ]}
          />
        </Field>
        <Field label="Refresh">
          <Slider
            value={Number(cfg.interval_ms ?? 2000)}
            onChange={(v) => set({ interval_ms: v })}
            min={500}
            max={10000}
            step={500}
            disabled={saving}
            format={(v) => `${(v / 1000).toFixed(1)}s`}
          />
        </Field>
        <Field label="Opacity">
          <Slider
            value={Math.round(Number(cfg.opacity ?? 0.9) * 100)}
            onChange={(v) => set({ opacity: v / 100 })}
            min={20}
            max={100}
            disabled={saving}
            format={(v) => `${v}%`}
          />
        </Field>
      </div>

      <div className="flex gap-6">
        <InlineToggle
          label="Compact"
          value={Boolean(cfg.compact)}
          onChange={(v) => set({ compact: v })}
          disabled={saving}
        />
        <InlineToggle
          label="Graphs"
          value={Boolean(cfg.show_graphs ?? true)}
          onChange={(v) => set({ show_graphs: v })}
          disabled={saving}
        />
        <InlineToggle
          label="Click through"
          value={Boolean(cfg.click_through ?? true)}
          onChange={(v) => set({ click_through: v })}
          disabled={saving}
        />
      </div>
    </div>
  );
}

// --------------------------------------------------------------------------
// Control Center
// --------------------------------------------------------------------------

function ControlCenterPanel({ module }: { module: ModuleStatus }) {
  const { controls, settings } = useApp();
  const { write, saving } = useConfigWriter(module);
  const toast = useToast();
  const cfg = module.config as Config;
  const enabled = (cfg.controls as string[]) ?? [];
  const [hotkey, setHotkey] = useState(
    String(cfg.hotkey ?? settings?.control_center_hotkey ?? "Alt+Shift+O"),
  );

  return (
    <div className="flex flex-col gap-5">
      <Field
        label="Global shortcut"
        hint="QuickDock's own shortcut is refused so both keep working."
      >
        <div className="flex gap-2">
          <TextInput
            value={hotkey}
            onChange={(e) => setHotkey(e.target.value)}
            placeholder="Alt+Shift+O"
          />
          <Button
            variant="secondary"
            disabled={saving}
            onClick={async () => {
              try {
                await api.setSetting("control_center_hotkey", hotkey);
                await write({ ...cfg, hotkey });
                toast.push("Shortcut bound", "ok");
              } catch (e) {
                toast.push("Could not bind that shortcut", "danger", errorMessage(e));
              }
            }}
          >
            Bind
          </Button>
        </div>
      </Field>

      <div>
        <p className="mb-2 text-[12px] font-medium text-[var(--text-secondary)]">
          Controls shown
        </p>
        <ul className="flex flex-col gap-1">
          {controls.map((c) => {
            const on = enabled.includes(c.id);
            const isShortcut = c.kind.kind === "opens_windows";
            return (
              <li
                key={c.id}
                className="flex items-center justify-between gap-4 rounded-[8px] bg-[var(--bg-base)] px-3 py-2"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-[12px]">{c.label}</span>
                    {isShortcut && <Badge tone="neutral">Opens Windows</Badge>}
                  </div>
                  <p className="mt-0.5 text-[11px] leading-snug text-[var(--text-tertiary)]">
                    {c.description}
                  </p>
                </div>
                <Toggle
                  checked={on}
                  disabled={saving}
                  srLabel={c.label}
                  onChange={(v) =>
                    void write({
                      ...cfg,
                      controls: v
                        ? [...enabled, c.id]
                        : enabled.filter((x) => x !== c.id),
                    })
                  }
                />
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}

// --------------------------------------------------------------------------
// Dropzone
// --------------------------------------------------------------------------

function DropzonePanel({ module }: { module: ModuleStatus }) {
  const { write, saving } = useConfigWriter(module);
  const cfg = module.config as Config;
  const set = (patch: Config) => void write({ ...cfg, ...patch });

  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-2 gap-x-6 gap-y-4">
        <Field label="Width">
          <Slider
            value={Number(cfg.width ?? 300)}
            onChange={(v) => set({ width: v })}
            min={220}
            max={600}
            step={10}
            disabled={saving}
            format={(v) => `${v}px`}
          />
        </Field>
        <Field label="Height">
          <Slider
            value={Number(cfg.height ?? 380)}
            onChange={(v) => set({ height: v })}
            min={240}
            max={900}
            step={10}
            disabled={saving}
            format={(v) => `${v}px`}
          />
        </Field>
        <Field
          label="If a file of that name already exists"
          hint="Renaming is the default because nothing can be lost that way."
        >
          <Select
            value={String(cfg.collision ?? "rename")}
            onChange={(v) => set({ collision: v })}
            disabled={saving}
            options={[
              { value: "rename", label: "Keep both (rename)" },
              { value: "skip", label: "Skip" },
              { value: "overwrite", label: "Overwrite" },
            ]}
          />
        </Field>
        <Field label="Opacity">
          <Slider
            value={Math.round(Number(cfg.opacity ?? 0.96) * 100)}
            onChange={(v) => set({ opacity: v / 100 })}
            min={40}
            max={100}
            disabled={saving}
            format={(v) => `${v}%`}
          />
        </Field>
      </div>
      <div className="flex gap-6">
        <InlineToggle
          label="Collapse when idle"
          value={Boolean(cfg.collapse_when_idle ?? true)}
          onChange={(v) => set({ collapse_when_idle: v })}
          disabled={saving}
        />
        <InlineToggle
          label="Show previews"
          value={Boolean(cfg.show_previews ?? true)}
          onChange={(v) => set({ show_previews: v })}
          disabled={saving}
        />
      </div>
    </div>
  );
}

// --------------------------------------------------------------------------
// WallFlow / QuickDock
// --------------------------------------------------------------------------

function CompanionPanel({ module }: { module: ModuleStatus }) {
  const { write, saving } = useConfigWriter(module);
  const cfg = module.config as Config;
  const set = (patch: Config) => void write({ ...cfg, ...patch });

  return (
    <div className="flex flex-col gap-4">
      <InlineToggle
        label="Start it when this profile is applied"
        value={Boolean(cfg.launch_on_activate ?? true)}
        onChange={(v) => set({ launch_on_activate: v })}
        disabled={saving}
      />
      {module.id === "quickdock" && (
        <InlineToggle
          label="Start quietly (into the tray)"
          value={Boolean(cfg.launch_silently ?? true)}
          onChange={(v) => set({ launch_silently: v })}
          disabled={saving}
        />
      )}
      {module.id === "wallflow" && (
        <InlineToggle
          label="Show the current wallpaper on Home"
          value={Boolean(cfg.show_on_home ?? true)}
          onChange={(v) => set({ show_on_home: v })}
          disabled={saving}
        />
      )}
    </div>
  );
}

// --------------------------------------------------------------------------

function InlineToggle({
  label,
  value,
  onChange,
  disabled,
}: {
  label: string;
  value: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <label className="flex cursor-pointer items-center gap-2.5">
      <Toggle checked={value} onChange={onChange} disabled={disabled} srLabel={label} />
      <span className="text-[12px] text-[var(--text-secondary)]">{label}</span>
    </label>
  );
}
