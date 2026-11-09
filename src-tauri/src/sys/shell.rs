//! Out-of-process shell manipulation.
//!
//! Everything here works by asking windows that already exist to change — via
//! documented messages or window-attribute calls. OVERLAY never injects a
//! thread or DLL into `explorer.exe`, never patches a system binary, and never
//! writes outside HKCU. That rules out a few effects other customisers achieve,
//! and it is a deliberate trade: a shell that keeps working is worth more than
//! one extra blur mode.

use crate::error::{OverlayError, Result};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
use windows::core::{BOOL, PCWSTR};
#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetWindowLongPtrW, IsWindowVisible, SendMessageTimeoutW,
    SendNotifyMessageW, SetWindowLongPtrW, GWL_EXSTYLE, SMTO_ABORTIFHUNG, WM_COMMAND,
    WM_SETTINGCHANGE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
};

/// `SHELLDLL_DefView`'s undocumented-but-stable command id for "Show desktop
/// icons". It is the exact message the desktop context menu sends, so we are
/// using the same supported path the shell uses on itself — we just send it
/// ourselves instead of making the user right-click.
#[cfg(windows)]
const IDM_TOGGLE_DESKTOP_ICONS: usize = 0x7402;

/// How long to wait for Explorer to answer. Explorer can be busy; we would
/// rather report "could not confirm" than hang a UI thread behind it.
#[cfg(windows)]
const SHELL_TIMEOUT_MS: u32 = 2_000;

/// A display's geometry in the **logical** pixels window managers position
/// windows in.
///
/// Win32 reports physical pixels. A 1920×1080 panel at 125% scaling is
/// 1536×864 to Tauri, so placing a surface with the physical numbers puts it
/// a quarter of the display off-screen — which is exactly what the HUD did
/// before this type existed. Every placement calculation in OVERLAY works
/// from here; nothing positions a window from the raw `MonitorInfo` fields.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LogicalBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub work_x: f64,
    pub work_y: f64,
    pub work_width: f64,
    pub work_height: f64,
}

/// A display as Win32 reports it: **physical** pixels, plus the scale factor
/// needed to convert. Use [`MonitorInfo::logical`] for anything that places
/// a window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorInfo {
    /// Win32 device name, e.g. `\\.\DISPLAY1`. Stable per port, so a widget
    /// pinned to a monitor finds it again after a reboot.
    pub device: String,
    pub primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Work area — the desktop minus the taskbar. Widgets default inside it.
    pub work_x: i32,
    pub work_y: i32,
    pub work_width: u32,
    pub work_height: u32,
    /// Effective scale factor, 1.0 at 96 DPI.
    pub scale: f64,
}

impl MonitorInfo {
    /// This display in logical pixels.
    pub fn logical(&self) -> LogicalBounds {
        // A nonsensical scale would divide placement into orbit; fall back
        // to 1:1, which is wrong by at most the scale factor rather than
        // unbounded.
        let scale = if self.scale.is_finite() && self.scale > 0.1 {
            self.scale
        } else {
            1.0
        };
        LogicalBounds {
            x: self.x as f64 / scale,
            y: self.y as f64 / scale,
            width: self.width as f64 / scale,
            height: self.height as f64 / scale,
            work_x: self.work_x as f64 / scale,
            work_y: self.work_y as f64 / scale,
            work_width: self.work_width as f64 / scale,
            work_height: self.work_height as f64 / scale,
        }
    }
}

// ---------------------------------------------------------------------------
// Monitors
// ---------------------------------------------------------------------------

