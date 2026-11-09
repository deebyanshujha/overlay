//! Live Windows tests. These touch the real machine, so they are `#[ignore]`d
//! by default and run explicitly:
//!
//! ```text
//! cargo test --test windows_live -- --ignored --nocapture
//! ```
//!
//! Nothing here modifies system state except `desktop_icons_round_trip`, which
//! hides icons and puts them straight back; it restores in every exit path,
//! including when an assertion fails.

#![cfg(windows)]

use overlay_lib::sys::{self, accent, registry, shell};
use overlay_lib::sys::registry::{RegTarget, RegValue};

const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

#[test]
#[ignore = "touches the live machine"]
fn reports_a_sane_environment() {
    let env = sys::environment();
    println!("{env:#?}");
    assert!(env.build > 0, "could not read the Windows build number");
    assert!(
        !env.monitors.is_empty(),
        "at least one display must be reported"
    );
    assert!(
        env.monitors.iter().any(|m| m.primary),
        "exactly one display should be primary"
    );
    for m in &env.monitors {
        assert!(m.width > 0 && m.height > 0, "display has no size: {m:?}");
        assert!(
            m.work_height <= m.height,
            "work area cannot be taller than the display"
        );
        assert!(m.scale >= 0.5 && m.scale <= 8.0, "implausible DPI scale");
    }
}

#[test]
#[ignore = "touches the live machine"]
fn absent_values_read_as_absent_not_as_zero() {
    // The invariant the whole restoration design rests on. A value name that
    // has never been written must come back `Absent`, so restore deletes it
    // instead of pinning a Windows default.
    let nonexistent = RegTarget::hkcu(ADVANCED, "OverlayNoSuchValue_7f3a");
    assert_eq!(registry::read(&nonexistent).unwrap(), RegValue::Absent);

    // A whole key that does not exist is also absence, not an error.
    let no_key = RegTarget::hkcu(r"Software\OverlayNoSuchKey_7f3a", "Anything");
    assert_eq!(registry::read(&no_key).unwrap(), RegValue::Absent);
}

#[test]
#[ignore = "touches the live machine"]
fn reads_real_taskbar_settings() {
    for name in [
        "TaskbarAl",
        "TaskbarSi",
        "TaskbarDa",
        "TaskbarMn",
        "HideIcons",
    ] {
        let v = registry::read(&RegTarget::hkcu(ADVANCED, name)).unwrap();
        println!("{name:>12} = {}", v.describe());
    }
}

#[test]
#[ignore = "writes to HKCU under a scratch key, then removes it"]
fn write_then_restore_absence_leaves_no_trace() {
    // Exercises the exact capture → change → restore cycle the journal runs,
    // against a scratch key so a failure cannot affect the user's desktop.
    let target = RegTarget::hkcu(r"Software\OVERLAY\__test__", "Scratch");

    let original = registry::read(&target).unwrap();
    assert_eq!(original, RegValue::Absent, "scratch key should start clean");

    registry::write_verified(&target, &RegValue::Dword(42)).unwrap();
    assert_eq!(registry::read(&target).unwrap(), RegValue::Dword(42));

    // Restoring absence must delete the value, not write 0.
    registry::write_verified(&target, &original).unwrap();
    assert_eq!(registry::read(&target).unwrap(), RegValue::Absent);
}

#[test]
#[ignore = "writes to HKCU under a scratch key, then removes it"]
fn string_and_binary_values_survive_the_registry() {
    let s = RegTarget::hkcu(r"Software\OVERLAY\__test__", "ScratchStr");
    registry::write_verified(&s, &RegValue::Str("hello world".into())).unwrap();
    assert_eq!(
        registry::read(&s).unwrap(),
        RegValue::Str("hello world".into())
    );
    registry::write_verified(&s, &RegValue::Absent).unwrap();

    // 40 bytes, the shape of the real StuckRects3 blob.
    let b = RegTarget::hkcu(r"Software\OVERLAY\__test__", "ScratchBin");
    let blob: Vec<u8> = (0u8..40).collect();
    registry::write_verified(&b, &RegValue::Binary(blob.clone())).unwrap();
    assert_eq!(registry::read(&b).unwrap(), RegValue::Binary(blob));
    registry::write_verified(&b, &RegValue::Absent).unwrap();
}

#[test]
#[ignore = "touches the live machine"]
fn finds_the_taskbar_and_reads_its_accent() {
    let bars = shell::taskbar_windows();
    assert!(
        !bars.is_empty(),
        "no taskbar found — is Explorer running?"
    );
    println!("taskbar windows: {}", bars.len());

    assert!(
        accent::available(),
        "SetWindowCompositionAttribute should exist on Windows 10 1803+"
    );
    for h in bars {
        let policy = accent::read_policy(h).unwrap();
        println!("  {h:?} -> {policy:?}");
    }
}

#[test]
#[ignore = "briefly hides desktop icons, then restores them"]
fn desktop_icons_round_trip() {
    let before = shell::desktop_icons_visible().unwrap();
    println!("desktop icons start: {before}");

    // Restore on every path, including a panicking assertion below.
    let restore = scopeguard(move || {
        let _ = shell::set_desktop_icons_visible(before);
    });

    shell::set_desktop_icons_visible(!before).unwrap();
    assert_eq!(shell::desktop_icons_visible().unwrap(), !before);

    // Setting the state it is already in must be a no-op, not a flip.
    shell::set_desktop_icons_visible(!before).unwrap();
    assert_eq!(
        shell::desktop_icons_visible().unwrap(),
        !before,
        "set() must be idempotent, not a toggle"
    );

    drop(restore);
    assert_eq!(
        shell::desktop_icons_visible().unwrap(),
        before,
        "icons were not put back"
    );
}

/// Minimal run-on-drop guard, so the test does not pull in a dependency just
/// to restore one setting.
fn scopeguard<F: FnMut()>(f: F) -> impl Drop {
    struct Guard<F: FnMut()>(F);
    impl<F: FnMut()> Drop for Guard<F> {
        fn drop(&mut self) {
            (self.0)();
        }
    }
    Guard(f)
}
