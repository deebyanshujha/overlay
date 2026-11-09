//! Bootstrap, environment, settings, metrics and diagnostics.

use super::{AppSettings, Bootstrap, RecoveryNotice};
use crate::error::Result;
use crate::modules::control_center::{self, ControlCenterModule};
use crate::monitoring;
use crate::state::AppState;
use crate::sys::Environment;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager, State};

pub const KEY_THEME: &str = "theme";
pub const KEY_ACCENT: &str = "accent";
pub const KEY_AUTOSTART: &str = "start_with_windows";
pub const KEY_START_MINIMISED: &str = "start_minimised";
pub const KEY_RESTORE_ON_START: &str = "restore_profile_on_start";
pub const KEY_HOTKEY: &str = "control_center_hotkey";
pub const KEY_REDUCE_MOTION: &str = "reduce_motion";
pub const KEY_ONBOARDED: &str = "onboarding_complete";

pub fn app_settings(state: &AppState) -> AppSettings {
    AppSettings {
        theme: state.setting(KEY_THEME).unwrap_or_else(|| "minimal".into()),
        accent: state.setting(KEY_ACCENT).unwrap_or_else(|| "ice".into()),
        start_with_windows: state.flag(KEY_AUTOSTART, false),
        start_minimised: state.flag(KEY_START_MINIMISED, false),
        restore_profile_on_start: state.flag(KEY_RESTORE_ON_START, true),
        control_center_hotkey: state
            .setting(KEY_HOTKEY)
            .unwrap_or_else(|| control_center::DEFAULT_HOTKEY.into()),
        reduce_motion: state.flag(KEY_REDUCE_MOTION, false),
    }
}

/// Everything the UI needs to render its first frame.
#[tauri::command(async)]
pub fn bootstrap(app: AppHandle, state: State<'_, AppState>) -> Result<Bootstrap> {
    let ctx = state.ctx(&app);
    let profiles = state.profiles.list()?;
    let active = state.profiles.active_id()?;

    // Module configs come from the active profile so the UI shows what is
    // actually in force, not a default that nothing is using.
    let active_profile = active.as_deref().and_then(|id| state.profiles.get(id).ok());
    let configs: BTreeMap<String, serde_json::Value> = active_profile
        .as_ref()
        .map(|p| p.configs())
        .unwrap_or_default();
    let enabled_in_profile = active_profile
        .as_ref()
        .map(|p| p.enabled_ids())
        .unwrap_or_default();

    // Cleared notices are stored as an empty string, not removed, so the
    // blank must be filtered out here — otherwise the UI renders an empty
    // warning banner with no text in it, forever.
    let recovery = state
        .setting("pending_recovery_notice")
        .filter(|json| !json.trim().is_empty())
        .and_then(|json| serde_json::from_str::<RecoveryNotice>(&json).ok())
        .filter(|notice| !notice.message.trim().is_empty());
    if recovery.is_some() {
        // Shown once; clearing it here means a notice cannot nag forever.
        let _ = state.set_setting("pending_recovery_notice", "");
    }

    Ok(Bootstrap {
        environment: (*state.env()).clone(),
        modules: state.registry.statuses(&ctx, &configs, &enabled_in_profile),
        profiles,
        active_profile: active,
        integrations: super::integrations::all_statuses(),
        owned_settings: state.journal.owned().map(|o| o.len()).unwrap_or(0),
        capabilities: monitoring::capabilities().clone(),
        widget_catalog: crate::modules::widgets::CATALOG,
        controls: control_center::available_controls(),
        settings: app_settings(&state),
        recovery,
        first_run: !state.flag(KEY_ONBOARDED, false),
    })
}

#[tauri::command(async)]
pub fn get_environment(state: State<'_, AppState>) -> Environment {
    (*state.env()).clone()
}

/// Re-detects displays. Called by the frontend when Windows reports a change.
#[tauri::command(async)]
pub fn refresh_environment(state: State<'_, AppState>) -> Environment {
    (*state.refresh_env()).clone()
}

#[tauri::command(async)]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    app_settings(&state)
}