#[cfg(windows)]
unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut windows::Win32::Foundation::RECT,
    data: LPARAM,
) -> BOOL {
    let out = &mut *(data.0 as *mut Vec<MonitorInfo>);

    let mut ex = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut ex as *mut _ as *mut MONITORINFO).as_bool() {
        return BOOL(1); // skip this one, keep enumerating
    }

    let device = String::from_utf16_lossy(&ex.szDevice)
        .trim_end_matches('\0')
        .to_string();
    let m = ex.monitorInfo;

    out.push(MonitorInfo {
        device,
        primary: m.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
        x: m.rcMonitor.left,
        y: m.rcMonitor.top,
        width: (m.rcMonitor.right - m.rcMonitor.left).max(0) as u32,
        height: (m.rcMonitor.bottom - m.rcMonitor.top).max(0) as u32,
        work_x: m.rcWork.left,
        work_y: m.rcWork.top,
        work_width: (m.rcWork.right - m.rcWork.left).max(0) as u32,
        work_height: (m.rcWork.bottom - m.rcWork.top).max(0) as u32,
        scale: monitor_scale(monitor),
    });
    BOOL(1)
}

#[cfg(windows)]
fn monitor_scale(monitor: HMONITOR) -> f64 {
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    let mut x = 96u32;
    let mut y = 96u32;
    unsafe {
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() {
            return x as f64 / 96.0;
        }
    }
    1.0
}

#[cfg(windows)]
pub fn monitors() -> Result<Vec<MonitorInfo>> {
    let mut out: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut out as *mut _ as isize),
        );
    }
    if out.is_empty() {
        return Err(OverlayError::Win32(
            "no displays were reported by Windows".into(),
        ));
    }
    // Primary first, then left-to-right: the order the UI lists them in.
    out.sort_by(|a, b| b.primary.cmp(&a.primary).then(a.x.cmp(&b.x)));
    Ok(out)
}

#[cfg(not(windows))]
pub fn monitors() -> Result<Vec<MonitorInfo>> {
    Err(OverlayError::Win32("displays are Windows-only".into()))
}

// ---------------------------------------------------------------------------
// Desktop icons
// ---------------------------------------------------------------------------

/// The desktop's `SHELLDLL_DefView` together with the `SysListView32` inside
/// it that actually draws the icons.
///
/// Both halves matter and they are not interchangeable. The command to toggle
/// icons must go to the **DefView**, but only the **listview**'s visibility
/// tracks whether icons are on screen — the DefView stays visible either way.
/// Reading the container instead of the listview silently reports "visible"
/// forever, which makes the idempotent setter below behave like a blind
/// toggle. Measured on Windows 11 build 26100; see `tests/probe_desktop2.rs`.
///
/// Explorer normally parents the view to `Progman`, but moves it under a
/// `WorkerW` when something is hosting animated wallpaper, so every candidate
/// host is searched and the one owning a listview wins.
#[cfg(windows)]
fn desktop_icon_windows() -> Option<(HWND, HWND)> {
    unsafe fn listview_under(host: HWND) -> Option<(HWND, HWND)> {
        let view = FindWindowExW(
            Some(host),
            None,
            windows::core::w!("SHELLDLL_DefView"),
            PCWSTR::null(),
        )
        .ok()
        .filter(|v| !v.is_invalid())?;
        let list = FindWindowExW(
            Some(view),
            None,
            windows::core::w!("SysListView32"),
            PCWSTR::null(),
        )
        .ok()
        .filter(|l| !l.is_invalid())?;
        Some((view, list))
    }

    unsafe {
        if let Ok(progman) = FindWindowW(windows::core::w!("Progman"), PCWSTR::null()) {
            if let Some(found) = listview_under(progman) {
                return Some(found);
            }
        }
        // Fall back to the WorkerW chain. Bounded so a shell in a strange
        // state cannot spin here forever.
        let mut worker = HWND::default();
        for _ in 0..64 {
            let Ok(next) = FindWindowExW(
                None,
                if worker.is_invalid() { None } else { Some(worker) },
                windows::core::w!("WorkerW"),
                PCWSTR::null(),
            ) else {
                return None;
            };
            if next.is_invalid() {
                return None;
            }
            worker = next;
            if let Some(found) = listview_under(worker) {
                return Some(found);
            }
        }
        None
    }
}

