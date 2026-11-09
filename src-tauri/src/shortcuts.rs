//! The global shortcut that opens Control Center.
//!
//! Only one shortcut is registered, and it is deliberately not `Ctrl+Space`:
//! QuickDock owns that, and taking it would break the user's launcher. The
//! control centre module refuses a colliding binding at validation time; this
//! module refuses it again at registration time, because the setting could
//! have been written before QuickDock was installed.

use crate::error::{OverlayError, Result};
use crate::modules::control_center::{self, ControlCenterModule};
use crate::state::AppState;
use parking_lot::Mutex;
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// What is currently bound, so it can be released before binding something
/// else. Without this, changing the shortcut would leave the old one live.
fn current() -> &'static Mutex<Option<Shortcut>> {
    static CELL: OnceLock<Mutex<Option<Shortcut>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(None))
}

fn parse(accelerator: &str) -> Result<Shortcut> {
    accelerator
        .parse::<Shortcut>()
        .map_err(|e| OverlayError::Config(format!("`{accelerator}` is not a valid shortcut: {e}")))
}

/// Binds the control-centre shortcut, replacing any previous binding.
pub fn rebind(app: &AppHandle, accelerator: &str) -> Result<()> {
    if control_center::is_quickdock_hotkey(accelerator) {
        return Err(OverlayError::Config(
            "that shortcut belongs to QuickDock. Choose another so both keep working.".into(),
        ));
    }
    let shortcut = parse(accelerator)?;
    let manager = app.global_shortcut();

    if let Some(previous) = current().lock().take() {
        let _ = manager.unregister(previous);
    }
    manager.register(shortcut).map_err(|e| {
        OverlayError::Config(format!(
            "Windows would not give OVERLAY `{accelerator}` — another application is \
             probably using it ({e})"
        ))
    })?;
    *current().lock() = Some(shortcut);
    Ok(())
}

/// Registers the saved shortcut at startup.
///
/// Falls back to the default if the saved one cannot be bound, so the control
/// centre is always reachable by *some* shortcut rather than silently
/// unreachable.
pub fn register_initial(app: &AppHandle) {
    let state = app.state::<AppState>();
    let preferred = state
        .setting(crate::commands::system::KEY_HOTKEY)
        .unwrap_or_else(|| control_center::DEFAULT_HOTKEY.into());

    if rebind(app, &preferred).is_ok() {
        return;
    }
    if preferred != control_center::DEFAULT_HOTKEY
        && rebind(app, control_center::DEFAULT_HOTKEY).is_ok()
    {
        // Not persisted: the preferred binding is tried again next launch, in
        // case whatever was holding it has gone.
        log::warn!("`{preferred}` was unavailable; using the default shortcut instead");
        return;
    }
    log::warn!("no global shortcut could be registered for Control Center");
}

/// The handler wired into the plugin at build time.
///
/// The work is handed to a worker thread because this runs on the event
/// loop, and opening the Control Center creates a window — which blocks
/// until the event loop services it. Doing that from the event loop
/// deadlocks the whole application, so pressing the shortcut would freeze
/// OVERLAY rather than open the panel.
pub fn handle(app: &AppHandle, state: ShortcutState) {
    // Fire on press only; holding the keys would otherwise repeat.
    if state != ShortcutState::Pressed {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let overlay = app.state::<AppState>();
        let ctx = overlay.ctx(&app);
        if let Err(e) = ControlCenterModule::toggle(&ctx) {
            log::warn!("could not toggle Control Center: {e}");
        }
    });
}

pub fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
    *current().lock() = None;
}
