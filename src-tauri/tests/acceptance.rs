//! The acceptance flow, end to end, against the real Windows registry.
//!
//! Walks the sequence the product is judged by: start untouched, enable a
//! customization, confirm Windows actually changed, switch profiles, restore,
//! and confirm nothing is left behind.
//!
//! Everything is driven through the same engine the UI uses — the module
//! registry, the switch engine and the restoration engine — so this is not a
//! parallel implementation that could pass while the product is broken.
//!
//! # Why a scratch key
//!
//! The system-changing modules write real settings. Rather than flip the
//! tester's actual taskbar, these tests drive the journal and engines
//! directly against `HKCU\Software\OVERLAY\__acceptance__`, which is
//! OVERLAY's own subtree and is removed afterwards. The Win32 behaviour
//! being exercised — absent-versus-zero, verification, ownership, conflict
//! detection — is identical either way.
//!
//! Surface modules (widgets, dock, HUD) need a Tauri runtime and are
//! verified by hand on Windows; see docs/TESTING.md.

#![cfg(windows)]

use overlay_lib::journal::{Journal, RestoreStatus, SettingTarget, SettingValue};
use overlay_lib::modules::{ModuleCtx, Registry};
use overlay_lib::profiles::{switch::SwitchEngine, ProfileStore, ORIGINAL_WINDOWS};
use overlay_lib::restoration::RestorationEngine;
use overlay_lib::storage::{self, paths::Paths};
use overlay_lib::sys::registry::{self, RegTarget, RegValue};
use overlay_lib::sys::Environment;
use parking_lot::Mutex;
use std::sync::Arc;

const SCRATCH: &str = r"Software\OVERLAY\__acceptance__";

struct Harness {
    journal: Journal,
    profiles: ProfileStore,
    switcher: SwitchEngine,
    restoration: RestorationEngine,
    registry: Registry,
    ctx: ModuleCtx,
    root: std::path::PathBuf,
    scratch: Vec<RegTarget>,
}

