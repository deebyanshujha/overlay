/**
 * Dropzone — a shelf for files in transit.
 *
 * Dropping adds a *reference*. Nothing is copied or moved until the user
 * explicitly chooses a destination, and moving — the only operation that
 * removes the original — asks first, naming the files.
 */

import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "@/services/api";
import { applyTheme } from "@/stores/app";
import { errorMessage } from "@/types/api";
import type { ShelfItem, ShelfState } from "@/types/api";
import { Button, CloseIcon, cx } from "@/components/ui";

export function DropzoneSurface() {
  const [state, setState] = useState<ShelfState>({
    items: [],
    total_bytes: 0,
    missing: 0,
  });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [over, setOver] = useState(false);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ text: string; bad: boolean } | null>(null);
  const [confirmMove, setConfirmMove] = useState<string | null>(null);

  const say = useCallback((text: string, bad = false) => {
    setNote({ text, bad });
    window.setTimeout(() => setNote(null), 4500);
  }, []);

  useEffect(() => {
    void api.getSettings().then(applyTheme).catch(() => undefined);
    void api.shelfRefresh().then(setState).catch(() => undefined);
  }, []);

  // Tauri delivers OS file drops as a window event rather than a DOM one.
  useEffect(() => {
    const win = getCurrentWindow();
    const unlisten = win.onDragDropEvent(async (event) => {
      if (event.payload.type === "over") {
        setOver(true);
      } else if (event.payload.type === "drop") {
        setOver(false);
        try {
          const next = await api.shelfAdd(event.payload.paths);
          setState(next);
          say(`Added ${event.payload.paths.length} item(s) — nothing was moved`);
        } catch (e) {
          say(errorMessage(e), true);
        }
      } else {
        setOver(false);
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [say]);

  const pickFolder = async (): Promise<string | null> => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ directory: true, title: "Choose a destination folder" });
    return typeof path === "string" ? path : null;
  };

  const ids = selected.size > 0 ? [...selected] : state.items.map((i) => i.id);

  const copy = async () => {
    const dest = await pickFolder();
    if (!dest) return;
    setBusy(true);
    try {
      const report = await api.shelfCopyTo(ids, dest);
      say(
        `Copied ${report.succeeded.length}` +
          (report.failed.length > 0 ? `, ${report.failed.length} failed` : ""),
        report.failed.length > 0,
      );
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };

  const move = async () => {
    const dest = await pickFolder();
    if (!dest) return;
    setConfirmMove(dest);
  };

  const doMove = async (dest: string) => {
    setConfirmMove(null);
    setBusy(true);
    try {
      const report = await api.shelfMoveTo(ids, dest, true);
      setState(await api.shelfRefresh());
      setSelected(new Set());
      say(
        `Moved ${report.succeeded.length}` +
          (report.failed.length > 0 ? `, ${report.failed.length} failed` : ""),
        report.failed.length > 0,
      );
    } catch (e) {
      say(errorMessage(e), true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full w-full p-1">
      <div
        className={cx(
          "glass flex h-full w-full flex-col overflow-hidden border shadow-[var(--shadow-lg)]",
          over ? "border-[var(--accent-500)]" : "border-[var(--border-default)]",
        )}
        style={{ borderRadius: 16 }}
      >
        <header className="drag flex items-center justify-between border-b border-[var(--border-subtle)] px-3.5 py-2.5">
          <div>
            <span className="text-[11.5px] font-semibold tracking-[0.1em]">
              DROPZONE
            </span>
            <p className="text-[10px] text-[var(--text-tertiary)]">
              {state.items.length === 0
                ? "Drag files here"
                : `${state.items.length} item(s) · ${formatBytes(state.total_bytes)}`}
            </p>
          </div>
          <button
            aria-label="Close"
            onClick={() => void api.closeSurface("dropzone")}
            className="no-drag text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
          >
            <CloseIcon size={14} />
          </button>
        </header>

        <div
          className={cx(
            "flex-1 overflow-y-auto px-2 py-2 transition-colors",
            over && "bg-[rgb(var(--accent-rgb)/0.08)]",
          )}
        >
          {state.items.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center px-5 text-center">
              <div
                className={cx(
                  "mb-3 flex h-14 w-14 items-center justify-center rounded-[14px]",
                  "border-2 border-dashed transition-colors",
                  over ? "border-[var(--accent-500)]" : "border-[var(--border-strong)]",
                )}
              >
                <span className="text-[20px] text-[var(--text-tertiary)]">+</span>
              </div>
              <p className="text-[12px] text-[var(--text-secondary)]">
                Drop files to collect them
              </p>
              <p className="mt-1 text-[10.5px] leading-snug text-[var(--text-tertiary)]">
                Files stay exactly where they are. Dropping here only keeps a
                reference until you copy or move them.
              </p>
            </div>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {state.items.map((item) => (
                <ShelfRow
                  key={item.id}
                  item={item}
                  selected={selected.has(item.id)}
                  onToggle={() =>
                    setSelected((s) => {
                      const next = new Set(s);
                      if (next.has(item.id)) next.delete(item.id);
                      else next.add(item.id);
                      return next;
                    })
                  }
                  onRemove={async () => setState(await api.shelfRemove([item.id]))}
                  onOpen={() => void api.shelfOpen(item.id).catch((e) => say(errorMessage(e), true))}
                  onReveal={() =>
                    void api.shelfReveal(item.id).catch((e) => say(errorMessage(e), true))
                  }
                />
              ))}
            </ul>
          )}
        </div>

        {note && (
          <div
            className="px-3.5 py-2 text-[11px]"
            style={{
              color: note.bad ? "var(--danger)" : "var(--text-secondary)",
              background: note.bad ? "var(--danger-dim)" : "transparent",
            }}
          >
            {note.text}
          </div>
        )}

        {state.items.length > 0 && (
          <footer className="flex items-center gap-1.5 border-t border-[var(--border-subtle)] px-3 py-2.5">
            <Button size="sm" variant="secondary" onClick={() => void copy()} disabled={busy}>
              Copy to…
            </Button>
            <Button size="sm" variant="secondary" onClick={() => void move()} disabled={busy}>
              Move to…
            </Button>
            <span className="flex-1" />
            <Button
              size="sm"
              variant="ghost"
              disabled={busy}
              onClick={async () => {
                setState(await api.shelfClear());
                setSelected(new Set());
                say("Shelf cleared — no files were deleted");
              }}
            >
              Clear
            </Button>
          </footer>
        )}

        {confirmMove && (
          <div className="absolute inset-0 flex items-center justify-center bg-[rgb(0_0_0/0.6)] p-4">
            <div className="card w-full p-4" style={{ background: "var(--bg-overlay)" }}>
              <p className="text-[12.5px] font-medium">
                Move {ids.length} file{ids.length === 1 ? "" : "s"}?
              </p>
              <p className="mt-1 text-[11px] leading-relaxed text-[var(--text-secondary)]">
                They will be copied to the destination and then removed from
                their current location. The original is only deleted after the
                copy is verified.
              </p>
              <p className="mt-1.5 truncate font-[var(--font-mono)] text-[10.5px] text-[var(--text-tertiary)]">
                {confirmMove}
              </p>
              <div className="mt-3 flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setConfirmMove(null)}>
                  Cancel
                </Button>
                <Button size="sm" variant="danger" onClick={() => void doMove(confirmMove)}>
                  Move files
                </Button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ShelfRow({
  item,
  selected,
  onToggle,
  onRemove,
  onOpen,
  onReveal,
}: {
  item: ShelfItem;
  selected: boolean;
  onToggle: () => void;
  onRemove: () => void;
  onOpen: () => void;
  onReveal: () => void;
}) {
  return (
    <li
      onClick={onToggle}
      onDoubleClick={onOpen}
      className={cx(
        "group flex cursor-pointer items-center gap-2 rounded-[8px] px-2 py-1.5",
        "transition-colors",
        selected ? "bg-[rgb(var(--accent-rgb)/0.14)]" : "hover:bg-[var(--bg-hover)]",
        !item.exists && "opacity-60",
      )}
    >
      <span
        className={cx(
          "flex h-7 w-7 shrink-0 items-center justify-center rounded-[6px]",
          "bg-[var(--bg-active)] text-[9px] font-semibold uppercase",
          "text-[var(--text-tertiary)]",
        )}
      >
        {item.is_dir ? "DIR" : item.extension.slice(0, 3) || "?"}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[11.5px]">{item.name}</p>
        <p className="truncate text-[10px] text-[var(--text-tertiary)]">
          {item.exists ? formatBytes(item.size_bytes) : "no longer on disk"}
        </p>
      </div>
      <span className="flex shrink-0 gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
        <IconAction label="Show in folder" onClick={onReveal}>
          ↗
        </IconAction>
        <IconAction label="Remove from shelf" onClick={onRemove}>
          ×
        </IconAction>
      </span>
    </li>
  );
}

function IconAction({
  children,
  label,
  onClick,
}: {
  children: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      title={label}
      aria-label={label}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="flex h-5 w-5 items-center justify-center rounded-[5px] text-[12px] text-[var(--text-tertiary)] hover:bg-[var(--bg-active)] hover:text-[var(--text-primary)]"
    >
      {children}
    </button>
  );
}

function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return u === 0 ? `${v} B` : `${v.toFixed(1)} ${units[u]}`;
}
