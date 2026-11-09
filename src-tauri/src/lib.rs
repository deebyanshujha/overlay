//! OVERLAY — Windows desktop customization studio.
//!
//! # Layering
//!
//! ```text
//!   commands/        Tauri surface — the only thing the frontend can call
//!   profiles/        profiles and transactional switching
//!   restoration/     putting the desktop back, with a headless path
//!   modules/         one customization each, behind a common lifecycle
//!   integrations/    adapters for WallFlow and QuickDock
//!   windows/         native surfaces (widgets, dock, HUD, …)
//!   journal/         every system change: capture, apply, verify, revert
//!   monitoring/      system metrics
//!   storage/         SQLite schema and migrations
//!   sys/             Win32. Nothing above this calls Windows directly.
//! ```
//!
//! Dependencies only ever point downward. In particular `sys` knows nothing
//! about modules or profiles, and `journal` knows nothing about which module
//! made a change — which is what lets recovery undo changes in a process
//! where no module was ever constructed.

pub mod autostart;
pub mod commands;
pub mod error;
pub mod integrations;
pub mod journal;
pub mod logging;
pub mod modules;
pub mod monitoring;
pub mod profiles;
pub mod restoration;
pub mod shortcuts;
pub mod state;
pub mod storage;
pub mod sys;
pub mod tray;
pub mod windows;

use state::AppState;
use std::sync::Arc;
use storage::paths::Paths;
use tauri::{AppHandle, Manager, WindowEvent};
use windows::SurfaceManager;

