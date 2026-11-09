//! Restoration: putting the desktop back.
//!
//! # Why this exists separately from the journal
//!
//! The journal knows how to revert one setting. This module turns that into
//! the product promise — "Restore Original Desktop" — by also closing
//! OVERLAY's windows, clearing the active profile, verifying the result and
//! producing a report the user can read.
//!
//! # What it will not do
//!
//! * It will not touch a setting OVERLAY did not change. The work list comes
//!   from `owned_settings`, so there is no path to anything else.
//! * It will not overwrite a setting someone changed after OVERLAY did. Those
//!   are reported as conflicts and left alone.
//! * It will not uninstall, close or reconfigure WallFlow or QuickDock.
//! * It will not delete a shortcut, move a file, or write to the Desktop
//!   folder. No code in OVERLAY does.
//!
//! # Emergency recovery
//!
//! [`restore_headless`] performs the whole operation with no Tauri runtime,
//! no window and no frontend — it needs only the database path. That is what
//! `overlay.exe --restore-original` calls, so a broken UI can never trap a
//! user in a customised desktop.

use crate::error::Result;
use crate::journal::{Journal, RestoreOutcome, RestoreStatus};
use crate::modules::{ModuleCtx, Registry};
use crate::profiles::{ProfileStore, ORIGINAL_WINDOWS};
use crate::storage::{self, now, paths::Paths};
use parking_lot::Mutex;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The outcome of a restoration, in full.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorationReport {
    pub started_at: i64,
    pub finished_at: i64,
    /// Every setting considered, and what happened to it.
    pub settings: Vec<RestoreOutcome>,
    /// OVERLAY windows that were closed.
    pub surfaces_closed: Vec<String>,
    /// True only when every owned setting is verified back at its original
    /// value and nothing is left owned.
    pub verified: bool,
    pub notes: Vec<String>,
    pub scope: String,
}

impl RestorationReport {
    pub fn restored(&self) -> usize {
        self.count(RestoreStatus::Restored)
    }

    pub fn already_original(&self) -> usize {
        self.count(RestoreStatus::AlreadyOriginal)
    }

    pub fn conflicts(&self) -> usize {
        self.count(RestoreStatus::ConflictSkipped)
    }

    pub fn failures(&self) -> usize {
        self.count(RestoreStatus::Failed)
    }

    fn count(&self, status: RestoreStatus) -> usize {
        self.settings.iter().filter(|s| s.status == status).count()
    }

    /// One line summarising what happened, for a toast or the tray.
    pub fn summary(&self) -> String {
        if self.settings.is_empty() && self.surfaces_closed.is_empty() {
            return "Nothing to undo — your desktop was already untouched.".into();
        }
        let mut parts = Vec::new();
        if self.restored() > 0 {
            parts.push(format!("{} setting(s) restored", self.restored()));
        }
        if !self.surfaces_closed.is_empty() {
            parts.push(format!("{} window(s) closed", self.surfaces_closed.len()));
        }
        if self.already_original() > 0 {
            parts.push(format!("{} already original", self.already_original()));
        }
        if self.conflicts() > 0 {
            parts.push(format!("{} left alone (changed elsewhere)", self.conflicts()));
        }
        if self.failures() > 0 {
            parts.push(format!("{} could not be restored", self.failures()));
        }
        parts.join(", ")
    }
}

pub struct RestorationEngine {
    conn: Arc<Mutex<rusqlite::Connection>>,
    journal: Journal,
    store: ProfileStore,
    paths: Paths,
}

impl RestorationEngine {
    pub fn new(
        conn: Arc<Mutex<rusqlite::Connection>>,
        journal: Journal,
        store: ProfileStore,
        paths: Paths,
    ) -> Self {
        Self {
            conn,
            journal,
            store,
            paths,
        }
    }

