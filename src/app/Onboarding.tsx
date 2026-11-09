/**
 * First launch.
 *
 * The one rule this screen exists to honour: **nothing is enabled for you.**
 * It welcomes, explains the philosophy, shows what was actually detected on
 * this machine, and then hands over. A customiser that configures itself on
 * first run is exactly the thing OVERLAY is not.
 */

import { useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useApp } from "@/stores/app";
import {
  Badge,
  Button,
  CheckIcon,
  CloseIcon,
  Dot,
  MonitorIcon,
  SparkIcon,
  cx,
} from "@/components/ui";

const STEPS = ["Welcome", "How it works", "Your machine"] as const;

export function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const { environment, integrations, completeOnboarding, capabilities } = useApp();

  const finish = async (goToCustomize: boolean) => {
    await completeOnboarding();
    if (goToCustomize) onDone();
  };

  return (
    <div className="relative z-10 flex h-full flex-col">
      {/* The window is frameless, so even the welcome screen needs somewhere
          to grab and a way out — otherwise first launch traps the window
          where it opened. */}
      <header className="drag flex h-11 shrink-0 items-center justify-between pl-5 pr-2">
        <span className="text-[13px] font-semibold tracking-[0.18em] text-[var(--text-tertiary)]">
          OVERLAY
        </span>
        <button
          aria-label="Close to tray"
          title="Close to tray"
          onClick={() => void getCurrentWindow().hide()}
          className="no-drag flex h-8 w-11 items-center justify-center rounded-[7px] text-[var(--text-tertiary)] transition-colors hover:bg-[var(--danger)] hover:text-white"
        >
          <CloseIcon size={14} />
        </button>
      </header>

      <div className="flex flex-1 items-center justify-center overflow-y-auto px-10 pb-10">
      <div className="w-full max-w-[640px]">
        <ol className="mb-7 flex items-center gap-2" aria-label="Setup progress">
          {STEPS.map((label, i) => (
            <li key={label} className="flex flex-1 items-center gap-2">
              <span
                className={cx(
                  "flex h-6 w-6 shrink-0 items-center justify-center rounded-full",
                  "text-[11px] font-semibold transition-colors",
                  i <= step
                    ? "bg-[var(--accent-500)] text-[var(--text-inverse)]"
                    : "bg-[var(--bg-active)] text-[var(--text-tertiary)]",
                )}
              >
                {i < step ? <CheckIcon size={12} /> : i + 1}
              </span>
              <span
                className={cx(
                  "text-[12px]",
                  i <= step ? "text-[var(--text-primary)]" : "text-[var(--text-tertiary)]",
                )}
              >
                {label}
              </span>
              {i < STEPS.length - 1 && (
                <span className="h-px flex-1 bg-[var(--border-subtle)]" />
              )}
            </li>
          ))}
        </ol>

        <div className="card animate-rise p-7" key={step}>
          {step === 0 && (
            <>
              <SparkIcon size={22} className="mb-3 text-[var(--accent-400)]" />
              <h1 className="text-[20px] font-semibold tracking-tight">
                Welcome to OVERLAY
              </h1>
              <p className="mt-2 text-[14px] text-[var(--text-secondary)]">
                Your desktop. Your rules.
              </p>
              <p className="mt-4 text-[13px] leading-relaxed text-[var(--text-secondary)]">
                OVERLAY is a studio for customising your Windows desktop —
                widgets, a floating dock, taskbar appearance, a system HUD and
                more. You choose what to switch on, one piece at a time.
              </p>
              <p className="mt-3 text-[13px] leading-relaxed text-[var(--text-secondary)]">
                Right now, nothing has changed. Your desktop is exactly as you
                left it, and it will stay that way until you say otherwise.
              </p>
            </>
          )}

          {step === 1 && (
            <>
              <h1 className="text-[18px] font-semibold tracking-tight">
                Everything is optional. Everything is reversible.
              </h1>
              <ul className="mt-5 flex flex-col gap-4">
                <Principle title="Nothing switches itself on">
                  OVERLAY ships with everything disabled. Enable only what you
                  want; leave the rest alone and your desktop stays untouched.
                </Principle>
                <Principle title="Your original desktop is recorded first">
                  Before changing any Windows setting, OVERLAY reads and stores
                  its current value — including settings that have never been
                  set, which are restored by being cleared again, not by
                  writing a default over them.
                </Principle>
                <Principle title="One button puts it all back">
                  <span className="font-medium text-[var(--text-primary)]">
                    Restore Original Desktop
                  </span>{" "}
                  undoes every change OVERLAY made, and nothing else. It never
                  touches a setting OVERLAY did not change, never deletes a
                  shortcut, and never alters your other applications.
                </Principle>
                <Principle title="It works even if OVERLAY does not">
                  The same restore runs from a terminal with{" "}
                  <code className="font-[var(--font-mono)] text-[12px]">
                    overlay.exe --restore-original
                  </code>
                  , so a broken window can never trap you.
                </Principle>
              </ul>
            </>
          )}

          {step === 2 && (
            <>
              <h1 className="text-[18px] font-semibold tracking-tight">
                What OVERLAY found
              </h1>
              <p className="mt-1.5 text-[12.5px] text-[var(--text-secondary)]">
                Detected on this machine. Nothing here has been modified.
              </p>

              <div className="mt-5 flex flex-col gap-2.5">
                <DetectRow
                  label="Windows"
                  value={environment?.windows_version ?? "Unknown"}
                  ok={Boolean(environment?.shell_running)}
                  note={
                    environment?.shell_running
                      ? undefined
                      : "Explorer is not running — taskbar and icon features are unavailable."
                  }
                />
                <DetectRow
                  label="Displays"
                  value={
                    environment
                      ? environment.monitors
                          .map(
                            (m) =>
                              `${m.width}×${m.height}${m.scale !== 1 ? ` @${Math.round(m.scale * 100)}%` : ""}`,
                          )
                          .join(", ")
                      : "—"
                  }
                  ok={(environment?.monitors.length ?? 0) > 0}
                />
                <DetectRow
                  label="Taskbar translucency"
                  value={
                    environment?.taskbar_accent_supported
                      ? "Supported"
                      : "Not available on this build"
                  }
                  ok={Boolean(environment?.taskbar_accent_supported)}
                />
                <DetectRow
                  label="GPU monitoring"
                  value={capabilities?.gpu_usage ? "Available" : "Not available"}
                  ok={Boolean(capabilities?.gpu_usage)}
                  note={capabilities?.gpu_usage ? undefined : capabilities?.gpu_detail}
                />
                {integrations.map((i) => (
                  <DetectRow
                    key={i.id}
                    label={i.name}
                    value={
                      i.installed
                        ? i.running
                          ? "Installed and running"
                          : "Installed"
                        : "Not installed"
                    }
                    ok={i.installed}
                  />
                ))}
              </div>

              <p className="mt-5 text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
                Explore everything freely — opening pages and reading settings
                changes nothing. A customization only applies when you switch
                it on.
              </p>
            </>
          )}

          <div className="mt-7 flex items-center justify-between">
            <Button
              variant="ghost"
              onClick={() => setStep((s) => s - 1)}
              disabled={step === 0}
            >
              Back
            </Button>
            <div className="flex gap-2">
              {step === STEPS.length - 1 ? (
                <>
                  <Button variant="secondary" onClick={() => void finish(false)}>
                    Take me to the overview
                  </Button>
                  <Button variant="primary" onClick={() => void finish(true)}>
                    Start customising
                  </Button>
                </>
              ) : (
                <Button variant="primary" onClick={() => setStep((s) => s + 1)}>
                  Continue
                </Button>
              )}
            </div>
          </div>
        </div>
      </div>
      </div>
    </div>
  );
}

function Principle({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="mt-[7px]">
        <Dot tone="accent" />
      </span>
      <div>
        <p className="text-[13px] font-medium">{title}</p>
        <p className="mt-0.5 text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
          {children}
        </p>
      </div>
    </li>
  );
}

function DetectRow({
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
    <div className="rounded-[9px] border border-[var(--border-subtle)] bg-[var(--bg-sunken)] px-3.5 py-2.5">
      <div className="flex items-center justify-between gap-4">
        <span className="flex items-center gap-2 text-[12.5px] text-[var(--text-secondary)]">
          <MonitorIcon size={13} className="text-[var(--text-tertiary)]" />
          {label}
        </span>
        <Badge tone={ok ? "ok" : "neutral"}>{value}</Badge>
      </div>
      {note && (
        <p className="mt-1.5 text-[11.5px] leading-snug text-[var(--text-tertiary)]">
          {note}
        </p>
      )}
    </div>
  );
}