/// Brings the studio window up, creating nothing — the window exists from
/// startup and is only hidden.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn run() {
    let surfaces = SurfaceManager::new();

    tauri::Builder::default()
        // First, so a second launch is turned away before it competes for
        // the global shortcut or opens a duplicate set of widgets.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // `--profile <id>` lets a shortcut, script or scheduled task
            // apply a profile without opening the window — the same
            // switch the UI performs, through the same transactional engine.
            if let Some(id) = profile_arg(&argv) {
                apply_profile_off_thread(app, &id);
                return;
            }
            // A second launch from the autostart entry is meaningless; any
            // other means "show me the window I already have".
            if !argv.iter().any(|a| a == "--minimised" || a == "--minimized") {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimised"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    shortcuts::handle(app.app_handle(), event.state());
                })
                .build(),
        )
        .manage(surfaces.clone())
        .invoke_handler(tauri::generate_handler![
            // system
            commands::system::bootstrap,
            commands::system::get_environment,
            commands::system::refresh_environment,
            commands::system::get_settings,
            commands::system::set_setting,
            commands::system::complete_onboarding,
            commands::system::get_metrics,
            commands::system::get_capabilities,
            commands::system::toggle_control_center,
            commands::system::close_control_center,
            commands::system::run_control,
            commands::system::show_main_window,
            commands::system::hide_to_tray,
            commands::system::quit,
            commands::system::diagnostics,
            commands::system::open_data_folder,
            // modules
            commands::modules::list_modules,
            commands::modules::set_module_enabled,
            commands::modules::configure_module,
            commands::modules::module_default_config,
            commands::modules::reset_module_config,
            // profiles
            commands::profiles::list_profiles,
            commands::profiles::get_profile,
            commands::profiles::create_profile,
            commands::profiles::rename_profile,
            commands::profiles::duplicate_profile,
            commands::profiles::delete_profile,
            commands::profiles::set_profile_module,
            commands::profiles::validate_profile,
            commands::profiles::switch_profile,
            commands::profiles::export_profile,
            commands::profiles::export_profile_to_file,
            commands::profiles::import_profile,
            commands::profiles::import_profile_from_file,
            commands::profiles::set_startup_profile,
            commands::profiles::get_startup_profile,
            // restoration
            commands::restoration::preview_restore,
            commands::restoration::restore_original_desktop,
            commands::restoration::restore_module,
            commands::restoration::restoration_history,
            commands::restoration::change_journal,
            commands::restoration::owned_settings,
            commands::restoration::reconcile_interrupted,
            commands::restoration::recovery_status,
            // widgets and surfaces
            commands::widgets::list_widgets,
            commands::widgets::widget_config,
            commands::widgets::add_widget,
            commands::widgets::remove_widget,
            commands::widgets::set_widget_enabled,
            commands::widgets::move_widget,
            commands::widgets::resize_widget,
            commands::widgets::configure_widget,
            commands::widgets::reset_widget,
            commands::widgets::set_surface_click_through,
            commands::widgets::close_surface,
            commands::widgets::reassert_layering,
            commands::widgets::launch_pinned,
            // integrations
            commands::integrations::list_integrations,
            commands::integrations::integration_status,
            commands::integrations::launch_integration,
            commands::integrations::current_wallpaper,
            commands::integrations::open_integration_settings,
            commands::integrations::refresh_integrations,
            // dropzone
            commands::dropzone::shelf_add,
            commands::dropzone::shelf_list,
            commands::dropzone::shelf_refresh,
            commands::dropzone::shelf_remove,
            commands::dropzone::shelf_clear,
            commands::dropzone::shelf_copy_to,
            commands::dropzone::shelf_move_to,
            commands::dropzone::shelf_open,
            commands::dropzone::shelf_reveal,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            let paths = Paths::resolve()?;
            logging::init(&paths);
            let state = AppState::new(paths, surfaces.clone())?;
            app.manage(state);

            tray::build(&handle)?;
            shortcuts::register_initial(&handle);
            startup_recovery(&handle);

            // Keep the autostart registration honest: it can be changed from
            // Task Manager behind our back.
            {
                let state = handle.state::<AppState>();
                let wanted = state.flag(commands::system::KEY_AUTOSTART, false);
                if autostart::is_enabled(&handle) != wanted {
                    let _ = autostart::set(&handle, wanted);
                }
            }

            // An explicit `--profile` on the very first launch wins over the
            // configured startup profile: the user asked for this one.
            let argv: Vec<String> = std::env::args().collect();
            match profile_arg(&argv) {
                Some(id) => apply_profile_off_thread(&handle, &id),
                None => apply_startup_profile_off_thread(&handle),
            }

            // Started by Windows at login, or asked only to apply a profile:
            // stay in the tray rather than taking over the screen.
            if !autostart::launched_at_login() && profile_arg(&argv).is_none() {
                show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                // Closing the studio hides it; the tray's Quit really exits,
                // so widgets and the HUD keep running after the window is
                // dismissed.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // Keep OVERLAY's own overlays from covering OVERLAY's own
                // window — see SurfaceManager::suspend_always_on_top.
                WindowEvent::Focused(focused) => {
                    let app = window.app_handle();
                    if let Some(surfaces) = app.try_state::<Arc<SurfaceManager>>() {
                        surfaces.suspend_always_on_top(app, *focused);
                    }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("OVERLAY failed to start")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { api, code, .. } => {
                // Stay alive in the tray unless an explicit quit asked for it.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
            tauri::RunEvent::Exit => {
                // Last chance to clean up: no OVERLAY window should outlive
                // the process, and no global shortcut should stay grabbed.
                shortcuts::unregister_all(app);
                if let Some(surfaces) = app.try_state::<Arc<SurfaceManager>>() {
                    surfaces.hide_all(app);
                }
            }
            _ => {}
        });
}

/// Reads `--profile <name>` out of a command line, if present.
///
/// Every following word up to the next flag is taken, so an unquoted
/// multi-word name works: `--profile Original Windows` means the profile
/// called "Original Windows", not a profile called "Original". Requiring
/// quotes here would be a trap, because the most important profile in the
/// product has a space in its name.
pub fn profile_arg(argv: &[String]) -> Option<String> {
    let index = argv.iter().position(|a| a == "--profile")?;
    let name = argv[index + 1..]
        .iter()
        .take_while(|v| !v.starts_with("--"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    (!name.trim().is_empty()).then(|| name.trim().to_string())
}

#[cfg(test)]
mod cli_tests {
    use super::profile_arg;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_single_word_profile_is_read() {
        assert_eq!(
            profile_arg(&argv(&["overlay.exe", "--profile", "Developer"])),
            Some("Developer".into())
        );
    }

    #[test]
    fn an_unquoted_multi_word_profile_is_read_whole() {
        // The regression this guards: "Original Windows" arriving as two
        // argv entries and being treated as the profile "Original".
        assert_eq!(
            profile_arg(&argv(&["overlay.exe", "--profile", "Original", "Windows"])),
            Some("Original Windows".into())
        );
    }

    #[test]
    fn a_following_flag_ends_the_name() {
        assert_eq!(
            profile_arg(&argv(&["overlay.exe", "--profile", "Gaming", "--minimised"])),
            Some("Gaming".into())
        );
    }

    #[test]
    fn a_missing_or_empty_value_yields_nothing() {
        assert_eq!(profile_arg(&argv(&["overlay.exe", "--profile"])), None);
        assert_eq!(
            profile_arg(&argv(&["overlay.exe", "--profile", "--minimised"])),
            None
        );
        assert_eq!(profile_arg(&argv(&["overlay.exe"])), None);
    }
}

/// Applies a profile off the main thread.
///
/// Applying a profile creates windows, and creating a window blocks until
/// the event loop services the request. Doing that *from* the event loop —
/// which is where a single-instance callback, a tray click and a global
/// shortcut all run — deadlocks the entire application: no clicks, no
/// repaint, and the close button stops working. Every such entry point
/// hands the work to a worker thread instead.
pub fn apply_profile_off_thread(app: &AppHandle, wanted: &str) {
    let app = app.clone();
    let wanted = wanted.to_string();
    std::thread::spawn(move || apply_profile_by_name(&app, &wanted));
}

/// Applies a profile by id or by name, case-insensitively.
///
/// Accepting the display name matters for a command line: nobody wants to
/// remember that "Original Windows" is spelled `original_windows`.
///
/// Must not be called on the main thread — see [`apply_profile_off_thread`].
fn apply_profile_by_name(app: &AppHandle, wanted: &str) {
    let state = app.state::<AppState>();
    let needle = wanted.trim().to_ascii_lowercase();

    let resolved = state.profiles.list().ok().and_then(|profiles| {
        profiles
            .iter()
            .find(|p| p.id.to_ascii_lowercase() == needle)
            .or_else(|| {
                profiles
                    .iter()
                    .find(|p| p.name.to_ascii_lowercase() == needle)
            })
            .map(|p| p.id.clone())
    });

    let Some(id) = resolved else {
        log::warn!("no profile matches `{wanted}`");
        return;
    };

    let ctx = state.ctx(app);
    match state.switcher.switch_to(&id, &state.registry, &ctx) {
        Ok(report) => log::info!(
            "applied `{}` from the command line ({} ms){}",
            report.to_name,
            report.duration_ms,
            if report.warnings.is_empty() {
                String::new()
            } else {
                format!("; {}", report.warnings.join("; "))
            }
        ),
        Err(e) => log::warn!("could not apply `{id}`: {e}"),
    }
    tray::refresh(app);
    // An open studio window would otherwise keep showing the profile that
    // was active when it loaded, which looks like the switch did nothing.
    commands::notify_surfaces_changed(app);
}

/// Settles anything the previous run left half-done.
///
/// Runs before any profile is applied, so the picture the switch engine works
/// from matches the machine.
fn startup_recovery(app: &AppHandle) {
    let state = app.state::<AppState>();
    let ctx = state.ctx(app);
    let mut details = Vec::new();

    match state.journal.reconcile_unsettled() {
        Ok(rows) if !rows.is_empty() => {
            details.push(format!(
                "{} change(s) interrupted by the last shutdown were checked against your \
                 desktop and recorded correctly.",
                rows.len()
            ));
        }
        Err(e) => details.push(format!("Could not check for interrupted changes: {e}")),
        _ => {}
    }

    match state.switcher.recover_interrupted(&state.registry, &ctx) {
        Ok(Some(report)) => details.extend(report.warnings),
        Err(e) => details.push(format!("Could not finish an interrupted profile switch: {e}")),
        _ => {}
    }

    if !details.is_empty() {
        let notice = commands::RecoveryNotice {
            message: "OVERLAY recovered from an interrupted operation.".into(),
            details,
        };
        if let Ok(json) = serde_json::to_string(&notice) {
            let _ = state.set_setting("pending_recovery_notice", &json);
        }
    }
}

/// Applies the startup profile, if the user has chosen one.
///
/// Deliberately not the last-active profile by default: silently re-applying
/// customizations at every login is the kind of thing that makes a tool feel
/// like it is doing things behind your back. The user opts in.
/// Applies the configured startup profile off the main thread.
///
/// `setup` runs before the event loop is pumping, so creating windows from
/// it would hang the launch itself.
fn apply_startup_profile_off_thread(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || apply_startup_profile(&app));
}

fn apply_startup_profile(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.flag(commands::system::KEY_RESTORE_ON_START, true) {
        return;
    }
    let Some(id) = state.setting("startup_profile").filter(|s| !s.is_empty()) else {
        return;
    };

    let ctx = state.ctx(app);
    match state.switcher.switch_to(&id, &state.registry, &ctx) {
        Ok(report) if report.succeeded => {
            log::info!("applied startup profile `{}`", report.to_name);
        }
        Ok(report) => log::warn!(
            "startup profile `{}` applied with problems: {:?}",
            report.to_name,
            report.warnings
        ),
        Err(e) => log::warn!("could not apply the startup profile: {e}"),
    }
    tray::refresh(app);
}
