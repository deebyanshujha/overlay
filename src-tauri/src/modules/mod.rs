//! The modular customization engine.
//!
//! Every customization OVERLAY offers is a [`CustomizationModule`]. The
//! registry below is the only thing that knows the full set, and the profile
//! engine drives modules exclusively through this trait — so adding a
//! customization never means touching the profile, restoration or UI layers.
//!
//! # Isolation
//!
//! A module that fails must not take anything else down with it. Two
//! guarantees enforce that:
//!
//! * every module writes through the [`Journal`], so its changes are tracked
//!   and reversible independently of every other module's;
//! * the registry's batch operations ([`Registry::enable_many`],
//!   [`Registry::disable_many`]) collect per-module results instead of
//!   short-circuiting, so one failure is reported alongside the successes.

pub mod control_center;
pub mod desktop_icons;
pub mod dock;
pub mod dropzone;
pub mod hud;
pub mod quickdock;
pub mod taskbar;
pub mod wallflow;
pub mod widgets;

use crate::error::{OverlayError, Result};
use crate::journal::{Journal, RestoreOutcome};
use crate::sys::Environment;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Where a module appears in the Customize page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Appearance,
    Widgets,
    Surfaces,
    Integrations,
}

/// Static facts about a module: what it is, not what it is doing.
/// Static, so it is only ever serialised outward to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModuleDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    pub category: Category,
    /// Modules that must be enabled first.
    pub requires: &'static [&'static str],
    /// Modules that cannot be enabled at the same time.
    pub conflicts_with: &'static [&'static str],
    /// True when enabling it changes Windows itself rather than only drawing
    /// an OVERLAY window. Drives the "this will change your desktop" warning
    /// and decides which modules the restore report must mention.
    pub touches_system: bool,
}

/// Whether a module can run here at all, and why not when it cannot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Availability {
    Available,
    /// Usable, but something is degraded — say so rather than pretending.
    Partial { note: String },
    Unavailable { reason: String },
}

impl Availability {
    pub fn is_usable(&self) -> bool {
        !matches!(self, Availability::Unavailable { .. })
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Availability::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn partial(note: impl Into<String>) -> Self {
        Availability::Partial { note: note.into() }
    }
}

/// A module's live state, as the UI renders it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub category: Category,
    pub availability: Availability,
    pub enabled: bool,
    pub touches_system: bool,
    pub requires: Vec<String>,
    pub conflicts_with: Vec<String>,
    /// Settings this module currently owns — i.e. what disabling it will undo.
    pub owned_settings: usize,
    pub config: Value,
    /// Anything the module wants to tell the user right now (e.g. "GPU
    /// temperature is not exposed by this driver").
    pub notes: Vec<String>,
}

/// Everything a module needs to do its job.
///
/// `app` is optional on purpose: the emergency recovery path constructs a
/// context with no Tauri runtime at all, and modules that only touch Windows
/// settings still work there. Modules that own windows check for it and
/// report honestly when there is nothing to drive.
#[derive(Clone)]
pub struct ModuleCtx {
    pub journal: Journal,
    pub env: Arc<Environment>,
    pub app: Option<tauri::AppHandle>,
}

impl ModuleCtx {
    pub fn headless(journal: Journal, env: Arc<Environment>) -> Self {
        Self {
            journal,
            env,
            app: None,
        }
    }

    pub fn app(&self) -> Result<&tauri::AppHandle> {
        self.app.as_ref().ok_or_else(|| {
            OverlayError::other("this action needs the OVERLAY application to be running")
        })
    }
}

/// The lifecycle every customization implements.
pub trait CustomizationModule: Send + Sync {
    fn descriptor(&self) -> ModuleDescriptor;

