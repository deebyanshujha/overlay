/**
 * Integrations — real detection, honest capabilities.
 *
 * Every row shows what OVERLAY can and cannot do with the build that is
 * actually installed, including the things it cannot. An integration page
 * that lists only successes teaches the user nothing and is wrong the moment
 * something is missing.
 */

import { useEffect, useState } from "react";
import { useApp } from "@/stores/app";
import * as api from "@/services/api";
import { errorMessage } from "@/types/api";
import type { IntegrationStatus, WallpaperInfo } from "@/types/api";
import {
  AlertIcon,
  Badge,
  Button,
  Card,
  CheckIcon,
  CloseIcon,
  Dot,
  ExternalIcon,
  MonitorIcon,
  SectionTitle,
  Tooltip,
  cx,
  useToast,
} from "@/components/ui";

const DETECTION_LABEL: Record<string, string> = {
  uninstall_registry: "Found in the Windows uninstall registry",
  known_path: "Found at its usual install location",
  data_directory_only: "Only its data folder was found",
  not_found: "Not found",
};

export function IntegrationsPage() {
  const { integrations, environment, capabilities, refreshIntegrations } = useApp();
  const toast = useToast();
  const [wallpaper, setWallpaper] = useState<WallpaperInfo | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    void api.currentWallpaper().then(setWallpaper).catch(() => undefined);
  }, [integrations]);

  return (
    <div className="mx-auto flex max-w-[860px] flex-col gap-7">
      <header className="flex items-start justify-between gap-6">
        <div>
          <h1 className="text-[22px] font-semibold tracking-tight">Integrations</h1>
          <p className="mt-1 max-w-[62ch] text-[13px] leading-relaxed text-[var(--text-secondary)]">
            OVERLAY works alongside your other desktop tools. It reads their
            state to show it here and can start them, but never rewrites their
            settings or closes them for you.
          </p>
        </div>
        <Button
          variant="secondary"
          loading={refreshing}
          onClick={async () => {
            setRefreshing(true);
            try {
              await refreshIntegrations();
              toast.push("Re-checked", "ok");
            } finally {
              setRefreshing(false);
            }
          }}
        >
          Re-check
        </Button>
      </header>

      {integrations.map((i) => (
        <IntegrationCard
          key={i.id}
          integration={i}
          wallpaper={i.id === "wallflow" ? wallpaper : null}
        />
      ))}

      <section>
        <SectionTitle
          title="Windows capabilities"
          description="What this machine lets OVERLAY do."
        />
        <Card className="divide-y divide-[var(--border-subtle)]">
          <CapabilityRow
            label="Windows version"
            value={environment?.windows_version ?? "Unknown"}
            ok
          />
          <CapabilityRow
            label="Explorer shell"
            value={environment?.shell_running ? "Running" : "Not running"}
            ok={Boolean(environment?.shell_running)}
            note={
              environment?.shell_running
                ? undefined
                : "Taskbar and desktop icon customizations need Explorer."
            }
          />
          <CapabilityRow
            label="Taskbar translucency"
            value={environment?.taskbar_accent_supported ? "Supported" : "Unavailable"}
            ok={Boolean(environment?.taskbar_accent_supported)}
            note="Applied to the taskbar window only — no code is injected into Explorer, and the effect disappears if OVERLAY stops."
          />
          <CapabilityRow
            label="Windows 11 taskbar settings"
            value={environment?.is_windows_11 ? "Available" : "Windows 11 only"}
            ok={Boolean(environment?.is_windows_11)}
            note="Alignment, icon size and the widgets button."
          />
          <CapabilityRow
            label="GPU monitoring"
            value={capabilities?.gpu_usage ? "Available" : "Unavailable"}
            ok={Boolean(capabilities?.gpu_usage)}
            note={capabilities?.gpu_detail}
          />
          <CapabilityRow
            label="Battery"
            value={capabilities?.battery ? "Present" : "No battery"}
            ok={Boolean(capabilities?.battery)}
          />
          <CapabilityRow
            label="Now Playing"
            value={capabilities?.media ? "Available" : "Not implemented"}
            ok={Boolean(capabilities?.media)}
            note={capabilities?.media_detail}
          />
          <CapabilityRow
            label="Administrator rights"
            value={environment?.elevated ? "Elevated" : "Not needed"}
            ok
            note="OVERLAY never requires elevation — everything it changes is per-user."
          />
        </Card>
      </section>
    </div>
  );
}

