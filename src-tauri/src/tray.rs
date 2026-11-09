//! System tray.
//!
//! The tray is the one piece of OVERLAY that must keep working when
//! everything else does not, so "Restore original desktop" lives here as well
//! as in the window. A user whose UI has broken can still undo everything
//! from the notification area.

use crate::state::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "overlay-tray";

/// Menu rows whose text tracks application state.
pub struct TrayItems {
    profile: MenuItem<tauri::Wry>,
    owned: MenuItem<tauri::Wry>,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open OVERLAY", true, None::<&str>)?;
    let control = MenuItem::with_id(app, "control", "Control Center", true, None::<&str>)?;

    // Informational rows: present but not clickable.
    let profile = MenuItem::with_id(app, "profile", "Profile: —", false, None::<&str>)?;
    let owned = MenuItem::with_id(app, "owned", "Changes: —", false, None::<&str>)?;

    let restore = MenuItem::with_id(
        app,
        "restore",
        "Restore original desktop",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit OVERLAY", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &control,
            &PredefinedMenuItem::separator(app)?,
            &profile,
            &owned,
            &PredefinedMenuItem::separator(app)?,
            &restore,
            &quit,
        ],
    )?;

    app.manage(TrayItems { profile, owned });

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .tooltip("OVERLAY — your desktop, your rules")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => crate::show_main(app),
            // Off the event loop: opening the panel creates a window, and
            // waiting for that from inside a tray callback deadlocks the
            // application. Same reason as the global shortcut.
            "control" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = app.state::<AppState>();
                    let ctx = state.ctx(&app);
                    if let Err(e) =
                        crate::modules::control_center::ControlCenterModule::toggle(&ctx)
                    {
                        log::warn!("tray could not open Control Center: {e}");
                    }
                });
            }
            "restore" => restore_from_tray(app),
            "quit" => {
                let state = app.state::<AppState>();
                // Close OVERLAY's surfaces before exiting so no widget
                // window is left behind without a process to own it.
                state.surfaces.hide_all(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main(tray.app_handle());
            }
        })
        .build(app)?;

    refresh(app);
    Ok(())
}

/// Runs a full restore from the tray and reports the result through a
/// notification rather than a window, since the window may be why the user
/// came to the tray.
fn restore_from_tray(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let ctx = state.ctx(&app);
        let message = match state.restoration.restore_all(&state.registry, &ctx) {
            Ok(report) => report.summary(),
            Err(e) => format!("Restore failed: {e}"),
        };
        log::info!("restore from tray: {message}");
        // The open window, if there is one, updates itself from this.
        use tauri::Emitter;
        let _ = app.emit("overlay://restored", &message);
        refresh(&app);
    });
}

/// Updates the state-tracking rows. Cheap, so it can be called after any
/// change without thinking about it.
pub fn refresh(app: &AppHandle) {
    let (Some(items), Some(state)) = (app.try_state::<TrayItems>(), app.try_state::<AppState>())
    else {
        return;
    };

    let profile = state
        .profiles
        .active_id()
        .ok()
        .flatten()
        .and_then(|id| state.profiles.get(&id).ok())
        .map(|p| p.name)
        .unwrap_or_else(|| "none".into());
    let _ = items.profile.set_text(format!("Profile: {profile}"));

    let owned = state.journal.owned().map(|o| o.len()).unwrap_or(0);
    let _ = items.owned.set_text(match owned {
        0 => "Changes: none — desktop untouched".to_string(),
        1 => "Changes: 1 setting".to_string(),
        n => format!("Changes: {n} settings"),
    });
}
