/**
 * Settings — general, appearance, startup, recovery and diagnostics.
 *
 * Recovery is a first-class section rather than an afterthought: it shows
 * exactly what OVERLAY currently owns, the full change journal, and the
 * command that works when this window does not.
 */

import { useEffect, useState } from "react";
import { useApp } from "@/stores/app";
import * as api from "@/services/api";
import { errorMessage } from "@/types/api";
import type {
  ChangeRecord,
  Diagnostics,
  OwnedSummary,
  RecoveryStatus,
} from "@/types/api";
import {
  AlertIcon,
  Badge,
  Button,
  Card,
  Dot,
  Field,
  SectionTitle,
  Segmented,
  Select,
  TextInput,
  Toggle,
  Tooltip,
  cx,
  useToast,
} from "@/components/ui";

type Tab = "general" | "appearance" | "recovery" | "diagnostics";

export function SettingsPage() {
  const [tab, setTab] = useState<Tab>("general");

  return (
    <div className="mx-auto flex max-w-[860px] flex-col gap-6">
      <header>
        <h1 className="text-[22px] font-semibold tracking-tight">Settings</h1>
      </header>

      <Segmented
        value={tab}
        onChange={setTab}
        options={[
          { value: "general", label: "General" },
          { value: "appearance", label: "Appearance" },
          { value: "recovery", label: "Recovery" },
          { value: "diagnostics", label: "Diagnostics" },
        ]}
      />

      <div key={tab} className="animate-rise">
        {tab === "general" && <GeneralTab />}
        {tab === "appearance" && <AppearanceTab />}
        {tab === "recovery" && <RecoveryTab />}
        {tab === "diagnostics" && <DiagnosticsTab />}
      </div>
    </div>
  );
}

// --------------------------------------------------------------------------

