//! Taskbar translucency via `SetWindowCompositionAttribute`.
//!
//! ## Why this API
//!
//! Windows 11 exposes only one translucency knob to users: the
//! `EnableTransparency` personalisation flag, which produces a fixed, fairly
//! opaque acrylic. Anything beyond it — a tint the user chooses, or a fully
//! clear taskbar — needs the compositor told directly.
//!
//! `SetWindowCompositionAttribute` is undocumented but has shipped unchanged
//! since Windows 10 1803 and is what every mainstream taskbar customiser uses.
//! Critically, it is an **out-of-process call against a window handle**: we do
//! not load code into Explorer, hook it, or patch any binary. If the call is
//! ever removed the symbol lookup simply fails and OVERLAY reports the feature
//! as unavailable — it cannot crash the shell.
//!
//! ## Reversibility
//!
//! The accent policy is window state, not persisted state. It is captured
//! before the first write and re-applied on restore; and because it lives only
//! in the window, killing OVERLAY or restarting Explorer also clears it. There
//! is no path by which this leaves a permanent mark on the system.

use crate::error::{OverlayError, Result};
use serde::{Deserialize, Serialize};

/// The blur/tint mode asked of the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskbarFinish {
    /// Leave the shell's own appearance alone.
    System,
    /// Flat tint at the chosen opacity, no blur. Crisp; cheapest to composite.
    Tinted,
    /// Gaussian blur behind the tint — the "frosted" look.
    Blurred,
    /// Windows 11 acrylic: blur plus noise. Slightly heavier.
    Acrylic,
    /// Fully see-through.
    Clear,
}

impl TaskbarFinish {
    /// Maps to `ACCENT_STATE`. The numbering is the OS's, not ours.
    fn accent_state(self) -> u32 {
        match self {
            TaskbarFinish::System => 0,  // ACCENT_DISABLED
            TaskbarFinish::Clear => 1,   // ACCENT_ENABLE_GRADIENT (alpha 0)
            TaskbarFinish::Tinted => 2,  // ACCENT_ENABLE_TRANSPARENTGRADIENT
            TaskbarFinish::Blurred => 3, // ACCENT_ENABLE_BLURBEHIND
            TaskbarFinish::Acrylic => 4, // ACCENT_ENABLE_ACRYLICBLURBEHIND
        }
    }
}

/// Captured or desired composition state for one taskbar window. This is the
/// "original state" the taskbar module stores before its first write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AccentPolicy {
    pub state: u32,
    pub flags: u32,
    /// AABBGGRR, as the compositor wants it.
    pub gradient_color: u32,
    pub animation_id: u32,
}

impl AccentPolicy {
    /// The policy a stock taskbar has: no OVERLAY accent at all.
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Builds a policy from a finish, an opacity and an RGB tint.
    ///
    /// `opacity` is 0.0–1.0 as the user sees it; `rgb` is 0xRRGGBB as the UI
    /// colour picker produces it. Both are converted here so no caller has to
    /// know about the compositor's byte order.
    pub fn new(finish: TaskbarFinish, opacity: f32, rgb: u32) -> Self {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
        let r = (rgb >> 16) & 0xFF;
        let g = (rgb >> 8) & 0xFF;
        let b = rgb & 0xFF;
        Self {
            state: finish.accent_state(),
            // 0x2 asks for the tint to apply to the whole window rather than
            // just its border.
            flags: 0x2,
            gradient_color: (alpha << 24) | (b << 16) | (g << 8) | r,
            animation_id: 0,
        }
    }
}

#[cfg(windows)]
#[repr(C)]
struct WindowCompositionAttributeData {
    attribute: u32,
    data: *mut std::ffi::c_void,
    size: usize,
}

/// `WCA_ACCENT_POLICY`.
#[cfg(windows)]
const WCA_ACCENT_POLICY: u32 = 19;

#[cfg(windows)]
type SetFn = unsafe extern "system" fn(
    windows::Win32::Foundation::HWND,
    *mut WindowCompositionAttributeData,
) -> windows::core::BOOL;

#[cfg(windows)]
type GetFn = unsafe extern "system" fn(
    windows::Win32::Foundation::HWND,
    *mut WindowCompositionAttributeData,
) -> windows::core::BOOL;

/// Resolved once; `None` means the OS build does not export the entry point,
/// which the taskbar module surfaces as an honest "unavailable".
#[cfg(windows)]
fn entry_points() -> Option<(SetFn, GetFn)> {
    use std::sync::OnceLock;
    use windows::core::s;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};

    static CELL: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    let pair = (*CELL.get_or_init(|| unsafe {
        let user32 = LoadLibraryA(s!("user32.dll")).ok()?;
        let set = GetProcAddress(user32, s!("SetWindowCompositionAttribute"))?;
        let get = GetProcAddress(user32, s!("GetWindowCompositionAttribute"))?;
        Some((set as usize, get as usize))
    }))?;
    unsafe {
        Some((
            std::mem::transmute::<usize, SetFn>(pair.0),
            std::mem::transmute::<usize, GetFn>(pair.1),
        ))
    }
}

