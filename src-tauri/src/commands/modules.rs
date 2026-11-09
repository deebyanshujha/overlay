//! Module enable/disable/configure, driven from the Customize page.
//!
//! Every one of these writes the change into the active profile as well as
//! applying it. Otherwise a toggle would be forgotten the next time the
//! profile was applied, which is the classic way a customiser's settings
//! "mysteriously reset".

use crate::error::{OverlayError, Result};
use crate::modules::ModuleStatus;
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use tauri::{AppHandle, State};

#[derive(Debug, Serialize)]
pub struct ModuleChange {
    pub modules: Vec<ModuleStatus>,
    pub active_profile: Option<String>,
    /// Non-fatal things the user should know, e.g. a partly available module.
    pub warnings: Vec<String>,
}

fn snapshot(app: &AppHandle, state: &AppState, warnings: Vec<String>) -> Result<ModuleChange> {
    let ctx = state.ctx(app);
    let active = state.profiles.active_id()?;
    let profile = active.as_deref().and_then(|id| state.profiles.get(id).ok());
    let configs: BTreeMap<String, Value> =
        profile.as_ref().map(|p| p.configs()).unwrap_or_default();
    let enabled = profile.as_ref().map(|p| p.enabled_ids()).unwrap_or_default();
    Ok(ModuleChange {
        modules: state.registry.statuses(&ctx, &configs, &enabled),
        active_profile: active,
        warnings,
    })
}

#[tauri::command(async)]
pub fn list_modules(app: AppHandle, state: State<'_, AppState>) -> Result<ModuleChange> {
    snapshot(&app, &state, Vec::new())
}

/// Turns a module on or off and records it in the active profile.
#[tauri::command(async)]
pub fn set_module_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: String,
    enabled: bool,
    config: Option<Value>,
) -> Result<ModuleChange> {
    let ctx = state.ctx(&app);
    let module = state.registry.get(&module_id)?;
    let mut warnings = Vec::new();

    if enabled {
        // Dependencies must be satisfied before we touch anything.
        for need in module.descriptor().requires {
            let dependency = state.registry.get(need)?;
            if !dependency.is_active(&ctx) {
                return Err(OverlayError::MissingDependency(
                    module.descriptor().name.to_string(),
                    dependency.descriptor().name.to_string(),
                ));
            }
        }
        for other in module.descriptor().conflicts_with {
            if let Ok(conflicting) = state.registry.get(other) {
                if conflicting.is_active(&ctx) {
                    return Err(OverlayError::Conflict {
                        id: module.descriptor().name.to_string(),
                        other: conflicting.descriptor().name.to_string(),
                        reason: "both manage the same part of the desktop".into(),
                    });
                }
            }
        }

        if let crate::modules::Availability::Partial { note } = module.availability(&ctx) {
            warnings.push(note);
        }
        state
            .registry
            .enable_one(&ctx, &module_id, config.as_ref())?;
    } else {
        // Turning something off also turns off anything that depends on it,
        // rather than leaving a dependent module running on a framework that
        // is no longer there.
        let dependents: Vec<String> = state
            .registry
            .iter()
            .filter(|m| {
                m.descriptor().requires.contains(&module_id.as_str()) && m.is_active(&ctx)
            })
            .map(|m| m.id().to_string())
            .collect();
        for dependent in &dependents {
            if let Ok(m) = state.registry.get(dependent) {
                let _ = m.disable(&ctx);
                warnings.push(format!(
                    "`{}` was switched off too, because it needs this.",
                    m.descriptor().name
                ));
                persist(&state, dependent, false, None)?;
            }
        }
        module.disable(&ctx)?;
    }

    persist(&state, &module_id, enabled, config)?;
    super::notify_surfaces_changed(&app);
    snapshot(&app, &state, warnings)
}

/// Updates a module's configuration, re-applying it if it is running.
#[tauri::command(async)]
pub fn configure_module(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: String,
    config: Value,
) -> Result<ModuleChange> {
    let ctx = state.ctx(&app);
    let module = state.registry.get(&module_id)?;
    let config = module.validate_config(&config)?;

    if module.is_active(&ctx) {
        module.enable(&ctx, &config)?;
    }
    persist(&state, &module_id, module.is_active(&ctx), Some(config))?;
    super::notify_surfaces_changed(&app);
    snapshot(&app, &state, Vec::new())
}

#[tauri::command(async)]
pub fn module_default_config(state: State<'_, AppState>, module_id: String) -> Result<Value> {
    Ok(state.registry.get(&module_id)?.default_config())
}

/// Puts one module's configuration back to the shipped default.
#[tauri::command(async)]
pub fn reset_module_config(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: String,
) -> Result<ModuleChange> {
    let defaults = state.registry.get(&module_id)?.default_config();
    configure_module(app, state, module_id, defaults)
}

/// Writes the module's state into the active profile.
///
/// Silently does nothing for Original Windows, which is immutable by design:
/// a toggle flipped while it is active applies to the live desktop but must
/// not edit the record of what the desktop originally looked like.
fn persist(
    state: &AppState,
    module_id: &str,
    enabled: bool,
    config: Option<Value>,
) -> Result<()> {
    let Some(active) = state.profiles.active_id()? else {
        return Ok(());
    };
    if active == crate::profiles::ORIGINAL_WINDOWS {
        return Ok(());
    }
    state
        .profiles
        .set_module(&active, module_id, enabled, config, &state.registry)
        .map(|_| ())
}
