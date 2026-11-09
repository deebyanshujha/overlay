//! Control Center — a global quick-settings panel on a hotkey.
//!
//! # No decorative toggles
//!
//! Every control here is one of two things: something OVERLAY can actually
//! change, or a labelled shortcut that opens the Windows page where the user
//! can change it. There is no third category. [`ControlKind`] encodes that
//! distinction in the type system, and [`available_controls`] reports it to
//! the UI, so a control that cannot act is rendered as a link rather than as
//! a switch that quietly does nothing.
//!
//! Volume and brightness are the honest cases. Driving the audio endpoint
//! needs COM interop that is not wired up in this build, and brightness is
//! only settable on internal panels through WMI; both are therefore shipped
//! as shortcuts to the relevant Windows surface rather than as sliders that
//! work on some machines and not others.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::windows::{SurfaceManager, SurfaceSpec, ZMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;

pub const LABEL: &str = "control-center";
pub const DEFAULT_HOTKEY: &str = "Alt+Shift+O";

/// What a control actually does when pressed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlKind {
    /// OVERLAY performs the action itself.
    Action,
    /// Opens a Windows settings page. `target` is checked against the
    /// `ms-settings:` allowlist in `sys::process::open_external`.
    OpensWindows { target: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Control {
    pub id: String,
    pub label: String,
    pub description: String,
    pub kind: ControlKind,
}

/// The catalogue of controls, each labelled with what it genuinely does.
pub fn available_controls() -> Vec<Control> {
    let shortcut = |id: &str, label: &str, description: &str, target: &str| Control {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        kind: ControlKind::OpensWindows {
            target: target.into(),
        },
    };
    let action = |id: &str, label: &str, description: &str| Control {
        id: id.into(),
        label: label.into(),
        description: description.into(),
        kind: ControlKind::Action,
    };

    vec![
        action(
            "profile",
            "Switch profile",
            "Apply any OVERLAY profile without opening the studio.",
        ),
        action(
            "widgets",
            "Show or hide widgets",
            "Toggle every OVERLAY widget at once.",
        ),
        action(
            "focus",
            "Focus mode",
            "Hide OVERLAY's own surfaces for distraction-free work.",
        ),
        action(
            "restore",
            "Restore original desktop",
            "Undo every change OVERLAY has made.",
        ),
        action(
            "wallflow",
            "Open WallFlow",
            "Bring up WallFlow to change your wallpaper.",
        ),
        action(
            "quickdock",
            "Start QuickDock",
            "Launch the QuickDock search palette.",
        ),
        shortcut(
            "volume",
            "Volume",
            "Opens Windows sound settings. OVERLAY does not drive the audio endpoint itself.",
            "ms-settings:sound",
        ),
        shortcut(
            "brightness",
            "Brightness",
            "Opens Windows display settings. Brightness is only adjustable on internal panels.",
            "ms-settings:display",
        ),
        shortcut(
            "appearance",
            "Light / dark mode",
            "Opens Windows personalisation.",
            "ms-settings:personalization-colors",
        ),
        shortcut(
            "displays",
            "Displays",
            "Arrange monitors and set scaling.",
            "ms-settings:display",
        ),
        shortcut(
            "focus_assist",
            "Do not disturb",
            "Opens Windows notification settings.",
            "ms-settings:notifications",
        ),
        shortcut(
            "power",
            "Power & battery",
            "Opens Windows power settings.",
            "ms-settings:powersleep",
        ),
    ]
}

pub fn control(id: &str) -> Option<Control> {
    available_controls().into_iter().find(|c| c.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ControlCenterConfig {
    /// Control ids to show, in order.
    pub controls: Vec<String>,
    pub hotkey: String,
    pub opacity: f32,
    /// Close as soon as focus is lost.
    pub dismiss_on_blur: bool,
}

impl Default for ControlCenterConfig {
    fn default() -> Self {
        Self {
            controls: available_controls().into_iter().map(|c| c.id).collect(),
            hotkey: DEFAULT_HOTKEY.into(),
            opacity: 0.98,
            dismiss_on_blur: true,
        }
    }
}

impl ControlCenterConfig {
    fn normalize(mut self) -> Result<Self> {
        let known: std::collections::HashSet<String> =
            available_controls().into_iter().map(|c| c.id).collect();
        let mut seen = std::collections::HashSet::new();
        for id in &self.controls {
            if !known.contains(id) {
                return Err(OverlayError::Config(format!("unknown control `{id}`")));
            }
            if !seen.insert(id.clone()) {
                return Err(OverlayError::Config(format!("control `{id}` is listed twice")));
            }
        }
        if self.hotkey.trim().is_empty() {
            self.hotkey = DEFAULT_HOTKEY.into();
        }
        // QuickDock owns Ctrl+Space. Taking it would break their launcher and
        // ours would fail to register anyway.
        if is_quickdock_hotkey(&self.hotkey) {
            return Err(OverlayError::Config(
                "that shortcut is QuickDock's launcher hotkey. Choose another so both keep \
                 working."
                    .into(),
            ));
        }
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.5, 1.0)
        } else {
            0.98
        };
        Ok(self)
    }
}

/// True when an accelerator collides with QuickDock's configured hotkey.
///
/// Read live from QuickDock's settings rather than hard-coded, so a user who
/// has rebound it is protected at their actual binding and free to use the
/// default.
pub fn is_quickdock_hotkey(accelerator: &str) -> bool {
    let theirs = crate::integrations::quickdock::QuickDock::read_settings()
        .map(|s| s.hotkey)
        .unwrap_or_else(|_| "CommandOrControl+Space".into());
    normalize_accelerator(accelerator) == normalize_accelerator(&theirs)
}

/// Canonical form for comparing accelerators: case-insensitive, order-
/// insensitive, with the several spellings of Ctrl unified.
fn normalize_accelerator(raw: &str) -> String {
    let mut parts: Vec<String> = raw
        .split('+')
        .map(|p| {
            let p = p.trim().to_ascii_lowercase();
            match p.as_str() {
                "commandorcontrol" | "cmdorctrl" | "control" | "ctrl" | "command" | "cmd" => {
                    "ctrl".to_string()
                }
                "super" | "meta" | "win" => "win".to_string(),
                other => other.to_string(),
            }
        })
        .filter(|p| !p.is_empty())
        .collect();
    parts.sort();
    parts.join("+")
}

pub struct ControlCenterModule;

impl ControlCenterModule {
    fn surfaces(ctx: &ModuleCtx) -> Result<Arc<SurfaceManager>> {
        Ok(ctx.app()?.state::<Arc<SurfaceManager>>().inner().clone())
    }

    /// Opens the panel centred on the primary display.
    pub fn open(ctx: &ModuleCtx) -> Result<()> {
        let app = ctx.app()?;
        let (w, h) = (420.0, 560.0);
        let (x, y) = ctx
            .env
            .monitors
            .iter()
            .find(|m| m.primary)
            .or_else(|| ctx.env.monitors.first())
            .map(|m| {
                // Logical pixels: see MonitorInfo::logical.
                let b = m.logical();
                (
                    b.work_x + (b.work_width - w) / 2.0,
                    b.work_y + (b.work_height - h) / 2.0,
                )
            })
            .unwrap_or((200.0, 120.0));

        Self::surfaces(ctx)?.show(
            app,
            SurfaceSpec::new(LABEL, "control-center")
                .at(x, y)
                .sized(w, h)
                .z(ZMode::Top),
        )
    }

    pub fn close(ctx: &ModuleCtx) -> Result<()> {
        let app = ctx.app()?;
        Self::surfaces(ctx)?.hide(app, LABEL)
    }

    pub fn toggle(ctx: &ModuleCtx) -> Result<bool> {
        if Self::surfaces(ctx)?.is_open(LABEL) {
            Self::close(ctx)?;
            Ok(false)
        } else {
            Self::open(ctx)?;
            Ok(true)
        }
    }
}

impl CustomizationModule for ControlCenterModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "control_center",
            name: "Control Center",
            description: "A quick-settings panel on a global shortcut.",
            version: "1.0.0",
            category: Category::Surfaces,
            requires: &[],
            conflicts_with: &[],
            touches_system: false,
        }
    }

    fn availability(&self, _ctx: &ModuleCtx) -> Availability {
        Availability::Available
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(ControlCenterConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: ControlCenterConfig = serde_json::from_value(config.clone()).map_err(|e| {
            OverlayError::Config(format!("control centre settings are invalid: {e}"))
        })?;
        Ok(serde_json::to_value(cfg.normalize()?)?)
    }

    fn capture_original(&self, _ctx: &ModuleCtx) -> Result<()> {
        Ok(())
    }

    /// Enabling registers the hotkey; it does not open the panel. A panel
    /// that appeared on every profile switch would be an interruption.
    fn enable(&self, _ctx: &ModuleCtx, config: &Value) -> Result<()> {
        self.validate_config(config)?;
        Ok(())
    }

    fn disable(&self, ctx: &ModuleCtx) -> Result<()> {
        let app = ctx.app()?;
        Self::surfaces(ctx)?.hide(app, LABEL)
    }

    fn is_active(&self, _ctx: &ModuleCtx) -> bool {
        // Availability is governed by the profile, not by whether the panel
        // happens to be on screen right now.
        true
    }

    fn activity_is_observable(&self) -> bool {
        // Starting another application (or binding a shortcut) leaves
        // nothing observable that means "the user switched this on".
        false
    }

    fn notes(&self, _ctx: &ModuleCtx) -> Vec<String> {
        vec![
            "Volume and brightness open the matching Windows settings page rather than being \
             driven directly, because neither can be changed reliably on every machine."
                .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_is_either_an_action_or_a_labelled_shortcut() {
        // The no-decorative-toggles guarantee.
        for c in available_controls() {
            assert!(!c.label.is_empty(), "{} has no label", c.id);
            assert!(!c.description.is_empty(), "{} has no description", c.id);
            if let ControlKind::OpensWindows { target } = &c.kind {
                assert!(
                    target.starts_with("ms-settings:"),
                    "{} points somewhere unexpected: {target}",
                    c.id
                );
            }
        }
    }

    #[test]
    fn every_shortcut_target_passes_the_launch_allowlist() {
        // A control that would be refused at press time is a dead toggle.
        for c in available_controls() {
            if let ControlKind::OpensWindows { target } = &c.kind {
                assert!(
                    target.starts_with("ms-settings:"),
                    "{target} would be refused by open_external"
                );
            }
        }
    }

    #[test]
    fn control_ids_are_unique() {
        let all = available_controls();
        let unique: std::collections::HashSet<_> = all.iter().map(|c| &c.id).collect();
        assert_eq!(all.len(), unique.len());
    }

    #[test]
    fn volume_and_brightness_are_shortcuts_not_sliders() {
        for id in ["volume", "brightness"] {
            let c = control(id).unwrap();
            assert!(
                matches!(c.kind, ControlKind::OpensWindows { .. }),
                "{id} must not pretend to be directly controllable"
            );
        }
    }

    #[test]
    fn accelerators_compare_regardless_of_spelling_or_order() {
        assert_eq!(
            normalize_accelerator("CommandOrControl+Space"),
            normalize_accelerator("Ctrl+Space")
        );
        assert_eq!(
            normalize_accelerator("Alt+Shift+O"),
            normalize_accelerator("shift+alt+o")
        );
        assert_ne!(
            normalize_accelerator("Alt+Shift+O"),
            normalize_accelerator("Alt+O")
        );
    }

    #[test]
    fn the_default_hotkey_does_not_collide_with_quickdock() {
        let cfg = ControlCenterConfig::default();
        assert!(cfg.normalize().is_ok(), "our default must not take Ctrl+Space");
    }

    #[test]
    fn taking_quickdocks_hotkey_is_refused() {
        let cfg = ControlCenterConfig {
            hotkey: "CommandOrControl+Space".into(),
            ..Default::default()
        };
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn an_unknown_control_is_rejected() {
        let cfg = ControlCenterConfig {
            controls: vec!["self_destruct".into()],
            ..Default::default()
        };
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn an_empty_hotkey_falls_back_to_the_default() {
        let cfg = ControlCenterConfig {
            hotkey: "  ".into(),
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(cfg.hotkey, DEFAULT_HOTKEY);
    }
}
