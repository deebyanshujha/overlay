/**
 * Shared test data, shaped exactly like what the Rust side serialises.
 *
 * Built from the real backend's output rather than invented, so a test that
 * passes here is exercising the data the app actually receives.
 */

import type {
  Bootstrap,
  Control,
  Environment,
  IntegrationStatus,
  ModuleStatus,
  MonitoringCapabilities,
  Profile,
  RestorationReport,
  RestoreOutcome,
  WidgetKind,
} from "@/types/api";

export const environment: Environment = {
  windows_version: "Windows 11 (10.0.26100)",
  build: 26100,
  is_windows_11: true,
  shell_running: true,
  taskbar_accent_supported: true,
  elevated: false,
  monitors: [
    {
      device: "\\\\.\\DISPLAY1",
      primary: true,
      x: 0,
      y: 0,
      width: 1920,
      height: 1080,
      work_x: 0,
      work_y: 0,
      work_width: 1920,
      work_height: 1020,
      scale: 1.25,
    },
  ],
};

export function moduleStatus(over: Partial<ModuleStatus> = {}): ModuleStatus {
  return {
    id: "taskbar",
    name: "Taskbar Studio",
    description: "Translucency, alignment, icon size and auto-hide.",
    version: "1.0.0",
    category: "appearance",
    availability: { state: "available" },
    enabled: false,
    touches_system: true,
    requires: [],
    conflicts_with: [],
    owned_settings: 0,
    config: { finish: "blurred", opacity: 0.35, tint: 0x0e1016 },
    notes: [],
    ...over,
  };
}

export const modules: ModuleStatus[] = [
  moduleStatus(),
  moduleStatus({
    id: "desktop_icons",
    name: "Desktop Icons",
    description: "Hide or show the icons on your desktop.",
    config: { show_icons: false },
  }),
  moduleStatus({
    id: "widgets",
    name: "Desktop Widgets",
    description: "Clocks, calendars and system monitors.",
    category: "widgets",
    touches_system: false,
    config: { widgets: [], allow_drag: true },
  }),
  moduleStatus({
    id: "hud",
    name: "System HUD",
    description: "A compact strip of live system metrics.",
    category: "widgets",
    touches_system: false,
    requires: ["widgets"],
    config: { metrics: ["cpu"], anchor: "top_right" },
  }),
  moduleStatus({
    id: "dropzone",
    name: "Dropzone",
    description: "A floating shelf for files in transit.",
    category: "surfaces",
    touches_system: false,
    config: {},
  }),
];

export function profile(over: Partial<Profile> = {}): Profile {
  return {
    id: "minimal",
    name: "Minimal",
    description: "A clock, a clear taskbar and nothing else.",
    builtin: true,
    theme: "minimal",
    accent: "ice",
    modules: {},
    created_at: 0,
    updated_at: 0,
    ...over,
  };
}

export const profiles: Profile[] = [
  profile({
    id: "original_windows",
    name: "Original Windows",
    description: "Your desktop exactly as OVERLAY first found it.",
    modules: {},
  }),
  profile(),
  profile({
    id: "developer",
    name: "Developer",
    description: "System HUD and monospaced widgets.",
    modules: {
      widgets: { enabled: true, config: {} },
      hud: { enabled: true, config: {} },
    },
  }),
];

export const capabilities: MonitoringCapabilities = {
  cpu: true,
  memory: true,
  gpu_usage: false,
  gpu_temperature: false,
  battery: true,
  network: true,
  storage: true,
  uptime: true,
  media: false,
  gpu_detail:
    "No NVIDIA GPU reachable through NVML. AMD and Intel GPUs do not expose load to an unprivileged process on Windows.",
  media_detail: "Media session reading is not implemented in this build.",
};

export const integrations: IntegrationStatus[] = [
  {
    id: "wallflow",
    name: "WallFlow",
    description: "Wallpaper manager.",
    installed: true,
    running: true,
    detected_via: "uninstall_registry",
    exe_path: "C:\\Users\\x\\AppData\\Local\\WallFlow\\wallflow.exe",
    data_dir: "C:\\Users\\x\\AppData\\Local\\com.wallflow.app",
    version: "0.1.0",
    capabilities: [
      { id: "detect", label: "Detect installation", supported: true, detail: "" },
      {
        id: "request_change",
        label: "Request a wallpaper change",
        supported: false,
        detail: "The installed build has no command-line entry point for this.",
      },
    ],
    details: [["Provider", "wallhaven"]],
    limitations: ["OVERLAY cannot ask WallFlow to change the wallpaper."],
  },
  {
    id: "quickdock",
    name: "QuickDock",
    description: "Keyboard-first launcher.",
    installed: true,
    running: false,
    detected_via: "uninstall_registry",
    exe_path: "C:\\Users\\x\\AppData\\Local\\QuickDock\\quickdock.exe",
    data_dir: "C:\\Users\\x\\AppData\\Roaming\\com.quickdock.app",
    version: "1.0.0",
    capabilities: [
      { id: "launch", label: "Start QuickDock", supported: true, detail: "" },
      { id: "quit", label: "Quit from OVERLAY", supported: false, detail: "Not attempted." },
    ],
    details: [["Hotkey", "Ctrl + Space"]],
    limitations: [],
  },
];

export const widgetCatalog: WidgetKind[] = [
  {
    id: "clock",
    name: "Clock",
    description: "Large time display.",
    default_width: 380,
    default_height: 150,
    min_width: 160,
    min_height: 70,
    default_interval_ms: 1000,
    needs_sensor: false,
  },
  {
    id: "gpu",
    name: "GPU Usage",
    description: "Graphics load.",
    default_width: 260,
    default_height: 130,
    min_width: 160,
    min_height: 90,
    default_interval_ms: 2000,
    needs_sensor: true,
  },
];

export const controls: Control[] = [
  {
    id: "widgets",
    label: "Show or hide widgets",
    description: "Toggle every OVERLAY widget at once.",
    kind: { kind: "action" },
  },
  {
    id: "volume",
    label: "Volume",
    description: "Opens Windows sound settings.",
    kind: { kind: "opens_windows", target: "ms-settings:sound" },
  },
];

export function bootstrap(over: Partial<Bootstrap> = {}): Bootstrap {
  return {
    environment,
    modules,
    profiles,
    active_profile: "minimal",
    integrations,
    owned_settings: 0,
    capabilities,
    widget_catalog: widgetCatalog,
    controls,
    settings: {
      theme: "minimal",
      accent: "ice",
      start_with_windows: false,
      start_minimised: false,
      restore_profile_on_start: true,
      control_center_hotkey: "Alt+Shift+O",
      reduce_motion: false,
    },
    recovery: null,
    first_run: false,
    ...over,
  };
}

export function restoreOutcome(over: Partial<RestoreOutcome> = {}): RestoreOutcome {
  return {
    target_id: "reg:HKCU\\Software\\X\\TaskbarAl",
    label: "TaskbarAl (Software\\X)",
    module_id: "taskbar",
    restored_to: "(not set)",
    status: "restored",
    detail: "",
    ...over,
  };
}

export function restorationReport(
  over: Partial<RestorationReport> = {},
): RestorationReport {
  return {
    started_at: 0,
    finished_at: 1,
    settings: [restoreOutcome()],
    surfaces_closed: ["widget-clock"],
    verified: true,
    notes: [],
    scope: "all",
    ...over,
  };
}