    fn id(&self) -> &'static str {
        self.descriptor().id
    }

    /// Can this run on this machine, right now?
    fn availability(&self, ctx: &ModuleCtx) -> Availability;

    /// Validates and normalises a configuration, returning the canonical form
    /// that gets stored. Rejecting bad input here is what keeps an imported
    /// profile from reaching the Windows layer with nonsense in it.
    fn validate_config(&self, config: &Value) -> Result<Value>;

    /// The configuration a module starts with.
    fn default_config(&self) -> Value;

    /// Captures whatever original state this module will modify, before it
    /// modifies anything. Called by the engine ahead of `enable`.
    fn capture_original(&self, ctx: &ModuleCtx) -> Result<()>;

    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()>;

    /// Turns the module off and puts back everything it owns.
    fn disable(&self, ctx: &ModuleCtx) -> Result<()>;

    /// Restores this module's system settings without the module needing to
    /// be functional. The default is the right answer for every module whose
    /// changes all go through the journal.
    fn restore(&self, ctx: &ModuleCtx) -> Result<Vec<RestoreOutcome>> {
        ctx.journal.restore_module(self.id())
    }

    /// Whether the module is currently doing something. Derived from the
    /// world — owned settings, live windows — not from a cached flag, so a
    /// crash cannot leave the UI claiming a module is on when it is not.
    fn is_active(&self, ctx: &ModuleCtx) -> bool;

    /// Whether [`is_active`](Self::is_active) can honestly be answered by
    /// observing the system.
    ///
    /// False for modules whose only effect happens at enable time — starting
    /// another application, or registering a shortcut. For those, "is
    /// WallFlow running?" is not the same question as "has the user switched
    /// this on", and answering the first while being asked the second made
    /// the Original Windows profile — which enables nothing — report three
    /// active customizations. Where this is false, the engine reads the
    /// active profile instead.
    fn activity_is_observable(&self) -> bool {
        true
    }

    /// Extra per-module notes for the UI.
    fn notes(&self, _ctx: &ModuleCtx) -> Vec<String> {
        Vec::new()
    }
}

/// Result of a batch enable/disable: one entry per module, successes and
/// failures together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleOutcome {
    pub module_id: String,
    pub ok: bool,
    pub error: Option<String>,
}

impl ModuleOutcome {
    fn ok(id: &str) -> Self {
        Self {
            module_id: id.to_string(),
            ok: true,
            error: None,
        }
    }

    fn failed(id: &str, e: &OverlayError) -> Self {
        Self {
            module_id: id.to_string(),
            ok: false,
            error: Some(e.to_string()),
        }
    }
}

/// The set of modules OVERLAY ships.
pub struct Registry {
    modules: BTreeMap<&'static str, Box<dyn CustomizationModule>>,
}

impl Registry {
    pub fn new() -> Self {
        let mut modules: BTreeMap<&'static str, Box<dyn CustomizationModule>> = BTreeMap::new();
        for m in all_modules() {
            modules.insert(m.descriptor().id, m);
        }
        Self { modules }
    }

    pub fn get(&self, id: &str) -> Result<&dyn CustomizationModule> {
        self.modules
            .get(id)
            .map(|b| b.as_ref())
            .ok_or_else(|| OverlayError::UnknownModule(id.to_string()))
    }

    pub fn ids(&self) -> Vec<&'static str> {
        self.modules.keys().copied().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn CustomizationModule> {
        self.modules.values().map(|b| b.as_ref())
    }

    /// Full status of every module.
    ///
    /// `enabled_in_profile` is the active profile's module set, used for the
    /// modules whose activity cannot be observed from the system — see
    /// [`CustomizationModule::activity_is_observable`].
    pub fn statuses(
        &self,
        ctx: &ModuleCtx,
        configs: &BTreeMap<String, Value>,
        enabled_in_profile: &HashSet<String>,
    ) -> Vec<ModuleStatus> {
        let owned = ctx.journal.owned().unwrap_or_default();
        self.iter()
            .map(|m| {
                let d = m.descriptor();
                let config = configs
                    .get(d.id)
                    .cloned()
                    .unwrap_or_else(|| m.default_config());
                let enabled = if m.activity_is_observable() {
                    m.is_active(ctx)
                } else {
                    enabled_in_profile.contains(d.id)
                };
                ModuleStatus {
                    id: d.id.to_string(),
                    name: d.name.to_string(),
                    description: d.description.to_string(),
                    version: d.version.to_string(),
                    category: d.category,
                    availability: m.availability(ctx),
                    enabled,
                    touches_system: d.touches_system,
                    requires: d.requires.iter().map(|s| s.to_string()).collect(),
                    conflicts_with: d.conflicts_with.iter().map(|s| s.to_string()).collect(),
                    owned_settings: owned.iter().filter(|o| o.module_id == d.id).count(),
                    config,
                    notes: m.notes(ctx),
                }
            })
            .collect()
    }

