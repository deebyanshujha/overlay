//! QuickDock adapter.
//!
//! # What QuickDock actually is
//!
//! Despite the name, QuickDock is **not** an icon dock. Reading its source at
//! `D:\QuickDock` shows a keyboard-first launcher — a command palette bound to
//! a global hotkey (`Ctrl+Space` by default) that searches installed
//! applications and files and launches the match. Its window is a 760×116
//! centred search bar, not a strip of pinned icons.
//!
//! That matters for OVERLAY's design. The brief anticipated that QuickDock
//! might already provide the floating dock, in which case OVERLAY should reuse
//! it rather than ship a worse copy. It does not, so the two are complementary
//! and both are offered: OVERLAY's dock for pinned icons and running-app
//! indicators, QuickDock for keyboard search. Neither duplicates the other.
//!
//! # Integration surface
//!
//! * `tauri-plugin-single-instance`: launching the executable again raises the
//!   running launcher, and the argument `--autostart` is explicitly honoured
//!   as "start silently" — a real, documented-in-source CLI contract.
//! * Settings live in plain JSON at
//!   `%APPDATA%\com.quickdock.app\settings.json`, which OVERLAY reads to show
//!   the bound hotkey. It is **not** rewritten: pinned items, recents and the
//!   search index belong to QuickDock.

use super::{Capability, DetectionSource, IntegrationStatus};
use crate::error::{OverlayError, Result};
use crate::sys::process;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DISPLAY_NAME: &str = "QuickDock";
const EXE: &str = "quickdock";
const CONFIG_DIR: &str = "com.quickdock.app";

/// The part of QuickDock's settings OVERLAY displays. Every field is optional
/// so a schema change on their side degrades to "unknown", not an error.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickDockSettings {
    pub hotkey: String,
    pub hide_on_blur: bool,
    pub max_results: usize,
    pub start_with_windows: bool,
    pub search_files: bool,
}

pub struct QuickDock;

impl QuickDock {
    pub fn config_dir() -> Option<PathBuf> {
        let dir = dirs::config_dir()?.join(CONFIG_DIR);
        dir.is_dir().then_some(dir)
    }

    pub fn locate() -> (Option<PathBuf>, Option<String>, DetectionSource) {
        if let Some((dir, version)) = super::find_installed(DISPLAY_NAME) {
            let exe = dir.join(format!("{EXE}.exe"));
            if exe.is_file() {
                return (Some(exe), version, DetectionSource::UninstallRegistry);
            }
        }
        if let Some(local) = dirs::data_local_dir() {
            let exe = local.join(DISPLAY_NAME).join(format!("{EXE}.exe"));
            if exe.is_file() {
                return (Some(exe), None, DetectionSource::KnownPath);
            }
        }
        if Self::config_dir().is_some() {
            return (None, None, DetectionSource::DataDirectoryOnly);
        }
        (None, None, DetectionSource::NotFound)
    }

    pub fn is_running() -> bool {
        process::is_running_cached(EXE)
    }

    /// Reads QuickDock's settings file. Never writes it.
    pub fn read_settings() -> Result<QuickDockSettings> {
        let dir = Self::config_dir().ok_or_else(|| {
            OverlayError::Integration("QuickDock has no configuration directory yet".into())
        })?;
        let file = dir.join("settings.json");
        let raw = std::fs::read_to_string(&file).map_err(|e| {
            OverlayError::Integration(format!("cannot read QuickDock's settings: {e}"))
        })?;
        serde_json::from_str(&raw).map_err(|e| {
            OverlayError::Integration(format!("QuickDock's settings are not readable JSON: {e}"))
        })
    }

    /// Starts QuickDock, or raises its launcher if it is already running.
    ///
    /// `silent` passes the `--autostart` flag its single-instance handler
    /// checks for, which starts it into the tray without popping the search
    /// bar in the user's face — what a profile switch wants.
    pub fn launch(silent: bool) -> Result<()> {
        let (Some(exe), _, _) = Self::locate() else {
            return Err(OverlayError::Integration(
                "QuickDock is not installed on this machine.".into(),
            ));
        };
        let args: &[&str] = if silent { &["--autostart"] } else { &[] };
        process::launch_detached(&exe, args)
    }

