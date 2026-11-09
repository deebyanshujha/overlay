/**
 * Customize — the module catalogue, grouped, each with an independent switch.
 *
 * Two deliberate choices:
 *
 * * A module that cannot run here is shown **disabled with the reason**,
 *   not hidden. Hiding it leaves the user wondering where a documented
 *   feature went.
 * * Configuration lives in a panel that opens on demand, so the list stays
 *   readable and the depth is there when wanted. Section 10's "do not expose
 *   every advanced option immediately".
 */

import { useState } from "react";
import { useApp } from "@/stores/app";
import { errorMessage } from "@/types/api";
import type { ModuleCategory, ModuleStatus } from "@/types/api";
import {
  AlertIcon,
  Badge,
  Button,
  Card,
  ChevronIcon,
  InfoIcon,
  SectionTitle,
  Toggle,
  cx,
  useToast,
} from "@/components/ui";
import { ModulePanel } from "@/pages/ModulePanel";

const GROUPS: Array<{ id: ModuleCategory; title: string; description: string }> = [
  {
    id: "appearance",
    title: "Appearance",
    description: "Changes Windows itself. Every setting is recorded first.",
  },
  {
    id: "widgets",
    title: "Widgets",
    description: "OVERLAY's own windows on your desktop. Nothing in Windows changes.",
  },
  {
    id: "surfaces",
    title: "Surfaces",
    description: "Dock, control centre and file shelf.",
  },
  {
    id: "integrations",
    title: "Applications",
    description: "Keep other apps running alongside this profile.",
  },
];

export function CustomizePage() {
  const { modules } = useApp();
  const [openModule, setOpenModule] = useState<string | null>(null);

  return (
    <div className="mx-auto flex max-w-[860px] flex-col gap-8">
      <header>
        <h1 className="text-[22px] font-semibold tracking-tight">Customize</h1>
        <p className="mt-1 max-w-[62ch] text-[13px] leading-relaxed text-[var(--text-secondary)]">
          Switch on only what you want. Each customization is independent —
          turning one off never affects the others, and anything that changes
          Windows is restored exactly as it was.
        </p>
      </header>

      {GROUPS.map((group) => {
        const items = modules.filter((m) => m.category === group.id);
        if (items.length === 0) return null;
        return (
          <section key={group.id}>
            <SectionTitle title={group.title} description={group.description} />
            <div className="flex flex-col gap-2">
              {items.map((m) => (
                <ModuleRow
                  key={m.id}
                  module={m}
                  expanded={openModule === m.id}
                  onToggleExpanded={() =>
                    setOpenModule((cur) => (cur === m.id ? null : m.id))
                  }
                />
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}

function ModuleRow({
  module,
  expanded,
  onToggleExpanded,
}: {
  module: ModuleStatus;
  expanded: boolean;
  onToggleExpanded: () => void;
}) {
  const { setModuleEnabled } = useApp();
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  // Narrowed rather than read through a boolean, so TypeScript knows which
  // of `reason` / `note` actually exists on the variant.
  const availability = module.availability;
  const unavailable = availability.state === "unavailable" ? availability : null;
  const partial = availability.state === "partial" ? availability : null;

  const onToggle = async (next: boolean) => {
    setBusy(true);
    try {
      const { warnings } = await setModuleEnabled(module.id, next);
      if (warnings.length > 0) {
        toast.push(`${module.name} ${next ? "enabled" : "disabled"}`, "warn", warnings[0]);
      } else {
        toast.push(
          next
            ? `${module.name} enabled`
            : module.touches_system
              ? `${module.name} disabled — settings restored`
              : `${module.name} disabled`,
          "ok",
        );
      }
    } catch (e) {
      toast.push(`Could not ${next ? "enable" : "disable"} ${module.name}`, "danger", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card className={cx("overflow-hidden", unavailable && "opacity-70")}>
      <div className="flex items-center gap-4 px-4 py-3.5">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="text-[13.5px] font-medium">{module.name}</h3>
            {module.touches_system && (
              <Badge tone={module.enabled ? "accent" : "neutral"}>
                Changes Windows
              </Badge>
            )}
            {module.enabled && module.owned_settings > 0 && (
              <Badge tone="neutral">
                {module.owned_settings} setting
                {module.owned_settings === 1 ? "" : "s"}
              </Badge>
            )}
          </div>
          <p className="mt-0.5 max-w-[64ch] text-[12px] leading-relaxed text-[var(--text-secondary)]">
            {module.description}
          </p>

          {unavailable && (
            <p className="mt-1.5 flex items-start gap-1.5 text-[11.5px] leading-snug text-[var(--warn)]">
              <AlertIcon size={12} className="mt-[2px] shrink-0" />
              {unavailable.reason}
            </p>
          )}
          {partial && (
            <p className="mt-1.5 flex items-start gap-1.5 text-[11.5px] leading-snug text-[var(--text-tertiary)]">
              <InfoIcon size={12} className="mt-[2px] shrink-0" />
              {partial.note}
            </p>
          )}
          {module.requires.length > 0 && (
            <p className="mt-1 text-[11px] text-[var(--text-tertiary)]">
              Needs: {module.requires.join(", ")}
            </p>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            onClick={onToggleExpanded}
            aria-expanded={expanded}
            icon={
              <ChevronIcon
                size={13}
                className={cx(
                  "transition-transform duration-[var(--dur-fast)]",
                  expanded && "rotate-90",
                )}
              />
            }
          >
            {expanded ? "Hide" : "Configure"}
          </Button>
          <Toggle
            checked={module.enabled}
            onChange={(v) => void onToggle(v)}
            disabled={unavailable !== null || busy}
            srLabel={`${module.enabled ? "Disable" : "Enable"} ${module.name}`}
          />
        </div>
      </div>

      {expanded && (
        <div className="animate-rise border-t border-[var(--border-subtle)] bg-[var(--bg-sunken)] px-4 py-4">
          <ModulePanel module={module} />
        </div>
      )}
    </Card>
  );
}
