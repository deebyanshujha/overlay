/**
 * System HUD — a strip of live metrics.
 *
 * Polls at the interval its config asks for and stops entirely when the
 * window is hidden: `visibilitychange` fires for a hidden Tauri window, so a
 * HUD behind a maximised game costs nothing.
 */

import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "@/services/api";
import { applyTheme } from "@/stores/app";
import { SURFACES_CHANGED } from "@/types/api";
import type { Metrics, MonitoringCapabilities } from "@/types/api";
import { cx } from "@/components/ui";

interface HudConfig {
  metrics: string[];
  layout: "horizontal" | "vertical";
  opacity: number;
  interval_ms: number;
  compact: boolean;
  show_graphs: boolean;
}

const DEFAULTS: HudConfig = {
  metrics: ["cpu", "ram", "network"],
  layout: "horizontal",
  opacity: 0.9,
  interval_ms: 2000,
  compact: false,
  show_graphs: true,
};

/** How many samples each sparkline keeps. */
const HISTORY = 40;

export function HudSurface() {
  const [config, setConfig] = useState<HudConfig>(DEFAULTS);
  const [metrics, setMetrics] = useState<Metrics | null>(null);
  const [caps, setCaps] = useState<MonitoringCapabilities | null>(null);
  const history = useRef<Record<string, number[]>>({});

  useEffect(() => {
    void api.getSettings().then(applyTheme).catch(() => undefined);
    void api.getCapabilities().then(setCaps).catch(() => undefined);

    const readConfig = () =>
      api
        .listModules()
        .then((r) => {
          const hud = r.modules.find((m) => m.id === "hud");
          if (hud) setConfig({ ...DEFAULTS, ...(hud.config as unknown as HudConfig) });
        })
        .catch(() => undefined);
    void readConfig();
    // Told, not polled: re-reading module status on a timer from every
    // surface loaded the thread that has to stay free for input.
    const unlisten = listen(SURFACES_CHANGED, () => void readConfig());
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    let stopped = false;

    const sample = async () => {
      if (document.hidden) return;
      try {
        const m = await api.getMetrics();
        if (stopped) return;
        setMetrics(m);
        record(history.current, m);
      } catch {
        // A dropped sample is not worth surfacing; the next one will land.
      }
    };

    void sample();
    const timer = window.setInterval(() => void sample(), config.interval_ms);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [config.interval_ms]);

  const vertical = config.layout === "vertical";

  return (
    <div className="h-full w-full p-[3px]" style={{ opacity: config.opacity }}>
      <div
        className={cx(
          "glass flex h-full w-full items-center gap-1 overflow-hidden",
          "border border-[var(--border-default)] shadow-[var(--shadow-md)]",
          vertical ? "flex-col justify-center" : "flex-row justify-around",
        )}
        style={{ borderRadius: "var(--radius-lg)" }}
      >
        {config.metrics.map((id) => (
          <MetricCell
            key={id}
            id={id}
            metrics={metrics}
            caps={caps}
            compact={config.compact}
            graph={config.show_graphs ? (history.current[id] ?? []) : null}
          />
        ))}
      </div>
    </div>
  );
}

function record(store: Record<string, number[]>, m: Metrics) {
  const push = (key: string, value: number | null) => {
    if (value === null) return;
    const series = store[key] ?? [];
    series.push(value);
    if (series.length > HISTORY) series.shift();
    store[key] = series;
  };
  push("cpu", m.cpu_percent);
  push(
    "ram",
    m.memory_used_bytes !== null && m.memory_total_bytes
      ? (m.memory_used_bytes / m.memory_total_bytes) * 100
      : null,
  );
  push("gpu", m.gpu_percent);
  push("gpu_temp", m.gpu_temperature_c);
  push("battery", m.battery_percent);
  push(
    "network",
    m.network_down_bps !== null ? m.network_down_bps / 1024 : null,
  );
  push(
    "storage",
    m.disks.length > 0
      ? ((m.disks[0].total_bytes - m.disks[0].free_bytes) / m.disks[0].total_bytes) * 100
      : null,
  );
}

function MetricCell({
  id,
  metrics,
  caps,
  compact,
  graph,
}: {
  id: string;
  metrics: Metrics | null;
  caps: MonitoringCapabilities | null;
  compact: boolean;
  graph: number[] | null;
}) {
  const reading = read(id, metrics, caps);

  return (
    <div
      className={cx(
        "relative flex min-w-0 flex-1 flex-col justify-center overflow-hidden px-3",
        compact ? "py-1.5" : "py-2.5",
      )}
    >
      {graph && graph.length > 2 && reading.value !== null && (
        <Sparkline values={graph} />
      )}
      <span className="relative text-[9px] uppercase tracking-[0.1em] text-[var(--text-tertiary)]">
        {reading.label}
      </span>
      {reading.value === null ? (
        <span
          className="relative truncate text-[11px] text-[var(--text-tertiary)]"
          title={reading.detail}
        >
          unavailable
        </span>
      ) : (
        <span className="tabular relative text-[15px] font-light leading-tight">
          {reading.value}
          <span className="ml-0.5 text-[10px] text-[var(--text-tertiary)]">
            {reading.suffix}
          </span>
        </span>
      )}
    </div>
  );
}

function Sparkline({ values }: { values: number[] }) {
  const max = Math.max(...values, 1);
  const points = values
    .map((v, i) => {
      const x = (i / (values.length - 1)) * 100;
      const y = 100 - (v / max) * 100;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  return (
    <svg
      className="pointer-events-none absolute inset-x-0 bottom-0 h-[55%] w-full opacity-30"
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      aria-hidden
    >
      <polyline
        points={points}
        fill="none"
        stroke="var(--accent-500)"
        strokeWidth="2.5"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

function read(
  id: string,
  m: Metrics | null,
  caps: MonitoringCapabilities | null,
): { label: string; value: string | null; suffix: string; detail: string } {
  const unavailable = (label: string, detail: string) => ({
    label,
    value: null,
    suffix: "",
    detail,
  });
  const reason = (metric: string) =>
    m?.unavailable.find((u) => u.metric === metric)?.reason ?? "Not reported.";

  switch (id) {
    case "cpu":
      return m?.cpu_percent === null || m?.cpu_percent === undefined
        ? unavailable("CPU", reason("cpu"))
        : { label: "CPU", value: Math.round(m.cpu_percent).toString(), suffix: "%", detail: "" };
    case "ram": {
      if (!m?.memory_used_bytes || !m.memory_total_bytes)
        return unavailable("Memory", reason("ram"));
      const pct = (m.memory_used_bytes / m.memory_total_bytes) * 100;
      return { label: "Memory", value: Math.round(pct).toString(), suffix: "%", detail: "" };
    }
    case "gpu":
      return m?.gpu_percent === null || m?.gpu_percent === undefined
        ? unavailable("GPU", caps?.gpu_detail ?? reason("gpu"))
        : { label: "GPU", value: Math.round(m.gpu_percent).toString(), suffix: "%", detail: "" };
    case "gpu_temp":
      return m?.gpu_temperature_c === null || m?.gpu_temperature_c === undefined
        ? unavailable("GPU temp", caps?.gpu_detail ?? reason("gpu_temp"))
        : { label: "GPU temp", value: String(m.gpu_temperature_c), suffix: "°C", detail: "" };
    case "battery":
      return m?.battery_percent === null || m?.battery_percent === undefined
        ? unavailable("Battery", reason("battery"))
        : { label: "Battery", value: String(m.battery_percent), suffix: "%", detail: "" };
    case "network": {
      if (m?.network_down_bps === null || m?.network_down_bps === undefined)
        return unavailable("Network", reason("network"));
      const kb = m.network_down_bps / 1024;
      return kb >= 1024
        ? { label: "Network", value: (kb / 1024).toFixed(1), suffix: "MB/s", detail: "" }
        : { label: "Network", value: Math.round(kb).toString(), suffix: "KB/s", detail: "" };
    }
    case "storage": {
      const d = m?.disks[0];
      if (!d) return unavailable("Storage", "No drives reported.");
      const pct = ((d.total_bytes - d.free_bytes) / d.total_bytes) * 100;
      return { label: "Storage", value: Math.round(pct).toString(), suffix: "%", detail: "" };
    }
    case "uptime": {
      const s = m?.uptime_seconds;
      if (s === null || s === undefined) return unavailable("Uptime", "Not reported.");
      const days = Math.floor(s / 86_400);
      const hours = Math.floor((s % 86_400) / 3_600);
      return days > 0
        ? { label: "Uptime", value: String(days), suffix: "d", detail: "" }
        : { label: "Uptime", value: String(hours), suffix: "h", detail: "" };
    }
    default:
      return unavailable(id, "Unknown metric.");
  }
}
