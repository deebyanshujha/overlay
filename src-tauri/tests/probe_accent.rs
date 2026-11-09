//! Probe: can this Windows build read a window's accent policy back?
//!
//! `SetWindowCompositionAttribute` is well established, but its `Get`
//! counterpart is not guaranteed to report what was written. The journal's
//! verify-by-readback step depends on knowing which it is here.
//!
//! `cargo test --test probe_accent -- --ignored --nocapture`
//! Restores whatever it found before exiting.

#![cfg(windows)]

use overlay_lib::sys::accent::{self, AccentPolicy, TaskbarFinish};
use overlay_lib::sys::shell;

#[test]
#[ignore = "briefly tints the taskbar, then puts it back"]
fn can_the_accent_policy_be_read_back() {
    let bars = shell::taskbar_windows();
    assert!(!bars.is_empty(), "no taskbar found");
    let hwnd = bars[0];

    let before = accent::read_policy(hwnd).unwrap();
    println!("before          : {before:?}");

    let wanted = AccentPolicy::new(TaskbarFinish::Blurred, 0.25, 0x0B0C10);
    println!("applying        : {wanted:?}");
    accent::apply_policy(hwnd, wanted).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(400));
    let after = accent::read_policy(hwnd).unwrap();
    println!("read back       : {after:?}");
    println!("matches written : {}", after == wanted);

    // Put it back however it started.
    accent::apply_policy(hwnd, before).unwrap();
    let restored = accent::read_policy(hwnd).unwrap();
    println!("after restore   : {restored:?}");
}
