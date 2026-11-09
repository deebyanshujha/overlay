/**
 * The widget renderers.
 *
 * One rule runs through all of them: **a reading that is not available is
 * labelled unavailable.** Every metric widget checks `metrics.unavailable`
 * for its own id first and renders the reason rather than a zero, a dash
 * with no explanation, or a plausible-looking number.
 */

import { useEffect, useState } from "react";
import type { Metrics, WidgetInstance } from "@/types/api";
import { cx } from "@/components/ui";

export function WidgetBody({
  id,
  config,
  metrics,
}: {
  id: string;
  config: WidgetInstance;
  metrics: Metrics | null;
}) {
  switch (id) {
    case "clock":
      return <ClockWidget />;
    case "date":
      return <DateWidget />;
    case "calendar":
      return <CalendarWidget />;
    case "weather":
      return <WeatherWidget config={config} />;
    case "cpu":
      return (
        <GaugeWidget
          label="CPU"
          metrics={metrics}
          metricId="cpu"
          value={metrics?.cpu_percent ?? null}
          suffix="%"
          caption={metrics?.cpu_name}
        />
      );
    case "ram": {
      const used = metrics?.memory_used_bytes ?? null;
      const total = metrics?.memory_total_bytes ?? null;
      const pct = used !== null && total ? (used / total) * 100 : null;
      return (
        <GaugeWidget
          label="Memory"
          metrics={metrics}
          metricId="ram"
          value={pct}
          suffix="%"
          caption={
            used !== null && total
              ? `${formatBytes(used)} of ${formatBytes(total)}`
              : undefined
          }
        />
      );
    }
    case "gpu":
      return (
        <GaugeWidget
          label="GPU"
          metrics={metrics}
          metricId="gpu"
          value={metrics?.gpu_percent ?? null}
          suffix="%"
          caption={metrics?.gpu_name ?? undefined}
        />
      );
    case "gpu_temp":
      return (
        <GaugeWidget
          label="GPU temp"
          metrics={metrics}
          metricId="gpu_temp"
          value={metrics?.gpu_temperature_c ?? null}
          suffix="°"
          max={100}
          caption={metrics?.gpu_name ?? undefined}
        />
      );
    case "battery":
      return <BatteryWidget metrics={metrics} />;
    case "network":
      return <NetworkWidget metrics={metrics} />;
    case "network_status":
      return <NetworkStatusWidget metrics={metrics} />;
    case "uptime":
      return <UptimeWidget metrics={metrics} />;
    case "storage":
      return <StorageWidget metrics={metrics} />;
    case "media":
      return <MediaWidget metrics={metrics} />;
    default:
      return <Unavailable label={id} reason="This widget is not available." />;
  }
}

// --------------------------------------------------------------------------
// Shared pieces
// --------------------------------------------------------------------------

function Frame({
  label,
  children,
  className,
}: {
  label?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cx("flex h-full w-full flex-col justify-center px-4 py-3", className)}>
      {label && (
        <span className="mb-1 text-[0.62em] uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
          {label}
        </span>
      )}
      {children}
    </div>
  );
}

/** The honest empty state. Always says why, never shows a fake value. */
function Unavailable({ label, reason }: { label: string; reason: string }) {
  return (
    <Frame label={label}>
      <p className="text-[0.85em] font-medium text-[var(--text-tertiary)]">
        Unavailable
      </p>
      <p className="mt-0.5 text-[0.6em] leading-snug text-[var(--text-tertiary)] opacity-80">
        {reason}
      </p>
    </Frame>
  );
}

function reasonFor(metrics: Metrics | null, id: string): string | null {
  if (!metrics) return null;
  return metrics.unavailable.find((u) => u.metric === id)?.reason ?? null;
}

function Bar({ value, max = 100 }: { value: number; max?: number }) {
  const pct = Math.max(0, Math.min(100, (value / max) * 100));
  return (
    <div className="mt-1.5 h-[3px] w-full overflow-hidden rounded-full bg-[var(--bg-active)]">
      <div
        className="h-full rounded-full transition-[width] duration-500"
        style={{ width: `${pct}%`, background: "var(--accent-500)" }}
      />
    </div>
  );
}

function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return u === 0 ? `${v} B` : `${v.toFixed(1)} ${units[u]}`;
}

function formatRate(bps: number): string {
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let v = bps;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return u === 0 ? `${Math.round(v)} ${units[u]}` : `${v.toFixed(1)} ${units[u]}`;
}

/** Ticks every second, shared by the clock and date widgets. */
function useNow(intervalMs = 1000) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}

// --------------------------------------------------------------------------
// Time
// --------------------------------------------------------------------------