impl Harness {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("overlay-accept-{name}-{}", std::process::id()));
        let paths = Paths::under(root.clone()).expect("paths");
        let conn = Arc::new(Mutex::new(storage::open(&paths.db).expect("db")));

        let journal =
            Journal::new(conn.clone()).with_snapshot(paths.original_snapshot.clone());
        let profiles = ProfileStore::new(conn.clone());
        let registry = Registry::new();
        profiles.ensure_builtins(&registry).expect("builtins");

        let switcher = SwitchEngine::new(conn.clone(), profiles.clone(), journal.clone());
        let restoration =
            RestorationEngine::new(conn, journal.clone(), profiles.clone(), paths);

        let env = Arc::new(Environment {
            windows_version: "test".into(),
            build: 26100,
            is_windows_11: true,
            shell_running: true,
            taskbar_accent_supported: true,
            monitors: overlay_lib::sys::shell::monitors().unwrap_or_default(),
            elevated: false,
        });

        Self {
            ctx: ModuleCtx::headless(journal.clone(), env),
            journal,
            profiles,
            switcher,
            restoration,
            registry,
            root,
            scratch: Vec::new(),
        }
    }

    /// A scratch registry target that is cleaned up when the harness drops.
    fn target(&mut self, name: &str) -> SettingTarget {
        let reg = RegTarget::hkcu(SCRATCH, format!("{name}_{}", std::process::id()));
        self.scratch.push(reg.clone());
        SettingTarget::Registry(reg)
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        for reg in &self.scratch {
            let _ = registry::write(reg, &RegValue::Absent);
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// ---------------------------------------------------------------------------

#[test]
fn a_fresh_install_has_changed_nothing() {
    // Criterion 3: "My existing Windows desktop remains unchanged."
    let h = Harness::new("fresh");

    assert!(
        h.journal.owned().unwrap().is_empty(),
        "a new install must own no settings"
    );
    assert_eq!(h.profiles.active_id().unwrap(), None);

    let original = h.profiles.get(ORIGINAL_WINDOWS).unwrap();
    assert!(
        original.is_bare(),
        "Original Windows must enable nothing at all"
    );

    // Every shipped profile must be applicable, or the product is broken
    // out of the box.
    for p in h.profiles.list().unwrap() {
        assert!(
            h.registry.plan(&p.enabled_ids()).is_ok(),
            "shipped profile `{}` cannot be planned",
            p.id
        );
    }
}

#[test]
fn enabling_a_customization_changes_windows_and_records_the_original() {
    // Criteria 4–5, at the layer where "it actually happened" is checkable.
    let mut h = Harness::new("enable");
    let target = h.target("alignment");

    assert_eq!(
        target.read().unwrap(),
        SettingValue::absent(),
        "scratch target should start unset"
    );

    h.journal.apply("taskbar", &target, &SettingValue::dword(1)).unwrap();

    // Windows really changed.
    assert_eq!(target.read().unwrap(), SettingValue::dword(1));

    // And the pre-OVERLAY value is recorded as *absent*, not as zero.
    assert_eq!(
        h.journal.original_of(&target).unwrap().unwrap(),
        SettingValue::absent()
    );
    assert_eq!(h.journal.owned().unwrap().len(), 1);
}

#[test]
fn restoring_returns_windows_exactly_to_its_original_state() {
    // Criteria 15–16, including the absent-vs-zero distinction that makes
    // "exactly" mean something.
    let mut h = Harness::new("restore");
    let unset = h.target("was_unset");
    let preset = h.target("was_set");

    let SettingTarget::Registry(preset_reg) = &preset else {
        unreachable!()
    };
    registry::write(preset_reg, &RegValue::Dword(7)).unwrap();

    h.journal.apply("taskbar", &unset, &SettingValue::dword(1)).unwrap();
    h.journal.apply("taskbar", &preset, &SettingValue::dword(99)).unwrap();

    let report = h
        .restoration
        .restore_all(&h.registry, &h.ctx)
        .expect("restore");

    assert!(report.verified, "restore must verify: {report:?}");
    assert_eq!(report.restored(), 2);
    assert!(h.journal.owned().unwrap().is_empty());

    // The value that was never set is gone again — not set to 0.
    assert_eq!(
        unset.read().unwrap(),
        SettingValue::absent(),
        "an originally-unset value must be deleted, not zeroed"
    );
    // The value that existed is back to exactly what it was.
    assert_eq!(preset.read().unwrap(), SettingValue::dword(7));
}

#[test]
fn restore_leaves_settings_overlay_never_touched_alone() {
    // Criterion 16's other half, and the promise the brief repeats most.
    let mut h = Harness::new("untouched");
    let ours = h.target("ours");
    let theirs = h.target("theirs");

    let SettingTarget::Registry(theirs_reg) = &theirs else {
        unreachable!()
    };
    registry::write(theirs_reg, &RegValue::Dword(1234)).unwrap();

    h.journal.apply("taskbar", &ours, &SettingValue::dword(1)).unwrap();
    h.restoration.restore_all(&h.registry, &h.ctx).unwrap();

    assert_eq!(
        registry::read(theirs_reg).unwrap(),
        RegValue::Dword(1234),
        "a setting OVERLAY never changed must be untouched by restore"
    );
}

#[test]
fn a_setting_the_user_changed_afterwards_survives_a_restore() {
    // OVERLAY must not take back a choice the user made later.
    let mut h = Harness::new("conflict");
    let target = h.target("user_edited");
    let SettingTarget::Registry(reg) = &target else {
        unreachable!()
    };

    h.journal.apply("taskbar", &target, &SettingValue::dword(1)).unwrap();
    registry::write(reg, &RegValue::Dword(55)).unwrap(); // the user's own edit

    let report = h.restoration.restore_all(&h.registry, &h.ctx).unwrap();

    assert_eq!(report.conflicts(), 1);
    assert_eq!(
        registry::read(reg).unwrap(),
        RegValue::Dword(55),
        "the user's later value must stand"
    );
    assert!(report
        .settings
        .iter()
        .any(|s| s.status == RestoreStatus::ConflictSkipped
            && s.detail.contains("outside OVERLAY")));
}

#[test]
fn switching_profiles_applies_the_difference_and_commits() {
    // Criteria 11–14 for the system-changing part of a switch.
    let h = Harness::new("switch");

    let report = h
        .switcher
        .switch_to(ORIGINAL_WINDOWS, &h.registry, &h.ctx)
        .expect("switch");
    assert!(report.succeeded);
    assert!(!report.rolled_back);
    assert_eq!(
        h.profiles.active_id().unwrap().as_deref(),
        Some(ORIGINAL_WINDOWS)
    );
    assert_eq!(
        h.switcher.state().unwrap(),
        overlay_lib::profiles::switch::SwitchState::Idle,
        "a completed switch must leave no in-flight marker"
    );
}

#[test]
fn an_interrupted_change_is_reconciled_and_still_restorable() {
    // Criterion 20: recovery works if an operation is interrupted.
    let mut h = Harness::new("interrupted");
    let target = h.target("interrupted");
    let SettingTarget::Registry(reg) = &target else {
        unreachable!()
    };

    // Reproduce a process killed between journalling intent and settling:
    // the row says `pending`, and the write did land.
    h.journal.capture_original("taskbar", &target).unwrap();
    h.journal
        .begin_row_for_test("taskbar", &target, &SettingValue::absent(), &SettingValue::dword(9))
        .unwrap();
    registry::write(reg, &RegValue::Dword(9)).unwrap();

    assert_eq!(h.journal.unsettled().unwrap().len(), 1);
    h.journal.reconcile_unsettled().unwrap();
    assert!(h.journal.unsettled().unwrap().is_empty());

    // Having landed, it is now owned — and therefore undoable.
    assert_eq!(h.journal.owned().unwrap().len(), 1);
    let report = h.restoration.restore_all(&h.registry, &h.ctx).unwrap();
    assert!(report.verified);
    assert_eq!(registry::read(reg).unwrap(), RegValue::Absent);
}

#[test]
fn the_original_state_snapshot_is_written_as_readable_json() {
    // The fallback that survives the database being lost.
    let mut h = Harness::new("snapshot");
    let target = h.target("snapshotted");
    h.journal.apply("taskbar", &target, &SettingValue::dword(1)).unwrap();

    let file = h.root.join("original-windows.json");
    assert!(file.is_file(), "the snapshot file must be written");

    let text = std::fs::read_to_string(&file).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let originals = json["originals"].as_array().unwrap();

    assert_eq!(originals.len(), 1);
    assert_eq!(
        originals[0]["original_value"], "(not set)",
        "absence must survive into the human-readable record"
    );
    assert_eq!(originals[0]["module"], "taskbar");
}

#[test]
fn a_profile_exported_and_reimported_keeps_its_configuration() {
    // Criteria 11–12 for portability, including the validation gate.
    let h = Harness::new("export");

    let json = serde_json::to_string(&h.profiles.export("developer").unwrap()).unwrap();
    let (imported, warnings) = h.profiles.import(&json, &h.registry).unwrap();

    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    assert_eq!(
        imported.enabled_ids(),
        h.profiles.get("developer").unwrap().enabled_ids()
    );
    assert!(!imported.builtin, "an imported profile must be editable");
    assert!(h.registry.plan(&imported.enabled_ids()).is_ok());
}

#[test]
fn restoring_twice_is_harmless() {
    // The tray, the UI and the CLI can all trigger a restore; doing it
    // again must not error or write anything.
    let mut h = Harness::new("twice");
    let target = h.target("twice");
    h.journal.apply("taskbar", &target, &SettingValue::dword(1)).unwrap();

    let first = h.restoration.restore_all(&h.registry, &h.ctx).unwrap();
    assert_eq!(first.restored(), 1);

    let second = h.restoration.restore_all(&h.registry, &h.ctx).unwrap();
    assert!(second.verified);
    assert_eq!(second.settings.len(), 0, "nothing left to restore");
    assert_eq!(target.read().unwrap(), SettingValue::absent());
}