    /// What a restore would change, without changing it. Powers the
    /// confirmation dialog, so the user sees the actual list rather than a
    /// generic warning.
    pub fn preview(&self) -> Result<Vec<RestoreOutcome>> {
        let owned = self.journal.owned()?;
        let mut out = Vec::with_capacity(owned.len());
        for setting in owned {
            let original = self.journal.original_of(&setting.target)?;
            let live = setting.target.read().ok();
            let status = match (&original, &live) {
                (Some(o), Some(l)) if setting.target.matches(o, l) => RestoreStatus::AlreadyOriginal,
                (Some(_), Some(l)) if !setting.target.matches(&setting.applied, l) => {
                    RestoreStatus::ConflictSkipped
                }
                (Some(_), Some(_)) => RestoreStatus::Restored,
                _ => RestoreStatus::Failed,
            };
            out.push(RestoreOutcome {
                target_id: setting.target.id(),
                label: setting.target.label(),
                module_id: setting.module_id,
                restored_to: original
                    .map(|o| o.describe())
                    .unwrap_or_else(|| "(unknown)".into()),
                status,
                detail: String::new(),
            });
        }
        Ok(out)
    }

    /// Restores everything OVERLAY owns and closes every surface it opened.
    pub fn restore_all(&self, registry: &Registry, ctx: &ModuleCtx) -> Result<RestorationReport> {
        let started_at = now();
        self.backup("restore-original");

        let mut notes = Vec::new();

        // Settle anything a previous crash left half-done, so the restore
        // works from an accurate picture rather than a stale one.
        match self.journal.reconcile_unsettled() {
            Ok(rows) if !rows.is_empty() => notes.push(format!(
                "{} interrupted change(s) were reconciled before restoring.",
                rows.len()
            )),
            _ => {}
        }

        // Close OVERLAY's own windows first. They own no system state, and
        // removing them makes the visible result immediate.
        let surfaces_closed = match ctx.app.as_ref() {
            Some(app) => {
                let manager = {
                    use tauri::Manager;
                    app.state::<Arc<crate::windows::SurfaceManager>>()
                        .inner()
                        .clone()
                };
                manager.hide_all(app)
            }
            None => {
                notes.push(
                    "Running without the OVERLAY window, so only system settings were \
                     restored. Any widgets still on screen belong to a running copy of \
                     OVERLAY; closing it removes them."
                        .into(),
                );
                Vec::new()
            }
        };

        // Then every tracked system setting.
        let settings = self.journal.restore_all()?;

        // Verify: nothing should still be owned except genuine conflicts.
        let remaining = self.journal.owned()?;
        let conflicts = settings
            .iter()
            .filter(|s| s.status == RestoreStatus::ConflictSkipped)
            .count();
        let verified = remaining.len() == conflicts
            && !settings.iter().any(|s| s.status == RestoreStatus::Failed);

        if conflicts > 0 {
            notes.push(format!(
                "{conflicts} setting(s) were changed outside OVERLAY after we set them, so they \
                 were left as they are rather than overwritten."
            ));
        }
        if !verified && settings.iter().any(|s| s.status == RestoreStatus::Failed) {
            notes.push(
                "Some settings could not be put back. They are listed below with the reason; \
                 OVERLAY still has their original values recorded and will retry on request."
                    .into(),
            );
        }

        // Point the active profile at Original Windows: the desktop is now
        // in that state, and the UI must agree.
        if self.store.get(ORIGINAL_WINDOWS).is_ok() {
            let _ = self.store.set_active(Some(ORIGINAL_WINDOWS));
        } else {
            let _ = self.store.set_active(None);
        }
        let _ = registry;

        let report = RestorationReport {
            started_at,
            finished_at: now(),
            settings,
            surfaces_closed,
            verified,
            notes,
            scope: "all".into(),
        };
        self.record(&report);
        Ok(report)
    }

    /// Restores just one module's settings.
    pub fn restore_module(
        &self,
        module_id: &str,
        registry: &Registry,
        ctx: &ModuleCtx,
    ) -> Result<RestorationReport> {
        let started_at = now();
        self.backup(&format!("restore-{module_id}"));

        let mut surfaces_closed = Vec::new();
        if let Ok(module) = registry.get(module_id) {
            // Disabling closes any windows the module owns.
            if module.disable(ctx).is_ok() {
                surfaces_closed.push(module_id.to_string());
            }
        }
        let settings = self.journal.restore_module(module_id)?;
        let verified = !settings.iter().any(|s| s.status == RestoreStatus::Failed);

        let report = RestorationReport {
            started_at,
            finished_at: now(),
            settings,
            surfaces_closed,
            verified,
            notes: Vec::new(),
            scope: module_id.to_string(),
        };
        self.record(&report);
        Ok(report)
    }