function ClockWidget() {
  const now = useNow();
  const time = now.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
  const seconds = now.toLocaleTimeString(undefined, { second: "2-digit" });

  return (
    <Frame>
      <div className="flex items-baseline gap-2">
        <span className="tabular text-[2.6em] font-light leading-none tracking-[-0.02em]">
          {time}
        </span>
        <span className="tabular text-[0.9em] text-[var(--text-tertiary)]">
          {seconds.padStart(2, "0")}
        </span>
      </div>
      <span className="mt-1 text-[0.72em] text-[var(--text-secondary)]">
        {now.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}
      </span>
    </Frame>
  );
}

function DateWidget() {
  const now = useNow(30_000);
  return (
    <Frame>
      <span className="text-[1.9em] font-light leading-none">
        {now.toLocaleDateString(undefined, { day: "numeric", month: "long" })}
      </span>
      <span className="mt-1 text-[0.8em] text-[var(--text-secondary)]">
        {now.toLocaleDateString(undefined, { weekday: "long", year: "numeric" })}
      </span>
    </Frame>
  );
}

function CalendarWidget() {
  const now = useNow(60_000);
  const year = now.getFullYear();
  const month = now.getMonth();
  const today = now.getDate();

  const first = new Date(year, month, 1);
  // Monday-first, which matches most of the world and the Windows default
  // in the locales OVERLAY is likely to run in.
  const offset = (first.getDay() + 6) % 7;
  const days = new Date(year, month + 1, 0).getDate();

  const cells: Array<number | null> = [
    ...Array<null>(offset).fill(null),
    ...Array.from({ length: days }, (_, i) => i + 1),
  ];

  return (
    <Frame>
      <div className="mb-1.5 flex items-baseline justify-between">
        <span className="text-[0.92em] font-medium">
          {now.toLocaleDateString(undefined, { month: "long" })}
        </span>
        <span className="text-[0.7em] text-[var(--text-tertiary)]">{year}</span>
      </div>
      <div className="grid grid-cols-7 gap-y-[1px] text-center">
        {["M", "T", "W", "T", "F", "S", "S"].map((d, i) => (
          <span key={i} className="text-[0.55em] text-[var(--text-tertiary)]">
            {d}
          </span>
        ))}
        {cells.map((d, i) => (
          <span
            key={i}
            className={cx(
              "tabular rounded-[4px] py-[1px] text-[0.64em]",
              d === today
                ? "font-semibold text-[var(--text-inverse)]"
                : "text-[var(--text-secondary)]",
            )}
            style={d === today ? { background: "var(--accent-500)" } : undefined}
          >
            {d ?? ""}
          </span>
        ))}
      </div>
    </Frame>
  );
}

// --------------------------------------------------------------------------
// Metrics
// --------------------------------------------------------------------------

function GaugeWidget({
  label,
  metrics,
  metricId,
  value,
  suffix,
  caption,
  max = 100,
}: {
  label: string;
  metrics: Metrics | null;
  metricId: string;
  value: number | null;
  suffix: string;
  caption?: string;
  max?: number;
}) {
  const reason = reasonFor(metrics, metricId);
  if (value === null) {
    return (
      <Unavailable
        label={label}
        reason={reason ?? "Waiting for the first reading…"}
      />
    );
  }
  return (
    <Frame label={label}>
      <div className="flex items-baseline gap-1">
        <span className="tabular text-[1.9em] font-light leading-none">
          {Math.round(value)}
        </span>
        <span className="text-[0.8em] text-[var(--text-tertiary)]">{suffix}</span>
      </div>
      <Bar value={value} max={max} />
      {caption && (
        <span className="mt-1 truncate text-[0.58em] text-[var(--text-tertiary)]">
          {caption}
        </span>
      )}
    </Frame>
  );
}

function BatteryWidget({ metrics }: { metrics: Metrics | null }) {
  const reason = reasonFor(metrics, "battery");
  const pct = metrics?.battery_percent ?? null;
  if (pct === null) {
    return <Unavailable label="Battery" reason={reason ?? "No battery detected."} />;
  }
  const charging = metrics?.battery_charging ?? false;
  const left = metrics?.battery_minutes_left ?? null;
  return (
    <Frame label="Battery">
      <div className="flex items-baseline gap-1">
        <span className="tabular text-[1.9em] font-light leading-none">{pct}</span>
        <span className="text-[0.8em] text-[var(--text-tertiary)]">%</span>
        {charging && (
          <span className="ml-1 text-[0.6em] text-[var(--ok)]">charging</span>
        )}
      </div>
      <Bar value={pct} />
      {left !== null && !charging && (
        <span className="mt-1 text-[0.58em] text-[var(--text-tertiary)]">
          about {Math.floor(left / 60)}h {left % 60}m left
        </span>
      )}
    </Frame>
  );
}