/// True when desktop icons are currently on screen.
///
/// Read from the live listview rather than the `HideIcons` registry value,
/// because the two genuinely disagree: this machine reports `HideIcons = 1`
/// while Explorer has not applied it. The window is what the user sees, so the
/// window is the source of truth.
#[cfg(windows)]
pub fn desktop_icons_visible() -> Result<bool> {
    let (_, list) = desktop_icon_windows().ok_or_else(|| {
        OverlayError::Win32("could not find the desktop icon view (is Explorer running?)".into())
    })?;
    Ok(unsafe { IsWindowVisible(list).as_bool() })
}

/// Sets icon visibility idempotently.
///
/// The shell only offers a *toggle*, so calling it blindly would flip icons the
/// wrong way whenever our idea of the state was stale. Reading the live window
/// first and toggling only on a mismatch makes `set(true)` mean "visible" no
/// matter how many times it runs — which is what profile switching needs.
#[cfg(windows)]
pub fn set_desktop_icons_visible(visible: bool) -> Result<()> {
    let (view, list) = desktop_icon_windows()
        .ok_or_else(|| OverlayError::Win32("could not find the desktop icon view".into()))?;
    if unsafe { IsWindowVisible(list).as_bool() } == visible {
        return Ok(());
    }
    unsafe {
        let mut result: usize = 0;
        let ok = SendMessageTimeoutW(
            view,
            WM_COMMAND,
            WPARAM(IDM_TOGGLE_DESKTOP_ICONS),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            SHELL_TIMEOUT_MS,
            Some(&mut result),
        );
        if ok.0 == 0 {
            return Err(OverlayError::Win32(
                "Explorer did not respond to the desktop icon request".into(),
            ));
        }
    }
    // Explorer applies the change on its own message loop, so the listview can
    // still report the old state for a few milliseconds. Poll briefly rather
    // than either sleeping a fixed slug of time or failing a change that did
    // in fact work.
    let now = wait_for_icon_state(list, visible);
    if now != visible {
        return Err(OverlayError::VerificationFailed {
            target: "desktop icons".into(),
            expected: if visible { "visible" } else { "hidden" }.into(),
            found: if now { "visible" } else { "hidden" }.into(),
        });
    }
    Ok(())
}

/// Polls the listview for up to ~600 ms waiting for the requested state.
#[cfg(windows)]
fn wait_for_icon_state(list: HWND, want: bool) -> bool {
    for _ in 0..12 {
        let now = unsafe { IsWindowVisible(list).as_bool() };
        if now == want {
            return now;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    unsafe { IsWindowVisible(list).as_bool() }
}

#[cfg(not(windows))]
pub fn desktop_icons_visible() -> Result<bool> {
    Err(OverlayError::Win32("desktop icons are Windows-only".into()))
}

#[cfg(not(windows))]
pub fn set_desktop_icons_visible(_visible: bool) -> Result<()> {
    Err(OverlayError::Win32("desktop icons are Windows-only".into()))
}

// ---------------------------------------------------------------------------
// Explorer notification
// ---------------------------------------------------------------------------

/// Tells Explorer that a personalisation setting changed, so taskbar alignment
/// and size repaint without a sign-out.
///
/// `SendNotifyMessageW` is used rather than `SendMessage` because broadcasting
/// a blocking message to every top-level window can stall behind any one hung
/// application; this hands the message off and returns.
#[cfg(windows)]
pub fn notify_settings_changed() {
    unsafe {
        const HWND_BROADCAST: HWND = HWND(0xffff as *mut _);
        let section = windows::core::w!("ImmersiveColorSet");
        let _ = SendNotifyMessageW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(section.0 as isize),
        );
        let policy = windows::core::w!("Policy");
        let _ = SendNotifyMessageW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(policy.0 as isize),
        );
    }
}

#[cfg(not(windows))]
pub fn notify_settings_changed() {}

// ---------------------------------------------------------------------------
// Taskbar
// ---------------------------------------------------------------------------

