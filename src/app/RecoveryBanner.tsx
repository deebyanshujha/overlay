/**
 * Shown once when OVERLAY recovered from an interrupted operation.
 *
 * It names what happened rather than saying "something went wrong", because
 * the user's actual question is whether their desktop is in a known state.
 */

import { useState } from "react";
import { useApp } from "@/stores/app";
import { AlertIcon, Button, CloseIcon } from "@/components/ui";

export function RecoveryBanner() {
  const { recovery, dismissRecovery } = useApp();
  const [expanded, setExpanded] = useState(false);

  if (!recovery) return null;

  return (
    <div
      role="status"
      className="animate-rise mx-7 mt-6 rounded-[11px] border px-4 py-3"
      style={{
        borderColor: "var(--warn)",
        background: "var(--warn-dim)",
      }}
    >
      <div className="flex items-start gap-3">
        <AlertIcon size={16} className="mt-[2px] shrink-0 text-[var(--warn)]" />
        <div className="min-w-0 flex-1">
          <p className="text-[12.5px] font-medium">{recovery.message}</p>

          {recovery.details.length > 0 && (
            <>
              {expanded ? (
                <ul className="mt-2 flex list-disc flex-col gap-1 pl-4">
                  {recovery.details.map((d, i) => (
                    <li
                      key={i}
                      className="text-[11.5px] leading-relaxed text-[var(--text-secondary)]"
                    >
                      {d}
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="mt-0.5 text-[11.5px] text-[var(--text-secondary)]">
                  {recovery.details[0]}
                </p>
              )}

              {recovery.details.length > 1 && (
                <Button
                  size="sm"
                  variant="ghost"
                  className="mt-1.5 -ml-2.5"
                  onClick={() => setExpanded((e) => !e)}
                >
                  {expanded
                    ? "Show less"
                    : `Show all ${recovery.details.length} details`}
                </Button>
              )}
            </>
          )}
        </div>
        <button
          aria-label="Dismiss"
          onClick={dismissRecovery}
          className="shrink-0 text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
        >
          <CloseIcon size={14} />
        </button>
      </div>
    </div>
  );
}
