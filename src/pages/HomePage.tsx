/**
 * Home — what is on, what changed, and the way back.
 *
 * The restore control is on this page rather than buried in settings, and it
 * states what it will and will not do before it runs. "Reversible" is only
 * credible if the reversal is visible.
 */

import { useEffect, useState } from "react";
import type { PageId } from "@/app/Studio";
import { useApp } from "@/stores/app";
import * as api from "@/services/api";
import { errorMessage } from "@/types/api";
import type { RestorationReport, RestorePreview, WallpaperInfo } from "@/types/api";
import {
  Badge,
  Button,
  Card,
  CheckIcon,
  Dot,
  ExternalIcon,
  Modal,
  RestoreIcon,
  SectionTitle,
  SparkIcon,
  Spinner,
  cx,
  useToast,
} from "@/components/ui";
import { RestoreDialog } from "@/pages/RestoreDialog";

export function HomePage({ onNavigate }: { onNavigate: (page: PageId) => void }) {
  const {
    modules,
    ownedSettings,
    integrations,
    environment,
    activeProfileObject,
    switchProfile,
    switching,
  } = useApp();
  const toast = useToast();
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [wallpaper, setWallpaper] = useState<WallpaperInfo | null>(null);

  const profile = activeProfileObject();
  const active = modules.filter((m) => m.enabled);
  const systemChanging = active.filter((m) => m.touches_system);

  useEffect(() => {
    void api.currentWallpaper().then(setWallpaper).catch(() => setWallpaper(null));
  }, []);

  return (
    <div className="mx-auto flex max-w-[860px] flex-col gap-7">
      <header>
        <h1 className="text-[22px] font-semibold tracking-tight">
          {profile ? profile.name : "No profile active"}
        </h1>
        <p className="mt-1 text-[13px] text-[var(--text-secondary)]">
          {profile?.description ||
            "Choose a profile, or switch on individual customizations."}
        </p>
      </header>

      {/* ----- Desktop state ----- */}
      <Card className="p-5">
        <div className="flex items-start justify-between gap-6">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Dot tone={ownedSettings === 0 ? "ok" : "accent"} />
              <span className="text-[13px] font-medium">
                {ownedSettings === 0
                  ? "Your Windows desktop is unmodified"
                  : `OVERLAY is managing ${ownedSettings} Windows setting${
                      ownedSettings === 1 ? "" : "s"
                    }`}
              </span>
            </div>
            <p className="mt-1.5 max-w-[56ch] text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
              {ownedSettings === 0
                ? "Nothing OVERLAY does right now touches Windows itself. Widgets and panels are its own windows, and close with it."
                : `Every one of these was recorded before it was changed, and will be put back exactly as it was. ${
                    systemChanging.length > 0
                      ? `Changed by: ${systemChanging.map((m) => m.name).join(", ")}.`
                      : ""
                  }`}
            </p>
          </div>

          <Button
            variant={ownedSettings === 0 ? "secondary" : "primary"}
            icon={<RestoreIcon size={14} />}
            onClick={() => setRestoreOpen(true)}
            className="shrink-0"
          >
            Restore original desktop
          </Button>
        </div>
      </Card>

      {/* ----- Enabled modules ----- */}
      <section>
        <SectionTitle
          title="Enabled customizations"
          description={
            active.length === 0
              ? "Nothing is switched on yet."
              : `${active.length} active`
          }
          action={
            <Button size="sm" variant="ghost" onClick={() => onNavigate("customize")}>
              Customize
            </Button>
          }
        />
        {active.length === 0 ? (
          <Card className="px-5 py-8 text-center">
            <SparkIcon size={20} className="mx-auto mb-2.5 text-[var(--text-tertiary)]" />
            <p className="text-[13px] text-[var(--text-secondary)]">
              Your desktop is untouched.
            </p>
            <p className="mx-auto mt-1 max-w-[46ch] text-[12px] leading-relaxed text-[var(--text-tertiary)]">
              Add a clock, a system HUD, or make the taskbar translucent — each
              one is independent, and each one can be undone on its own.
            </p>
            <Button
              size="sm"
              variant="primary"
              className="mt-4"
              onClick={() => onNavigate("customize")}
            >
              Browse customizations
            </Button>
          </Card>
        ) : (
          <div className="grid grid-cols-2 gap-2.5">
            {active.map((m) => (
              <Card key={m.id} className="flex items-center gap-3 px-4 py-3">
                <CheckIcon size={14} className="shrink-0 text-[var(--ok)]" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12.5px] font-medium">{m.name}</p>
                  <p className="truncate text-[11px] text-[var(--text-tertiary)]">
                    {m.touches_system
                      ? `${m.owned_settings} Windows setting${m.owned_settings === 1 ? "" : "s"}`
                      : "OVERLAY window only"}
                  </p>
                </div>
              </Card>
            ))}
          </div>
        )}
      </section>

      {/* ----- Profiles ----- */}
      <section>
        <SectionTitle
          title="Quick switch"
          description="Apply a saved desktop in one step."
          action={
            <Button size="sm" variant="ghost" onClick={() => onNavigate("profiles")}>
              All profiles
            </Button>
          }
        />
        <QuickProfiles
          onSwitch={async (id) => {
            try {
              const report = await switchProfile(id);
              if (report.rolled_back) {
                toast.push(
                  `Could not apply ${report.to_name} — rolled back`,
                  "danger",
                  report.warnings[0],
                );
              } else if (report.warnings.length > 0) {
                toast.push(`Switched to ${report.to_name}`, "warn", report.warnings[0]);
              } else {
                toast.push(`Switched to ${report.to_name}`, "ok");
              }
            } catch (e) {
              toast.push("Could not switch profile", "danger", errorMessage(e));
            }
          }}
          busy={switching}
        />
      </section>

      {/* ----- Integrations ----- */}
      <section>
        <SectionTitle
          title="Connected applications"
          action={
            <Button size="sm" variant="ghost" onClick={() => onNavigate("integrations")}>
              Manage
            </Button>
          }
        />
        <div className="grid grid-cols-2 gap-2.5">
          {integrations.map((i) => (
            <Card key={i.id} className="px-4 py-3.5">
              <div className="flex items-center justify-between gap-3">
                <span className="text-[12.5px] font-medium">{i.name}</span>
                <Badge tone={i.running ? "ok" : i.installed ? "neutral" : "warn"}>
                  {i.running ? "Running" : i.installed ? "Installed" : "Not found"}
                </Badge>
              </div>
              {i.id === "wallflow" && wallpaper?.available && wallpaper.topic && (
                <p className="mt-1.5 truncate text-[11.5px] text-[var(--text-tertiary)]">
                  Wallpaper: {wallpaper.topic}
                  {wallpaper.author ? ` — ${wallpaper.author}` : ""}
                </p>
              )}
              {i.installed && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="-ml-2.5 mt-1.5"
                  icon={<ExternalIcon size={12} />}
                  onClick={() =>
                    void api
                      .launchIntegration(i.id)
                      .then((m) => toast.push(m, "ok"))
                      .catch((e) => toast.push(errorMessage(e), "danger"))
                  }
                >
                  Open
                </Button>
              )}
            </Card>
          ))}
        </div>
      </section>

      {environment && !environment.shell_running && (
        <Card
          className="px-4 py-3"
          style={{ borderColor: "var(--warn)", background: "var(--warn-dim)" }}
        >
          <p className="text-[12.5px]">
            Windows Explorer is not running, so taskbar and desktop icon
            customizations are unavailable until it restarts.
          </p>
        </Card>
      )}

      <RestoreDialog open={restoreOpen} onClose={() => setRestoreOpen(false)} />
    </div>
  );
}