function IntegrationCard({
  integration: i,
  wallpaper,
}: {
  integration: IntegrationStatus;
  wallpaper: WallpaperInfo | null;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  const act = async (fn: () => Promise<string>) => {
    setBusy(true);
    try {
      toast.push(await fn(), "ok");
    } catch (e) {
      toast.push(errorMessage(e), "danger");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className="overflow-hidden">
      <div className="flex items-start gap-4 px-5 py-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="text-[15px] font-semibold">{i.name}</h2>
            <Badge tone={i.running ? "ok" : i.installed ? "neutral" : "warn"}>
              {i.running ? "Running" : i.installed ? "Installed" : "Not found"}
            </Badge>
            {i.version && <Badge tone="neutral">v{i.version}</Badge>}
          </div>
          <p className="mt-1 max-w-[62ch] text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
            {i.description}
          </p>
          <p className="mt-1.5 text-[11px] text-[var(--text-tertiary)]">
            {DETECTION_LABEL[i.detected_via] ?? i.detected_via}
            {i.exe_path ? ` · ${i.exe_path}` : ""}
          </p>
        </div>

        {i.installed && (
          <div className="flex shrink-0 gap-2">
            <Button
              variant="secondary"
              disabled={busy}
              icon={<ExternalIcon size={13} />}
              onClick={() => void act(() => api.launchIntegration(i.id))}
            >
              {i.running ? "Bring to front" : "Start"}
            </Button>
          </div>
        )}
      </div>

      {i.details.length > 0 && (
        <div className="grid grid-cols-2 gap-x-6 gap-y-1.5 border-t border-[var(--border-subtle)] bg-[var(--bg-sunken)] px-5 py-3.5">
          {i.details.map(([k, v]) => (
            <div key={k} className="flex justify-between gap-3">
              <span className="text-[11.5px] text-[var(--text-tertiary)]">{k}</span>
              <span className="truncate text-[11.5px] text-[var(--text-secondary)]">{v}</span>
            </div>
          ))}
        </div>
      )}

      {wallpaper?.available && wallpaper.path && (
        <div className="flex items-center gap-3 border-t border-[var(--border-subtle)] px-5 py-3.5">
          <img
            src={`http://asset.localhost/${encodeURIComponent(wallpaper.path)}`}
            alt=""
            className="h-11 w-[72px] rounded-[6px] border border-[var(--border-subtle)] object-cover"
            onError={(e) => {
              e.currentTarget.style.display = "none";
            }}
          />
          <div className="min-w-0">
            <p className="text-[12px]">
              {wallpaper.topic}
              {wallpaper.author ? ` — ${wallpaper.author}` : ""}
            </p>
            <p className="text-[11px] text-[var(--text-tertiary)]">
              {wallpaper.width}×{wallpaper.height} · from {wallpaper.provider}
            </p>
          </div>
        </div>
      )}

      <div className="border-t border-[var(--border-subtle)] px-5 py-3.5">
        <p className="mb-2 text-[11px] uppercase tracking-[0.08em] text-[var(--text-tertiary)]">
          What OVERLAY can do
        </p>
        <div className="grid grid-cols-2 gap-x-6 gap-y-1.5">
          {i.capabilities.map((c) => (
            <Tooltip key={c.id} label={c.detail} side="top">
              <span className="flex items-center gap-2">
                {c.supported ? (
                  <CheckIcon size={12} className="shrink-0 text-[var(--ok)]" />
                ) : (
                  <CloseIcon size={12} className="shrink-0 text-[var(--text-tertiary)]" />
                )}
                <span
                  className={cx(
                    "text-[12px]",
                    c.supported
                      ? "text-[var(--text-secondary)]"
                      : "text-[var(--text-tertiary)] line-through decoration-[var(--border-strong)]",
                  )}
                >
                  {c.label}
                </span>
              </span>
            </Tooltip>
          ))}
        </div>
      </div>

      {i.limitations.length > 0 && (
        <div className="border-t border-[var(--border-subtle)] bg-[var(--bg-sunken)] px-5 py-3.5">
          <p className="mb-1.5 text-[11px] uppercase tracking-[0.08em] text-[var(--text-tertiary)]">
            Limitations
          </p>
          <ul className="flex flex-col gap-1.5">
            {i.limitations.map((l, idx) => (
              <li key={idx} className="flex gap-2">
                <AlertIcon size={12} className="mt-[3px] shrink-0 text-[var(--warn)]" />
                <span className="text-[11.5px] leading-relaxed text-[var(--text-secondary)]">
                  {l}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </Card>
  );
}

function CapabilityRow({
  label,
  value,
  ok,
  note,
}: {
  label: string;
  value: string;
  ok: boolean;
  note?: string;
}) {
  return (
    <div className="px-5 py-3">
      <div className="flex items-center justify-between gap-4">
        <span className="flex items-center gap-2 text-[12.5px]">
          <MonitorIcon size={13} className="text-[var(--text-tertiary)]" />
          {label}
        </span>
        <span className="flex items-center gap-2">
          <Dot tone={ok ? "ok" : "neutral"} />
          <span className="text-[12px] text-[var(--text-secondary)]">{value}</span>
        </span>
      </div>
      {note && (
        <p className="ml-[21px] mt-1 max-w-[70ch] text-[11px] leading-relaxed text-[var(--text-tertiary)]">
          {note}
        </p>
      )}
    </div>
  );
}
