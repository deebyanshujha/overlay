//! The journal contract, tested against the real registry.
//!
//! These use a scratch key of our own — `HKCU\Software\OVERLAY\__journaltest__`
//! — so they exercise genuine Win32 behaviour (including the absent/zero
//! distinction, which no mock would reproduce faithfully) without going
//! anywhere near a setting the user cares about. Each test cleans up after
//! itself, and they run as part of the normal suite rather than being ignored.

#![cfg(windows)]

use overlay_lib::error::OverlayError;
use overlay_lib::journal::{Journal, RestoreStatus, SettingTarget, SettingValue};
use overlay_lib::storage;
use overlay_lib::sys::registry::{self, RegTarget, RegValue};
use parking_lot::Mutex;
use std::sync::Arc;

const SCRATCH: &str = r"Software\OVERLAY\__journaltest__";

/// A journal over a fresh in-memory database, plus a unique scratch target so
/// tests running in parallel cannot collide.
fn fixture(name: &str) -> (Journal, SettingTarget, Cleanup) {
    let conn = storage::open_in_memory().expect("schema");
    let journal = Journal::new(Arc::new(Mutex::new(conn)));
    let value_name = format!("{name}_{}", std::process::id());
    let reg = RegTarget::hkcu(SCRATCH, value_name);
    let target = SettingTarget::Registry(reg.clone());
    (journal, target, Cleanup(reg))
}

/// Removes the scratch value however the test ends.
struct Cleanup(RegTarget);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = registry::write(&self.0, &RegValue::Absent);
    }
}

fn live(target: &SettingTarget) -> SettingValue {
    target.read().expect("read")
}

#[test]
fn apply_changes_the_setting_and_records_ownership() {
    let (j, t, _c) = fixture("apply_basic");

    j.apply("taskbar", &t, &SettingValue::dword(1)).unwrap();

    assert_eq!(live(&t), SettingValue::dword(1));
    let owned = j.owned().unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].module_id, "taskbar");
}

#[test]
fn the_original_is_captured_once_and_never_overwritten() {
    // The single most important property: after many changes, the recorded
    // original must still be the pre-OVERLAY value, not an OVERLAY-written one.
    let (j, t, _c) = fixture("capture_once");
    assert_eq!(live(&t), SettingValue::absent());

    j.apply("m", &t, &SettingValue::dword(1)).unwrap();
    j.apply("m", &t, &SettingValue::dword(2)).unwrap();
    j.apply("m", &t, &SettingValue::dword(3)).unwrap();

    assert_eq!(
        j.original_of(&t).unwrap().unwrap(),
        SettingValue::absent(),
        "the original must still be the absence we started from"
    );
}

#[test]
fn restoring_an_absent_original_deletes_the_value_rather_than_writing_zero() {
    // Writing 0 here would pin a Windows default the user never set. This is
    // the behaviour the whole RegValue::Absent design exists to guarantee.
    let (j, t, _c) = fixture("absent_restore");
    assert_eq!(live(&t), SettingValue::absent());

    j.apply("m", &t, &SettingValue::dword(1)).unwrap();
    let outcome = j.restore_one(&t).unwrap();

    assert_eq!(outcome.status, RestoreStatus::Restored);
    assert_eq!(
        live(&t),
        SettingValue::absent(),
        "value should be gone, not set to 0"
    );
    assert!(
        matches!(live(&t), SettingValue::Reg(RegValue::Absent)),
        "must be Absent, never Dword(0)"
    );
}

#[test]
fn restore_puts_back_a_pre_existing_value_exactly() {
    let (j, t, _c) = fixture("existing_restore");
    let SettingTarget::Registry(reg) = &t else {
        unreachable!()
    };
    registry::write(reg, &RegValue::Dword(7)).unwrap();

    j.apply("m", &t, &SettingValue::dword(99)).unwrap();
    assert_eq!(live(&t), SettingValue::dword(99));

    let outcome = j.restore_one(&t).unwrap();
    assert_eq!(outcome.status, RestoreStatus::Restored);
    assert_eq!(live(&t), SettingValue::dword(7));
}

