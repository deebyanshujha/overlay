//! Integration status and actions for WallFlow and QuickDock.

use crate::error::Result;
use crate::integrations::{quickdock::QuickDock, wallflow::WallFlow, IntegrationStatus};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

/// Detection for both companion apps. Cached briefly — see
/// `integrations::statuses_cached`.
pub fn all_statuses() -> Vec<IntegrationStatus> {
    crate::integrations::statuses_cached()
}

#[tauri::command(async)]
pub fn list_integrations() -> Vec<IntegrationStatus> {
    all_statuses()
}

#[tauri::command(async)]
pub fn integration_status(id: String) -> Result<IntegrationStatus> {
    match id.as_str() {
        "wallflow" => Ok(WallFlow::status()),
        "quickdock" => Ok(QuickDock::status()),
        other => Err(crate::error::OverlayError::Integration(format!(
            "unknown integration `{other}`"
        ))),
    }
}

/// Launches a companion app.
///
/// Dispatched by id, never by path: the executable comes from OVERLAY's own
/// detection, so the frontend cannot ask this to start something else.
#[tauri::command(async)]
pub fn launch_integration(id: String, silent: Option<bool>) -> Result<String> {
    match id.as_str() {
        "wallflow" => {
            WallFlow::launch()?;
            crate::integrations::invalidate_cache();
            Ok("WallFlow opened".into())
        }
        "quickdock" => {
            QuickDock::launch(silent.unwrap_or(false))?;
            crate::integrations::invalidate_cache();
            Ok("QuickDock started".into())
        }
        other => Err(crate::error::OverlayError::Integration(format!(
            "unknown integration `{other}`"
        ))),
    }
}

#[derive(Debug, Serialize)]
pub struct WallpaperInfo {
    pub available: bool,
    pub provider: Option<String>,
    pub topic: Option<String>,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Absolute path to the image, when the file is still cached.
    pub path: Option<String>,
    pub next_change: Option<i64>,
    pub note: Option<String>,
}

/// The wallpaper WallFlow currently has set, read from its own database.
#[tauri::command(async)]
pub fn current_wallpaper() -> WallpaperInfo {
    match WallFlow::read_state() {
        Ok((settings, Some(w))) => WallpaperInfo {
            available: true,
            provider: Some(w.provider),
            topic: Some(w.category),
            author: w.author_name,
            source_url: w.source_url,
            width: Some(w.width),
            height: Some(w.height),
            path: w.path.map(|p| p.to_string_lossy().to_string()),
            next_change: (settings.next_change_at > 0).then_some(settings.next_change_at),
            note: None,
        },
        Ok((settings, None)) => WallpaperInfo {
            available: true,
            provider: Some(settings.provider),
            topic: None,
            author: None,
            source_url: None,
            width: None,
            height: None,
            path: None,
            next_change: (settings.next_change_at > 0).then_some(settings.next_change_at),
            note: Some("WallFlow is using its default wallpaper.".into()),
        },
        Err(e) => WallpaperInfo {
            available: false,
            provider: None,
            topic: None,
            author: None,
            source_url: None,
            width: None,
            height: None,
            path: None,
            next_change: None,
            note: Some(e.to_string()),
        },
    }
}

/// Opens the companion app's own settings.
///
/// Both apps keep settings inside their own window, and neither exposes a
/// deep link, so this raises the app and says what to do next rather than
/// pretending to navigate somewhere.
#[tauri::command(async)]
pub fn open_integration_settings(id: String) -> Result<String> {
    match id.as_str() {
        "wallflow" => {
            WallFlow::launch()?;
            Ok("WallFlow is open — its settings are in the app's own window.".into())
        }
        "quickdock" => {
            QuickDock::launch(false)?;
            Ok("QuickDock is open — press its hotkey and use its tray icon for settings.".into())
        }
        other => Err(crate::error::OverlayError::Integration(format!(
            "unknown integration `{other}`"
        ))),
    }
}

/// Records what OVERLAY found, so the Integrations page can show when each
/// app was last seen even while it is closed.
#[tauri::command(async)]
pub fn refresh_integrations(state: State<'_, AppState>) -> Result<Vec<IntegrationStatus>> {
    // An explicit re-check must actually re-check.
    crate::integrations::invalidate_cache();
    let statuses = all_statuses();
    let conn = state.conn.lock();
    for s in &statuses {
        let _ = conn.execute(
            "INSERT INTO integration_registry (id, exe_path, data_dir, version, last_seen, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                exe_path = excluded.exe_path,
                data_dir = excluded.data_dir,
                version = excluded.version,
                last_seen = excluded.last_seen",
            rusqlite::params![
                s.id,
                s.exe_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                s.data_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
                s.version,
                crate::storage::now(),
                s.limitations.join(" | "),
            ],
        );
    }
    drop(conn);
    Ok(statuses)
}
