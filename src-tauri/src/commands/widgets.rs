//! Widget placement and the dock, driven by the surfaces themselves.
//!
//! Dragging a widget calls [`move_widget`], which both moves the window and
//! writes the position into the active profile — so a widget stays where it
//! was put, across restarts and profile switches.

use crate::error::{OverlayError, Result};
use crate::modules::widgets::{self, WidgetInstance, WidgetsConfig, LABEL_PREFIX};
use crate::state::AppState;
use crate::windows::clamp_to_visible;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

#[derive(Debug, Serialize)]
pub struct WidgetView {
    pub instance: WidgetInstance,
    pub kind_name: String,
    pub needs_sensor: bool,
}

fn load_config(state: &AppState) -> Result<(String, WidgetsConfig)> {
    let active = state
        .profiles
        .active_id()?
        .ok_or_else(|| OverlayError::other("no profile is active"))?;
    let profile = state.profiles.get(&active)?;
    let raw = profile
        .modules
        .get("widgets")
        .map(|s| s.config.clone())
        .unwrap_or_else(|| state.registry.get("widgets").unwrap().default_config());
    let cfg: WidgetsConfig = serde_json::from_value(raw)
        .map_err(|e| OverlayError::Config(format!("widget settings are unreadable: {e}")))?;
    Ok((active, cfg))
}

fn save_config(state: &AppState, profile_id: &str, cfg: &WidgetsConfig) -> Result<()> {
    let enabled = state
        .profiles
        .get(profile_id)?
        .modules
        .get("widgets")
        .map(|s| s.enabled)
        .unwrap_or(true);
    // Original Windows is immutable; a widget dragged while it is active
    // moves on screen but is not written into the record of the original
    // desktop.
    if profile_id == crate::profiles::ORIGINAL_WINDOWS {
        return Ok(());
    }
    state
        .profiles
        .set_module(
            profile_id,
            "widgets",
            enabled,
            Some(serde_json::to_value(cfg)?),
            &state.registry,
        )
        .map(|_| ())
}

#[tauri::command(async)]
pub fn list_widgets(state: State<'_, AppState>) -> Result<Vec<WidgetView>> {
    let (_, cfg) = load_config(&state)?;
    Ok(cfg
        .widgets
        .into_iter()
        .filter_map(|instance| {
            let kind = widgets::kind(&instance.id)?;
            Some(WidgetView {
                instance,
                kind_name: kind.name.to_string(),
                needs_sensor: kind.needs_sensor,
            })
        })
        .collect())
}

/// Configuration for one widget, used by the surface window to render itself.
#[tauri::command(async)]
pub fn widget_config(state: State<'_, AppState>, widget_id: String) -> Result<WidgetInstance> {
    let (_, cfg) = load_config(&state)?;
    cfg.widgets
        .into_iter()
        .find(|w| w.id == widget_id)
        .ok_or_else(|| OverlayError::other(format!("`{widget_id}` is not in this profile")))
}

/// Adds a widget to the active profile and shows it.
#[tauri::command(async)]
pub fn add_widget(
    app: AppHandle,
    state: State<'_, AppState>,
    widget_id: String,
) -> Result<Vec<WidgetView>> {
    let kind = widgets::kind(&widget_id)
        .ok_or_else(|| OverlayError::Config(format!("unknown widget `{widget_id}`")))?;
    let (profile_id, mut cfg) = load_config(&state)?;

    if let Some(existing) = cfg.widgets.iter_mut().find(|w| w.id == widget_id) {
        existing.enabled = true;
    } else {
        cfg.widgets
            .push(WidgetInstance::from_kind(kind, cfg.widgets.len()));
    }
    save_config(&state, &profile_id, &cfg)?;
    reapply(&app, &state, &cfg)?;
    list_widgets(state)
}

