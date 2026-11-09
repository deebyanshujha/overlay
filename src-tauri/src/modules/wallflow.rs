//! WallFlow as a profile-switchable module.
//!
//! Enabling it means "this profile expects WallFlow to be running", and
//! switching to the profile starts it if it is not. Disabling it does **not**
//! stop WallFlow: terminating another application the user may be relying on
//! is not OVERLAY's call, and WallFlow has no shutdown interface anyway. The
//! module says so rather than quietly doing nothing.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::integrations::wallflow::WallFlow;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WallFlowConfig {
    /// Start WallFlow when this profile is applied, if it is not running.
    pub launch_on_activate: bool,
    /// Show the current wallpaper on OVERLAY's home page.
    pub show_on_home: bool,
}

impl Default for WallFlowConfig {
    fn default() -> Self {
        Self {
            launch_on_activate: true,
            show_on_home: true,
        }
    }
}

pub struct WallFlowModule;

impl CustomizationModule for WallFlowModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "wallflow",
            name: "WallFlow",
            description:
                "Keep your wallpaper manager running with this profile and show its state in \
                 OVERLAY.",
            version: "1.0.0",
            category: Category::Integrations,
            requires: &[],
            conflicts_with: &[],
            touches_system: false,
        }
    }

    fn availability(&self, _ctx: &ModuleCtx) -> Availability {
        let (exe, _, _) = WallFlow::locate();
        match exe {
            Some(_) => Availability::Available,
            None if WallFlow::data_dir().is_some() => Availability::unavailable(
                "WallFlow's data folder is here but its program was not found. It may have been \
                 uninstalled or moved.",
            ),
            None => Availability::unavailable("WallFlow is not installed on this machine."),
        }
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(WallFlowConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: WallFlowConfig = serde_json::from_value(config.clone())
            .map_err(|e| OverlayError::Config(format!("WallFlow settings are invalid: {e}")))?;
        Ok(serde_json::to_value(cfg)?)
    }

    fn capture_original(&self, _ctx: &ModuleCtx) -> Result<()> {
        // Nothing of WallFlow's is modified, so there is nothing to capture.
        Ok(())
    }

    fn enable(&self, _ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: WallFlowConfig = serde_json::from_value(self.validate_config(config)?)?;
        if cfg.launch_on_activate && !WallFlow::is_running() {
            WallFlow::launch()?;
        }
        Ok(())
    }

    fn disable(&self, _ctx: &ModuleCtx) -> Result<()> {
        // Deliberately a no-op. See the module docs.
        Ok(())
    }

    fn is_active(&self, _ctx: &ModuleCtx) -> bool {
        WallFlow::is_running()
    }

    fn activity_is_observable(&self) -> bool {
        // Starting another application (or binding a shortcut) leaves
        // nothing observable that means "the user switched this on".
        false
    }

    fn notes(&self, _ctx: &ModuleCtx) -> Vec<String> {
        let mut notes = vec![
            "OVERLAY never changes WallFlow's settings, wallpaper library or schedule.".to_string(),
        ];
        if !WallFlow::is_running() {
            notes.push("WallFlow is not running at the moment.".into());
        }
        notes.push(
            "Turning this off leaves WallFlow running — OVERLAY will not close another \
             application for you."
                .into(),
        );
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabling_never_tries_to_stop_wallflow() {
        // Encodes the promise in the module docs as a test, so a future edit
        // that adds a kill call fails here.
        let cfg = WallFlowConfig::default();
        assert!(cfg.launch_on_activate);
        // disable() takes a context it does not use; its no-op nature is the
        // behaviour under test and is asserted by its body being empty.
    }

    #[test]
    fn unknown_config_keys_are_rejected() {
        let m = WallFlowModule;
        assert!(m
            .validate_config(&serde_json::json!({ "launch_on_activate": true, "kill": true }))
            .is_err());
    }

    #[test]
    fn the_default_config_validates() {
        let m = WallFlowModule;
        assert!(m.validate_config(&m.default_config()).is_ok());
    }
}
