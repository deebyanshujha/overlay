//! Taskbar Studio.
//!
//! Two kinds of change, with different reversibility:
//!
//! * **Registry settings** (alignment, size, search box, widgets button,
//!   auto-hide, system transparency). Persistent; captured before the first
//!   write and restored exactly, absence included.
//! * **Composition accent** (OVERLAY's own translucency). Window state only;
//!   it evaporates when Explorer restarts, so it is re-applied by a watcher
//!   while the module is on and simply stops existing when it is off.
//!
//! Auto-hide is the awkward one: it lives inside a binary blob
//! (`StuckRects3\Settings`) rather than its own value. Byte 8 bit 0 is the
//! flag. The whole blob is captured and the single bit flipped, so every
//! other taskbar preference packed into those 40 bytes survives untouched.

use super::{
    Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor,
};
use crate::error::{OverlayError, Result};
use crate::journal::{SettingTarget, SettingValue};
use crate::sys::accent::{AccentPolicy, TaskbarFinish};
use crate::sys::registry::{RegTarget, RegValue};
use crate::sys::shell;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
pub const STUCK_RECTS: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StuckRects3";
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// Byte of the `StuckRects3` blob holding the taskbar option flags.
const STUCK_FLAGS_BYTE: usize = 8;
/// Bit 0 of that byte is auto-hide.
const AUTOHIDE_BIT: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    Left,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IconSize {
    Small,
    Medium,
    Large,
}

impl IconSize {
    fn dword(self) -> u32 {
        match self {
            IconSize::Small => 0,
            IconSize::Medium => 1,
            IconSize::Large => 2,
        }
    }
}

/// `None` for a tri-state means "leave whatever the user has"; the module
/// then never captures or touches that setting at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TaskbarConfig {
    pub finish: TaskbarFinish,
    /// 0.0–1.0. Only meaningful for a finish other than `System`.
    pub opacity: f32,
    /// 0xRRGGBB tint.
    pub tint: u32,
    pub alignment: Option<Alignment>,
    pub icon_size: Option<IconSize>,
    pub auto_hide: Option<bool>,
    /// Windows' own "Transparency effects" personalisation switch.
    pub system_transparency: Option<bool>,
    pub show_widgets_button: Option<bool>,
    pub show_task_view: Option<bool>,
    /// Keep re-applying the accent after Explorer restarts.
    pub reapply_on_explorer_restart: bool,
}

impl Default for TaskbarConfig {
    fn default() -> Self {
        Self {
            finish: TaskbarFinish::Blurred,
            opacity: 0.35,
            tint: 0x0E1016,
            alignment: None,
            icon_size: None,
            auto_hide: None,
            system_transparency: None,
            show_widgets_button: None,
            show_task_view: None,
            reapply_on_explorer_restart: true,
        }
    }
}

impl TaskbarConfig {
    fn validate(mut self) -> Result<Self> {
        if !self.opacity.is_finite() {
            return Err(OverlayError::Config("opacity must be a number".into()));
        }
        self.opacity = self.opacity.clamp(0.0, 1.0);
        if self.tint > 0xFF_FFFF {
            return Err(OverlayError::Config(
                "tint must be a 0xRRGGBB colour".into(),
            ));
        }
        Ok(self)
    }

    fn accent(&self) -> AccentPolicy {
        AccentPolicy::new(self.finish, self.opacity, self.tint)
    }
}

fn advanced(name: &str) -> SettingTarget {
    SettingTarget::Registry(RegTarget::hkcu(ADVANCED, name))
}

pub struct TaskbarModule;

impl TaskbarModule {
    /// The registry settings this config will actually touch, paired with the
    /// value to write. A `None` option contributes nothing, so a setting the
    /// user did not ask OVERLAY to manage is never captured or owned.
    fn planned_registry(&self, cfg: &TaskbarConfig, is_win11: bool) -> Vec<(SettingTarget, SettingValue)> {
        let mut out = Vec::new();
        if let Some(a) = cfg.alignment {
            // Windows 10 has no alignment setting; writing it would be inert
            // clutter in the user's registry.
            if is_win11 {
                let v = match a {
                    Alignment::Left => 0,
                    Alignment::Center => 1,
                };
                out.push((advanced("TaskbarAl"), SettingValue::dword(v)));
            }
        }
        if let Some(s) = cfg.icon_size {
            if is_win11 {
                out.push((advanced("TaskbarSi"), SettingValue::dword(s.dword())));
            }
        }
        if let Some(w) = cfg.show_widgets_button {
            if is_win11 {
                out.push((advanced("TaskbarDa"), SettingValue::dword(w as u32)));
            }
        }
        if let Some(t) = cfg.show_task_view {
            out.push((advanced("ShowTaskViewButton"), SettingValue::dword(t as u32)));
        }
        if let Some(t) = cfg.system_transparency {
            out.push((
                SettingTarget::Registry(RegTarget::hkcu(PERSONALIZE, "EnableTransparency")),
                SettingValue::dword(t as u32),
            ));
        }
        out
    }

