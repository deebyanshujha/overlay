//! Profile management commands.

use crate::error::Result;
use crate::profiles::{switch::SwitchReport, Profile};
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

#[derive(Debug, Serialize)]
pub struct ProfileList {
    pub profiles: Vec<Profile>,
    pub active: Option<String>,
}

fn list_of(state: &AppState) -> Result<ProfileList> {
    Ok(ProfileList {
        profiles: state.profiles.list()?,
        active: state.profiles.active_id()?,
    })
}

#[tauri::command(async)]
pub fn list_profiles(state: State<'_, AppState>) -> Result<ProfileList> {
    list_of(&state)
}

#[tauri::command(async)]
pub fn get_profile(state: State<'_, AppState>, id: String) -> Result<Profile> {
    state.profiles.get(&id)
}

#[tauri::command(async)]
pub fn create_profile(
    state: State<'_, AppState>,
    name: String,
    description: String,
    copy_from: Option<String>,
) -> Result<Profile> {
    let source = match copy_from {
        Some(id) => Some(state.profiles.get(&id)?),
        None => None,
    };
    state.profiles.create(&name, &description, source.as_ref())
}

#[tauri::command(async)]
pub fn rename_profile(
    state: State<'_, AppState>,
    id: String,
    name: String,
    description: String,
) -> Result<Profile> {
    state.profiles.rename(&id, &name, &description)
}

#[tauri::command(async)]
pub fn duplicate_profile(state: State<'_, AppState>, id: String, name: String) -> Result<Profile> {
    state.profiles.duplicate(&id, &name)
}

#[tauri::command(async)]
pub fn delete_profile(state: State<'_, AppState>, id: String) -> Result<ProfileList> {
    state.profiles.delete(&id)?;
    list_of(&state)
}

#[tauri::command(async)]
pub fn set_profile_module(
    state: State<'_, AppState>,
    profile_id: String,
    module_id: String,
    enabled: bool,
    config: Option<Value>,
) -> Result<Profile> {
    state
        .profiles
        .set_module(&profile_id, &module_id, enabled, config, &state.registry)
}

/// Checks a profile without applying it, so the UI can warn before the user
/// commits to a switch.
#[tauri::command(async)]
pub fn validate_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<String>> {
    let profile = state.profiles.get(&id)?;
    let ctx = state.ctx(&app);
    match state.switcher.validate(&profile, &state.registry, &ctx) {
        Ok(()) => Ok(Vec::new()),
        Err(e) => Ok(vec![e.to_string()]),
    }
}

/// Applies a profile. Transactional — see `profiles::switch`.
#[tauri::command(async)]
pub fn switch_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<SwitchReport> {
    let ctx = state.ctx(&app);
    let report = state.switcher.switch_to(&id, &state.registry, &ctx)?;

    super::notify_surfaces_changed(&app);

    // Keep OVERLAY's own appearance in step with the profile that asked for
    // it, so switching to Elegant also dresses the studio.
    if report.succeeded {
        if let Ok(profile) = state.profiles.get(&id) {
            let _ = state.set_setting(super::system::KEY_THEME, &profile.theme);
            let _ = state.set_setting(super::system::KEY_ACCENT, &profile.accent);
        }
    }
    Ok(report)
}

#[tauri::command(async)]
pub fn export_profile(state: State<'_, AppState>, id: String) -> Result<String> {
    let export = state.profiles.export(&id)?;
    Ok(serde_json::to_string_pretty(&export)?)
}

/// Writes a profile export next to the database and returns the path.
#[tauri::command(async)]
pub fn export_profile_to_file(state: State<'_, AppState>, id: String) -> Result<String> {
    let profile = state.profiles.get(&id)?;
    let export = state.profiles.export(&id)?;
    let file = state
        .paths
        .exports
        .join(format!("{}.overlay.json", crate::profiles::slug(&profile.name)));
    std::fs::write(&file, serde_json::to_string_pretty(&export)?)?;
    Ok(file.to_string_lossy().to_string())
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub profile: Profile,
    pub warnings: Vec<String>,
}

#[tauri::command(async)]
pub fn import_profile(state: State<'_, AppState>, json: String) -> Result<ImportResult> {
    let (profile, warnings) = state.profiles.import(&json, &state.registry)?;
    Ok(ImportResult { profile, warnings })
}

#[tauri::command(async)]
pub fn import_profile_from_file(state: State<'_, AppState>, path: String) -> Result<ImportResult> {
    let json = std::fs::read_to_string(&path).map_err(|e| {
        crate::error::OverlayError::Config(format!("could not read {path}: {e}"))
    })?;
    import_profile(state, json)
}

/// The profile applied at startup, if the user has chosen one.
#[tauri::command(async)]
pub fn set_startup_profile(state: State<'_, AppState>, id: Option<String>) -> Result<()> {
    match id {
        Some(id) => {
            // Checked now so a deleted profile cannot be left configured as
            // the startup target.
            state.profiles.get(&id)?;
            state.set_setting("startup_profile", &id)
        }
        None => state.set_setting("startup_profile", ""),
    }
}

#[tauri::command(async)]
pub fn get_startup_profile(state: State<'_, AppState>) -> Option<String> {
    state.setting("startup_profile").filter(|s| !s.is_empty())
}