    /// Checks that a desired set of enabled modules is internally consistent,
    /// *before* anything is applied. Returns the set in dependency order.
    ///
    /// Running this up front is what lets a profile switch fail cleanly rather
    /// than halfway through.
    pub fn plan(&self, desired: &HashSet<String>) -> Result<Vec<String>> {
        for id in desired {
            let m = self.get(id)?;
            let d = m.descriptor();

            for need in d.requires {
                if !desired.contains(*need) {
                    return Err(OverlayError::MissingDependency(
                        id.clone(),
                        need.to_string(),
                    ));
                }
            }
            for other in d.conflicts_with {
                if desired.contains(*other) {
                    return Err(OverlayError::Conflict {
                        id: id.clone(),
                        other: other.to_string(),
                        reason: "both manage the same part of the desktop".into(),
                    });
                }
            }
        }
        Ok(self.order(desired))
    }

    /// Dependency order: a module is listed after everything it requires.
    ///
    /// The graph is tiny and acyclic by construction (dependencies are
    /// declared as constants), so a depth-limited walk is both sufficient and
    /// immune to a cycle introduced by a future edit.
    fn order(&self, desired: &HashSet<String>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut placed: HashSet<String> = HashSet::new();

        // At most one pass per module is needed for a correct acyclic graph;
        // the bound also guarantees termination if one is ever introduced.
        for _ in 0..=desired.len() {
            for id in desired {
                if placed.contains(id) {
                    continue;
                }
                let ready = match self.get(id) {
                    Ok(m) => m
                        .descriptor()
                        .requires
                        .iter()
                        .all(|r| !desired.contains(*r) || placed.contains(*r)),
                    Err(_) => true,
                };
                if ready {
                    placed.insert(id.clone());
                    out.push(id.clone());
                }
            }
            if placed.len() == desired.len() {
                break;
            }
        }
        // Anything left over is part of a cycle; append it so it is still
        // attempted rather than silently dropped.
        for id in desired {
            if !placed.contains(id) {
                out.push(id.clone());
            }
        }
        out
    }

    /// Enables several modules, continuing past failures so one broken module
    /// cannot block the rest.
    pub fn enable_many(
        &self,
        ctx: &ModuleCtx,
        ids: &[String],
        configs: &BTreeMap<String, Value>,
    ) -> Vec<ModuleOutcome> {
        ids.iter()
            .map(|id| match self.enable_one(ctx, id, configs.get(id)) {
                Ok(()) => ModuleOutcome::ok(id),
                Err(e) => ModuleOutcome::failed(id, &e),
            })
            .collect()
    }

    pub fn enable_one(&self, ctx: &ModuleCtx, id: &str, config: Option<&Value>) -> Result<()> {
        let m = self.get(id)?;
        let availability = m.availability(ctx);
        if let Availability::Unavailable { reason } = &availability {
            return Err(OverlayError::Unavailable {
                id: id.to_string(),
                reason: reason.clone(),
            });
        }
        let config = match config {
            Some(c) => m.validate_config(c)?,
            None => m.default_config(),
        };
        // Capture before the first write, always.
        m.capture_original(ctx)?;
        m.enable(ctx, &config)
    }

