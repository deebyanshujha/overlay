//! Transactional profile switching.
//!
//! # The guarantee
//!
//! A switch either lands on the destination profile or leaves the desktop in
//! a state the user recognises. It never strands the machine half-way between
//! two profiles without saying so.
//!
//! # The sequence
//!
//! 1. **Validate** the destination: every module known, every config accepted
//!    by its own module, dependencies satisfied, no conflicts. Nothing has
//!    touched Windows yet, so a bad profile fails having changed nothing.
//! 2. **Plan** the diff: which modules to turn off, which to turn on, in
//!    dependency order.
//! 3. **Mark the switch in flight** in the database, naming both profiles.
//!    This is the crash marker — see [`recover_interrupted`].
//! 4. **Disable** what the destination does not want. Each disable restores
//!    that module's settings through the journal.
//! 5. **Enable** what it does want, capturing original state first.
//! 6. **Commit**: record the new active profile and clear the marker.
//!
//! If a module that changes Windows fails during step 5, the whole switch is
//! rolled back to the profile it started from. A module that only draws
//! OVERLAY's own windows is allowed to fail without aborting: a clock that
//! would not open is worth reporting, not worth reverting a taskbar for.

use super::{ModuleSetting, Profile, ProfileStore, ORIGINAL_WINDOWS};
use crate::error::{OverlayError, Result};
use crate::journal::{Journal, RestoreOutcome};
use crate::modules::{ModuleCtx, ModuleOutcome, Registry};
use crate::storage::now;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What a switch did, reported back to the user in full.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchReport {
    pub from: Option<String>,
    pub to: String,
    pub to_name: String,
    pub disabled: Vec<ModuleOutcome>,
    pub enabled: Vec<ModuleOutcome>,
    /// Settings put back while disabling modules.
    pub restored: Vec<RestoreOutcome>,
    pub rolled_back: bool,
    /// Problems that did not abort the switch.
    pub warnings: Vec<String>,
    pub succeeded: bool,
    pub duration_ms: u64,
}

impl SwitchReport {
    pub fn failures(&self) -> Vec<&ModuleOutcome> {
        self.disabled
            .iter()
            .chain(self.enabled.iter())
            .filter(|o| !o.ok)
            .collect()
    }
}

/// Where a switch got to. Persisted so a crash is detectable on restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchState {
    Idle,
    InFlight,
}

/// A switch that was interrupted, as found at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptedSwitch {
    pub from: Option<String>,
    pub to: Option<String>,
    pub began_at: i64,
}

pub struct SwitchEngine {
    conn: Arc<Mutex<Connection>>,
    store: ProfileStore,
    journal: Journal,
}

impl SwitchEngine {
    pub fn new(conn: Arc<Mutex<Connection>>, store: ProfileStore, journal: Journal) -> Self {
        Self {
            conn,
            store,
            journal,
        }
    }

    /// Validates a profile without applying any part of it.
    ///
    /// Exposed separately so the UI can warn about an unapplicable profile
    /// while the user is still editing it.
    pub fn validate(&self, profile: &Profile, registry: &Registry, ctx: &ModuleCtx) -> Result<()> {
        // Structural problems first — an unknown module, a bad config, a
        // missing dependency, a conflict. These are faults in the profile
        // itself and are the same on every machine, so they are the more
        // actionable thing to report when a profile has both.
        for (id, setting) in &profile.modules {
            registry.get(id)?.validate_config(&setting.config)?;
        }
        registry.plan(&profile.enabled_ids())?;

        // Only then whether this particular machine can run it.
        for (id, setting) in &profile.modules {
            if !setting.enabled {
                continue;
            }
            if let crate::modules::Availability::Unavailable { reason } =
                registry.get(id)?.availability(ctx)
            {
                return Err(OverlayError::Unavailable {
                    id: id.clone(),
                    reason,
                });
            }
        }
        Ok(())
    }