#[test]
fn restore_releases_ownership_so_it_cannot_run_twice() {
    let (j, t, _c) = fixture("release");
    j.apply("m", &t, &SettingValue::dword(1)).unwrap();
    assert_eq!(j.owned().unwrap().len(), 1);

    j.restore_one(&t).unwrap();
    assert!(j.owned().unwrap().is_empty());

    // A second restore is a no-op, not an error and not a second write.
    let again = j.restore_one(&t).unwrap();
    assert_eq!(again.status, RestoreStatus::AlreadyOriginal);
}

#[test]
fn restore_never_touches_a_setting_overlay_does_not_own() {
    // The data-model guarantee: no owned row, no write.
    let (j, t, _c) = fixture("unowned");
    let SettingTarget::Registry(reg) = &t else {
        unreachable!()
    };
    registry::write(reg, &RegValue::Dword(1234)).unwrap();

    let outcome = j.restore_one(&t).unwrap();
    assert_eq!(outcome.status, RestoreStatus::AlreadyOriginal);
    assert_eq!(
        live(&t),
        SettingValue::dword(1234),
        "an unowned setting must be left exactly as it was"
    );
}

#[test]
fn an_external_change_is_reported_as_a_conflict_not_overwritten() {
    let (j, t, _c) = fixture("conflict");
    let SettingTarget::Registry(reg) = &t else {
        unreachable!()
    };

    j.apply("m", &t, &SettingValue::dword(1)).unwrap();

    // Something else — the user, another tool — sets a different value.
    registry::write(reg, &RegValue::Dword(55)).unwrap();

    // Applying again must refuse rather than silently clobber.
    let err = j.apply("m", &t, &SettingValue::dword(2)).unwrap_err();
    assert!(
        matches!(err, OverlayError::OwnershipConflict { .. }),
        "expected an ownership conflict, got {err:?}"
    );
    assert_eq!(live(&t), SettingValue::dword(55), "their value must stand");
}

#[test]
fn restore_skips_and_reports_an_externally_changed_setting() {
    let (j, t, _c) = fixture("conflict_restore");
    let SettingTarget::Registry(reg) = &t else {
        unreachable!()
    };

    j.apply("m", &t, &SettingValue::dword(1)).unwrap();
    registry::write(reg, &RegValue::Dword(55)).unwrap();

    let outcome = j.restore_one(&t).unwrap();
    assert_eq!(outcome.status, RestoreStatus::ConflictSkipped);
    assert_eq!(
        live(&t),
        SettingValue::dword(55),
        "the user's later choice must survive a restore"
    );
    assert!(outcome.detail.contains("outside OVERLAY"));
}

#[test]
fn applying_a_value_that_is_already_set_still_records_ownership() {
    // Otherwise a profile that matches the current desktop would leave
    // nothing for restore to put back.
    let (j, t, _c) = fixture("already_set");
    let SettingTarget::Registry(reg) = &t else {
        unreachable!()
    };
    registry::write(reg, &RegValue::Dword(5)).unwrap();

    j.apply("m", &t, &SettingValue::dword(5)).unwrap();
    assert_eq!(j.owned().unwrap().len(), 1);
}

#[test]
fn restore_all_clears_everything_overlay_owns() {
    let conn = storage::open_in_memory().unwrap();
    let j = Journal::new(Arc::new(Mutex::new(conn)));
    let pid = std::process::id();

    let targets: Vec<_> = (0..3)
        .map(|i| RegTarget::hkcu(SCRATCH, format!("multi{i}_{pid}")))
        .collect();
    let _cleanup: Vec<Cleanup> = targets.iter().cloned().map(Cleanup).collect();

    for (i, reg) in targets.iter().enumerate() {
        let t = SettingTarget::Registry(reg.clone());
        j.apply("m", &t, &SettingValue::dword(i as u32 + 1)).unwrap();
    }
    assert_eq!(j.owned().unwrap().len(), 3);

    let report = j.restore_all().unwrap();
    assert_eq!(report.len(), 3);
    assert!(report.iter().all(|r| r.status == RestoreStatus::Restored));
    assert!(j.owned().unwrap().is_empty());

    for reg in &targets {
        assert_eq!(registry::read(reg).unwrap(), RegValue::Absent);
    }
}

