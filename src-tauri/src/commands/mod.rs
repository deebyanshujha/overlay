//! The Tauri command surface — the only way the frontend reaches the system.
//!
//! Two rules hold throughout:
//!
//! * **The frontend never touches Windows.** Every command here goes through
//!   the module registry, the journal or the restoration engine. There is no
//!   command that takes a registry path, a command line or a window handle
//!   from the UI.
//! * **Validation happens on this side.** Configurations arrive as untyped
//!   JSON and are parsed by the owning module before anything acts on them,
//!   so a compromised or buggy frontend cannot produce an invalid system
//!   change.

pub mod dropzone;
pub mod integrations;
pub mod modules;
pub mod profiles;
pub mod restoration;
pub mod system;
pub mod widgets;

use serde::{Deserialize, Serialize};

/// Emitted whenever a surface's configuration changes.
///
/// Surfaces listen for this instead of polling. They used to re-read their
/// configuration on a timer — every two to five seconds, per window — and
/// each of those reads went through the module registry on the main thread.
/// With several widgets open that was a steady stream of work on the very
/// thread that has to stay free to handle clicks.
pub const SURFACES_CHANGED: &str = "overlay://surfaces-changed";

/// Tells every open surface to re-read its configuration.
pub fn notify_surfaces_changed(app: &tauri::AppHandle) {
    use tauri::Emitter;
    if let Err(e) = app.emit(SURFACES_CHANGED, ()) {
        log::warn!("could not notify surfaces of a configuration change: {e}");
    }
}

/// Everything the UI needs for a first paint, in one round trip.
#[derive(Debug, Serialize)]
pub struct Bootstrap {
    pub environment: crate::sys::Environment,
    pub modules: Vec<crate::modules::ModuleStatus>,
    pub profiles: Vec<crate::profiles::Profile>,
    pub active_profile: Option<String>,
    pub integrations: Vec<crate::integrations::IntegrationStatus>,
    pub owned_settings: usize,
    pub capabilities: crate::monitoring::Capabilities,
    pub widget_catalog: &'static [crate::modules::widgets::WidgetKind],
    pub controls: Vec<crate::modules::control_center::Control>,
    pub settings: AppSettings,
    /// Set when the previous run ended mid-change.
    pub recovery: Option<RecoveryNotice>,
    pub first_run: bool,
}

#[derive(Debug, Serialize)]
pub struct AppSettings {
    pub theme: String,
    pub accent: String,
    pub start_with_windows: bool,
    pub start_minimised: bool,
    pub restore_profile_on_start: bool,
    pub control_center_hotkey: String,
    pub reduce_motion: bool,
}

/// Round-trips through the settings table, so it needs both directions.
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoveryNotice {
    pub message: String,
    pub details: Vec<String>,
}