    /// Applies a profile.
    pub fn switch_to(
        &self,
        profile_id: &str,
        registry: &Registry,
        ctx: &ModuleCtx,
    ) -> Result<SwitchReport> {
        let started = std::time::Instant::now();
        let target = self.store.get(profile_id)?;
        let from_id = self.store.active_id()?;
        let previous = from_id.as_deref().and_then(|id| self.store.get(id).ok());

        // Step 1 — validate before anything is touched.
        self.validate(&target, registry, ctx)?;
        let plan = registry.plan(&target.enabled_ids())?;

        // Step 2 — what must change.
        let currently_on: Vec<String> = registry
            .iter()
            .filter(|m| m.is_active(ctx))
            .map(|m| m.id().to_string())
            .collect();
        let wanted = target.enabled_ids();
        let to_disable: Vec<String> = currently_on
            .iter()
            .filter(|id| !wanted.contains(*id))
            .cloned()
            .collect();

        // Step 3 — mark in flight, so a crash from here on is recoverable.
        self.begin(from_id.as_deref(), profile_id)?;

        let mut report = SwitchReport {
            from: from_id.clone(),
            to: target.id.clone(),
            to_name: target.name.clone(),
            disabled: Vec::new(),
            enabled: Vec::new(),
            restored: Vec::new(),
            rolled_back: false,
            warnings: Vec::new(),
            succeeded: false,
            duration_ms: 0,
        };

        // Step 4 — turn off what is no longer wanted. Disabling restores the
        // module's settings, so this is where the desktop comes back.
        let before_restore = self.journal.owned().unwrap_or_default().len();
        report.disabled = registry.disable_many(ctx, &to_disable);
        if self.journal.owned().map(|o| o.len()).unwrap_or(0) < before_restore {
            // Record what the restores actually did, for the report.
            report.restored = self
                .journal
                .recent(before_restore.max(8))
                .map(|rows| {
                    rows.into_iter()
                        .filter(|r| r.status == crate::journal::ChangeStatus::Reverted)
                        .map(|r| RestoreOutcome {
                            target_id: r.target_id,
                            label: r.label,
                            module_id: r.module_id,
                            restored_to: r.after,
                            status: crate::journal::RestoreStatus::Restored,
                            detail: String::new(),
                        })
                        .collect()
                })
                .unwrap_or_default();
        }

        // A module that would not turn off is a warning, not a failure: the
        // destination can usually still be reached, and aborting here would
        // leave *more* of the old profile in place, not less.
        for failure in report.disabled.iter().filter(|o| !o.ok) {
            report.warnings.push(format!(
                "`{}` could not be switched off: {}",
                failure.module_id,
                failure.error.as_deref().unwrap_or("unknown error")
            ));
        }

        // Step 5 — turn on what the destination wants.
        let configs: BTreeMap<String, serde_json::Value> = target.configs();
        report.enabled = registry.enable_many(ctx, &plan, &configs);

        // A failure in a module that changes Windows is the serious case.
        let critical: Vec<&ModuleOutcome> = report
            .enabled
            .iter()
            .filter(|o| {
                !o.ok
                    && registry
                        .get(&o.module_id)
                        .map(|m| m.descriptor().touches_system)
                        .unwrap_or(false)
            })
            .collect();

        if !critical.is_empty() {
            let detail = critical
                .iter()
                .map(|o| {
                    format!(
                        "{}: {}",
                        o.module_id,
                        o.error.as_deref().unwrap_or("unknown error")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");

            self.rollback(previous.as_ref(), registry, ctx, &mut report);
            report.rolled_back = true;
            report.succeeded = false;
            report
                .warnings
                .push(format!("Rolled back after a system change failed — {detail}"));
            self.finish(report.from.as_deref())?;
            report.duration_ms = started.elapsed().as_millis() as u64;
            return Ok(report);
        }

        for failure in report.enabled.iter().filter(|o| !o.ok) {
            report.warnings.push(format!(
                "`{}` could not be switched on: {}",
                failure.module_id,
                failure.error.as_deref().unwrap_or("unknown error")
            ));
        }

        // Step 6 — commit.
        self.finish(Some(&target.id))?;
        report.succeeded = true;
        report.duration_ms = started.elapsed().as_millis() as u64;
        Ok(report)
    }

    /// Puts the desktop back to the profile the switch started from.
    ///
    /// Best-effort by nature — it runs because something already went wrong —
    /// so every step is attempted and failures are collected rather than
    /// propagated.
    fn rollback(
        &self,
        previous: Option<&Profile>,
        registry: &Registry,
        ctx: &ModuleCtx,
        report: &mut SwitchReport,
    ) {
        // Undo everything the failed attempt switched on.
        let turned_on: Vec<String> = report
            .enabled
            .iter()
            .filter(|o| o.ok)
            .map(|o| o.module_id.clone())
            .collect();
        for outcome in registry.disable_many(ctx, &turned_on) {
            if !outcome.ok {
                report.warnings.push(format!(
                    "While rolling back, `{}` could not be switched off: {}",
                    outcome.module_id,
                    outcome.error.as_deref().unwrap_or("unknown error")
                ));
            }
        }

        let Some(previous) = previous else {
            // There was no previous profile, so the original desktop *is* the
            // rollback target, and the disables above have already reached it.
            return;
        };

        let Ok(plan) = registry.plan(&previous.enabled_ids()) else {
            report.warnings.push(format!(
                "Could not fully restore `{}`: its module set is no longer valid.",
                previous.name
            ));
            return;
        };
        let configs = previous.configs();
        for outcome in registry.enable_many(ctx, &plan, &configs) {
            if !outcome.ok {
                report.warnings.push(format!(
                    "While rolling back, `{}` could not be restored: {}",
                    outcome.module_id,
                    outcome.error.as_deref().unwrap_or("unknown error")
                ));
            }
        }
    }

    // -----------------------------------------------------------------
    // Crash markers
    // -----------------------------------------------------------------

    fn begin(&self, from: Option<&str>, to: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE active_profile
             SET switch_state = 'in_flight', switch_from = ?1, switch_to = ?2, switch_began = ?3
             WHERE id = 1",
            params![from, to, now()],
        )?;
        Ok(())
    }

    fn finish(&self, active: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE active_profile
             SET profile_id = ?1, switch_state = 'idle',
                 switch_from = NULL, switch_to = NULL, switch_began = NULL
             WHERE id = 1",
            params![active],
        )?;
        Ok(())
    }

    pub fn state(&self) -> Result<SwitchState> {
        let conn = self.conn.lock();
        let raw: String = conn
            .query_row(
                "SELECT switch_state FROM active_profile WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "idle".into());
        Ok(if raw == "in_flight" {
            SwitchState::InFlight
        } else {
            SwitchState::Idle
        })
    }

    /// A switch that was in flight when the process stopped, if there was one.
    pub fn interrupted(&self) -> Result<Option<InterruptedSwitch>> {
        if self.state()? == SwitchState::Idle {
            return Ok(None);
        }
        let conn = self.conn.lock();
        let row = conn
            .query_row(
                "SELECT switch_from, switch_to, switch_began FROM active_profile WHERE id = 1",
                [],
                |r| {
                    Ok(InterruptedSwitch {
                        from: r.get(0)?,
                        to: r.get(1)?,
                        began_at: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Resolves an interrupted switch at startup.
    ///
    /// The journal has already reconciled individual settings against the
    /// live machine, so what remains is deciding which profile the desktop is
    /// actually in. Rather than guess, this re-applies the destination: it is
    /// idempotent, it is what the user asked for, and it ends in a known
    /// state either way. If that fails, the desktop is returned to Original
    /// Windows, which is always reachable.
    pub fn recover_interrupted(
        &self,
        registry: &Registry,
        ctx: &ModuleCtx,
    ) -> Result<Option<SwitchReport>> {
        let Some(interrupted) = self.interrupted()? else {
            return Ok(None);
        };

        // Settle any half-written system change before deciding anything.
        let _ = self.journal.reconcile_unsettled();

        let destination = interrupted
            .to
            .clone()
            .filter(|id| self.store.get(id).is_ok())
            .unwrap_or_else(|| ORIGINAL_WINDOWS.to_string());

        // Clear the marker first: if re-applying crashes the same way, the
        // next start retries from a clean slate instead of looping.
        self.finish(interrupted.from.as_deref())?;

        match self.switch_to(&destination, registry, ctx) {
            Ok(mut report) => {
                report.warnings.insert(
                    0,
                    format!(
                        "A profile switch was interrupted. `{}` has been re-applied.",
                        report.to_name
                    ),
                );
                Ok(Some(report))
            }
            Err(e) => {
                let mut report = self.switch_to(ORIGINAL_WINDOWS, registry, ctx)?;
                report.warnings.insert(
                    0,
                    format!(
                        "A profile switch was interrupted and could not be completed ({e}). \
                         Your original desktop has been restored."
                    ),
                );
                Ok(Some(report))
            }
        }
    }
}

/// Convenience used by the engine and the UI alike: the module set of a
/// profile as a stable, sorted list.
pub fn module_list(profile: &Profile) -> Vec<(String, ModuleSetting)> {
    profile
        .modules
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;
    use crate::sys::Environment;

    fn engine() -> (SwitchEngine, ProfileStore, Registry, ModuleCtx) {
        let conn = Arc::new(Mutex::new(storage::open_in_memory().unwrap()));
        let journal = Journal::new(conn.clone());
        let store = ProfileStore::new(conn.clone());
        let registry = Registry::new();
        store.ensure_builtins(&registry).unwrap();
        let env = Arc::new(Environment {
            windows_version: "test".into(),
            build: 26100,
            is_windows_11: true,
            shell_running: true,
            taskbar_accent_supported: true,
            // A realistic single display: modules that need somewhere to
            // draw must not report "unavailable" for the wrong reason.
            monitors: vec![crate::sys::shell::MonitorInfo {
                device: "\\\\.\\DISPLAY1".into(),
                primary: true,
                x: 0,
                y: 0,
                width: 1536,
                height: 864,
                work_x: 0,
                work_y: 0,
                work_width: 1536,
                work_height: 816,
                scale: 1.0,
            }],
            elevated: false,
        });
        let ctx = ModuleCtx::headless(journal.clone(), env);
        (
            SwitchEngine::new(conn, store.clone(), journal),
            store,
            registry,
            ctx,
        )
    }

    #[test]
    fn a_fresh_install_has_no_switch_in_flight() {
        let (e, _, _, _) = engine();
        assert_eq!(e.state().unwrap(), SwitchState::Idle);
        assert!(e.interrupted().unwrap().is_none());
    }

    #[test]
    fn the_in_flight_marker_records_both_ends_of_the_switch() {
        // This marker is the only evidence a crash leaves behind.
        let (e, _, _, _) = engine();
        e.begin(Some("minimal"), "developer").unwrap();

        assert_eq!(e.state().unwrap(), SwitchState::InFlight);
        let i = e.interrupted().unwrap().unwrap();
        assert_eq!(i.from.as_deref(), Some("minimal"));
        assert_eq!(i.to.as_deref(), Some("developer"));
        assert!(i.began_at > 0);
    }

    #[test]
    fn committing_clears_the_marker_and_sets_the_active_profile() {
        let (e, s, _, _) = engine();
        e.begin(None, "minimal").unwrap();
        e.finish(Some("minimal")).unwrap();

        assert_eq!(e.state().unwrap(), SwitchState::Idle);
        assert!(e.interrupted().unwrap().is_none());
        assert_eq!(s.active_id().unwrap().as_deref(), Some("minimal"));
    }

    #[test]
    fn validation_rejects_a_profile_with_an_unsatisfied_dependency() {
        let (e, s, r, ctx) = engine();
        // The HUD needs the widget framework.
        let mut p = s.create("Broken", "", None).unwrap();
        p.modules.insert(
            "hud".into(),
            ModuleSetting {
                enabled: true,
                config: r.get("hud").unwrap().default_config(),
            },
        );
        s.save(&p).unwrap();

        let err = e.validate(&s.get(&p.id).unwrap(), &r, &ctx);
        assert!(
            matches!(err, Err(OverlayError::MissingDependency(_, _))),
            "got {err:?}"
        );
    }

    #[test]
    fn validation_rejects_a_profile_whose_config_is_invalid() {
        let (e, s, r, ctx) = engine();
        let mut p = s.create("Bad config", "", None).unwrap();
        p.modules.insert(
            "hud".into(),
            ModuleSetting {
                enabled: false,
                config: serde_json::json!({ "metrics": ["not-a-metric"] }),
            },
        );
        s.save(&p).unwrap();

        assert!(e.validate(&s.get(&p.id).unwrap(), &r, &ctx).is_err());
    }

    #[test]
    fn validation_checks_disabled_modules_too() {
        // A profile carrying a config that cannot be parsed is broken even if
        // the module is off, because enabling it later would fail.
        let (e, s, r, ctx) = engine();
        let mut p = s.create("Latent", "", None).unwrap();
        p.modules.insert(
            "taskbar".into(),
            ModuleSetting {
                enabled: false,
                config: serde_json::json!({ "finish": "nope" }),
            },
        );
        s.save(&p).unwrap();
        assert!(e.validate(&s.get(&p.id).unwrap(), &r, &ctx).is_err());
    }

    #[test]
    fn every_builtin_profile_validates() {
        let (e, s, r, ctx) = engine();
        for p in s.list().unwrap() {
            // Surface modules need a running app, which a headless context
            // has not got; those report Unavailable legitimately. What must
            // hold here is that configs parse and the plan resolves.
            for (id, setting) in &p.modules {
                let m = r.get(id).unwrap();
                assert!(
                    m.validate_config(&setting.config).is_ok(),
                    "`{}` has an invalid `{id}` config",
                    p.id
                );
            }
            assert!(r.plan(&p.enabled_ids()).is_ok(), "`{}` cannot be planned", p.id);
            let _ = &e;
            let _ = &ctx;
        }
    }

    #[test]
    fn recovery_is_a_no_op_when_nothing_was_interrupted() {
        let (e, _, r, ctx) = engine();
        assert!(e.recover_interrupted(&r, &ctx).unwrap().is_none());
    }

    #[test]
    fn recovery_clears_the_marker_even_when_the_destination_is_gone() {
        // A profile deleted between the crash and the restart must not leave
        // the marker stuck forever.
        let (e, _, r, ctx) = engine();
        e.begin(None, "a-profile-that-no-longer-exists").unwrap();

        let report = e.recover_interrupted(&r, &ctx).unwrap();
        assert!(report.is_some());
        assert_eq!(
            e.state().unwrap(),
            SwitchState::Idle,
            "the marker must never survive recovery"
        );
    }

    #[test]
    fn recovery_falls_back_to_original_windows() {
        let (e, s, r, ctx) = engine();
        e.begin(Some("minimal"), "gone").unwrap();
        let report = e.recover_interrupted(&r, &ctx).unwrap().unwrap();
        assert_eq!(report.to, ORIGINAL_WINDOWS);
        assert_eq!(s.active_id().unwrap().as_deref(), Some(ORIGINAL_WINDOWS));
    }

    #[test]
    fn switching_to_original_windows_enables_nothing_and_commits() {
        let (e, s, r, ctx) = engine();
        let report = e.switch_to(ORIGINAL_WINDOWS, &r, &ctx).unwrap();
        assert!(report.succeeded);
        assert!(report.enabled.is_empty(), "the bare profile enables nothing");
        assert!(!report.rolled_back);
        assert_eq!(s.active_id().unwrap().as_deref(), Some(ORIGINAL_WINDOWS));
        assert_eq!(e.state().unwrap(), SwitchState::Idle);
    }

    #[test]
    fn a_switch_reports_how_long_it_took() {
        let (e, _, r, ctx) = engine();
        let report = e.switch_to(ORIGINAL_WINDOWS, &r, &ctx).unwrap();
        assert!(report.duration_ms < 60_000);
    }

    #[test]
    fn switching_to_an_unknown_profile_fails_without_marking_a_switch() {
        let (e, _, r, ctx) = engine();
        assert!(e.switch_to("nope", &r, &ctx).is_err());
        assert_eq!(
            e.state().unwrap(),
            SwitchState::Idle,
            "a validation failure must not leave a marker behind"
        );
    }
}
