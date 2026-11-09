/**
 * The studio's state.
 *
 * One store, because almost every page shows some projection of the same
 * thing: which profile is active, which modules are on, what OVERLAY
 * currently owns. Keeping that in one place is what stops the Home page and
 * the Customize page disagreeing after a toggle.
 *
 * Mutations go through the backend first and update state from what it
 * returns — never optimistically. A toggle that failed must not look like it
 * worked, because the user would then believe their desktop changed when it
 * did not.
 */

import { create } from "zustand";
import * as api from "@/services/api";
import { errorMessage } from "@/types/api";
import type {
  AppSettings,
  Bootstrap,
  Control,
  Environment,
  IntegrationStatus,
  ModuleStatus,
  MonitoringCapabilities,
  Profile,
  RecoveryNotice,
  SwitchReport,
  WidgetKind,
} from "@/types/api";

interface AppStore {
  ready: boolean;
  loadError: string | null;

  environment: Environment | null;
  modules: ModuleStatus[];
  profiles: Profile[];
  activeProfile: string | null;
  integrations: IntegrationStatus[];
  ownedSettings: number;
  capabilities: MonitoringCapabilities | null;
  widgetCatalog: WidgetKind[];
  controls: Control[];
  settings: AppSettings | null;
  recovery: RecoveryNotice | null;
  firstRun: boolean;

  /** True while a profile switch is running, so the UI can lock out a second. */
  switching: boolean;

  load: () => Promise<void>;
  refreshModules: () => Promise<void>;
  refreshProfiles: () => Promise<void>;
  refreshIntegrations: () => Promise<void>;

  setModuleEnabled: (
    id: string,
    enabled: boolean,
  ) => Promise<{ warnings: string[] }>;
  configureModule: (
    id: string,
    config: Record<string, unknown>,
  ) => Promise<{ warnings: string[] }>;

  switchProfile: (id: string) => Promise<SwitchReport>;
  applySettings: (next: AppSettings) => void;
  setSetting: (key: string, value: string) => Promise<void>;
  dismissRecovery: () => void;
  completeOnboarding: () => Promise<void>;

  moduleById: (id: string) => ModuleStatus | undefined;
  activeProfileObject: () => Profile | undefined;
}

function applyFromBootstrap(data: Bootstrap) {
  return {
    ready: true,
    loadError: null,
    environment: data.environment,
    modules: data.modules,
    profiles: data.profiles,
    activeProfile: data.active_profile,
    integrations: data.integrations,
    ownedSettings: data.owned_settings,
    capabilities: data.capabilities,
    widgetCatalog: data.widget_catalog,
    controls: data.controls,
    settings: data.settings,
    recovery: data.recovery,
    firstRun: data.first_run,
  };
}

export const useApp = create<AppStore>((set, get) => ({
  ready: false,
  loadError: null,
  environment: null,
  modules: [],
  profiles: [],
  activeProfile: null,
  integrations: [],
  ownedSettings: 0,
  capabilities: null,
  widgetCatalog: [],
  controls: [],
  settings: null,
  recovery: null,
  firstRun: false,
  switching: false,

  load: async () => {
    try {
      const data = await api.bootstrap();
      set(applyFromBootstrap(data));
      applyTheme(data.settings);
    } catch (e) {
      set({ ready: true, loadError: errorMessage(e) });
    }
  },

  refreshModules: async () => {
    const result = await api.listModules();
    set({ modules: result.modules, activeProfile: result.active_profile });
  },

  refreshProfiles: async () => {
    const list = await api.listProfiles();
    set({ profiles: list.profiles, activeProfile: list.active });
  },

  refreshIntegrations: async () => {
    set({ integrations: await api.refreshIntegrations() });
  },

  setModuleEnabled: async (id, enabled) => {
    const result = await api.setModuleEnabled(id, enabled);
    set({ modules: result.modules, activeProfile: result.active_profile });
    // Owned-setting count drives the Home summary and the restore button.
    void refreshOwned(set);
    return { warnings: result.warnings };
  },

  configureModule: async (id, config) => {
    const result = await api.configureModule(id, config);
    set({ modules: result.modules, activeProfile: result.active_profile });
    void refreshOwned(set);
    return { warnings: result.warnings };
  },

  switchProfile: async (id) => {
    set({ switching: true });
    try {
      const report = await api.switchProfile(id);
      // Re-read everything: a switch can change modules, ownership and the
      // app's own theme at once.
      const data = await api.bootstrap();
      set({ ...applyFromBootstrap(data), switching: false });
      applyTheme(data.settings);
      return report;
    } catch (e) {
      set({ switching: false });
      throw e;
    }
  },

  applySettings: (next) => {
    set({ settings: next });
    applyTheme(next);
  },

  setSetting: async (key, value) => {
    const next = await api.setSetting(key, value);
    set({ settings: next });
    applyTheme(next);
  },

  dismissRecovery: () => set({ recovery: null }),

  completeOnboarding: async () => {
    await api.completeOnboarding();
    set({ firstRun: false });
  },

  moduleById: (id) => get().modules.find((m) => m.id === id),
  activeProfileObject: () => {
    const { profiles, activeProfile } = get();
    return profiles.find((p) => p.id === activeProfile);
  },
}));

async function refreshOwned(set: (partial: Partial<AppStore>) => void) {
  try {
    const owned = await api.ownedSettings();
    set({ ownedSettings: owned.length });
  } catch {
    // A failed count is cosmetic; never let it break a toggle.
  }
}

/**
 * Applies theme, accent and motion preference to the document.
 *
 * Done by attribute rather than by class so the CSS can express the whole
 * theme as variable overrides, and so surface windows — which mount a
 * different React tree — pick up the same look from the same code.
 */
export function applyTheme(settings: Pick<AppSettings, "theme" | "accent" | "reduce_motion">) {
  const root = document.documentElement;
  root.dataset.theme = settings.theme;
  root.dataset.accent = settings.accent;
  root.dataset.reduceMotion = String(settings.reduce_motion);
}
