/**
 * Customize page: independent toggles, honest availability, dependency
 * handling.
 *
 * These assert product promises rather than markup. "A module that cannot
 * run here is shown with the reason" is a claim the brief makes; this is
 * where it is kept true.
 */

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import * as fixtures from "./fixtures";

vi.mock("@/services/api");
import * as api from "@/services/api";
import { CustomizePage } from "@/pages/CustomizePage";
import { ToastProvider } from "@/components/ui";
import { useApp } from "@/stores/app";

const mocked = vi.mocked(api);

function seed(modules = fixtures.modules) {
  useApp.setState({
    ready: true,
    loadError: null,
    modules,
    profiles: fixtures.profiles,
    activeProfile: "minimal",
    environment: fixtures.environment,
    capabilities: fixtures.capabilities,
    integrations: fixtures.integrations,
    widgetCatalog: fixtures.widgetCatalog,
    controls: fixtures.controls,
    ownedSettings: 0,
    settings: fixtures.bootstrap().settings,
    recovery: null,
    firstRun: false,
    switching: false,
  });
}

function renderPage() {
  return render(
    <ToastProvider>
      <CustomizePage />
    </ToastProvider>,
  );
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.ownedSettings.mockResolvedValue([]);
  seed();
});

describe("Customize page", () => {
  it("lists every module grouped by what it affects", () => {
    renderPage();
    expect(screen.getByText("Taskbar Studio")).toBeInTheDocument();
    expect(screen.getByText("Desktop Widgets")).toBeInTheDocument();
    expect(screen.getByText("Dropzone")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
  });

  it("marks the modules that change Windows itself", () => {
    renderPage();
    // The user's main question about any customiser: what touches my system?
    const badges = screen.getAllByText("Changes Windows");
    expect(badges.length).toBe(2); // taskbar and desktop icons
  });

  it("enables a module through the backend and reports it", async () => {
    mocked.setModuleEnabled.mockResolvedValue({
      modules: fixtures.modules.map((m) =>
        m.id === "taskbar" ? { ...m, enabled: true, owned_settings: 2 } : m,
      ),
      active_profile: "minimal",
      warnings: [],
    });

    renderPage();
    await userEvent.click(
      screen.getByRole("switch", { name: /Enable Taskbar Studio/i }),
    );

    expect(mocked.setModuleEnabled).toHaveBeenCalledWith("taskbar", true);
    expect(await screen.findByText(/Taskbar Studio enabled/)).toBeInTheDocument();
  });

  it("never reports success when the backend refuses", async () => {
    // The rule that stops a toggle lying about the desktop.
    mocked.setModuleEnabled.mockRejectedValue({
      kind: "unavailable",
      message: "Windows Explorer is not running.",
    });

    renderPage();
    await userEvent.click(
      screen.getByRole("switch", { name: /Enable Taskbar Studio/i }),
    );

    expect(await screen.findByText(/Could not enable Taskbar Studio/)).toBeInTheDocument();
    expect(screen.getByText("Windows Explorer is not running.")).toBeInTheDocument();
    expect(screen.queryByText(/Taskbar Studio enabled/)).not.toBeInTheDocument();
  });

  it("shows an unavailable module with the reason instead of hiding it", () => {
    seed(
      fixtures.modules.map((m) =>
        m.id === "taskbar"
          ? {
              ...m,
              availability: {
                state: "unavailable" as const,
                reason: "Windows Explorer is not running, so there is no taskbar.",
              },
            }
          : m,
      ),
    );
    renderPage();

    expect(screen.getByText("Taskbar Studio")).toBeInTheDocument();
    expect(
      screen.getByText(/Windows Explorer is not running, so there is no taskbar/),
    ).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: /Taskbar Studio/i })).toBeDisabled();
  });

  it("surfaces a partial-availability note without disabling the module", () => {
    seed(
      fixtures.modules.map((m) =>
        m.id === "taskbar"
          ? {
              ...m,
              availability: {
                state: "partial" as const,
                note: "Alignment needs Windows 11.",
              },
            }
          : m,
      ),
    );
    renderPage();

    expect(screen.getByText("Alignment needs Windows 11.")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: /Taskbar Studio/i })).toBeEnabled();
  });

  it("states a module's dependencies", () => {
    renderPage();
    expect(screen.getByText("Needs: widgets")).toBeInTheDocument();
  });

  it("passes a warning from the backend on to the user", async () => {
    mocked.setModuleEnabled.mockResolvedValue({
      modules: fixtures.modules,
      active_profile: "minimal",
      warnings: ["GPU temperature is unavailable on this machine."],
    });

    renderPage();
    await userEvent.click(screen.getByRole("switch", { name: /Enable System HUD/i }));

    expect(
      await screen.findByText("GPU temperature is unavailable on this machine."),
    ).toBeInTheDocument();
  });

  it("says that disabling a system module restores its settings", async () => {
    seed(
      fixtures.modules.map((m) =>
        m.id === "taskbar" ? { ...m, enabled: true, owned_settings: 2 } : m,
      ),
    );
    mocked.setModuleEnabled.mockResolvedValue({
      modules: fixtures.modules,
      active_profile: "minimal",
      warnings: [],
    });

    renderPage();
    await userEvent.click(
      screen.getByRole("switch", { name: /Disable Taskbar Studio/i }),
    );

    expect(
      await screen.findByText("Taskbar Studio disabled — settings restored"),
    ).toBeInTheDocument();
  });

  it("shows how many settings an enabled module owns", () => {
    seed(
      fixtures.modules.map((m) =>
        m.id === "taskbar" ? { ...m, enabled: true, owned_settings: 3 } : m,
      ),
    );
    renderPage();
    expect(screen.getByText("3 settings")).toBeInTheDocument();
  });

  it("opens a configuration panel on demand rather than up front", async () => {
    renderPage();
    expect(screen.queryByText("Finish")).not.toBeInTheDocument();

    await userEvent.click(screen.getAllByRole("button", { name: /Configure/ })[0]);
    await waitFor(() => expect(screen.getByText("Finish")).toBeInTheDocument());
  });
});