/// Handles of the primary taskbar and any secondary-monitor taskbars.
#[cfg(windows)]
pub fn taskbar_windows() -> Vec<HWND> {
    let mut out = Vec::new();
    unsafe {
        if let Ok(h) = FindWindowW(windows::core::w!("Shell_TrayWnd"), PCWSTR::null()) {
            if !h.is_invalid() {
                out.push(h);
            }
        }
        let mut prev = HWND::default();
        while let Ok(h) = FindWindowExW(
            None,
            if prev.is_invalid() { None } else { Some(prev) },
            windows::core::w!("Shell_SecondaryTrayWnd"),
            PCWSTR::null(),
        ) {
            if h.is_invalid() {
                break;
            }
            prev = h;
            out.push(h);
        }
    }
    out
}

/// True when a taskbar window can be found — i.e. Explorer's shell is up.
pub fn shell_present() -> bool {
    #[cfg(windows)]
    {
        !taskbar_windows().is_empty()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Window attributes used by widget windows
// ---------------------------------------------------------------------------

/// Makes a window ignore the mouse entirely, so clicks land on whatever is
/// behind it. Used for click-through widgets sitting over the desktop.
#[cfg(windows)]
pub fn set_click_through(hwnd: isize, enabled: bool) -> Result<()> {
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let pass_through = (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0 | WS_EX_NOACTIVATE.0) as isize;
        let next = if enabled {
            current | pass_through
        } else {
            // Keep LAYERED: dropping it on a transparent WebView2 window makes
            // the rounded corners and shadow render as black boxes.
            current & !((WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0) as isize)
        };
        if next != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_click_through(_hwnd: isize, _enabled: bool) -> Result<()> {
    Ok(())
}

/// Stops a window taking focus when it is clicked.
///
/// Essential for a desktop-layer surface: activating a window raises it, so
/// without this the first click on a widget pulls it above the windows it is
/// meant to sit behind — and leaves it there.
#[cfg(windows)]
pub fn set_no_activate(hwnd: isize, enabled: bool) -> Result<()> {
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = if enabled {
            current | WS_EX_NOACTIVATE.0 as isize
        } else {
            current & !(WS_EX_NOACTIVATE.0 as isize)
        };
        if next != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_no_activate(_hwnd: isize, _enabled: bool) -> Result<()> {
    Ok(())
}

/// Keeps a window out of Alt-Tab and the taskbar without making it
/// always-on-top — the combination widgets want.
#[cfg(windows)]
pub fn set_tool_window(hwnd: isize) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{WS_EX_APPWINDOW, WS_EX_TOOLWINDOW};
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = (current | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        if next != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_tool_window(_hwnd: isize) -> Result<()> {
    Ok(())
}

/// Places one window immediately behind another.
///
/// Clearing the always-on-top flag is not enough to get a surface out from
/// in front of the studio: Windows moves a former topmost window to the top
/// of the ordinary band, which is still above everything else. This puts it
/// directly behind a named window instead.
#[cfg(windows)]
pub fn place_behind(hwnd: isize, behind: isize) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND(behind as *mut _)),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .map_err(|e| OverlayError::Win32(format!("could not restack a surface: {e}")))
    }
}

#[cfg(not(windows))]
pub fn place_behind(_hwnd: isize, _behind: isize) -> Result<()> {
    Ok(())
}

/// Pushes a window to the bottom of the Z-order so it sits above the wallpaper
/// but below ordinary application windows.
///
/// True WorkerW parenting — making the window a child of the wallpaper host —
/// gives a more convincing "part of the desktop" effect, but a reparented
/// WebView2 surface stops receiving input and composites unreliably across
/// Windows builds. `HWND_BOTTOM` plus `NOACTIVATE` achieves the same visual
/// result through documented calls, at the cost of needing a re-assert when
/// the user presses Win+D. `desktop_layer::watch` does that re-asserting.
#[cfg(windows)]
pub fn send_to_desktop_layer(hwnd: isize) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        SetWindowPos(
            hwnd,
            Some(HWND_BOTTOM),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
        .map_err(|e| OverlayError::Win32(format!("could not lower the widget window: {e}")))
    }
}

#[cfg(not(windows))]
pub fn send_to_desktop_layer(_hwnd: isize) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_info_round_trips() {
        let m = MonitorInfo {
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
            scale: 1.25,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<MonitorInfo>(&json).unwrap(), m);
    }
}