#[tauri::command(async)]
pub fn remove_widget(
    app: AppHandle,
    state: State<'_, AppState>,
    widget_id: String,
) -> Result<Vec<WidgetView>> {
    let (profile_id, mut cfg) = load_config(&state)?;
    cfg.widgets.retain(|w| w.id != widget_id);
    save_config(&state, &profile_id, &cfg)?;
    state
        .surfaces
        .hide(&app, &format!("{LABEL_PREFIX}{widget_id}"))?;
    reapply(&app, &state, &cfg)?;
    list_widgets(state)
}

#[tauri::command(async)]
pub fn set_widget_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    widget_id: String,
    enabled: bool,
) -> Result<Vec<WidgetView>> {
    let (profile_id, mut cfg) = load_config(&state)?;
    let Some(widget) = cfg.widgets.iter_mut().find(|w| w.id == widget_id) else {
        return Err(OverlayError::other(format!(
            "`{widget_id}` is not in this profile"
        )));
    };
    widget.enabled = enabled;
    save_config(&state, &profile_id, &cfg)?;
    if !enabled {
        state
            .surfaces
            .hide(&app, &format!("{LABEL_PREFIX}{widget_id}"))?;
    }
    reapply(&app, &state, &cfg)?;
    list_widgets(state)
}

/// Moves a widget and remembers where it went.
///
/// Called continuously while dragging, so it does the cheap thing: move the
/// window, update one entry, write the profile. A locked widget refuses the
/// move rather than silently ignoring it.
#[tauri::command(async)]
pub fn move_widget(
    app: AppHandle,
    state: State<'_, AppState>,
    widget_id: String,
    x: f64,
    y: f64,
) -> Result<()> {
    let (profile_id, mut cfg) = load_config(&state)?;
    let env = state.env();
    let Some(widget) = cfg.widgets.iter_mut().find(|w| w.id == widget_id) else {
        return Err(OverlayError::other(format!(
            "`{widget_id}` is not in this profile"
        )));
    };
    if widget.locked || !cfg.allow_drag {
        return Err(OverlayError::other("that widget's position is locked"));
    }
    let (x, y) = clamp_to_visible(x, y, widget.width, widget.height, &env.monitors);
    widget.x = x;
    widget.y = y;

    state
        .surfaces
        .move_to(&app, &format!("{LABEL_PREFIX}{widget_id}"), x, y)?;
    save_config(&state, &profile_id, &cfg)
}

#[tauri::command(async)]
pub fn resize_widget(
    app: AppHandle,
    state: State<'_, AppState>,
    widget_id: String,
    width: f64,
    height: f64,
) -> Result<()> {
    let (profile_id, mut cfg) = load_config(&state)?;
    let Some(widget) = cfg.widgets.iter_mut().find(|w| w.id == widget_id) else {
        return Err(OverlayError::other(format!(
            "`{widget_id}` is not in this profile"
        )));
    };
    let kind = widgets::kind(&widget_id).expect("present in the catalogue");
    widget.width = width.clamp(kind.min_width, 2_000.0);
    widget.height = height.clamp(kind.min_height, 2_000.0);

    state.surfaces.resize(
        &app,
        &format!("{LABEL_PREFIX}{widget_id}"),
        widget.width,
        widget.height,
    )?;
    save_config(&state, &profile_id, &cfg)
}

/// Replaces one widget's appearance settings.
#[tauri::command(async)]
pub fn configure_widget(
    app: AppHandle,
    state: State<'_, AppState>,
    widget_id: String,
    patch: Value,
) -> Result<Vec<WidgetView>> {
    let (profile_id, mut cfg) = load_config(&state)?;
    let Some(index) = cfg.widgets.iter().position(|w| w.id == widget_id) else {
        return Err(OverlayError::other(format!(
            "`{widget_id}` is not in this profile"
        )));
    };

    // Merge rather than replace, so the UI can send just the field it
    // changed without having to round-trip the whole instance.
    let mut merged = serde_json::to_value(&cfg.widgets[index])?;
    if let (Some(target), Some(source)) = (merged.as_object_mut(), patch.as_object()) {
        for (k, v) in source {
            // The id is identity, not configuration.
            if k != "id" {
                target.insert(k.clone(), v.clone());
            }
        }
    }
    cfg.widgets[index] = serde_json::from_value(merged)
        .map_err(|e| OverlayError::Config(format!("invalid widget settings: {e}")))?;

    // Validate the whole collection, which clamps the new values.
    let validated = state
        .registry
        .get("widgets")?
        .validate_config(&serde_json::to_value(&cfg)?)?;
    let cfg: WidgetsConfig = serde_json::from_value(validated)?;

    save_config(&state, &profile_id, &cfg)?;
    reapply(&app, &state, &cfg)?;
    list_widgets(state)
}