#[tauri::command(async)]
pub fn set_setting(
    app: AppHandle,
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<AppSettings> {
    match key.as_str() {
        KEY_AUTOSTART => {
            let enabled = value == "true";
            crate::autostart::set(&app, enabled)?;
            state.set_flag(KEY_AUTOSTART, enabled)?;
        }
        KEY_HOTKEY => {
            // Registering here rather than only storing means an unusable
            // shortcut is rejected while the user is still looking at it.
            crate::shortcuts::rebind(&app, &value)?;
            state.set_setting(KEY_HOTKEY, &value)?;
        }
        KEY_THEME | KEY_ACCENT | KEY_START_MINIMISED | KEY_RESTORE_ON_START
        | KEY_REDUCE_MOTION | KEY_ONBOARDED => {
            state.set_setting(&key, &value)?;
        }
        other => {
            return Err(crate::error::OverlayError::Config(format!(
                "unknown setting `{other}`"
            )))
        }
    }
    Ok(app_settings(&state))
}

#[tauri::command(async)]
pub fn complete_onboarding(state: State<'_, AppState>) -> Result<()> {
    state.set_flag(KEY_ONBOARDED, true)
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn get_metrics() -> monitoring::Metrics {
    monitoring::shared().lock().sample()
}

#[tauri::command(async)]
pub fn get_capabilities() -> monitoring::Capabilities {
    monitoring::capabilities().clone()
}

// ---------------------------------------------------------------------------
// Control Center
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn toggle_control_center(app: AppHandle, state: State<'_, AppState>) -> Result<bool> {
    ControlCenterModule::toggle(&state.ctx(&app))
}

#[tauri::command(async)]
pub fn close_control_center(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    ControlCenterModule::close(&state.ctx(&app))
}

/// Runs one Control Center entry.
///
/// A control that opens a Windows page is dispatched by **id**, and the URL
/// comes from OVERLAY's own catalogue — the frontend never supplies a target.
/// That is what stops this being a general-purpose "open anything" command.
#[tauri::command(async)]
pub fn run_control(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<String> {
    let Some(control) = control_center::control(&id) else {
        return Err(crate::error::OverlayError::Config(format!(
            "unknown control `{id}`"
        )));
    };

    match control.kind {
        control_center::ControlKind::OpensWindows { target } => {
            crate::sys::process::open_external(&target)?;
            Ok(format!("Opened {}", control.label))
        }
        control_center::ControlKind::Action => {
            let ctx = state.ctx(&app);
            match id.as_str() {
                "widgets" => {
                    let module = state.registry.get("widgets")?;
                    if module.is_active(&ctx) {
                        module.disable(&ctx)?;
                        Ok("Widgets hidden".into())
                    } else {
                        let config = active_config(&state, "widgets")
                            .unwrap_or_else(|| module.default_config());
                        module.enable(&ctx, &config)?;
                        Ok("Widgets shown".into())
                    }
                }
                "focus" => {
                    let closed = state.surfaces.hide_all(&app);
                    Ok(format!("Focus mode: {} surface(s) hidden", closed.len()))
                }
                "wallflow" => {
                    crate::integrations::wallflow::WallFlow::launch()?;
                    Ok("WallFlow opened".into())
                }
                "quickdock" => {
                    crate::integrations::quickdock::QuickDock::launch(false)?;
                    Ok("QuickDock started".into())
                }
                // `profile` and `restore` are handled by dedicated commands so
                // they can carry their own confirmation flow.
                "profile" | "restore" => Ok(String::new()),
                other => Err(crate::error::OverlayError::Config(format!(
                    "`{other}` has no action"
                ))),
            }
        }
    }
}

fn active_config(state: &AppState, module_id: &str) -> Option<serde_json::Value> {
    let active = state.profiles.active_id().ok().flatten()?;
    let profile = state.profiles.get(&active).ok()?;
    profile.modules.get(module_id).map(|s| s.config.clone())
}

// ---------------------------------------------------------------------------
// Window chrome and lifecycle
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn show_main_window(app: AppHandle) {
    crate::show_main(&app);
}

#[tauri::command(async)]
pub fn hide_to_tray(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

/// Quits properly: every surface closed, then exit.
///
/// Going through this rather than `app.exit` directly is what stops a widget
/// window outliving the application.
#[tauri::command(async)]
pub fn quit(app: AppHandle, state: State<'_, AppState>) {
    state.surfaces.hide_all(&app);
    app.exit(0);
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Diagnostics {
    pub environment: Environment,
    pub data_dir: String,
    pub database_bytes: u64,
    pub open_surfaces: Vec<String>,
    pub owned_settings: usize,
    pub unsettled_changes: usize,
    pub recent_changes: Vec<crate::journal::ChangeRecord>,
    pub memory_bytes: Option<u64>,
    pub monitoring: monitoring::Capabilities,
    pub schema_version: i64,
    pub app_version: &'static str,
}

#[tauri::command(async)]
pub fn diagnostics(state: State<'_, AppState>) -> Result<Diagnostics> {
    let database_bytes = std::fs::metadata(&state.paths.db).map(|m| m.len()).unwrap_or(0);
    Ok(Diagnostics {
        environment: (*state.env()).clone(),
        data_dir: state.paths.root.to_string_lossy().to_string(),
        database_bytes,
        open_surfaces: state.surfaces.open_labels(),
        owned_settings: state.journal.owned().map(|o| o.len()).unwrap_or(0),
        unsettled_changes: state.journal.unsettled().map(|u| u.len()).unwrap_or(0),
        recent_changes: state.journal.recent(50)?,
        memory_bytes: own_memory(),
        monitoring: monitoring::capabilities().clone(),
        schema_version: crate::storage::SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION"),
    })
}

/// This process's working set. Measured, not estimated — the diagnostics page
/// would be worthless with an invented number on it.
#[cfg(windows)]
fn own_memory() -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    unsafe {
        GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb)
            .ok()
            .map(|_| counters.WorkingSetSize as u64)
    }
}

#[cfg(not(windows))]
fn own_memory() -> Option<u64> {
    None
}

/// Opens the folder holding the database, backups and exports.
#[tauri::command(async)]
pub fn open_data_folder(state: State<'_, AppState>) -> Result<()> {
    let url = format!("file:///{}", state.paths.root.to_string_lossy().replace('\\', "/"));
    crate::sys::process::open_external(&url)
}