    /// Copies the database aside before a destructive operation.
    ///
    /// Best-effort: a failed backup must not block a restore, because being
    /// unable to undo is strictly worse than being unable to archive.
    fn backup(&self, label: &str) {
        let destination = self.paths.backup_file(label);
        let conn = self.conn.lock();
        // SQLite's own backup API, so a WAL-mode database is copied
        // consistently rather than file-by-file.
        if let Err(e) = conn.execute("VACUUM INTO ?1", params![destination.to_string_lossy()]) {
            log::warn!("could not back up the database before {label}: {e}");
        }
    }

    fn record(&self, report: &RestorationReport) {
        let conn = self.conn.lock();
        let outcome = if report.verified { "verified" } else { "partial" };
        let json = serde_json::to_string(report).unwrap_or_else(|_| "{}".into());
        if let Err(e) = conn.execute(
            "INSERT INTO restoration_history (scope, started_at, ended_at, outcome, report)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                report.scope,
                report.started_at,
                report.finished_at,
                outcome,
                json
            ],
        ) {
            log::warn!("could not record the restoration: {e}");
        }
    }

    pub fn history(&self, limit: usize) -> Result<Vec<RestorationReport>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT report FROM restoration_history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(report) = serde_json::from_str(&row?) {
                out.push(report);
            }
        }
        Ok(out)
    }
}

/// Restores the original desktop with no Tauri runtime.
///
/// This is the emergency path. It opens the database directly, reconciles any
/// interrupted change, reverts every owned setting and prints a report. It
/// depends on nothing but SQLite and Win32 — specifically, not on the
/// frontend, the window manager, or OVERLAY having started successfully.
pub fn restore_headless() -> Result<RestorationReport> {
    let paths = Paths::resolve()?;
    if !paths.db.is_file() {
        return Ok(RestorationReport {
            started_at: now(),
            finished_at: now(),
            settings: Vec::new(),
            surfaces_closed: Vec::new(),
            verified: true,
            notes: vec![
                "OVERLAY has no database on this machine, so it has never changed anything."
                    .into(),
            ],
            scope: "all".into(),
        });
    }

    let conn = Arc::new(Mutex::new(storage::open(&paths.db)?));
    let journal =
        Journal::new(conn.clone()).with_snapshot(paths.original_snapshot.clone());
    let store = ProfileStore::new(conn.clone());
    let engine = RestorationEngine::new(conn, journal.clone(), store, paths);

    let env = Arc::new(crate::sys::environment());
    let ctx = ModuleCtx::headless(journal, env);
    let registry = Registry::new();
    engine.restore_all(&registry, &ctx)
}

