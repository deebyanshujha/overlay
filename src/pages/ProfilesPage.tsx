/**
 * Profiles — create, edit, switch, import and export.
 *
 * Original Windows is presented first and differently: it is the safety net
 * rather than a style, and it is the one profile that cannot be edited.
 */

import { useEffect, useState } from "react";
import { useApp } from "@/stores/app";
import * as api from "@/services/api";
import { errorMessage, ORIGINAL_WINDOWS } from "@/types/api";
import type { Profile, SwitchReport } from "@/types/api";
import {
  Badge,
  Button,
  Card,
  CheckIcon,
  ConfirmDialog,
  CopyIcon,
  Dot,
  DownloadIcon,
  Field,
  Modal,
  RestoreIcon,
  SectionTitle,
  TextInput,
  Toggle,
  TrashIcon,
  UploadIcon,
  cx,
  useToast,
} from "@/components/ui";

export function ProfilesPage() {
  const {
    profiles,
    activeProfile,
    modules,
    switchProfile,
    switching,
    refreshProfiles,
    load,
  } = useApp();
  const toast = useToast();

  const [editing, setEditing] = useState<Profile | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Profile | null>(null);
  const [report, setReport] = useState<SwitchReport | null>(null);
  const [startupProfile, setStartupProfile] = useState<string | null>(null);

  useEffect(() => {
    void api.getStartupProfile().then(setStartupProfile);
  }, []);

  const original = profiles.find((p) => p.id === ORIGINAL_WINDOWS);
  const rest = profiles.filter((p) => p.id !== ORIGINAL_WINDOWS);

  const apply = async (id: string) => {
    try {
      const result = await switchProfile(id);
      setReport(result);
    } catch (e) {
      toast.push("Could not switch profile", "danger", errorMessage(e));
    }
  };

  const importProfile = async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({
      title: "Import an OVERLAY profile",
      filters: [{ name: "OVERLAY profile", extensions: ["json"] }],
    });
    if (!path || Array.isArray(path)) return;
    try {
      const result = await api.importProfileFromFile(path);
      await refreshProfiles();
      toast.push(
        `Imported “${result.profile.name}”`,
        result.warnings.length > 0 ? "warn" : "ok",
        result.warnings[0],
      );
    } catch (e) {
      toast.push("Could not import that profile", "danger", errorMessage(e));
    }
  };

  return (
    <div className="mx-auto flex max-w-[860px] flex-col gap-7">
      <header className="flex items-start justify-between gap-6">
        <div>
          <h1 className="text-[22px] font-semibold tracking-tight">Profiles</h1>
          <p className="mt-1 max-w-[60ch] text-[13px] leading-relaxed text-[var(--text-secondary)]">
            A profile is a saved set of customizations. Switching applies the
            difference and puts back anything the new profile does not want.
          </p>
        </div>
        <div className="flex shrink-0 gap-2">
          <Button variant="secondary" icon={<UploadIcon size={13} />} onClick={() => void importProfile()}>
            Import
          </Button>
          <Button variant="primary" onClick={() => setCreating(true)}>
            New profile
          </Button>
        </div>
      </header>

      {original && (
        <section>
          <SectionTitle
            title="Safety net"
            description="Always available, never edited."
          />
          <Card
            className={cx(
              "flex items-center gap-4 px-5 py-4",
              activeProfile === original.id && "border-[var(--accent-500)]",
            )}
          >
            <RestoreIcon size={18} className="shrink-0 text-[var(--accent-400)]" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <h3 className="text-[13.5px] font-medium">{original.name}</h3>
                {activeProfile === original.id && <Badge tone="accent">Active</Badge>}
              </div>
              <p className="mt-0.5 text-[12px] leading-relaxed text-[var(--text-secondary)]">
                {original.description}
              </p>
            </div>
            <Button
              variant={activeProfile === original.id ? "ghost" : "secondary"}
              disabled={activeProfile === original.id || switching}
              loading={switching}
              onClick={() => void apply(original.id)}
            >
              {activeProfile === original.id ? "Current" : "Restore"}
            </Button>
          </Card>
        </section>
      )}

      <section>
        <SectionTitle title="Your profiles" description={`${rest.length} available`} />
        <div className="grid grid-cols-2 gap-3">
          {rest.map((p) => (
            <ProfileCard
              key={p.id}
              profile={p}
              active={p.id === activeProfile}
              isStartup={p.id === startupProfile}
              moduleNames={Object.entries(p.modules)
                .filter(([, s]) => s.enabled)
                .map(([id]) => modules.find((m) => m.id === id)?.name ?? id)}
              busy={switching}
              onApply={() => void apply(p.id)}
              onEdit={() => setEditing(p)}
              onDelete={() => setDeleting(p)}
              onDuplicate={async () => {
                try {
                  const copy = await api.duplicateProfile(p.id, "");
                  await refreshProfiles();
                  toast.push(`Created “${copy.name}”`, "ok");
                } catch (e) {
                  toast.push("Could not duplicate", "danger", errorMessage(e));
                }
              }}
              onExport={async () => {
                try {
                  const path = await api.exportProfileToFile(p.id);
                  toast.push("Profile exported", "ok", path);
                } catch (e) {
                  toast.push("Could not export", "danger", errorMessage(e));
                }
              }}
              onSetStartup={async (on) => {
                try {
                  await api.setStartupProfile(on ? p.id : null);
                  setStartupProfile(on ? p.id : null);
                  toast.push(
                    on
                      ? `“${p.name}” will be applied when Windows starts`
                      : "No profile will be applied at startup",
                    "ok",
                  );
                } catch (e) {
                  toast.push("Could not set the startup profile", "danger", errorMessage(e));
                }
              }}
            />
          ))}
        </div>
      </section>

      {creating && (
        <CreateProfileModal
          onClose={() => setCreating(false)}
          onCreated={async () => {
            setCreating(false);
            await refreshProfiles();
          }}
        />
      )}

      {editing && (
        <EditProfileModal
          profile={editing}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await load();
          }}
        />
      )}

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        title={`Delete “${deleting?.name}”?`}
        description="The profile is removed. Your desktop and its current settings are not affected."
        confirmLabel="Delete profile"
        destructive
        onConfirm={async () => {
          if (!deleting) return;
          try {
            await api.deleteProfile(deleting.id);
            await refreshProfiles();
            toast.push("Profile deleted", "ok");
          } catch (e) {
            toast.push("Could not delete", "danger", errorMessage(e));
          } finally {
            setDeleting(null);
          }
        }}
      />

      {report && <SwitchResult report={report} onClose={() => setReport(null)} />}
    </div>
  );
}