    pub fn status() -> IntegrationStatus {
        let (exe_path, version, detected_via) = Self::locate();
        let config_dir = Self::config_dir();
        let installed = exe_path.is_some();

        if !installed && config_dir.is_none() {
            return IntegrationStatus::missing(
                "quickdock",
                DISPLAY_NAME,
                "Keyboard-first application launcher. OVERLAY can show its status and start it.",
                "QuickDock was not found. Install it and OVERLAY will pick it up automatically.",
            );
        }

        let mut details = Vec::new();
        let mut limitations = vec![
            "QuickDock has no interface for showing or hiding its window on demand, so a \
             profile can start it but cannot close it. Quit it from its own tray icon."
                .to_string(),
        ];

        match Self::read_settings() {
            Ok(s) => {
                if !s.hotkey.is_empty() {
                    details.push(("Hotkey".into(), pretty_accelerator(&s.hotkey)));
                }
                details.push((
                    "Searches files".into(),
                    if s.search_files { "yes" } else { "apps only" }.into(),
                ));
                details.push((
                    "Starts with Windows".into(),
                    if s.start_with_windows { "yes" } else { "no" }.into(),
                ));
            }
            Err(e) => limitations.push(format!("Could not read QuickDock's settings: {e}")),
        }

        let capabilities = vec![
            Capability::yes(
                "detect",
                "Detect installation",
                "Found through the Windows uninstall registry.",
            ),
            Capability::yes(
                "status",
                "Report status",
                "Reads its settings file read-only; pinned items and recents are untouched.",
            ),
            Capability::yes(
                "launch",
                "Start QuickDock",
                "Starts it, silently with --autostart when a profile asks for it.",
            ),
            Capability::yes(
                "profile_preference",
                "Per-profile preference",
                "A profile can record that QuickDock should be running and start it on switch.",
            ),
            Capability::no(
                "toggle_visibility",
                "Show or hide on demand",
                "QuickDock exposes no command for this; it is driven by its own global hotkey.",
            ),
            Capability::no(
                "quit",
                "Quit from OVERLAY",
                "Deliberately not attempted. Force-terminating another app risks losing its \
                 search index; quit it from its own tray icon.",
            ),
        ];

        IntegrationStatus {
            id: "quickdock".into(),
            name: DISPLAY_NAME.into(),
            description:
                "Keyboard-first launcher — a search palette, not an icon dock. It complements \
                 OVERLAY's floating dock rather than replacing it, and keeps full ownership of \
                 its settings and index."
                    .into(),
            installed,
            running: Self::is_running(),
            detected_via,
            exe_path,
            data_dir: config_dir,
            version,
            capabilities,
            details,
            limitations,
        }
    }
}

/// Turns a Tauri accelerator into something readable on a keycap.
pub fn pretty_accelerator(raw: &str) -> String {
    raw.split('+')
        .map(|part| match part.trim() {
            "CommandOrControl" | "CmdOrCtrl" => "Ctrl",
            "Control" => "Ctrl",
            "Super" | "Meta" => "Win",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_parsing_matches_the_real_file_shape() {
        // Copied from the installed QuickDock's settings.json.
        let json = r#"{"hotkey":"CommandOrControl+Space","hide_on_blur":true,
                       "max_results":40,"start_with_windows":true,"search_files":true}"#;
        let s: QuickDockSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.hotkey, "CommandOrControl+Space");
        assert_eq!(s.max_results, 40);
        assert!(s.search_files);
    }

    #[test]
    fn settings_parsing_tolerates_a_changed_schema() {
        let s: QuickDockSettings = serde_json::from_str(r#"{"hotkey":"Alt+Space"}"#).unwrap();
        assert_eq!(s.hotkey, "Alt+Space");
        assert_eq!(s.max_results, 0, "absent fields fall back, they do not fail");
    }

    #[test]
    fn accelerators_render_readably() {
        assert_eq!(pretty_accelerator("CommandOrControl+Space"), "Ctrl + Space");
        assert_eq!(pretty_accelerator("Alt+Shift+D"), "Alt + Shift + D");
        assert_eq!(pretty_accelerator("Super+K"), "Win + K");
    }

    #[test]
    fn overlay_never_claims_it_can_quit_or_hide_quickdock() {
        let s = QuickDock::status();
        for id in ["toggle_visibility", "quit"] {
            if let Some(c) = s.capabilities.iter().find(|c| c.id == id) {
                assert!(!c.supported, "{id} must not be advertised as supported");
            }
        }
    }
}