function QuickProfiles({
  onSwitch,
  busy,
}: {
  onSwitch: (id: string) => Promise<void>;
  busy: boolean;
}) {
  const { profiles, activeProfile } = useApp();
  const shown = profiles.slice(0, 4);

  return (
    <div className="grid grid-cols-4 gap-2.5">
      {shown.map((p) => {
        const active = p.id === activeProfile;
        const count = Object.values(p.modules).filter((m) => m.enabled).length;
        return (
          <Card
            key={p.id}
            interactive={!active && !busy}
            onClick={() => !active && !busy && void onSwitch(p.id)}
            className={cx(
              "px-3.5 py-3",
              active && "border-[var(--accent-500)]",
              busy && "pointer-events-none opacity-60",
            )}
          >
            <p className="truncate text-[12.5px] font-medium">{p.name}</p>
            <p className="mt-0.5 text-[11px] text-[var(--text-tertiary)]">
              {count === 0 ? "Nothing enabled" : `${count} enabled`}
            </p>
            {active && (
              <span className="mt-2 inline-flex items-center gap-1 text-[10.5px] text-[var(--accent-400)]">
                <Dot tone="accent" /> Active
              </span>
            )}
          </Card>
        );
      })}
    </div>
  );
}

/** Report shown after a restore, so success is verified rather than assumed. */
export function RestorationResult({
  report,
  onClose,
}: {
  report: RestorationReport;
  onClose: () => void;
}) {
  const failures = report.settings.filter((s) => s.status === "failed");
  const conflicts = report.settings.filter((s) => s.status === "conflict_skipped");

  return (
    <Modal
      open
      onClose={onClose}
      title={report.verified ? "Original desktop restored" : "Restore finished with notes"}
      description={
        report.verified
          ? "Every setting OVERLAY changed has been put back and verified."
          : "Some settings need your attention — details below."
      }
      footer={
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      }
      width={540}
    >
      <div className="flex flex-col gap-4">
        <div className="grid grid-cols-3 gap-2">
          <Stat label="Restored" value={report.settings.filter((s) => s.status === "restored").length} />
          <Stat label="Windows closed" value={report.surfaces_closed.length} />
          <Stat
            label="Left alone"
            value={conflicts.length}
            tone={conflicts.length > 0 ? "warn" : undefined}
          />
        </div>

        {report.settings.length > 0 && (
          <div>
            <p className="mb-1.5 text-[11px] uppercase tracking-[0.08em] text-[var(--text-tertiary)]">
              Settings
            </p>
            <ul className="flex flex-col gap-1">
              {report.settings.map((s) => (
                <li
                  key={s.target_id}
                  className="flex items-start gap-2 rounded-[7px] bg-[var(--bg-sunken)] px-2.5 py-1.5"
                >
                  <span className="mt-[6px]">
                    <Dot
                      tone={
                        s.status === "restored" || s.status === "already_original"
                          ? "ok"
                          : s.status === "conflict_skipped"
                            ? "warn"
                            : "danger"
                      }
                    />
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="text-[12px]">{s.label}</p>
                    <p className="text-[11px] text-[var(--text-tertiary)]">
                      {s.status === "conflict_skipped"
                        ? s.detail
                        : s.status === "failed"
                          ? s.detail
                          : `Back to ${s.restored_to}`}
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          </div>
        )}

        {report.notes.map((n, i) => (
          <p key={i} className="text-[12px] leading-relaxed text-[var(--text-secondary)]">
            {n}
          </p>
        ))}

        {failures.length > 0 && (
          <p className="text-[12px] leading-relaxed text-[var(--danger)]">
            OVERLAY still holds the original values for the settings above and
            can retry from Settings → Recovery.
          </p>
        )}
      </div>
    </Modal>
  );
}

function Stat({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone?: "warn";
}) {
  return (
    <div className="rounded-[9px] bg-[var(--bg-sunken)] px-3 py-2.5 text-center">
      <p
        className="tabular text-[18px] font-semibold"
        style={tone === "warn" ? { color: "var(--warn)" } : undefined}
      >
        {value}
      </p>
      <p className="text-[11px] text-[var(--text-tertiary)]">{label}</p>
    </div>
  );
}

export { Spinner as _Spinner };
export type { RestorePreview };
