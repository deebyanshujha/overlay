/**
 * The Restore Original Desktop confirmation.
 *
 * It reads the live preview from the backend and lists the actual settings
 * that will change, plus the explicit guarantees about what will not. A
 * generic "are you sure?" would be worse than useless here: the whole point
 * of the feature is that the user can trust it, and trust comes from being
 * shown the specifics.
 */

import { useEffect, useState } from "react";
import { useApp } from "@/stores/app";
import * as api from "@/services/api";
import { errorMessage } from "@/types/api";
import type { RestorationReport, RestorePreview } from "@/types/api";
import {
  Button,
  CheckIcon,
  Dot,
  Modal,
  Spinner,
  useToast,
} from "@/components/ui";
import { RestorationResult } from "@/pages/HomePage";

export function RestoreDialog({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) {
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<RestorationReport | null>(null);
  const { load } = useApp();
  const toast = useToast();

  useEffect(() => {
    if (!open) {
      setPreview(null);
      return;
    }
    setLoading(true);
    api
      .previewRestore()
      .then(setPreview)
      .catch((e) => toast.push("Could not read the restore plan", "danger", errorMessage(e)))
      .finally(() => setLoading(false));
  }, [open, toast]);

  const run = async () => {
    setBusy(true);
    try {
      const result = await api.restoreOriginalDesktop();
      setReport(result);
      onClose();
      await load();
    } catch (e) {
      toast.push("Restore failed", "danger", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const nothingToDo =
    preview !== null &&
    preview.settings.length === 0 &&
    preview.surfaces.length === 0;

  return (
    <>
      <Modal
        open={open}
        onClose={onClose}
        width={540}
        title="Restore your original desktop"
        description={
          nothingToDo
            ? "There is nothing to undo — OVERLAY has not changed anything."
            : "This undoes every change OVERLAY has made, and nothing else."
        }
        footer={
          <>
            <Button variant="ghost" onClick={onClose} disabled={busy}>
              Cancel
            </Button>
            <Button
              variant="primary"
              onClick={() => void run()}
              loading={busy}
              disabled={loading || nothingToDo}
            >
              {nothingToDo ? "Nothing to restore" : "Restore original desktop"}
            </Button>
          </>
        }
      >
        {loading && (
          <div className="flex items-center gap-2 py-6 text-[var(--text-tertiary)]">
            <Spinner /> Reading your current settings…
          </div>
        )}

        {preview && (
          <div className="flex flex-col gap-5">
            {preview.settings.length > 0 && (
              <section>
                <p className="mb-2 text-[11px] uppercase tracking-[0.08em] text-[var(--text-tertiary)]">
                  Windows settings to put back ({preview.settings.length})
                </p>
                <ul className="flex flex-col gap-1">
                  {preview.settings.map((s) => (
                    <li
                      key={s.target_id}
                      className="flex items-start justify-between gap-3 rounded-[7px] bg-[var(--bg-sunken)] px-3 py-2"
                    >
                      <div className="min-w-0">
                        <p className="truncate text-[12px]">{s.label}</p>
                        <p className="text-[11px] text-[var(--text-tertiary)]">
                          {s.status === "conflict_skipped"
                            ? "Changed outside OVERLAY — will be left as it is"
                            : s.status === "already_original"
                              ? "Already at its original value"
                              : `Back to ${s.restored_to}`}
                        </p>
                      </div>
                      <Dot
                        tone={
                          s.status === "conflict_skipped"
                            ? "warn"
                            : s.status === "failed"
                              ? "danger"
                              : "ok"
                        }
                      />
                    </li>
                  ))}
                </ul>
              </section>
            )}

            {preview.surfaces.length > 0 && (
              <section>
                <p className="mb-2 text-[11px] uppercase tracking-[0.08em] text-[var(--text-tertiary)]">
                  OVERLAY windows to close ({preview.surfaces.length})
                </p>
                <p className="text-[12px] text-[var(--text-secondary)]">
                  {preview.surfaces.join(", ")}
                </p>
              </section>
            )}

            <section>
              <p className="mb-2 text-[11px] uppercase tracking-[0.08em] text-[var(--text-tertiary)]">
                What will not happen
              </p>
              <ul className="flex flex-col gap-1.5">
                {preview.guarantees.map((g, i) => (
                  <li key={i} className="flex gap-2">
                    <CheckIcon size={13} className="mt-[3px] shrink-0 text-[var(--ok)]" />
                    <span className="text-[12px] leading-relaxed text-[var(--text-secondary)]">
                      {g}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          </div>
        )}
      </Modal>

      {report && (
        <RestorationResult report={report} onClose={() => setReport(null)} />
      )}
    </>
  );
}