    pub fn disable_many(&self, ctx: &ModuleCtx, ids: &[String]) -> Vec<ModuleOutcome> {
        ids.iter()
            .map(|id| match self.get(id).and_then(|m| m.disable(ctx)) {
                Ok(()) => ModuleOutcome::ok(id),
                Err(e) => ModuleOutcome::failed(id, &e),
            })
            .collect()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

fn all_modules() -> Vec<Box<dyn CustomizationModule>> {
    vec![
        Box::new(taskbar::TaskbarModule),
        Box::new(desktop_icons::DesktopIconsModule),
        Box::new(widgets::WidgetsModule),
        Box::new(dock::DockModule),
        Box::new(hud::HudModule),
        Box::new(control_center::ControlCenterModule),
        Box::new(dropzone::DropzoneModule),
        Box::new(wallflow::WallFlowModule),
        Box::new(quickdock::QuickDockModule),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_module_has_a_unique_id_and_sane_descriptor() {
        let r = Registry::new();
        let ids = r.ids();
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate module id");
        assert!(ids.len() >= 9, "expected every shipped module registered");

        for m in r.iter() {
            let d = m.descriptor();
            assert!(!d.name.is_empty(), "{} has no name", d.id);
            assert!(!d.description.is_empty(), "{} has no description", d.id);
            assert!(!d.version.is_empty(), "{} has no version", d.id);
        }
    }

    #[test]
    fn declared_dependencies_and_conflicts_all_name_real_modules() {
        // A typo here would turn into an unsatisfiable profile at runtime.
        let r = Registry::new();
        let known: HashSet<&str> = r.ids().into_iter().collect();
        for m in r.iter() {
            let d = m.descriptor();
            for x in d.requires.iter().chain(d.conflicts_with.iter()) {
                assert!(known.contains(x), "{} references unknown module {x}", d.id);
            }
        }
    }

    #[test]
    fn a_module_never_declares_itself_a_dependency_or_conflict() {
        let r = Registry::new();
        for m in r.iter() {
            let d = m.descriptor();
            assert!(!d.requires.contains(&d.id), "{} requires itself", d.id);
            assert!(
                !d.conflicts_with.contains(&d.id),
                "{} conflicts with itself",
                d.id
            );
        }
    }

    #[test]
    fn every_default_config_passes_its_own_validator() {
        // Otherwise enabling a module straight out of the box would fail.
        let r = Registry::new();
        for m in r.iter() {
            let d = m.default_config();
            assert!(
                m.validate_config(&d).is_ok(),
                "{}'s default config is rejected by its own validator",
                m.id()
            );
        }
    }

    #[test]
    fn validation_is_idempotent() {
        // Normalising an already-normalised config must not change it, or
        // saving a profile twice would drift.
        let r = Registry::new();
        for m in r.iter() {
            let once = m.validate_config(&m.default_config()).unwrap();
            let twice = m.validate_config(&once).unwrap();
            assert_eq!(once, twice, "{} normalises unstably", m.id());
        }
    }

    #[test]
    fn plan_rejects_a_missing_dependency() {
        let r = Registry::new();
        // The HUD is a widget surface and declares that dependency.
        let err = r.plan(&set(&["hud"]));
        match err {
            Err(OverlayError::MissingDependency(a, b)) => {
                assert_eq!(a, "hud");
                assert_eq!(b, "widgets");
            }
            other => panic!("expected a missing dependency, got {other:?}"),
        }
    }

    #[test]
    fn plan_accepts_the_dependency_when_it_is_present() {
        let r = Registry::new();
        let plan = r.plan(&set(&["hud", "widgets"])).unwrap();
        let hud = plan.iter().position(|x| x == "hud").unwrap();
        let widgets = plan.iter().position(|x| x == "widgets").unwrap();
        assert!(widgets < hud, "widgets must be enabled before the HUD");
    }

    #[test]
    fn plan_rejects_two_conflicting_docks() {
        let r = Registry::new();
        match r.plan(&set(&["dock", "quickdock"])) {
            Err(OverlayError::Conflict { .. }) => {}
            other => panic!("expected a conflict, got {other:?}"),
        }
    }

    #[test]
    fn plan_rejects_an_unknown_module() {
        let r = Registry::new();
        assert!(matches!(
            r.plan(&set(&["not-a-module"])),
            Err(OverlayError::UnknownModule(_))
        ));
    }

    #[test]
    fn an_empty_plan_is_valid() {
        // "Original Windows" is exactly this: nothing enabled.
        let r = Registry::new();
        assert!(r.plan(&HashSet::new()).unwrap().is_empty());
    }

    #[test]
    fn ordering_includes_every_requested_module_exactly_once() {
        let r = Registry::new();
        let want = set(&["widgets", "hud", "taskbar", "desktop_icons", "dropzone"]);
        let plan = r.plan(&want).unwrap();
        assert_eq!(plan.len(), want.len());
        let unique: HashSet<_> = plan.iter().collect();
        assert_eq!(unique.len(), want.len());
    }
}