/// Whether this machine supports OVERLAY-driven taskbar translucency.
pub fn available() -> bool {
    #[cfg(windows)]
    {
        entry_points().is_some()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Reads the window's current accent policy, so it can be put back verbatim.
#[cfg(windows)]
pub fn read_policy(hwnd: windows::Win32::Foundation::HWND) -> Result<AccentPolicy> {
    let (_, get) = entry_points().ok_or_else(|| {
        OverlayError::Win32("this Windows build does not expose window composition".into())
    })?;
    let mut policy = AccentPolicy::default();
    let mut data = WindowCompositionAttributeData {
        attribute: WCA_ACCENT_POLICY,
        data: &mut policy as *mut _ as *mut std::ffi::c_void,
        size: std::mem::size_of::<AccentPolicy>(),
    };
    unsafe {
        // A failure here means "nothing set yet", which is the disabled policy.
        if !get(hwnd, &mut data).as_bool() {
            return Ok(AccentPolicy::disabled());
        }
    }
    Ok(policy)
}

/// Applies an accent policy to a window.
#[cfg(windows)]
pub fn apply_policy(
    hwnd: windows::Win32::Foundation::HWND,
    policy: AccentPolicy,
) -> Result<()> {
    let (set, _) = entry_points().ok_or_else(|| {
        OverlayError::Win32("this Windows build does not expose window composition".into())
    })?;
    let mut policy = policy;
    let mut data = WindowCompositionAttributeData {
        attribute: WCA_ACCENT_POLICY,
        data: &mut policy as *mut _ as *mut std::ffi::c_void,
        size: std::mem::size_of::<AccentPolicy>(),
    };
    unsafe {
        if !set(hwnd, &mut data).as_bool() {
            return Err(OverlayError::Win32(
                "Windows rejected the taskbar appearance change".into(),
            ));
        }
    }
    Ok(())
}

/// Captures the accent policy of every taskbar window, keyed by handle.
#[cfg(windows)]
pub fn capture_all() -> Vec<(isize, AccentPolicy)> {
    crate::sys::shell::taskbar_windows()
        .into_iter()
        .filter_map(|h| read_policy(h).ok().map(|p| (h.0 as isize, p)))
        .collect()
}

/// Applies one policy to every taskbar window, including secondary monitors.
///
/// Returns how many windows accepted it. Zero with taskbars present means the
/// feature is not working and the caller must not claim success.
#[cfg(windows)]
pub fn apply_all(policy: AccentPolicy) -> Result<usize> {
    let bars = crate::sys::shell::taskbar_windows();
    if bars.is_empty() {
        return Err(OverlayError::Win32(
            "no taskbar was found — Explorer may not be running".into(),
        ));
    }
    let mut applied = 0usize;
    let mut last_error = None;
    for h in bars {
        match apply_policy(h, policy) {
            Ok(()) => applied += 1,
            Err(e) => last_error = Some(e),
        }
    }
    if applied == 0 {
        return Err(last_error.unwrap_or_else(|| {
            OverlayError::Win32("the taskbar did not accept the appearance change".into())
        }));
    }
    Ok(applied)
}

#[cfg(not(windows))]
pub fn capture_all() -> Vec<(isize, AccentPolicy)> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn apply_all(_policy: AccentPolicy) -> Result<usize> {
    Err(OverlayError::Win32(
        "taskbar appearance is Windows-only".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_lands_in_the_alpha_byte() {
        let p = AccentPolicy::new(TaskbarFinish::Tinted, 1.0, 0x000000);
        assert_eq!(p.gradient_color >> 24, 0xFF);
        let p = AccentPolicy::new(TaskbarFinish::Tinted, 0.0, 0x000000);
        assert_eq!(p.gradient_color >> 24, 0x00);
    }

    #[test]
    fn rgb_is_byte_swapped_for_the_compositor() {
        // 0xRRGGBB in, AABBGGRR out: red must end up in the low byte.
        let p = AccentPolicy::new(TaskbarFinish::Tinted, 1.0, 0xFF0000);
        assert_eq!(p.gradient_color & 0xFF, 0xFF);
        assert_eq!((p.gradient_color >> 16) & 0xFF, 0x00);

        let p = AccentPolicy::new(TaskbarFinish::Tinted, 1.0, 0x0000FF);
        assert_eq!(p.gradient_color & 0xFF, 0x00);
        assert_eq!((p.gradient_color >> 16) & 0xFF, 0xFF);
    }

    #[test]
    fn opacity_outside_the_range_is_clamped_not_wrapped() {
        assert_eq!(
            AccentPolicy::new(TaskbarFinish::Tinted, 5.0, 0).gradient_color >> 24,
            0xFF
        );
        assert_eq!(
            AccentPolicy::new(TaskbarFinish::Tinted, -3.0, 0).gradient_color >> 24,
            0x00
        );
    }

    #[test]
    fn system_finish_maps_to_the_disabled_state() {
        assert_eq!(
            AccentPolicy::new(TaskbarFinish::System, 1.0, 0).state,
            AccentPolicy::disabled().state
        );
    }

    #[test]
    fn policy_round_trips_through_json() {
        let p = AccentPolicy::new(TaskbarFinish::Acrylic, 0.6, 0x1A1A2E);
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<AccentPolicy>(&json).unwrap(), p);
    }
}
