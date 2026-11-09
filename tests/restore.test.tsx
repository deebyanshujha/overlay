/**
 * The restore flow.
 *
 * This is the product's central promise, so the tests are about what the
 * user is told: the dialog must show the real list before acting, the result
 * must not claim success it does not have, and a setting changed outside
 * OVERLAY must be reported as left alone rather than quietly overwritten.
 */

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as fixtures from "./fixtures";

vi.mock("@/services/api");
import * as api from "@/services/api";
import { RestoreDialog } from "@/pages/RestoreDialog";
import { RestorationResult } from "@/pages/HomePage";
import { ToastProvider } from "@/components/ui";
import { useApp } from "@/stores/app";
import { restorationSummary } from "@/types/api";

const mocked = vi.mocked(api);

beforeEach(() => {
  vi.resetAllMocks();
  useApp.setState({ ready: true, ...fixtures.bootstrap() } as never);
  mocked.bootstrap.mockResolvedValue(fixtures.bootstrap());
});

function renderDialog(onClose = vi.fn()) {
  return render(
    <ToastProvider>
      <RestoreDialog open onClose={onClose} />
    </ToastProvider>,
  );
}

describe("Restore dialog", () => {
  it("lists the actual settings that will change, not a generic warning", async () => {
    mocked.previewRestore.mockResolvedValue({
      settings: [
        fixtures.restoreOutcome({ label: "TaskbarAl", restored_to: "(not set)" }),
        fixtures.restoreOutcome({
          target_id: "shell:desktop-icons",
          label: "Desktop icons",
          restored_to: "visible",
        }),
      ],
      surfaces: ["widget-clock", "hud"],
      guarantees: ["No desktop shortcut, file or folder is touched."],
    });

    renderDialog();

    expect(await screen.findByText("TaskbarAl")).toBeInTheDocument();
    expect(screen.getByText("Desktop icons")).toBeInTheDocument();
    expect(screen.getByText("Back to (not set)")).toBeInTheDocument();
    expect(screen.getByText(/Windows settings to put back \(2\)/)).toBeInTheDocument();
    expect(screen.getByText(/OVERLAY windows to close \(2\)/)).toBeInTheDocument();
  });

  it("states what will not happen, which is what users actually worry about", async () => {
    mocked.previewRestore.mockResolvedValue({
      settings: [fixtures.restoreOutcome()],
      surfaces: [],
      guarantees: [
        "No desktop shortcut, file or folder is touched.",
        "WallFlow and QuickDock keep running, with their settings unchanged.",
      ],
    });

    renderDialog();

    expect(
      await screen.findByText("No desktop shortcut, file or folder is touched."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("WallFlow and QuickDock keep running, with their settings unchanged."),
    ).toBeInTheDocument();
  });

  it("flags a setting changed outside OVERLAY as one it will leave alone", async () => {
    mocked.previewRestore.mockResolvedValue({
      settings: [fixtures.restoreOutcome({ status: "conflict_skipped" })],
      surfaces: [],
      guarantees: [],
    });

    renderDialog();
    expect(
      await screen.findByText(/Changed outside OVERLAY — will be left as it is/),
    ).toBeInTheDocument();
  });

  it("offers nothing to do when OVERLAY has changed nothing", async () => {
    mocked.previewRestore.mockResolvedValue({
      settings: [],
      surfaces: [],
      guarantees: [],
    });

    renderDialog();
    const button = await screen.findByRole("button", { name: "Nothing to restore" });
    expect(button).toBeDisabled();
    expect(
      screen.getByText(/There is nothing to undo — OVERLAY has not changed anything/),
    ).toBeInTheDocument();
  });

  it("only restores once the user confirms", async () => {
    mocked.previewRestore.mockResolvedValue({
      settings: [fixtures.restoreOutcome()],
      surfaces: [],
      guarantees: [],
    });
    mocked.restoreOriginalDesktop.mockResolvedValue(fixtures.restorationReport());

    renderDialog();
    await screen.findByText("TaskbarAl (Software\\X)");
    expect(mocked.restoreOriginalDesktop).not.toHaveBeenCalled();

    await userEvent.click(
      screen.getByRole("button", { name: "Restore original desktop" }),
    );
    await waitFor(() => expect(mocked.restoreOriginalDesktop).toHaveBeenCalledOnce());
  });

  it("reports a failed restore instead of closing quietly", async () => {
    mocked.previewRestore.mockResolvedValue({
      settings: [fixtures.restoreOutcome()],
      surfaces: [],
      guarantees: [],
    });
    mocked.restoreOriginalDesktop.mockRejectedValue({
      kind: "win32",
      message: "Explorer did not respond.",
    });

    const onClose = vi.fn();
    renderDialog(onClose);
    await screen.findByText("TaskbarAl (Software\\X)");
    await userEvent.click(
      screen.getByRole("button", { name: "Restore original desktop" }),
    );

    expect(await screen.findByText("Restore failed")).toBeInTheDocument();
    expect(screen.getByText("Explorer did not respond.")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("Restoration result", () => {
  it("confirms verification only when the backend verified it", () => {
    render(
      <RestorationResult report={fixtures.restorationReport()} onClose={vi.fn()} />,
    );
    expect(screen.getByText("Original desktop restored")).toBeInTheDocument();
    expect(
      screen.getByText(/Every setting OVERLAY changed has been put back and verified/),
    ).toBeInTheDocument();
  });

  it("does not claim success when verification failed", () => {
    render(
      <RestorationResult
        report={fixtures.restorationReport({
          verified: false,
          settings: [
            fixtures.restoreOutcome({
              status: "failed",
              detail: "Explorer did not respond",
            }),
          ],
        })}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText("Restore finished with notes")).toBeInTheDocument();
    expect(screen.getByText("Explorer did not respond")).toBeInTheDocument();
    expect(screen.queryByText(/has been put back and verified/)).not.toBeInTheDocument();
  });

  it("counts conflicts separately from restorations", () => {
    render(
      <RestorationResult
        report={fixtures.restorationReport({
          settings: [
            fixtures.restoreOutcome(),
            fixtures.restoreOutcome({
              target_id: "b",
              status: "conflict_skipped",
              detail: "changed outside OVERLAY since we set it",
            }),
          ],
        })}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText("Left alone")).toBeInTheDocument();
    expect(screen.getByText("changed outside OVERLAY since we set it")).toBeInTheDocument();
  });
});

describe("restorationSummary", () => {
  it("says nothing was touched when there is nothing to undo", () => {
    const report = fixtures.restorationReport({ settings: [], surfaces_closed: [] });
    expect(restorationSummary(report)).toMatch(/already untouched/);
  });

  it("counts each outcome separately", () => {
    const report = fixtures.restorationReport({
      settings: [
        fixtures.restoreOutcome({ target_id: "a" }),
        fixtures.restoreOutcome({ target_id: "b" }),
        fixtures.restoreOutcome({ target_id: "c", status: "conflict_skipped" }),
        fixtures.restoreOutcome({ target_id: "d", status: "failed" }),
      ],
      surfaces_closed: ["hud"],
    });
    const summary = restorationSummary(report);
    expect(summary).toContain("2 setting(s) restored");
    expect(summary).toContain("1 window(s) closed");
    expect(summary).toContain("changed elsewhere");
    expect(summary).toContain("could not be restored");
  });
});