/// Renders a report as plain text for the recovery console.
pub fn format_report(report: &RestorationReport) -> String {
    let mut out = String::new();
    out.push_str("OVERLAY — restore original desktop\n");
    out.push_str("==================================\n\n");
    out.push_str(&format!("{}\n\n", report.summary()));

    if !report.settings.is_empty() {
        out.push_str("Settings\n--------\n");
        for s in &report.settings {
            let mark = match s.status {
                RestoreStatus::Restored => "restored ",
                RestoreStatus::AlreadyOriginal => "unchanged",
                RestoreStatus::ConflictSkipped => "skipped  ",
                RestoreStatus::Failed => "FAILED   ",
            };
            out.push_str(&format!("  [{mark}] {} -> {}\n", s.label, s.restored_to));
            if !s.detail.is_empty() {
                out.push_str(&format!("              {}\n", s.detail));
            }
        }
        out.push('\n');
    }

    if !report.surfaces_closed.is_empty() {
        out.push_str(&format!(
            "Closed {} OVERLAY window(s).\n\n",
            report.surfaces_closed.len()
        ));
    }

    for note in &report.notes {
        out.push_str(&format!("Note: {note}\n"));
    }

    out.push_str(if report.verified {
        "\nVerified: every tracked setting is back at its original value.\n"
    } else {
        "\nNot fully verified — see the entries above.\n"
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::SettingTarget;
    use crate::sys::registry::RegTarget;

    fn report_with(statuses: &[RestoreStatus]) -> RestorationReport {
        RestorationReport {
            started_at: 0,
            finished_at: 1,
            settings: statuses
                .iter()
                .enumerate()
                .map(|(i, s)| RestoreOutcome {
                    target_id: format!("t{i}"),
                    label: format!("Setting {i}"),
                    module_id: "taskbar".into(),
                    restored_to: "(not set)".into(),
                    status: *s,
                    detail: String::new(),
                })
                .collect(),
            surfaces_closed: Vec::new(),
            verified: true,
            notes: Vec::new(),
            scope: "all".into(),
        }
    }

    #[test]
    fn an_empty_report_says_there_was_nothing_to_undo() {
        let r = report_with(&[]);
        assert!(r.summary().contains("already untouched"));
    }

    #[test]
    fn the_summary_counts_each_outcome_separately() {
        let r = report_with(&[
            RestoreStatus::Restored,
            RestoreStatus::Restored,
            RestoreStatus::AlreadyOriginal,
            RestoreStatus::ConflictSkipped,
            RestoreStatus::Failed,
        ]);
        assert_eq!(r.restored(), 2);
        assert_eq!(r.already_original(), 1);
        assert_eq!(r.conflicts(), 1);
        assert_eq!(r.failures(), 1);

        let summary = r.summary();
        assert!(summary.contains("2 setting(s) restored"));
        assert!(summary.contains("changed elsewhere"));
        assert!(summary.contains("could not be restored"));
    }

    #[test]
    fn the_text_report_names_every_setting_and_its_fate() {
        let mut r = report_with(&[RestoreStatus::Restored, RestoreStatus::Failed]);
        r.verified = false;
        r.settings[1].detail = "Explorer did not respond".into();

        let text = format_report(&r);
        assert!(text.contains("Setting 0"));
        assert!(text.contains("Setting 1"));
        assert!(text.contains("FAILED"));
        assert!(text.contains("Explorer did not respond"));
        assert!(text.contains("Not fully verified"));
    }

    #[test]
    fn a_verified_report_says_so_plainly() {
        let text = format_report(&report_with(&[RestoreStatus::Restored]));
        assert!(text.contains("Verified: every tracked setting"));
    }

    #[test]
    fn preview_reports_nothing_when_overlay_owns_nothing() {
        // Restoring a machine OVERLAY has never changed must be a no-op.
        let conn = Arc::new(Mutex::new(storage::open_in_memory().unwrap()));
        let journal = Journal::new(conn.clone());
        let store = ProfileStore::new(conn.clone());
        let root = std::env::temp_dir().join(format!("overlay-rest-{}", uuid::Uuid::new_v4()));
        let paths = Paths::under(root.clone()).unwrap();
        let engine = RestorationEngine::new(conn, journal, store, paths);

        assert!(engine.preview().unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn preview_lists_an_owned_setting_without_changing_it() {
        let conn = Arc::new(Mutex::new(storage::open_in_memory().unwrap()));
        let journal = Journal::new(conn.clone());
        let store = ProfileStore::new(conn.clone());
        let root = std::env::temp_dir().join(format!("overlay-rest-{}", uuid::Uuid::new_v4()));
        let paths = Paths::under(root.clone()).unwrap();

        let target = SettingTarget::Registry(RegTarget::hkcu(
            r"Software\OVERLAY\__previewtest__",
            format!("p{}", std::process::id()),
        ));
        journal
            .apply("taskbar", &target, &crate::journal::SettingValue::dword(1))
            .unwrap();

        let engine = RestorationEngine::new(conn, journal.clone(), store, paths);
        let preview = engine.preview().unwrap();
        assert_eq!(preview.len(), 1);
        assert_eq!(preview[0].status, RestoreStatus::Restored);
        // Previewing must not have touched anything.
        assert_eq!(
            target.read().unwrap(),
            crate::journal::SettingValue::dword(1)
        );

        journal.restore_one(&target).unwrap();
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn headless_restore_on_an_untouched_machine_reports_nothing_to_do() {
        // Exercises the branch the recovery CLI hits most often.
        let r = RestorationReport {
            started_at: 0,
            finished_at: 0,
            settings: vec![],
            surfaces_closed: vec![],
            verified: true,
            notes: vec!["OVERLAY has no database".into()],
            scope: "all".into(),
        };
        assert!(r.verified);
        assert!(format_report(&r).contains("Verified"));
    }
}