#[tauri::command(async)]
pub fn reset_widget(
    app: AppHandle,
    state: State<'_, AppState>,
    widget_id: String,
) -> Result<Vec<WidgetView>> {
    let kind = widgets::kind(&widget_id)
        .ok_or_else(|| OverlayError::Config(format!("unknown widget `{widget_id}`")))?;
    let (profile_id, mut cfg) = load_config(&state)?;
    if let Some(index) = cfg.widgets.iter().position(|w| w.id == widget_id) {
        let enabled = cfg.widgets[index].enabled;
        cfg.widgets[index] = WidgetInstance::from_kind(kind, index);
        cfg.widgets[index].enabled = enabled;
    }
    save_config(&state, &profile_id, &cfg)?;
    reapply(&app, &state, &cfg)?;
    list_widgets(state)
}

/// Re-applies the widget module if it is running, so a change is visible at
/// once. Does nothing when widgets are switched off.
fn reapply(app: &AppHandle, state: &AppState, cfg: &WidgetsConfig) -> Result<()> {
    let ctx = state.ctx(app);
    let module = state.registry.get("widgets")?;
    if module.is_active(&ctx) {
        module.enable(&ctx, &serde_json::to_value(cfg)?)?;
    }
    // Open widget windows re-read their own settings on this.
    super::notify_surfaces_changed(app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Surface self-service
// ---------------------------------------------------------------------------

/// Lets a surface window turn click-through on while it is not being
/// interacted with — how a widget becomes draggable on hover and transparent
/// to the mouse otherwise.
#[tauri::command(async)]
pub fn set_surface_click_through(
    app: AppHandle,
    state: State<'_, AppState>,
    label: String,
    enabled: bool,
) -> Result<()> {
    state.surfaces.set_click_through(&app, &label, enabled)
}

#[tauri::command(async)]
pub fn close_surface(app: AppHandle, state: State<'_, AppState>, label: String) -> Result<()> {
    state.surfaces.hide(&app, &label)
}

/// Re-asserts desktop-layer stacking. The frontend calls this after Win+D or
/// a full-screen app exits, both of which can raise a bottom-stacked window.
#[tauri::command(async)]
pub fn reassert_layering(app: AppHandle, state: State<'_, AppState>) {
    state.surfaces.reassert_layering(&app);
}

// ---------------------------------------------------------------------------
// Dock
// ---------------------------------------------------------------------------

#[tauri::command(async)]
pub fn launch_pinned(state: State<'_, AppState>, path: String) -> Result<()> {
    // The path must already be pinned in the active profile. That is what
    // keeps this from being an arbitrary-execution command: the frontend can
    // only ask for something the validated config already contains.
    let active = state
        .profiles
        .active_id()?
        .ok_or_else(|| OverlayError::other("no profile is active"))?;
    let profile = state.profiles.get(&active)?;
    let raw = profile
        .modules
        .get("dock")
        .map(|s| s.config.clone())
        .ok_or_else(|| OverlayError::other("the dock is not configured in this profile"))?;
    let cfg: crate::modules::dock::DockConfig = serde_json::from_value(raw)
        .map_err(|e| OverlayError::Config(format!("dock settings are unreadable: {e}")))?;

    let pinned = cfg
        .pinned
        .iter()
        .find(|p| p.path.to_string_lossy() == path)
        .ok_or_else(|| {
            OverlayError::other("that application is not pinned to this profile's dock")
        })?;

    let args: Vec<&str> = pinned.args.iter().map(|s| s.as_str()).collect();
    crate::sys::process::launch_detached(&pinned.path, &args)
}
