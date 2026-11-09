//! QuickDock as a profile-switchable module.
//!
//! Same shape as the WallFlow module: enabling means "this profile expects
//! QuickDock available", and the switch starts it silently with the
//! `--autostart` flag its own single-instance handler honours. Disabling does
//! not quit it — QuickDock holds an in-memory search index it persists on
//! exit, and force-terminating it would throw that away.
//!
//! Note this module is *not* in conflict with OVERLAY's dock. QuickDock is a
//! keyboard search palette; see `integrations::quickdock` for why the two are
//! complementary.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::integrations::quickdock::{pretty_accelerator, QuickDock};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct QuickDockConfig {
    /// Start QuickDock when this profile is applied, if it is not running.
    pub launch_on_activate: bool,
    /// Start it into the tray rather than popping the search bar.
    pub launch_silently: bool,
}

impl Default for QuickDockConfig {
    fn default() -> Self {
        Self {
            launch_on_activate: true,
            launch_silently: true,
        }
    }
}

pub struct QuickDockModule;

impl CustomizationModule for QuickDockModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "quickdock",
            name: "QuickDock",
            description:
                "Keep the QuickDock search palette available with this profile.",
            version: "1.0.0",
            category: Category::Integrations,
            requires: &[],
            // Declared so a profile cannot ask OVERLAY to treat two different
            // things as "the launcher for this profile"; see Registry::plan.
            conflicts_with: &["dock"],
            touches_system: false,
        }
    }

    fn availability(&self, _ctx: &ModuleCtx) -> Availability {
        let (exe, _, _) = QuickDock::locate();
        match exe {
            Some(_) => Availability::Available,
            None if QuickDock::config_dir().is_some() => Availability::unavailable(
                "QuickDock's settings folder is here but its program was not found. It may have \
                 been uninstalled or moved.",
            ),
            None => Availability::unavailable("QuickDock is not installed on this machine."),
        }
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(QuickDockConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: QuickDockConfig = serde_json::from_value(config.clone())
            .map_err(|e| OverlayError::Config(format!("QuickDock settings are invalid: {e}")))?;
        Ok(serde_json::to_value(cfg)?)
    }

    fn capture_original(&self, _ctx: &ModuleCtx) -> Result<()> {
        Ok(())
    }

    fn enable(&self, _ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: QuickDockConfig = serde_json::from_value(self.validate_config(config)?)?;
        if cfg.launch_on_activate && !QuickDock::is_running() {
            QuickDock::launch(cfg.launch_silently)?;
        }
        Ok(())
    }

    fn disable(&self, _ctx: &ModuleCtx) -> Result<()> {
        // Deliberately a no-op. See the module docs.
        Ok(())
    }

    fn is_active(&self, _ctx: &ModuleCtx) -> bool {
        QuickDock::is_running()
    }

    fn activity_is_observable(&self) -> bool {
        // Starting another application (or binding a shortcut) leaves
        // nothing observable that means "the user switched this on".
        false
    }

    fn notes(&self, _ctx: &ModuleCtx) -> Vec<String> {
        let mut notes = vec![
            "QuickDock is a keyboard launcher rather than an icon dock, so it does a different \
             job from OVERLAY's dock."
                .to_string(),
        ];
        if let Ok(s) = QuickDock::read_settings() {
            if !s.hotkey.is_empty() {
                notes.push(format!(
                    "Open it with {}. OVERLAY avoids binding that shortcut.",
                    pretty_accelerator(&s.hotkey)
                ));
            }
        }
        notes.push(
            "Turning this off leaves QuickDock running; quit it from its own tray icon so it \
             can save its search index."
                .into(),
        );
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_starts_quickdock_quietly() {
        // Popping a search bar in someone's face on profile switch would be
        // an interruption, not a feature.
        let cfg = QuickDockConfig::default();
        assert!(cfg.launch_on_activate && cfg.launch_silently);
    }

    #[test]
    fn unknown_config_keys_are_rejected() {
        let m = QuickDockModule;
        assert!(m
            .validate_config(&serde_json::json!({ "launch_on_activate": true, "quit": true }))
            .is_err());
    }

    #[test]
    fn the_default_config_validates() {
        let m = QuickDockModule;
        assert!(m.validate_config(&m.default_config()).is_ok());
    }
}
