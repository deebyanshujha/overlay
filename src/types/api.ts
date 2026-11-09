/**
 * Types mirroring the Rust command surface.
 *
 * Hand-written rather than generated, and kept deliberately close to the
 * serde output: when these drift from `src-tauri`, the mismatch shows up as a
 * type error at the call site rather than as `undefined` at runtime.
 */

// --------------------------------------------------------------------------
// Errors
// --------------------------------------------------------------------------

/** Every command rejects with this shape — see `error.rs`. */
export interface OverlayError {
  kind:
    | "registry"
    | "win32"
    | "database"
    | "io"
    | "config"
    | "unknown_module"
    | "unavailable"
    | "conflict"
    | "missing_dependency"
    | "unknown_profile"
    | "profile_immutable"
    | "verification_failed"
    | "ownership_conflict"
    | "integration"
    | "other";
  message: string;
}

export function isOverlayError(value: unknown): value is OverlayError {
  return (
    typeof value === "object" &&
    value !== null &&
    "kind" in value &&
    "message" in value
  );
}

/** Human-readable message for anything a command can reject with. */
export function errorMessage(value: unknown): string {
  if (isOverlayError(value)) return value.message;
  if (value instanceof Error) return value.message;
  if (typeof value === "string") return value;
  return "Something went wrong.";
}

// --------------------------------------------------------------------------
// Environment
// --------------------------------------------------------------------------

export interface MonitorInfo {
  device: string;
  primary: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  work_x: number;
  work_y: number;
  work_width: number;
  work_height: number;
  scale: number;
}

export interface Environment {
  windows_version: string;
  build: number;
  is_windows_11: boolean;
  shell_running: boolean;
  taskbar_accent_supported: boolean;
  monitors: MonitorInfo[];
  elevated: boolean;
}

// --------------------------------------------------------------------------
// Modules
// --------------------------------------------------------------------------

export type ModuleCategory =
  | "appearance"
  | "widgets"
  | "surfaces"
  | "integrations";

export type Availability =
  | { state: "available" }
  | { state: "partial"; note: string }
  | { state: "unavailable"; reason: string };

export interface ModuleStatus {
  id: string;
  name: string;
  description: string;
  version: string;
  category: ModuleCategory;
  availability: Availability;
  enabled: boolean;
  touches_system: boolean;
  requires: string[];
  conflicts_with: string[];
  owned_settings: number;
  config: Record<string, unknown>;
  notes: string[];
}

export interface ModuleOutcome {
  module_id: string;
  ok: boolean;
  error: string | null;
}

export interface ModuleChange {
  modules: ModuleStatus[];
  active_profile: string | null;
  warnings: string[];
}

// --------------------------------------------------------------------------
// Profiles
// --------------------------------------------------------------------------

export interface ModuleSetting {
  enabled: boolean;
  config: Record<string, unknown>;
}

export interface Profile {
  id: string;
  name: string;
  description: string;
  builtin: boolean;
  theme: ThemeName;
  accent: AccentName;
  modules: Record<string, ModuleSetting>;
  created_at: number;
  updated_at: number;
}

export interface ProfileList {
  profiles: Profile[];
  active: string | null;
}

export interface SwitchReport {
  from: string | null;
  to: string;
  to_name: string;
  disabled: ModuleOutcome[];
  enabled: ModuleOutcome[];
  restored: RestoreOutcome[];
  rolled_back: boolean;
  warnings: string[];
  succeeded: boolean;
  duration_ms: number;
}

export interface ImportResult {
  profile: Profile;
  warnings: string[];
}

export const ORIGINAL_WINDOWS = "original_windows";

// --------------------------------------------------------------------------
// Restoration
// --------------------------------------------------------------------------

export type RestoreStatus =
  | "restored"
  | "already_original"
  | "conflict_skipped"
  | "failed";

export interface RestoreOutcome {
  target_id: string;
  label: string;
  module_id: string;
  restored_to: string;
  status: RestoreStatus;
  detail: string;
}

export interface RestorationReport {
  started_at: number;
  finished_at: number;
  settings: RestoreOutcome[];
  surfaces_closed: string[];
  verified: boolean;
  notes: string[];
  scope: string;
}

export interface RestorePreview {
  settings: RestoreOutcome[];
  surfaces: string[];
  guarantees: string[];
}

/**
 * One-line summary of a restoration, matching `RestorationReport::summary`
 * on the Rust side.
 *
 * Reimplemented here rather than serialised because it is presentation, and
 * the places that need it (a toast, the control centre) want it without
 * another round trip.
 */
export function restorationSummary(report: RestorationReport): string {
  const count = (status: RestoreStatus) =>
    report.settings.filter((s) => s.status === status).length;

  if (report.settings.length === 0 && report.surfaces_closed.length === 0) {
    return "Nothing to undo — your desktop was already untouched.";
  }

  const parts: string[] = [];
  const restored = count("restored");
  if (restored > 0) parts.push(`${restored} setting(s) restored`);
  if (report.surfaces_closed.length > 0) {
    parts.push(`${report.surfaces_closed.length} window(s) closed`);
  }
  const already = count("already_original");
  if (already > 0) parts.push(`${already} already original`);
  const conflicts = count("conflict_skipped");
  if (conflicts > 0) parts.push(`${conflicts} left alone (changed elsewhere)`);
  const failed = count("failed");
  if (failed > 0) parts.push(`${failed} could not be restored`);
  return parts.join(", ");
}

export type ChangeStatus = "pending" | "applied" | "failed" | "reverted";

export interface ChangeRecord {
  id: number;
  target_id: string;
  module_id: string;
  label: string;
  before: string;
  after: string;
  status: ChangeStatus;
  detail: string;
  started_at: number;
  settled_at: number | null;
}

