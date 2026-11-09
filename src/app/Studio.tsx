/**
 * The studio shell: title bar, navigation, and the page currently showing.
 *
 * Routing is a piece of state rather than a router library — there are five
 * destinations, none of them deep-linkable from outside, and a frameless
 * desktop window has no address bar to reflect a URL into.
 */

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

import * as api from "@/services/api";
import { SURFACES_CHANGED } from "@/types/api";

import { useApp } from "@/stores/app";
import {
  Button,
  CloseIcon,
  GearIcon,
  HomeIcon,
  LayersIcon,
  MaximiseIcon,
  MinimiseIcon,
  PlugIcon,
  SlidersIcon,
  Spinner,
  cx,
  useToast,
} from "@/components/ui";
import { HomePage } from "@/pages/HomePage";
import { CustomizePage } from "@/pages/CustomizePage";
import { ProfilesPage } from "@/pages/ProfilesPage";
import { IntegrationsPage } from "@/pages/IntegrationsPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { Onboarding } from "@/app/Onboarding";
import { RecoveryBanner } from "@/app/RecoveryBanner";

export type PageId = "home" | "customize" | "profiles" | "integrations" | "settings";

const NAV: Array<{ id: PageId; label: string; icon: typeof HomeIcon }> = [
  { id: "home", label: "Home", icon: HomeIcon },
  { id: "customize", label: "Customize", icon: SlidersIcon },
  { id: "profiles", label: "Profiles", icon: LayersIcon },
  { id: "integrations", label: "Integrations", icon: PlugIcon },
  { id: "settings", label: "Settings", icon: GearIcon },
];