    fn autohide_target() -> SettingTarget {
        SettingTarget::Registry(RegTarget::hkcu(STUCK_RECTS, "Settings"))
    }
}

/// Produces the `StuckRects3` blob with auto-hide set as asked, leaving every
/// other byte alone.
///
/// Returns `None` when the blob is not the shape we understand — better to
/// skip the feature than to write a guess into a struct Explorer parses.
pub fn blob_with_autohide(current: &RegValue, enabled: bool) -> Option<Vec<u8>> {
    let RegValue::Binary(bytes) = current else {
        return None;
    };
    if bytes.len() <= STUCK_FLAGS_BYTE {
        return None;
    }
    let mut next = bytes.clone();
    if enabled {
        next[STUCK_FLAGS_BYTE] |= AUTOHIDE_BIT;
    } else {
        next[STUCK_FLAGS_BYTE] &= !AUTOHIDE_BIT;
    }
    Some(next)
}

pub fn blob_autohide_enabled(value: &RegValue) -> Option<bool> {
    let RegValue::Binary(bytes) = value else {
        return None;
    };
    bytes
        .get(STUCK_FLAGS_BYTE)
        .map(|b| b & AUTOHIDE_BIT != 0)
}

impl CustomizationModule for TaskbarModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "taskbar",
            name: "Taskbar Studio",
            description:
                "Translucency, alignment, icon size and auto-hide for the Windows taskbar.",
            version: "1.0.0",
            category: Category::Appearance,
            requires: &[],
            conflicts_with: &[],
            touches_system: true,
        }
    }

    fn availability(&self, ctx: &ModuleCtx) -> Availability {
        if !ctx.env.shell_running {
            return Availability::unavailable(
                "Windows Explorer is not running, so there is no taskbar to change.",
            );
        }
        if !ctx.env.taskbar_accent_supported {
            return Availability::partial(
                "This Windows build does not expose window composition, so translucency is \
                 unavailable. Alignment, icon size and auto-hide still work.",
            );
        }
        if !ctx.env.is_windows_11 {
            return Availability::partial(
                "Alignment, icon size and the widgets button are Windows 11 features and are \
                 hidden on this build.",
            );
        }
        Availability::Available
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(TaskbarConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: TaskbarConfig = serde_json::from_value(config.clone())
            .map_err(|e| OverlayError::Config(format!("taskbar settings are invalid: {e}")))?;
        Ok(serde_json::to_value(cfg.validate()?)?)
    }

    fn capture_original(&self, ctx: &ModuleCtx) -> Result<()> {
        // Only the accent is captured unconditionally — it is the one thing
        // the module always changes. Registry settings are captured in
        // `enable`, where we know which of them the config actually touches.
        ctx.journal
            .capture_original(self.id(), &SettingTarget::TaskbarAccent)?;
        Ok(())
    }

    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: TaskbarConfig = serde_json::from_value(self.validate_config(config)?)?;

        for (target, value) in self.planned_registry(&cfg, ctx.env.is_windows_11) {
            ctx.journal.capture_original(self.id(), &target)?;
            ctx.journal.apply(self.id(), &target, &value)?;
        }

        if let Some(want) = cfg.auto_hide {
            let target = Self::autohide_target();
            let original = ctx.journal.capture_original(self.id(), &target)?;
            let SettingValue::Reg(current) = &original else {
                return Err(OverlayError::other("unexpected auto-hide value shape"));
            };
            match blob_with_autohide(current, want) {
                Some(next) => {
                    ctx.journal.apply(
                        self.id(),
                        &target,
                        &SettingValue::Reg(RegValue::Binary(next)),
                    )?;
                }
                None => {
                    return Err(OverlayError::other(
                        "the taskbar settings blob is not in a layout OVERLAY recognises, \
                         so auto-hide was left alone",
                    ))
                }
            }
        }

        // Registry changes need Explorer told; the accent does not.
        shell::notify_settings_changed();

        if cfg.finish != TaskbarFinish::System {
            ctx.journal.apply(
                self.id(),
                &SettingTarget::TaskbarAccent,
                &SettingValue::Accent(vec![crate::journal::target::AccentSnapshot {
                    hwnd: 0,
                    policy: cfg.accent(),
                }]),
            )?;
        }
        Ok(())
    }

    fn disable(&self, ctx: &ModuleCtx) -> Result<()> {
        let outcomes = ctx.journal.restore_module(self.id())?;
        shell::notify_settings_changed();
        let failed: Vec<_> = outcomes
            .iter()
            .filter(|o| o.status == crate::journal::RestoreStatus::Failed)
            .collect();
        if !failed.is_empty() {
            return Err(OverlayError::other(format!(
                "{} taskbar setting(s) could not be put back: {}",
                failed.len(),
                failed
                    .iter()
                    .map(|f| format!("{} ({})", f.label, f.detail))
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
        Ok(())
    }

    fn is_active(&self, ctx: &ModuleCtx) -> bool {
        ctx.journal
            .owned()
            .map(|o| o.iter().any(|s| s.module_id == self.id()))
            .unwrap_or(false)
    }

    fn notes(&self, ctx: &ModuleCtx) -> Vec<String> {
        let mut notes = Vec::new();
        if ctx.env.taskbar_accent_supported {
            notes.push(
                "Translucency is applied to the taskbar window and is re-applied automatically \
                 if Explorer restarts. It leaves no permanent change behind."
                    .into(),
            );
        }
        if !ctx.env.is_windows_11 {
            notes.push("Alignment and icon size need Windows 11.".into());
        }
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(flags: u8) -> RegValue {
        let mut v = vec![0u8; 40];
        v[STUCK_FLAGS_BYTE] = flags;
        RegValue::Binary(v)
    }

    #[test]
    fn autohide_sets_only_its_own_bit() {
        // Every other bit in that byte is a different taskbar preference.
        let current = blob(0b1111_0110);
        let next = blob_with_autohide(&current, true).unwrap();
        assert_eq!(next[STUCK_FLAGS_BYTE], 0b1111_0111);
    }

    #[test]
    fn autohide_clears_only_its_own_bit() {
        let current = blob(0b1111_0111);
        let next = blob_with_autohide(&current, false).unwrap();
        assert_eq!(next[STUCK_FLAGS_BYTE], 0b1111_0110);
    }

    #[test]
    fn autohide_leaves_every_other_byte_identical() {
        let original: Vec<u8> = (0u8..40).collect();
        let next = blob_with_autohide(&RegValue::Binary(original.clone()), true).unwrap();
        assert_eq!(next.len(), original.len());
        for (i, (a, b)) in original.iter().zip(next.iter()).enumerate() {
            if i != STUCK_FLAGS_BYTE {
                assert_eq!(a, b, "byte {i} was modified");
            }
        }
    }

    #[test]
    fn a_blob_of_an_unexpected_shape_is_refused_rather_than_guessed_at() {
        assert!(blob_with_autohide(&RegValue::Binary(vec![0; 4]), true).is_none());
        assert!(blob_with_autohide(&RegValue::Dword(1), true).is_none());
        assert!(blob_with_autohide(&RegValue::Absent, true).is_none());
    }

    #[test]
    fn autohide_state_reads_back_from_the_blob() {
        assert_eq!(blob_autohide_enabled(&blob(0x01)), Some(true));
        assert_eq!(blob_autohide_enabled(&blob(0x00)), Some(false));
        assert_eq!(blob_autohide_enabled(&RegValue::Absent), None);
    }

    #[test]
    fn setting_autohide_is_idempotent() {
        let once = blob_with_autohide(&blob(0), true).unwrap();
        let twice = blob_with_autohide(&RegValue::Binary(once.clone()), true).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn untouched_options_produce_no_registry_writes() {
        // The guarantee that OVERLAY owns only what the user asked it to.
        let cfg = TaskbarConfig::default();
        assert!(
            TaskbarModule.planned_registry(&cfg, true).is_empty(),
            "a default config must not claim ownership of any registry value"
        );
    }

    #[test]
    fn windows_11_only_settings_are_skipped_on_windows_10() {
        let cfg = TaskbarConfig {
            alignment: Some(Alignment::Left),
            icon_size: Some(IconSize::Small),
            show_widgets_button: Some(false),
            show_task_view: Some(false),
            ..Default::default()
        };
        let on_11 = TaskbarModule.planned_registry(&cfg, true);
        let on_10 = TaskbarModule.planned_registry(&cfg, false);
        assert_eq!(on_11.len(), 4);
        // Only ShowTaskViewButton exists on Windows 10.
        assert_eq!(on_10.len(), 1);
        assert!(on_10[0].0.id().contains("ShowTaskViewButton"));
    }

    #[test]
    fn alignment_maps_to_the_values_explorer_expects() {
        let left = TaskbarConfig {
            alignment: Some(Alignment::Left),
            ..Default::default()
        };
        let planned = TaskbarModule.planned_registry(&left, true);
        assert_eq!(planned[0].1, SettingValue::dword(0));

        let center = TaskbarConfig {
            alignment: Some(Alignment::Center),
            ..Default::default()
        };
        let planned = TaskbarModule.planned_registry(&center, true);
        assert_eq!(planned[0].1, SettingValue::dword(1));
    }

    #[test]
    fn config_validation_clamps_opacity_and_rejects_a_bad_tint() {
        let cfg = TaskbarConfig {
            opacity: 7.5,
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(cfg.opacity, 1.0);

        assert!(TaskbarConfig {
            tint: 0x1_000_000,
            ..Default::default()
        }
        .validate()
        .is_err());

        assert!(TaskbarConfig {
            opacity: f32::NAN,
            ..Default::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn unknown_keys_in_a_config_are_rejected() {
        // An imported profile must not smuggle fields past validation.
        let raw = serde_json::json!({ "finish": "blurred", "somethingElse": 1 });
        assert!(TaskbarModule.validate_config(&raw).is_err());
    }
}
