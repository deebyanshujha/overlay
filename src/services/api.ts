/**
 * Typed wrappers over the Tauri command surface.
 *
 * Every backend call in the app goes through this file. Keeping it in one
 * place means command names are spelled once, argument shapes are checked
 * once, and nothing in `components/` has to know that Tauri exists.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  Bootstrap,
  ChangeRecord,
  Diagnostics,
  Environment,
  ImportResult,
  IntegrationStatus,
  Metrics,
  ModuleChange,
  MonitoringCapabilities,
  OwnedSummary,
  Profile,
  ProfileList,
  RecoveryStatus,
  RestorationReport,
  RestorePreview,
  ShelfState,
  SwitchReport,
  TransferReport,
  WidgetInstance,
  WidgetView,
} from "@/types/api";

// --------------------------------------------------------------------------
// System
// --------------------------------------------------------------------------

export const bootstrap = () => invoke<Bootstrap>("bootstrap");
export const getEnvironment = () => invoke<Environment>("get_environment");
export const refreshEnvironment = () => invoke<Environment>("refresh_environment");
export const getSettings = () => invoke<AppSettings>("get_settings");

export const setSetting = (key: string, value: string) =>
  invoke<AppSettings>("set_setting", { key, value });

export const completeOnboarding = () => invoke<void>("complete_onboarding");
export const getMetrics = () => invoke<Metrics>("get_metrics");
export const getCapabilities = () =>
  invoke<MonitoringCapabilities>("get_capabilities");

export const toggleControlCenter = () => invoke<boolean>("toggle_control_center");
export const closeControlCenter = () => invoke<void>("close_control_center");
export const runControl = (id: string) => invoke<string>("run_control", { id });

export const showMainWindow = () => invoke<void>("show_main_window");
export const hideToTray = () => invoke<void>("hide_to_tray");
export const quit = () => invoke<void>("quit");
export const diagnostics = () => invoke<Diagnostics>("diagnostics");
export const openDataFolder = () => invoke<void>("open_data_folder");

// --------------------------------------------------------------------------
// Modules
// --------------------------------------------------------------------------

export const listModules = () => invoke<ModuleChange>("list_modules");

export const setModuleEnabled = (
  moduleId: string,
  enabled: boolean,
  config?: Record<string, unknown>,
) =>
  invoke<ModuleChange>("set_module_enabled", {
    moduleId,
    enabled,
    config: config ?? null,
  });

export const configureModule = (
  moduleId: string,
  config: Record<string, unknown>,
) => invoke<ModuleChange>("configure_module", { moduleId, config });

export const moduleDefaultConfig = (moduleId: string) =>
  invoke<Record<string, unknown>>("module_default_config", { moduleId });

export const resetModuleConfig = (moduleId: string) =>
  invoke<ModuleChange>("reset_module_config", { moduleId });

// --------------------------------------------------------------------------
// Profiles
// --------------------------------------------------------------------------

export const listProfiles = () => invoke<ProfileList>("list_profiles");
export const getProfile = (id: string) => invoke<Profile>("get_profile", { id });

export const createProfile = (
  name: string,
  description: string,
  copyFrom?: string,
) =>
  invoke<Profile>("create_profile", {
    name,
    description,
    copyFrom: copyFrom ?? null,
  });

export const renameProfile = (id: string, name: string, description: string) =>
  invoke<Profile>("rename_profile", { id, name, description });

export const duplicateProfile = (id: string, name: string) =>
  invoke<Profile>("duplicate_profile", { id, name });

export const deleteProfile = (id: string) =>
  invoke<ProfileList>("delete_profile", { id });

export const setProfileModule = (
  profileId: string,
  moduleId: string,
  enabled: boolean,
  config?: Record<string, unknown>,
) =>
  invoke<Profile>("set_profile_module", {
    profileId,
    moduleId,
    enabled,
    config: config ?? null,
  });

export const validateProfile = (id: string) =>
  invoke<string[]>("validate_profile", { id });

export const switchProfile = (id: string) =>
  invoke<SwitchReport>("switch_profile", { id });

export const exportProfile = (id: string) =>
  invoke<string>("export_profile", { id });

export const exportProfileToFile = (id: string) =>
  invoke<string>("export_profile_to_file", { id });

export const importProfile = (json: string) =>
  invoke<ImportResult>("import_profile", { json });

export const importProfileFromFile = (path: string) =>
  invoke<ImportResult>("import_profile_from_file", { path });

export const setStartupProfile = (id: string | null) =>
  invoke<void>("set_startup_profile", { id });

export const getStartupProfile = () =>
  invoke<string | null>("get_startup_profile");

// --------------------------------------------------------------------------
// Restoration
// --------------------------------------------------------------------------

export const previewRestore = () => invoke<RestorePreview>("preview_restore");

export const restoreOriginalDesktop = () =>
  invoke<RestorationReport>("restore_original_desktop");

export const restoreModule = (moduleId: string) =>
  invoke<RestorationReport>("restore_module", { moduleId });

export const restorationHistory = (limit?: number) =>
  invoke<RestorationReport[]>("restoration_history", { limit: limit ?? null });

export const changeJournal = (limit?: number) =>
  invoke<ChangeRecord[]>("change_journal", { limit: limit ?? null });

export const ownedSettings = () => invoke<OwnedSummary[]>("owned_settings");
export const reconcileInterrupted = () => invoke<number>("reconcile_interrupted");
export const recoveryStatus = () => invoke<RecoveryStatus>("recovery_status");

// --------------------------------------------------------------------------
// Widgets and surfaces
// --------------------------------------------------------------------------

export const listWidgets = () => invoke<WidgetView[]>("list_widgets");

export const widgetConfig = (widgetId: string) =>
  invoke<WidgetInstance>("widget_config", { widgetId });

export const addWidget = (widgetId: string) =>
  invoke<WidgetView[]>("add_widget", { widgetId });

export const removeWidget = (widgetId: string) =>
  invoke<WidgetView[]>("remove_widget", { widgetId });

export const setWidgetEnabled = (widgetId: string, enabled: boolean) =>
  invoke<WidgetView[]>("set_widget_enabled", { widgetId, enabled });

export const moveWidget = (widgetId: string, x: number, y: number) =>
  invoke<void>("move_widget", { widgetId, x, y });

export const resizeWidget = (widgetId: string, width: number, height: number) =>
  invoke<void>("resize_widget", { widgetId, width, height });

export const configureWidget = (
  widgetId: string,
  patch: Record<string, unknown>,
) => invoke<WidgetView[]>("configure_widget", { widgetId, patch });

export const resetWidget = (widgetId: string) =>
  invoke<WidgetView[]>("reset_widget", { widgetId });

export const setSurfaceClickThrough = (label: string, enabled: boolean) =>
  invoke<void>("set_surface_click_through", { label, enabled });

export const closeSurface = (label: string) =>
  invoke<void>("close_surface", { label });

export const reassertLayering = () => invoke<void>("reassert_layering");

export const launchPinned = (path: string) =>
  invoke<void>("launch_pinned", { path });

// --------------------------------------------------------------------------
// Integrations
// --------------------------------------------------------------------------

export const listIntegrations = () =>
  invoke<IntegrationStatus[]>("list_integrations");

export const integrationStatus = (id: string) =>
  invoke<IntegrationStatus>("integration_status", { id });

export const launchIntegration = (id: string, silent = false) =>
  invoke<string>("launch_integration", { id, silent });

export const currentWallpaper = () => invoke<import("@/types/api").WallpaperInfo>("current_wallpaper");

export const openIntegrationSettings = (id: string) =>
  invoke<string>("open_integration_settings", { id });

export const refreshIntegrations = () =>
  invoke<IntegrationStatus[]>("refresh_integrations");

// --------------------------------------------------------------------------
// Dropzone
// --------------------------------------------------------------------------

export const shelfAdd = (paths: string[]) =>
  invoke<ShelfState>("shelf_add", { paths });

export const shelfList = () => invoke<ShelfState>("shelf_list");
export const shelfRefresh = () => invoke<ShelfState>("shelf_refresh");

export const shelfRemove = (ids: string[]) =>
  invoke<ShelfState>("shelf_remove", { ids });

export const shelfClear = () => invoke<ShelfState>("shelf_clear");

export const shelfCopyTo = (ids: string[], destination: string) =>
  invoke<TransferReport>("shelf_copy_to", { ids, destination });

export const shelfMoveTo = (
  ids: string[],
  destination: string,
  confirmed: boolean,
) => invoke<TransferReport>("shelf_move_to", { ids, destination, confirmed });

export const shelfOpen = (id: string) => invoke<void>("shelf_open", { id });
export const shelfReveal = (id: string) => invoke<void>("shelf_reveal", { id });
