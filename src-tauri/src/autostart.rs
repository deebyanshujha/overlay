//! Start-with-Windows registration.
//!
//! Thin wrapper over the autostart plugin so the rest of the codebase does
//! not care which mechanism is used, and so the `--minimised` flag is always
//! attached: an app that launches itself at login should appear in the tray,
//! not throw a window in front of whatever the user is doing.

use crate::error::{OverlayError, Result};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

pub fn set(app: &AppHandle, enabled: bool) -> Result<()> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| {
        OverlayError::other(format!(
            "could not {} starting with Windows: {e}",
            if enabled { "enable" } else { "disable" }
        ))
    })
}

pub fn is_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// True when this process was started by the autostart entry rather than by
/// the user.
pub fn launched_at_login() -> bool {
    std::env::args().any(|a| a == "--minimised" || a == "--minimized")
}