function NetworkWidget({ metrics }: { metrics: Metrics | null }) {
  const reason = reasonFor(metrics, "network");
  const down = metrics?.network_down_bps ?? null;
  const up = metrics?.network_up_bps ?? null;
  if (down === null || up === null) {
    return (
      <Unavailable
        label="Network"
        reason={reason ?? "Waiting for the first reading…"}
      />
    );
  }
  return (
    <Frame label="Network">
      <div className="flex flex-col gap-0.5">
        <div className="flex items-baseline justify-between">
          <span className="text-[0.66em] text-[var(--text-tertiary)]">↓ down</span>
          <span className="tabular text-[0.98em]">{formatRate(down)}</span>
        </div>
        <div className="flex items-baseline justify-between">
          <span className="text-[0.66em] text-[var(--text-tertiary)]">↑ up</span>
          <span className="tabular text-[0.98em]">{formatRate(up)}</span>
        </div>
      </div>
      {metrics?.network_interface && (
        <span className="mt-1 truncate text-[0.56em] text-[var(--text-tertiary)]">
          {metrics.network_interface}
        </span>
      )}
    </Frame>
  );
}

function NetworkStatusWidget({ metrics }: { metrics: Metrics | null }) {
  const iface = metrics?.network_interface ?? null;
  const active =
    (metrics?.network_down_bps ?? 0) > 0 || (metrics?.network_up_bps ?? 0) > 0;
  return (
    <Frame label="Network">
      <div className="flex items-center gap-2">
        <span
          className="inline-block h-[7px] w-[7px] rounded-full"
          style={{ background: iface ? "var(--ok)" : "var(--text-tertiary)" }}
        />
        <span className="truncate text-[0.95em]">
          {iface ?? "No active connection"}
        </span>
      </div>
      <span className="mt-0.5 text-[0.6em] text-[var(--text-tertiary)]">
        {iface ? (active ? "Traffic flowing" : "Connected, idle") : "Nothing transferring"}
      </span>
    </Frame>
  );
}

function UptimeWidget({ metrics }: { metrics: Metrics | null }) {
  const seconds = metrics?.uptime_seconds ?? null;
  if (seconds === null) {
    return <Unavailable label="Uptime" reason="Not reported." />;
  }
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  return (
    <Frame label="Uptime">
      <span className="tabular text-[1.5em] font-light leading-none">
        {days > 0 ? `${days}d ${hours}h` : hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`}
      </span>
      <span className="mt-1 text-[0.58em] text-[var(--text-tertiary)]">
        since last restart
      </span>
    </Frame>
  );
}

function StorageWidget({ metrics }: { metrics: Metrics | null }) {
  const disks = metrics?.disks ?? [];
  if (disks.length === 0) {
    return <Unavailable label="Storage" reason="No drives reported." />;
  }
  return (
    <Frame label="Storage">
      <div className="flex flex-col gap-1.5">
        {disks.slice(0, 4).map((d) => {
          const used = d.total_bytes - d.free_bytes;
          const pct = d.total_bytes > 0 ? (used / d.total_bytes) * 100 : 0;
          return (
            <div key={d.mount}>
              <div className="flex items-baseline justify-between">
                <span className="text-[0.66em] text-[var(--text-secondary)]">
                  {d.mount.replace(/\\$/, "")}
                </span>
                <span className="tabular text-[0.6em] text-[var(--text-tertiary)]">
                  {formatBytes(d.free_bytes)} free
                </span>
              </div>
              <Bar value={pct} />
            </div>
          );
        })}
      </div>
    </Frame>
  );
}

function MediaWidget({ metrics }: { metrics: Metrics | null }) {
  // Always unavailable in this build; the backend says so explicitly rather
  // than letting the widget imply nothing is playing.
  const reason =
    reasonFor(metrics, "media") ??
    "Reading the Now Playing session is not implemented in this build.";
  return <Unavailable label="Now playing" reason={reason} />;
}

function WeatherWidget({ config }: { config: WidgetInstance }) {
  const location = (config.settings?.location as string) ?? "";
  // No provider is configured by default, and OVERLAY makes no network calls
  // without one. Saying so is better than an empty card.
  return (
    <Frame label="Weather">
      <p className="text-[0.85em] font-medium text-[var(--text-tertiary)]">
        {location ? location : "No provider configured"}
      </p>
      <p className="mt-0.5 text-[0.58em] leading-snug text-[var(--text-tertiary)] opacity-80">
        OVERLAY makes no network requests until you add a weather provider in
        this widget's settings.
      </p>
    </Frame>
  );
}