export function Studio() {
  const [page, setPage] = useState<PageId>("home");
  const { ready, loadError, load, firstRun, recovery } = useApp();
  const toast = useToast();

  useEffect(() => {
    void load();
  }, [load]);

  // The desktop can change from outside this window — the tray's restore,
  // or a profile applied from the command line. Reflect it rather than
  // leaving a stale picture on screen.
  useEffect(() => {
    const restored = listen<string>("overlay://restored", (event) => {
      toast.push("Original desktop restored", "ok", event.payload);
      void load();
    });
    const changed = listen(SURFACES_CHANGED, () => void load());
    return () => {
      void restored.then((f) => f());
      void changed.then((f) => f());
    };
  }, [load, toast]);

  // Displays can be plugged in or unplugged while OVERLAY is open, which
  // changes where widgets are allowed to sit.
  useEffect(() => {
    const onResize = () => {
      void api.refreshEnvironment().catch(() => undefined);
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  if (!ready) {
    return (
      <div className="flex h-full items-center justify-center text-[var(--text-tertiary)]">
        <Spinner size={20} />
      </div>
    );
  }

  if (loadError) {
    return <StartupFailure message={loadError} onRetry={() => void load()} />;
  }

  if (firstRun) {
    return <Onboarding onDone={() => setPage("customize")} />;
  }

  return (
    <div className="relative z-10 flex h-full flex-col overflow-hidden">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <Sidebar page={page} onNavigate={setPage} />
        <main className="min-w-0 flex-1 overflow-y-auto">
          {recovery && <RecoveryBanner />}
          <div key={page} className="animate-rise px-7 py-6">
            {page === "home" && <HomePage onNavigate={setPage} />}
            {page === "customize" && <CustomizePage />}
            {page === "profiles" && <ProfilesPage />}
            {page === "integrations" && <IntegrationsPage />}
            {page === "settings" && <SettingsPage />}
          </div>
        </main>
      </div>
    </div>
  );
}

function TitleBar() {
  const [maximised, setMaximised] = useState(false);

  const win = getCurrentWindow();

  return (
    <header
      className={cx(
        "drag flex h-11 shrink-0 items-center justify-between gap-3 border-b",
        "border-[var(--border-subtle)] bg-[var(--bg-raised)] pl-5 pr-2",
      )}
    >
      <div className="flex items-center gap-2.5">
        <Wordmark />
        <span className="text-[11px] tracking-[0.14em] text-[var(--text-tertiary)]">
          YOUR DESKTOP. YOUR RULES.
        </span>
      </div>

      <div className="no-drag flex items-center">
        <WindowButton label="Minimise" onClick={() => void win.minimize()}>
          <MinimiseIcon size={14} />
        </WindowButton>
        <WindowButton
          label={maximised ? "Restore" : "Maximise"}
          onClick={async () => {
            await win.toggleMaximize();
            setMaximised(await win.isMaximized());
          }}
        >
          <MaximiseIcon size={13} />
        </WindowButton>
        <WindowButton label="Close to tray" danger onClick={() => void win.hide()}>
          <CloseIcon size={14} />
        </WindowButton>
      </div>
    </header>
  );
}

function WindowButton({
  children,
  onClick,
  label,
  danger,
}: {
  children: React.ReactNode;
  onClick: () => void;
  label: string;
  danger?: boolean;
}) {
  return (
    <button
      aria-label={label}
      title={label}
      onClick={onClick}
      className={cx(
        "flex h-8 w-11 items-center justify-center rounded-[7px]",
        "text-[var(--text-tertiary)] transition-colors duration-[var(--dur-fast)]",
        danger
          ? "hover:bg-[var(--danger)] hover:text-white"
          : "hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]",
      )}
    >
      {children}
    </button>
  );
}

function Wordmark() {
  return (
    <span className="flex items-center gap-2">
      <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden>
        <rect x="4" y="5" width="13" height="4" rx="2" fill="currentColor" opacity="0.3" />
        <rect x="6" y="10" width="13" height="4" rx="2" fill="currentColor" opacity="0.6" />
        <rect x="8" y="15" width="13" height="4" rx="2" fill="currentColor" />
      </svg>
      <span className="text-[13px] font-semibold tracking-[0.18em]">OVERLAY</span>
    </span>
  );
}

function Sidebar({
  page,
  onNavigate,
}: {
  page: PageId;
  onNavigate: (page: PageId) => void;
}) {
  const { ownedSettings, activeProfileObject } = useApp();
  const profile = activeProfileObject();

  return (
    <nav
      className={cx(
        "flex w-[216px] shrink-0 flex-col justify-between border-r",
        "border-[var(--border-subtle)] bg-[var(--bg-sunken)] px-3 py-4",
      )}
    >
      <ul className="flex flex-col gap-0.5">
        {NAV.map(({ id, label, icon: Icon }) => {
          const active = page === id;
          return (
            <li key={id}>
              <button
                onClick={() => onNavigate(id)}
                aria-current={active ? "page" : undefined}
                className={cx(
                  "relative flex w-full items-center gap-2.5 rounded-[9px] px-3 py-2",
                  "text-[13px] transition-all duration-[var(--dur-fast)]",
                  active
                    ? "bg-[var(--bg-raised)] font-medium text-[var(--text-primary)]"
                    : "text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]",
                )}
              >
                {active && (
                  <span
                    className="absolute left-0 top-1/2 h-4 w-[2.5px] -translate-y-1/2 rounded-r-full"
                    style={{ background: "var(--accent-500)" }}
                  />
                )}
                <Icon size={15} className={active ? "text-[var(--accent-400)]" : undefined} />
                {label}
              </button>
            </li>
          );
        })}
      </ul>

      <div className="flex flex-col gap-2 px-1">
        <div className="rounded-[9px] border border-[var(--border-subtle)] bg-[var(--bg-raised)] px-3 py-2.5">
          <p className="text-[10px] uppercase tracking-[0.1em] text-[var(--text-tertiary)]">
            Active profile
          </p>
          <p className="mt-0.5 truncate text-[12.5px] font-medium">
            {profile?.name ?? "None"}
          </p>
          <p className="mt-1.5 text-[11px] text-[var(--text-tertiary)]">
            {ownedSettings === 0
              ? "Windows untouched"
              : `${ownedSettings} setting${ownedSettings === 1 ? "" : "s"} changed`}
          </p>
        </div>
      </div>
    </nav>
  );
}

/**
 * Shown when the backend could not start. Deliberately includes the recovery
 * command: if the studio is broken, the user's first question is how to get
 * their desktop back, and they should not have to find the answer elsewhere.
 */
function StartupFailure({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex h-full items-center justify-center p-10">
      <div className="card max-w-[520px] p-6">
        <h1 className="text-[15px] font-semibold">OVERLAY could not start</h1>
        <p className="mt-2 text-[13px] leading-relaxed text-[var(--text-secondary)]">
          {message}
        </p>
        <p className="mt-4 text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
          Your desktop settings are safe. If you need to undo OVERLAY's changes
          without this window, run the recovery command from a terminal:
        </p>
        <code className="mt-2 block rounded-[8px] border border-[var(--border-subtle)] bg-[var(--bg-sunken)] px-3 py-2 font-[var(--font-mono)] text-[12px] text-[var(--text-primary)]">
          overlay.exe --restore-original
        </code>
        <div className="mt-5 flex gap-2">
          <Button variant="primary" onClick={onRetry}>
            Try again
          </Button>
        </div>
      </div>
    </div>
  );
}