function GeneralTab() {
  const { settings, setSetting } = useApp();
  const toast = useToast();
  const [hotkey, setHotkey] = useState(settings?.control_center_hotkey ?? "");

  useEffect(() => {
    if (settings) setHotkey(settings.control_center_hotkey);
  }, [settings]);

  if (!settings) return null;

  const update = async (key: string, value: string) => {
    try {
      await setSetting(key, value);
    } catch (e) {
      toast.push("Could not save that setting", "danger", errorMessage(e));
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <section>
        <SectionTitle title="Startup" />
        <Card className="divide-y divide-[var(--border-subtle)]">
          <SettingRow
            label="Start OVERLAY with Windows"
            hint="Starts in the tray without opening the window."
          >
            <Toggle
              checked={settings.start_with_windows}
              onChange={(v) => void update("start_with_windows", String(v))}
              srLabel="Start with Windows"
            />
          </SettingRow>
          <SettingRow
            label="Apply the startup profile at launch"
            hint="When off, OVERLAY starts without changing anything, whatever profile was last active. Choose which profile on the Profiles page."
          >
            <Toggle
              checked={settings.restore_profile_on_start}
              onChange={(v) => void update("restore_profile_on_start", String(v))}
              srLabel="Apply startup profile"
            />
          </SettingRow>
        </Card>
      </section>

      <section>
        <SectionTitle title="Keyboard" />
        <Card className="p-4">
          <Field
            label="Control Center shortcut"
            hint="QuickDock's shortcut is refused, so both stay usable."
          >
            <div className="flex gap-2">
              <TextInput
                value={hotkey}
                onChange={(e) => setHotkey(e.target.value)}
                placeholder="Alt+Shift+O"
              />
              <Button
                variant="secondary"
                onClick={async () => {
                  try {
                    await setSetting("control_center_hotkey", hotkey);
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
        </Card>
      </section>

      <section>
        <SectionTitle title="Performance" />
        <Card className="divide-y divide-[var(--border-subtle)]">
          <SettingRow
            label="Reduce motion"
            hint="Removes animations throughout OVERLAY and its surfaces."
          >
            <Toggle
              checked={settings.reduce_motion}
              onChange={(v) => void update("reduce_motion", String(v))}
              srLabel="Reduce motion"
            />
          </SettingRow>
          <div className="px-4 py-3">
            <p className="text-[11.5px] leading-relaxed text-[var(--text-tertiary)]">
              Each widget has its own refresh interval, set per widget under
              Customize → Desktop Widgets. Widgets that are switched off poll
              nothing at all, because their window does not exist.
            </p>
          </div>
        </Card>
      </section>

      <section>
        <SectionTitle title="Data" />
        <Card className="p-4">
          <div className="flex items-center justify-between gap-4">
            <div>
              <p className="text-[12.5px]">Application data folder</p>
              <p className="mt-0.5 text-[11.5px] text-[var(--text-tertiary)]">
                Database, backups and exported profiles. Nothing leaves this
                machine — OVERLAY has no telemetry and makes no network calls.
              </p>
            </div>
            <Button variant="secondary" onClick={() => void api.openDataFolder()}>
              Open folder
            </Button>
          </div>
        </Card>
      </section>
    </div>
  );
}

function AppearanceTab() {
  const { settings, setSetting } = useApp();
  if (!settings) return null;

  return (
    <div className="flex flex-col gap-6">
      <Card className="p-5">
        <p className="mb-3 text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
          This changes how OVERLAY itself looks. It is completely separate from
          your Windows desktop — you can run a futuristic studio over a stock
          Windows desktop, or the reverse.
        </p>

        <div className="grid grid-cols-2 gap-6">
          <Field label="Theme">
            <Segmented
              value={settings.theme}
              onChange={(v) => void setSetting("theme", v)}
              options={[
                { value: "minimal", label: "Minimal" },
                { value: "futuristic", label: "Futuristic" },
              ]}
            />
          </Field>
          <Field label="Accent">
            <Select
              value={settings.accent}
              onChange={(v) => void setSetting("accent", v)}
              options={[
                { value: "ice", label: "Ice" },
                { value: "violet", label: "Violet" },
                { value: "emerald", label: "Emerald" },
                { value: "amber", label: "Amber" },
                { value: "rose", label: "Rose" },
              ]}
            />
          </Field>
        </div>

        <div className="mt-5 flex gap-2">
          {(["ice", "violet", "emerald", "amber", "rose"] as const).map((a) => (
            <button
              key={a}
              onClick={() => void setSetting("accent", a)}
              aria-label={a}
              data-accent={a}
              className={cx(
                "h-8 flex-1 rounded-[8px] border-2 transition-transform hover:scale-[1.03]",
                settings.accent === a
                  ? "border-[var(--text-primary)]"
                  : "border-transparent",
              )}
              style={{ background: "var(--accent-500)" }}
            />
          ))}
        </div>
      </Card>
    </div>
  );
}

// --------------------------------------------------------------------------

function RecoveryTab() {
  const toast = useToast();
  const [status, setStatus] = useState<RecoveryStatus | null>(null);
  const [owned, setOwned] = useState<OwnedSummary[]>([]);
  const [journal, setJournal] = useState<ChangeRecord[]>([]);
  const [showJournal, setShowJournal] = useState(false);

  const reload = () => {
    void api.recoveryStatus().then(setStatus).catch(() => undefined);
    void api.ownedSettings().then(setOwned).catch(() => undefined);
    void api.changeJournal(100).then(setJournal).catch(() => undefined);
  };

  useEffect(reload, []);

  return (
    <div className="flex flex-col gap-6">
      {status && (status.unsettled_changes > 0 || status.switch_in_flight) && (
        <Card
          className="p-4"
          style={{ borderColor: "var(--warn)", background: "var(--warn-dim)" }}
        >
          <div className="flex items-start gap-3">
            <AlertIcon size={15} className="mt-[2px] shrink-0 text-[var(--warn)]" />
            <div className="flex-1">
              <p className="text-[12.5px] font-medium">
                {status.unsettled_changes} change
                {status.unsettled_changes === 1 ? "" : "s"} were interrupted
              </p>
              <p className="mt-0.5 text-[11.5px] leading-relaxed text-[var(--text-secondary)]">
                OVERLAY can check each one against your desktop and record what
                actually happened.
              </p>
            </div>
            <Button
              size="sm"
              variant="secondary"
              onClick={async () => {
                try {
                  const n = await api.reconcileInterrupted();
                  toast.push(`${n} change(s) reconciled`, "ok");
                  reload();
                } catch (e) {
                  toast.push("Could not reconcile", "danger", errorMessage(e));
                }
              }}
            >
              Reconcile now
            </Button>
          </div>
        </Card>
      )}

      <section>
        <SectionTitle
          title="Settings OVERLAY currently owns"
          description="Exactly what a restore would put back — nothing else is touched."
        />
        <Card>
          {owned.length === 0 ? (
            <p className="px-4 py-6 text-center text-[12.5px] text-[var(--text-tertiary)]">
              OVERLAY has not changed any Windows setting.
            </p>
          ) : (
            <ul className="divide-y divide-[var(--border-subtle)]">
              {owned.map((o) => (
                <li key={o.target_id} className="flex items-center gap-4 px-4 py-3">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[12.5px]">{o.label}</p>
                    <p className="text-[11px] text-[var(--text-tertiary)]">
                      changed by {o.module_id}
                    </p>
                  </div>
                  <div className="shrink-0 text-right">
                    <p className="tabular text-[11.5px] text-[var(--text-secondary)]">
                      now {o.applied}
                    </p>
                    <p className="tabular text-[11px] text-[var(--text-tertiary)]">
                      was {o.original}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={async () => {
                      try {
                        const report = await api.restoreModule(o.module_id);
                        toast.push(
                          report.verified
                            ? "Setting restored"
                            : "Restore finished with notes",
                          report.verified ? "ok" : "warn",
                        );
                        reload();
                      } catch (e) {
                        toast.push("Could not restore", "danger", errorMessage(e));
                      }
                    }}
                  >
                    Undo
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </section>

      <section>
        <SectionTitle
          title="If OVERLAY will not open"
          description="This works with no interface at all."
        />
        <Card className="p-4">
          <p className="text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
            Run this from a terminal. It restores every tracked setting and
            prints a report, using only the database — it does not need the
            window, the frontend or WebView2.
          </p>
          <div className="mt-2.5 flex items-center gap-2">
            <code className="flex-1 overflow-x-auto rounded-[8px] border border-[var(--border-subtle)] bg-[var(--bg-sunken)] px-3 py-2 font-[var(--font-mono)] text-[11.5px]">
              {status?.recovery_command ?? "overlay.exe --restore-original"}
            </code>
            <Tooltip label="Copy">
              <Button
                variant="secondary"
                onClick={() => {
                  void navigator.clipboard.writeText(
                    status?.recovery_command ?? "overlay.exe --restore-original",
                  );
                  toast.push("Copied", "ok");
                }}
              >
                Copy
              </Button>
            </Tooltip>
          </div>
          <p className="mt-2 text-[11px] text-[var(--text-tertiary)]">
            <code className="font-[var(--font-mono)]">--status</code> lists what
            OVERLAY owns without changing anything.
          </p>
        </Card>
      </section>

      <section>
        <SectionTitle
          title="Backups"
          description="Taken automatically before each restore and profile switch."
        />
        <Card>
          {!status || status.backups.length === 0 ? (
            <p className="px-4 py-6 text-center text-[12.5px] text-[var(--text-tertiary)]">
              No backups yet.
            </p>
          ) : (
            <ul className="divide-y divide-[var(--border-subtle)]">
              {status.backups.slice(0, 8).map((b) => (
                <li key={b.name} className="flex items-center justify-between px-4 py-2.5">
                  <span className="truncate font-[var(--font-mono)] text-[11.5px]">
                    {b.name}
                  </span>
                  <span className="shrink-0 text-[11px] text-[var(--text-tertiary)]">
                    {(b.bytes / 1024).toFixed(0)} KB ·{" "}
                    {new Date(b.modified * 1000).toLocaleString()}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </section>

      <section>
        <SectionTitle
          title="Change journal"
          description="Every system change OVERLAY has made, with its before and after."
          action={
            <Button size="sm" variant="ghost" onClick={() => setShowJournal((s) => !s)}>
              {showJournal ? "Hide" : `Show ${journal.length}`}
            </Button>
          }
        />
        {showJournal && (
          <Card className="max-h-[320px] overflow-y-auto">
            <ul className="divide-y divide-[var(--border-subtle)]">
              {journal.map((c) => (
                <li key={c.id} className="flex items-center gap-3 px-4 py-2.5">
                  <Dot
                    tone={
                      c.status === "applied"
                        ? "accent"
                        : c.status === "reverted"
                          ? "ok"
                          : c.status === "failed"
                            ? "danger"
                            : "warn"
                    }
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-[12px]">{c.label}</p>
                    <p className="tabular text-[11px] text-[var(--text-tertiary)]">
                      {c.before} → {c.after}
                      {c.detail ? ` · ${c.detail}` : ""}
                    </p>
                  </div>
                  <Badge
                    tone={
                      c.status === "failed"
                        ? "danger"
                        : c.status === "pending"
                          ? "warn"
                          : "neutral"
                    }
                  >
                    {c.status}
                  </Badge>
                  <span className="shrink-0 text-[10.5px] text-[var(--text-tertiary)]">
                    {new Date(c.started_at * 1000).toLocaleTimeString()}
                  </span>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </section>
    </div>
  );
}

// --------------------------------------------------------------------------

function DiagnosticsTab() {
  const [data, setData] = useState<Diagnostics | null>(null);
  const [memory, setMemory] = useState<number | null>(null);

  useEffect(() => {
    const read = () =>
      void api
        .diagnostics()
        .then((d) => {
          setData(d);
          setMemory(d.memory_bytes);
        })
        .catch(() => undefined);
    read();
    // Live, because the point of this page is seeing OVERLAY's actual cost.
    const timer = window.setInterval(read, 3000);
    return () => window.clearInterval(timer);
  }, []);

  if (!data) return null;

  return (
    <div className="flex flex-col gap-6">
      <section>
        <SectionTitle
          title="Resource use"
          description="Measured from this process, not estimated."
        />
        <div className="grid grid-cols-3 gap-3">
          <Metric
            label="Memory (working set)"
            value={memory ? `${(memory / 1024 / 1024).toFixed(1)} MB` : "—"}
          />
          <Metric label="Open surfaces" value={String(data.open_surfaces.length)} />
          <Metric
            label="Database"
            value={`${(data.database_bytes / 1024).toFixed(0)} KB`}
          />
        </div>
        {data.open_surfaces.length > 0 && (
          <p className="mt-2 text-[11px] text-[var(--text-tertiary)]">
            {data.open_surfaces.join(", ")}
          </p>
        )}
      </section>

      <section>
        <SectionTitle title="Build" />
        <Card className="divide-y divide-[var(--border-subtle)]">
          <InfoRow label="OVERLAY version" value={data.app_version} />
          <InfoRow label="Database schema" value={`v${data.schema_version}`} />
          <InfoRow label="Windows" value={data.environment.windows_version} />
          <InfoRow
            label="Displays"
            value={data.environment.monitors
              .map((m) => `${m.width}×${m.height} @${Math.round(m.scale * 100)}%`)
              .join(", ")}
          />
          <InfoRow label="Data folder" value={data.data_dir} mono />
          <InfoRow
            label="Settings owned"
            value={String(data.owned_settings)}
          />
          <InfoRow
            label="Unsettled changes"
            value={String(data.unsettled_changes)}
          />
        </Card>
      </section>

      <section>
        <SectionTitle title="Sensors" />
        <Card className="divide-y divide-[var(--border-subtle)]">
          <InfoRow
            label="GPU"
            value={data.monitoring.gpu_usage ? "Available" : "Unavailable"}
            note={data.monitoring.gpu_detail}
          />
          <InfoRow
            label="Battery"
            value={data.monitoring.battery ? "Present" : "Not present"}
          />
          <InfoRow
            label="Media session"
            value={data.monitoring.media ? "Available" : "Not implemented"}
            note={data.monitoring.media_detail}
          />
        </Card>
      </section>
    </div>
  );
}

// --------------------------------------------------------------------------

function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 px-4 py-3.5">
      <div className="min-w-0">
        <p className="text-[12.5px]">{label}</p>
        {hint && (
          <p className="mt-0.5 max-w-[62ch] text-[11.5px] leading-relaxed text-[var(--text-tertiary)]">
            {hint}
          </p>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function InfoRow({
  label,
  value,
  mono,
  note,
}: {
  label: string;
  value: string;
  mono?: boolean;
  note?: string;
}) {
  return (
    <div className="px-4 py-2.5">
      <div className="flex items-baseline justify-between gap-4">
        <span className="shrink-0 text-[12px] text-[var(--text-tertiary)]">{label}</span>
        <span
          className={cx(
            "truncate text-right text-[12px] text-[var(--text-secondary)]",
            mono && "font-[var(--font-mono)] text-[11px]",
          )}
        >
          {value}
        </span>
      </div>
      {note && (
        <p className="mt-1 text-[11px] leading-relaxed text-[var(--text-tertiary)]">
          {note}
        </p>
      )}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <Card className="px-4 py-3.5">
      <p className="tabular text-[20px] font-semibold tracking-tight">{value}</p>
      <p className="mt-0.5 text-[11px] text-[var(--text-tertiary)]">{label}</p>
    </Card>
  );
}
