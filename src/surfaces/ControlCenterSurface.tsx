/**
 * Control Center — the quick-settings panel on a global shortcut.
 *
 * Controls are rendered from the backend catalogue, and one that opens a
 * Windows page is visibly labelled as such. The distinction is kept in the UI
 * because a user pressing "Volume" and landing in Windows settings should
 * have expected that, not be surprised by it.
 */

import { useEffect, useState } from "react";
import * as api from "@/services/api";
import { applyTheme } from "@/stores/app";
import { errorMessage, restorationSummary } from "@/types/api";
import type { Control, Profile } from "@/types/api";
import {
  Badge,
  Button,
  ExternalIcon,
  RestoreIcon,
  Spinner,
  cx,
} from "@/components/ui";

export function ControlCenterSurface() {
  const [controls, setControls] = useState<Control[]>([]);
  const [enabled, setEnabled] = useState<string[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; bad: boolean } | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const data = await api.bootstrap();
        applyTheme(data.settings);
        setControls(data.controls);
        setProfiles(data.profiles);
        setActive(data.active_profile);
        const cc = data.modules.find((m) => m.id === "control_center");
        setEnabled((cc?.config.controls as string[]) ?? data.controls.map((c) => c.id));
      } catch (e) {
        setMessage({ text: errorMessage(e), bad: true });
      }
    })();
  }, []);

  // Escape closes, matching every other transient panel on Windows.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") void api.closeControlCenter();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const run = async (control: Control) => {
    setBusy(control.id);
    try {
      if (control.id === "restore") {
        const report = await api.restoreOriginalDesktop();
        setMessage({ text: restorationSummary(report), bad: !report.verified });
      } else {
        const result = await api.runControl(control.id);
        if (result) setMessage({ text: result, bad: false });
      }
    } catch (e) {
      setMessage({ text: errorMessage(e), bad: true });
    } finally {
      setBusy(null);
      window.setTimeout(() => setMessage(null), 4000);
    }
  };

  const shown = controls.filter((c) => enabled.includes(c.id));
  const actions = shown.filter((c) => c.kind.kind === "action" && c.id !== "profile");
  const shortcuts = shown.filter((c) => c.kind.kind === "opens_windows");
  const showProfiles = enabled.includes("profile");

  return (
    <div className="h-full w-full p-1">
      <div
        className="glass flex h-full w-full flex-col overflow-hidden border border-[var(--border-default)] shadow-[var(--shadow-lg)]"
        style={{ borderRadius: 16 }}
      >
        <header className="flex items-center justify-between border-b border-[var(--border-subtle)] px-4 py-3">
          <span className="text-[12px] font-semibold tracking-[0.12em]">
            CONTROL CENTER
          </span>
          <Button size="sm" variant="ghost" onClick={() => void api.closeControlCenter()}>
            Esc
          </Button>
        </header>

        <div className="flex-1 overflow-y-auto px-4 py-3.5">
          {showProfiles && profiles.length > 0 && (
            <section className="mb-4">
              <p className="mb-1.5 text-[10px] uppercase tracking-[0.1em] text-[var(--text-tertiary)]">
                Profile
              </p>
              <div className="grid grid-cols-2 gap-1.5">
                {profiles.slice(0, 6).map((p) => (
                  <button
                    key={p.id}
                    disabled={busy !== null}
                    onClick={async () => {
                      setBusy(p.id);
                      try {
                        const report = await api.switchProfile(p.id);
                        setActive(p.id);
                        setMessage({
                          text: report.rolled_back
                            ? `Could not apply ${report.to_name}`
                            : `Switched to ${report.to_name}`,
                          bad: report.rolled_back,
                        });
                      } catch (e) {
                        setMessage({ text: errorMessage(e), bad: true });
                      } finally {
                        setBusy(null);
                        window.setTimeout(() => setMessage(null), 4000);
                      }
                    }}
                    className={cx(
                      "truncate rounded-[9px] border px-2.5 py-2 text-left text-[11.5px]",
                      "transition-colors disabled:opacity-50",
                      p.id === active
                        ? "border-[var(--accent-500)] bg-[rgb(var(--accent-rgb)/0.12)] text-[var(--accent-400)]"
                        : "border-[var(--border-default)] hover:bg-[var(--bg-hover)]",
                    )}
                  >
                    {busy === p.id ? <Spinner size={11} /> : p.name}
                  </button>
                ))}
              </div>
            </section>
          )}

          {actions.length > 0 && (
            <section className="mb-4">
              <p className="mb-1.5 text-[10px] uppercase tracking-[0.1em] text-[var(--text-tertiary)]">
                Actions
              </p>
              <div className="flex flex-col gap-1">
                {actions.map((c) => (
                  <ControlRow
                    key={c.id}
                    control={c}
                    busy={busy === c.id}
                    disabled={busy !== null}
                    onRun={() => void run(c)}
                  />
                ))}
              </div>
            </section>
          )}

          {shortcuts.length > 0 && (
            <section>
              <p className="mb-1.5 text-[10px] uppercase tracking-[0.1em] text-[var(--text-tertiary)]">
                Windows settings
              </p>
              <div className="flex flex-col gap-1">
                {shortcuts.map((c) => (
                  <ControlRow
                    key={c.id}
                    control={c}
                    busy={busy === c.id}
                    disabled={busy !== null}
                    onRun={() => void run(c)}
                  />
                ))}
              </div>
            </section>
          )}
        </div>

        {message && (
          <div
            className="border-t border-[var(--border-subtle)] px-4 py-2.5 text-[11.5px]"
            style={{
              color: message.bad ? "var(--danger)" : "var(--text-secondary)",
              background: message.bad ? "var(--danger-dim)" : "transparent",
            }}
          >
            {message.text}
          </div>
        )}
      </div>
    </div>
  );
}

function ControlRow({
  control,
  busy,
  disabled,
  onRun,
}: {
  control: Control;
  busy: boolean;
  disabled: boolean;
  onRun: () => void;
}) {
  const opensWindows = control.kind.kind === "opens_windows";
  const destructive = control.id === "restore";

  return (
    <button
      onClick={onRun}
      disabled={disabled}
      title={control.description}
      className={cx(
        "flex items-center gap-2.5 rounded-[9px] px-3 py-2 text-left transition-colors",
        "hover:bg-[var(--bg-hover)] disabled:opacity-50",
      )}
    >
      {busy ? (
        <Spinner size={13} />
      ) : destructive ? (
        <RestoreIcon size={14} className="shrink-0 text-[var(--accent-400)]" />
      ) : opensWindows ? (
        <ExternalIcon size={13} className="shrink-0 text-[var(--text-tertiary)]" />
      ) : (
        <span className="w-[13px] shrink-0" />
      )}
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[12px]">{control.label}</span>
      </span>
      {opensWindows && <Badge tone="neutral">Opens Windows</Badge>}
    </button>
  );
}