export interface OwnedSummary {
  target_id: string;
  label: string;
  module_id: string;
  applied: string;
  original: string;
  applied_at: number;
}

export interface BackupFile {
  name: string;
  bytes: number;
  modified: number;
}

export interface RecoveryStatus {
  unsettled_changes: number;
  switch_in_flight: boolean;
  owned_settings: number;
  database: string;
  backups: BackupFile[];
  recovery_command: string;
}

// --------------------------------------------------------------------------
// Integrations
// --------------------------------------------------------------------------

export type DetectionSource =
  | "uninstall_registry"
  | "known_path"
  | "data_directory_only"
  | "not_found";

export interface Capability {
  id: string;
  label: string;
  supported: boolean;
  detail: string;
}

export interface IntegrationStatus {
  id: string;
  name: string;
  description: string;
  installed: boolean;
  running: boolean;
  detected_via: DetectionSource;
  exe_path: string | null;
  data_dir: string | null;
  version: string | null;
  capabilities: Capability[];
  details: [string, string][];
  limitations: string[];
}

export interface WallpaperInfo {
  available: boolean;
  provider: string | null;
  topic: string | null;
  author: string | null;
  source_url: string | null;
  width: number | null;
  height: number | null;
  path: string | null;
  next_change: number | null;
  note: string | null;
}

// --------------------------------------------------------------------------
// Widgets
// --------------------------------------------------------------------------

export type WidgetTheme = "glass" | "solid" | "outline" | "bare";
export type WidgetFont = "sans" | "mono" | "display";
export type ZMode = "desktop" | "normal" | "top";

export interface WidgetKind {
  id: string;
  name: string;
  description: string;
  default_width: number;
  default_height: number;
  min_width: number;
  min_height: number;
  default_interval_ms: number;
  needs_sensor: boolean;
}

export interface WidgetInstance {
  id: string;
  enabled: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  monitor: string;
  opacity: number;
  theme: WidgetTheme;
  font: WidgetFont;
  font_scale: number;
  interval_ms: number;
  locked: boolean;
  click_through: boolean;
  z: ZMode;
  settings: Record<string, unknown>;
}

export interface WidgetView {
  instance: WidgetInstance;
  kind_name: string;
  needs_sensor: boolean;
}

// --------------------------------------------------------------------------
// Monitoring
// --------------------------------------------------------------------------

export interface DiskUsage {
  mount: string;
  total_bytes: number;
  free_bytes: number;
}

export interface Unavailable {
  metric: string;
  reason: string;
}

export interface Metrics {
  cpu_percent: number | null;
  cpu_name: string;
  cpu_cores: number;
  memory_used_bytes: number | null;
  memory_total_bytes: number | null;
  gpu_percent: number | null;
  gpu_temperature_c: number | null;
  gpu_name: string | null;
  battery_percent: number | null;
  battery_charging: boolean | null;
  battery_minutes_left: number | null;
  network_down_bps: number | null;
  network_up_bps: number | null;
  network_interface: string | null;
  disks: DiskUsage[];
  uptime_seconds: number | null;
  unavailable: Unavailable[];
}

export interface MonitoringCapabilities {
  cpu: boolean;
  memory: boolean;
  gpu_usage: boolean;
  gpu_temperature: boolean;
  battery: boolean;
  network: boolean;
  storage: boolean;
  uptime: boolean;
  media: boolean;
  gpu_detail: string;
  media_detail: string;
}

// --------------------------------------------------------------------------
// Control Center
// --------------------------------------------------------------------------

export type ControlKind =
  | { kind: "action" }
  | { kind: "opens_windows"; target: string };

export interface Control {
  id: string;
  label: string;
  description: string;
  kind: ControlKind;
}

// --------------------------------------------------------------------------
// Dropzone
// --------------------------------------------------------------------------

export interface ShelfItem {
  id: string;
  path: string;
  name: string;
  size_bytes: number;
  is_dir: boolean;
  exists: boolean;
  extension: string;
  added_at: number;
}

export interface ShelfState {
  items: ShelfItem[];
  total_bytes: number;
  missing: number;
}

export interface TransferReport {
  moved: boolean;
  succeeded: string[];
  skipped: string[];
  failed: [string, string][];
}

// --------------------------------------------------------------------------
// Settings and bootstrap
// --------------------------------------------------------------------------

export type ThemeName = "minimal" | "futuristic";
export type AccentName = "ice" | "violet" | "emerald" | "amber" | "rose";

export interface AppSettings {
  theme: ThemeName;
  accent: AccentName;
  start_with_windows: boolean;
  start_minimised: boolean;
  restore_profile_on_start: boolean;
  control_center_hotkey: string;
  reduce_motion: boolean;
}

export interface RecoveryNotice {
  message: string;
  details: string[];
}

export interface Bootstrap {
  environment: Environment;
  modules: ModuleStatus[];
  profiles: Profile[];
  active_profile: string | null;
  integrations: IntegrationStatus[];
  owned_settings: number;
  capabilities: MonitoringCapabilities;
  widget_catalog: WidgetKind[];
  controls: Control[];
  settings: AppSettings;
  recovery: RecoveryNotice | null;
  first_run: boolean;
}

export interface Diagnostics {
  environment: Environment;
  data_dir: string;
  database_bytes: number;
  open_surfaces: string[];
  owned_settings: number;
  unsettled_changes: number;
  recent_changes: ChangeRecord[];
  memory_bytes: number | null;
  monitoring: MonitoringCapabilities;
  schema_version: number;
  app_version: string;
}

/**
 * Emitted by the backend whenever a surface's configuration changes.
 *
 * Surfaces listen for this rather than polling: a timer in each widget
 * window meant continuous work on the thread that has to stay responsive.
 */
export const SURFACES_CHANGED = "overlay://surfaces-changed";
