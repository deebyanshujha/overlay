/**
 * Widget rendering.
 *
 * The rule under test throughout: a reading that is not available is shown
 * as unavailable with a reason, never as a zero or a plausible-looking
 * number. A fake sensor value is worse than none, because the user cannot
 * tell it is fake.
 */

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { WidgetBody } from "@/widgets/WidgetBody";
import type { Metrics, WidgetInstance } from "@/types/api";

function instance(over: Partial<WidgetInstance> = {}): WidgetInstance {
  return {
    id: "clock",
    enabled: true,
    x: 80,
    y: 80,
    width: 380,
    height: 150,
    monitor: "",
    opacity: 1,
    theme: "glass",
    font: "sans",
    font_scale: 1,
    interval_ms: 1000,
    locked: false,
    click_through: false,
    z: "desktop",
    settings: {},
    ...over,
  };
}

function metrics(over: Partial<Metrics> = {}): Metrics {
  return {
    cpu_percent: 23.4,
    cpu_name: "Intel Core i7",
    cpu_cores: 8,
    memory_used_bytes: 8 * 1024 ** 3,
    memory_total_bytes: 16 * 1024 ** 3,
    gpu_percent: null,
    gpu_temperature_c: null,
    gpu_name: null,
    battery_percent: 72,
    battery_charging: false,
    battery_minutes_left: 145,
    network_down_bps: 1536 * 1024,
    network_up_bps: 64 * 1024,
    network_interface: "Wi-Fi",
    disks: [{ mount: "C:\\", total_bytes: 500 * 1024 ** 3, free_bytes: 120 * 1024 ** 3 }],
    uptime_seconds: 3 * 86_400 + 5 * 3_600,
    unavailable: [],
    ...over,
  };
}

describe("Metric widgets", () => {
  it("shows a real CPU reading", () => {
    render(<WidgetBody id="cpu" config={instance({ id: "cpu" })} metrics={metrics()} />);
    expect(screen.getByText("23")).toBeInTheDocument();
    expect(screen.getByText("Intel Core i7")).toBeInTheDocument();
  });

  it("shows GPU as unavailable with the reason, never as zero", () => {
    render(
      <WidgetBody
        id="gpu"
        config={instance({ id: "gpu" })}
        metrics={metrics({
          unavailable: [
            { metric: "gpu", reason: "No NVIDIA GPU reachable through NVML." },
          ],
        })}
      />,
    );
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByText("No NVIDIA GPU reachable through NVML.")).toBeInTheDocument();
    expect(screen.queryByText("0")).not.toBeInTheDocument();
  });

  it("explains a pending first CPU sample rather than reporting 0%", () => {
    render(
      <WidgetBody
        id="cpu"
        config={instance({ id: "cpu" })}
        metrics={metrics({
          cpu_percent: null,
          unavailable: [{ metric: "cpu", reason: "waiting for a second reading" }],
        })}
      />,
    );
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByText("waiting for a second reading")).toBeInTheDocument();
  });

  it("falls back to a readable message when no metrics have arrived", () => {
    render(<WidgetBody id="cpu" config={instance({ id: "cpu" })} metrics={null} />);
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByText(/Waiting for the first reading/)).toBeInTheDocument();
  });

  it("computes memory as a percentage of the real total", () => {
    render(<WidgetBody id="ram" config={instance({ id: "ram" })} metrics={metrics()} />);
    expect(screen.getByText("50")).toBeInTheDocument();
    expect(screen.getByText("8.0 GB of 16.0 GB")).toBeInTheDocument();
  });

  it("shows battery state including time remaining", () => {
    render(
      <WidgetBody id="battery" config={instance({ id: "battery" })} metrics={metrics()} />,
    );
    expect(screen.getByText("72")).toBeInTheDocument();
    expect(screen.getByText(/about 2h 25m left/)).toBeInTheDocument();
  });

  it("says charging rather than showing a misleading time estimate", () => {
    render(
      <WidgetBody
        id="battery"
        config={instance({ id: "battery" })}
        metrics={metrics({ battery_charging: true })}
      />,
    );
    expect(screen.getByText("charging")).toBeInTheDocument();
    expect(screen.queryByText(/left/)).not.toBeInTheDocument();
  });

  it("reports no battery as absence, not as 0%", () => {
    render(
      <WidgetBody
        id="battery"
        config={instance({ id: "battery" })}
        metrics={metrics({
          battery_percent: null,
          unavailable: [{ metric: "battery", reason: "no battery is present" }],
        })}
      />,
    );
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByText("no battery is present")).toBeInTheDocument();
  });

  it("formats network rates at a readable scale", () => {
    render(
      <WidgetBody id="network" config={instance({ id: "network" })} metrics={metrics()} />,
    );
    expect(screen.getByText("1.5 MB/s")).toBeInTheDocument();
    expect(screen.getByText("64.0 KB/s")).toBeInTheDocument();
    expect(screen.getByText("Wi-Fi")).toBeInTheDocument();
  });

  it("shows uptime in the largest useful unit", () => {
    render(
      <WidgetBody id="uptime" config={instance({ id: "uptime" })} metrics={metrics()} />,
    );
    expect(screen.getByText("3d 5h")).toBeInTheDocument();
  });

  it("shows free space per drive", () => {
    render(
      <WidgetBody id="storage" config={instance({ id: "storage" })} metrics={metrics()} />,
    );
    expect(screen.getByText("C:")).toBeInTheDocument();
    expect(screen.getByText("120.0 GB free")).toBeInTheDocument();
  });

  it("admits the media widget is not implemented rather than showing nothing playing", () => {
    // Saying "nothing is playing" when we cannot read the session would be
    // indistinguishable from a wrong answer.
    render(
      <WidgetBody
        id="media"
        config={instance({ id: "media" })}
        metrics={metrics({
          unavailable: [
            { metric: "media", reason: "Media session reading is not implemented." },
          ],
        })}
      />,
    );
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
    expect(screen.getByText("Media session reading is not implemented.")).toBeInTheDocument();
  });

  it("says the weather widget makes no network calls until configured", () => {
    render(
      <WidgetBody id="weather" config={instance({ id: "weather" })} metrics={null} />,
    );
    expect(screen.getByText("No provider configured")).toBeInTheDocument();
    expect(screen.getByText(/makes no network requests/)).toBeInTheDocument();
  });
});

describe("Time widgets", () => {
  it("renders the clock without needing any metrics", () => {
    render(<WidgetBody id="clock" config={instance()} metrics={null} />);
    // Locale-independent: the time is rendered as digits with a separator.
    expect(screen.getByText(/\d{1,2}[:.]\d{2}/)).toBeInTheDocument();
  });

  it("marks today in the calendar", () => {
    render(
      <WidgetBody id="calendar" config={instance({ id: "calendar" })} metrics={null} />,
    );
    const today = String(new Date().getDate());
    const cells = screen.getAllByText(today);
    expect(cells.length).toBeGreaterThan(0);
  });

  it("renders an unknown widget id as unavailable rather than blank", () => {
    render(<WidgetBody id="teapot" config={instance({ id: "teapot" })} metrics={null} />);
    expect(screen.getByText("Unavailable")).toBeInTheDocument();
  });
});
