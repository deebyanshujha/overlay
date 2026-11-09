//! Desktop icon visibility.
//!
//! This module hides and shows icons. It does not move, rename or delete
//! anything on the Desktop — there is no code path here that touches the
//! filesystem at all, which is why "no desktop shortcuts are deleted" is a
//! property of the design rather than a promise.
//!
//! Arrangement is preserved for free: Explorer keeps icon positions in its own
//! view state, and hiding the listview does not disturb them.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::journal::{SettingTarget, SettingValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DesktopIconsConfig {
    /// `false` hides the icons. Named for what the user sees rather than for
    /// the registry value, which is inverted and easy to get backwards.
    pub show_icons: bool,
}

impl Default for DesktopIconsConfig {
    fn default() -> Self {
        // Hiding icons is the point of the module, so that is the default when
        // a user switches it on.
        Self { show_icons: false }
    }
}

pub struct DesktopIconsModule;

impl CustomizationModule for DesktopIconsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "desktop_icons",
            name: "Desktop Icons",
            description:
                "Hide or show the icons on your desktop. Shortcuts are never moved or deleted.",
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
                "Windows Explorer is not running, so there is no desktop to change.",
            );
        }
        match crate::sys::shell::desktop_icons_visible() {
            Ok(_) => Availability::Available,
            Err(e) => Availability::unavailable(format!("the desktop icon view was not found: {e}")),
        }
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(DesktopIconsConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: DesktopIconsConfig = serde_json::from_value(config.clone()).map_err(|e| {
            OverlayError::Config(format!("desktop icon settings are invalid: {e}"))
        })?;
        Ok(serde_json::to_value(cfg)?)
    }

    fn capture_original(&self, ctx: &ModuleCtx) -> Result<()> {
        ctx.journal
            .capture_original(self.id(), &SettingTarget::DesktopIcons)?;
        Ok(())
    }

    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: DesktopIconsConfig = serde_json::from_value(self.validate_config(config)?)?;
        ctx.journal.apply(
            self.id(),
            &SettingTarget::DesktopIcons,
            &SettingValue::Bool(cfg.show_icons),
        )
    }

    fn disable(&self, ctx: &ModuleCtx) -> Result<()> {
        let outcomes = ctx.journal.restore_module(self.id())?;
        if let Some(f) = outcomes
            .iter()
            .find(|o| o.status == crate::journal::RestoreStatus::Failed)
        {
            return Err(OverlayError::other(format!(
                "desktop icons could not be put back: {}",
                f.detail
            )));
        }
        Ok(())
    }

    fn is_active(&self, ctx: &ModuleCtx) -> bool {
        ctx.journal
            .owned_one(&SettingTarget::DesktopIcons)
            .map(|o| o.is_some())
            .unwrap_or(false)
    }

    fn notes(&self, ctx: &ModuleCtx) -> Vec<String> {
        let mut notes = vec![
            "Only visibility changes. Icon positions, names and files are left exactly as they are."
                .to_string(),
        ];
        if let Ok(Some(original)) = ctx.journal.original_of(&SettingTarget::DesktopIcons) {
            notes.push(format!(
                "Before OVERLAY, your desktop icons were {}.",
                original.describe()
            ));
        }
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_to_hide_since_that_is_what_enabling_means() {
        let cfg = DesktopIconsConfig::default();
        assert!(!cfg.show_icons);
    }

    #[test]
    fn config_round_trips_and_rejects_unknown_keys() {
        let m = DesktopIconsModule;
        let good = serde_json::json!({ "show_icons": true });
        assert_eq!(m.validate_config(&good).unwrap(), good);

        let bad = serde_json::json!({ "show_icons": true, "deleteEverything": true });
        assert!(m.validate_config(&bad).is_err());
    }

    #[test]
    fn a_non_boolean_is_rejected() {
        let m = DesktopIconsModule;
        assert!(m
            .validate_config(&serde_json::json!({ "show_icons": "yes" }))
            .is_err());
    }
}