#[test]
fn restore_module_only_touches_that_modules_settings() {
    let conn = storage::open_in_memory().unwrap();
    let j = Journal::new(Arc::new(Mutex::new(conn)));
    let pid = std::process::id();

    let a = RegTarget::hkcu(SCRATCH, format!("mod_a_{pid}"));
    let b = RegTarget::hkcu(SCRATCH, format!("mod_b_{pid}"));
    let (_ca, _cb) = (Cleanup(a.clone()), Cleanup(b.clone()));
    let (ta, tb) = (
        SettingTarget::Registry(a.clone()),
        SettingTarget::Registry(b.clone()),
    );

    j.apply("taskbar", &ta, &SettingValue::dword(1)).unwrap();
    j.apply("icons", &tb, &SettingValue::dword(1)).unwrap();

    j.restore_module("taskbar").unwrap();

    assert_eq!(registry::read(&a).unwrap(), RegValue::Absent);
    assert_eq!(
        registry::read(&b).unwrap(),
        RegValue::Dword(1),
        "the other module's setting must be untouched"
    );
    assert_eq!(j.owned().unwrap().len(), 1);
}

#[test]
fn a_change_interrupted_after_it_landed_is_recovered_as_applied() {
    // Simulates a crash between "write" and "settle the row": the journal
    // must notice on restart that the change did in fact take effect, and
    // claim ownership so it is still restorable.
    let (j, t, _c) = fixture("recover_applied");
    let SettingTarget::Registry(reg) = &t else {
        unreachable!()
    };

    j.capture_original("m", &t).unwrap();
    // The row the real code writes before touching Windows.
    j.begin_row_for_test("m", &t, &SettingValue::absent(), &SettingValue::dword(9))
        .unwrap();
    // The write happened; the process then died before settling.
    registry::write(reg, &RegValue::Dword(9)).unwrap();

    assert_eq!(j.unsettled().unwrap().len(), 1);
    j.reconcile_unsettled().unwrap();
    assert!(j.unsettled().unwrap().is_empty());

    assert_eq!(
        j.owned().unwrap().len(),
        1,
        "a change that landed must become owned, so restore can undo it"
    );
    j.restore_one(&t).unwrap();
    assert_eq!(registry::read(reg).unwrap(), RegValue::Absent);
}

#[test]
fn a_change_interrupted_before_it_landed_is_recovered_as_failed() {
    let (j, t, _c) = fixture("recover_failed");

    j.capture_original("m", &t).unwrap();
    j.begin_row_for_test("m", &t, &SettingValue::absent(), &SettingValue::dword(9))
        .unwrap();
    // No write happened before the crash.

    j.reconcile_unsettled().unwrap();
    assert!(j.unsettled().unwrap().is_empty());
    assert!(
        j.owned().unwrap().is_empty(),
        "a change that never landed must not be claimed as ours"
    );
}

#[test]
fn the_journal_records_each_change_for_the_user_to_read() {
    let (j, t, _c) = fixture("history");
    j.apply("taskbar", &t, &SettingValue::dword(1)).unwrap();
    j.restore_one(&t).unwrap();

    let recent = j.recent(10).unwrap();
    assert_eq!(recent.len(), 2);
    // Newest first: the restore, then the change.
    assert!(recent[0].label.contains("history"));
    assert_eq!(recent[0].after, "(not set)");
    assert_eq!(recent[1].after, "1");
}