function ProfileCard({
  profile,
  active,
  isStartup,
  moduleNames,
  busy,
  onApply,
  onEdit,
  onDelete,
  onDuplicate,
  onExport,
  onSetStartup,
}: {
  profile: Profile;
  active: boolean;
  isStartup: boolean;
  moduleNames: string[];
  busy: boolean;
  onApply: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onExport: () => void;
  onSetStartup: (on: boolean) => void;
}) {
  return (
    <Card className={cx("flex flex-col p-4", active && "border-[var(--accent-500)]")}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-[13.5px] font-medium">{profile.name}</h3>
            {profile.builtin && <Badge tone="neutral">Built in</Badge>}
            {active && <Badge tone="accent">Active</Badge>}
          </div>
          <p className="mt-0.5 line-clamp-2 text-[12px] leading-relaxed text-[var(--text-secondary)]">
            {profile.description || "No description."}
          </p>
        </div>
      </div>

      <p className="mt-2.5 min-h-[32px] text-[11px] leading-relaxed text-[var(--text-tertiary)]">
        {moduleNames.length === 0
          ? "Nothing enabled."
          : moduleNames.join(" · ")}
      </p>

      <label className="mt-2 flex cursor-pointer items-center gap-2">
        <Toggle
          checked={isStartup}
          onChange={onSetStartup}
          srLabel={`Apply ${profile.name} at startup`}
        />
        <span className="text-[11px] text-[var(--text-tertiary)]">
          Apply when Windows starts
        </span>
      </label>

      <div className="mt-3 flex items-center gap-1 border-t border-[var(--border-subtle)] pt-3">
        <Button
          size="sm"
          variant={active ? "ghost" : "primary"}
          disabled={active || busy}
          onClick={onApply}
          className="flex-1"
        >
          {active ? "Current" : "Apply"}
        </Button>
        <Button size="sm" variant="ghost" onClick={onEdit}>
          Edit
        </Button>
        <Button size="sm" variant="ghost" onClick={onDuplicate} aria-label="Duplicate">
          <CopyIcon size={13} />
        </Button>
        <Button size="sm" variant="ghost" onClick={onExport} aria-label="Export">
          <DownloadIcon size={13} />
        </Button>
        {!profile.builtin && (
          <Button size="sm" variant="ghost" onClick={onDelete} aria-label="Delete">
            <TrashIcon size={13} />
          </Button>
        )}
      </div>
    </Card>
  );
}

function CreateProfileModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: () => void;
}) {
  const { profiles } = useApp();
  const toast = useToast();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [copyFrom, setCopyFrom] = useState("");
  const [busy, setBusy] = useState(false);

  return (
    <Modal
      open
      onClose={onClose}
      title="New profile"
      description="Start empty, or from a copy of one you already have."
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={name.trim().length === 0}
            onClick={async () => {
              setBusy(true);
              try {
                await api.createProfile(name, description, copyFrom || undefined);
                toast.push(`Created “${name.trim()}”`, "ok");
                onCreated();
              } catch (e) {
                toast.push("Could not create the profile", "danger", errorMessage(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            Create
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field label="Name">
          <TextInput
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Focus, Streaming, Weekend…"
            autoFocus
          />
        </Field>
        <Field label="Description" hint="Optional.">
          <TextInput
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What this desktop is for"
          />
        </Field>
        <Field label="Start from">
          <select
            value={copyFrom}
            onChange={(e) => setCopyFrom(e.target.value)}
            className="h-9 w-full cursor-pointer rounded-[8px] border border-[var(--border-default)] bg-[var(--bg-sunken)] px-3 text-[13px]"
          >
            <option value="">Empty — nothing enabled</option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                Copy of {p.name}
              </option>
            ))}
          </select>
        </Field>
      </div>
    </Modal>
  );
}

function EditProfileModal({
  profile,
  onClose,
  onSaved,
}: {
  profile: Profile;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { modules } = useApp();
  const toast = useToast();
  const [name, setName] = useState(profile.name);
  const [description, setDescription] = useState(profile.description);
  const [local, setLocal] = useState(profile.modules);
  const [busy, setBusy] = useState(false);

  const immutable = profile.id === ORIGINAL_WINDOWS;

  return (
    <Modal
      open
      onClose={onClose}
      width={520}
      title={`Edit “${profile.name}”`}
      description={
        immutable
          ? "Original Windows cannot be changed — it is the record of your untouched desktop."
          : "Changes apply the next time you switch to this profile."
      }
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button
            variant="primary"
            loading={busy}
            disabled={immutable}
            onClick={async () => {
              setBusy(true);
              try {
                await api.renameProfile(profile.id, name, description);
                for (const [id, setting] of Object.entries(local)) {
                  if (profile.modules[id]?.enabled !== setting.enabled) {
                    await api.setProfileModule(profile.id, id, setting.enabled);
                  }
                }
                toast.push("Profile saved", "ok");
                onSaved();
              } catch (e) {
                toast.push("Could not save", "danger", errorMessage(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            Save
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <Field label="Name">
          <TextInput
            value={name}
            onChange={(e) => setName(e.target.value)}
            disabled={immutable}
          />
        </Field>
        <Field label="Description">
          <TextInput
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            disabled={immutable}
          />
        </Field>

        <div>
          <p className="mb-2 text-[12px] font-medium text-[var(--text-secondary)]">
            Customizations in this profile
          </p>
          <ul className="flex flex-col gap-1">
            {modules.map((m) => (
              <li
                key={m.id}
                className="flex items-center justify-between gap-3 rounded-[8px] bg-[var(--bg-sunken)] px-3 py-2"
              >
                <div className="min-w-0">
                  <p className="text-[12px]">{m.name}</p>
                  {m.requires.length > 0 && (
                    <p className="text-[10.5px] text-[var(--text-tertiary)]">
                      Needs {m.requires.join(", ")}
                    </p>
                  )}
                </div>
                <Toggle
                  checked={local[m.id]?.enabled ?? false}
                  disabled={immutable || busy}
                  srLabel={m.name}
                  onChange={(v) =>
                    setLocal((cur) => ({
                      ...cur,
                      [m.id]: { enabled: v, config: cur[m.id]?.config ?? {} },
                    }))
                  }
                />
              </li>
            ))}
          </ul>
        </div>
      </div>
    </Modal>
  );
}

/** Shown after a switch, including anything that did not go to plan. */
function SwitchResult({
  report,
  onClose,
}: {
  report: SwitchReport;
  onClose: () => void;
}) {
  const failures = [...report.enabled, ...report.disabled].filter((o) => !o.ok);
  const ok = report.succeeded && failures.length === 0;

  return (
    <Modal
      open
      onClose={onClose}
      width={500}
      title={
        report.rolled_back
          ? "Switch rolled back"
          : ok
            ? `Switched to ${report.to_name}`
            : `Switched to ${report.to_name}, with notes`
      }
      description={
        report.rolled_back
          ? "A change to Windows failed, so OVERLAY put your previous profile back rather than leaving the desktop half-switched."
          : ok
            ? `Applied in ${report.duration_ms} ms.`
            : undefined
      }
      footer={
        <Button variant="primary" onClick={onClose}>
          Done
        </Button>
      }
    >
      <div className="flex flex-col gap-3">
        {report.enabled.filter((o) => o.ok).length > 0 && (
          <Line
            tone="ok"
            text={`Enabled: ${report.enabled
              .filter((o) => o.ok)
              .map((o) => o.module_id)
              .join(", ")}`}
          />
        )}
        {report.disabled.filter((o) => o.ok).length > 0 && (
          <Line
            tone="ok"
            text={`Switched off: ${report.disabled
              .filter((o) => o.ok)
              .map((o) => o.module_id)
              .join(", ")}`}
          />
        )}
        {report.restored.length > 0 && (
          <Line
            tone="ok"
            text={`${report.restored.length} Windows setting${
              report.restored.length === 1 ? "" : "s"
            } put back`}
          />
        )}
        {report.warnings.map((w, i) => (
          <Line key={i} tone="warn" text={w} />
        ))}
        {failures.map((f) => (
          <Line
            key={f.module_id}
            tone="danger"
            text={`${f.module_id}: ${f.error ?? "unknown error"}`}
          />
        ))}
      </div>
    </Modal>
  );
}

function Line({ tone, text }: { tone: "ok" | "warn" | "danger"; text: string }) {
  return (
    <div className="flex gap-2.5">
      <span className="mt-[6px]">
        {tone === "ok" ? <CheckIcon size={12} className="text-[var(--ok)]" /> : <Dot tone={tone} />}
      </span>
      <p className="text-[12px] leading-relaxed text-[var(--text-secondary)]">{text}</p>
    </div>
  );
}
