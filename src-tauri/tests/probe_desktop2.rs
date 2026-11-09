//! Second desktop probe: enumerate every shell view on the machine, so we can
//! tell which one actually hosts the icons on this Windows build.

#![cfg(windows)]

use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, FindWindowExW, GetClassNameW, GetWindowRect, IsWindowVisible,
    SendMessageW, WM_COMMAND,
};

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n as usize])
}

unsafe extern "system" fn collect_top(h: HWND, data: LPARAM) -> BOOL {
    let out = &mut *(data.0 as *mut Vec<HWND>);
    let c = class_of(h);
    if c == "Progman" || c == "WorkerW" {
        out.push(h);
    }
    BOOL(1)
}

unsafe extern "system" fn collect_children(h: HWND, data: LPARAM) -> BOOL {
    let out = &mut *(data.0 as *mut Vec<(HWND, String)>);
    out.push((h, class_of(h)));
    BOOL(1)
}

#[test]
#[ignore = "probe"]
fn enumerate_every_desktop_host() {
    let mut tops: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_top), LPARAM(&mut tops as *mut _ as isize));
    }
    println!("found {} Progman/WorkerW windows\n", tops.len());

    for h in tops {
        let c = class_of(h);
        let vis = unsafe { IsWindowVisible(h).as_bool() };
        let view = unsafe {
            FindWindowExW(
                Some(h),
                None,
                windows::core::w!("SHELLDLL_DefView"),
                PCWSTR::null(),
            )
            .ok()
            .filter(|v| !v.is_invalid())
        };
        println!("{c:<10} {h:?} visible={vis} defview={view:?}");

        if let Some(v) = view {
            let mut kids: Vec<(HWND, String)> = Vec::new();
            unsafe {
                let _ = EnumChildWindows(
                    Some(v),
                    Some(collect_children),
                    LPARAM(&mut kids as *mut _ as isize),
                );
            }
            for (k, kc) in kids {
                let kvis = unsafe { IsWindowVisible(k).as_bool() };
                let mut r = Default::default();
                let _ = unsafe { GetWindowRect(k, &mut r) };
                println!(
                    "    child {kc:<18} {k:?} visible={kvis} rect=({},{})-({},{})",
                    r.left, r.top, r.right, r.bottom
                );
            }
        }
    }
}

#[test]
#[ignore = "probe: toggles icons twice"]
fn toggle_and_watch_the_listview() {
    // Whatever the container reports, the listview is the thing the user sees.
    let (view, list) = find_live_view().expect("no desktop listview found");
    let before = unsafe { IsWindowVisible(list).as_bool() };
    println!("listview visible before = {before}");

    unsafe {
        let r = SendMessageW(view, WM_COMMAND, Some(WPARAM(0x7402)), Some(LPARAM(0)));
        println!("toggle ret={}", r.0);
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    let after = unsafe { IsWindowVisible(list).as_bool() };
    println!("listview visible after  = {after}");

    if after != before {
        unsafe {
            let _ = SendMessageW(view, WM_COMMAND, Some(WPARAM(0x7402)), Some(LPARAM(0)));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
        println!(
            "restored = {}",
            unsafe { IsWindowVisible(list).as_bool() }
        );
    }
    assert_ne!(before, after, "0x7402 did not move the listview");
}

/// The DefView that actually owns a `SysListView32`, plus that listview.
fn find_live_view() -> Option<(HWND, HWND)> {
    let mut tops: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_top), LPARAM(&mut tops as *mut _ as isize));
    }
    for h in tops {
        unsafe {
            let Some(view) = FindWindowExW(
                Some(h),
                None,
                windows::core::w!("SHELLDLL_DefView"),
                PCWSTR::null(),
            )
            .ok()
            .filter(|v| !v.is_invalid()) else {
                continue;
            };
            if let Ok(list) = FindWindowExW(
                Some(view),
                None,
                windows::core::w!("SysListView32"),
                PCWSTR::null(),
            ) {
                if !list.is_invalid() {
                    return Some((view, list));
                }
            }
        }
    }
    None
}
