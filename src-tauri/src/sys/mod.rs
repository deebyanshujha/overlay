//! Windows platform layer. Nothing above this module calls Win32 directly, and
//! nothing in here knows about profiles, modules or the journal — the
//! dependency only ever points downward.

pub mod accent;
pub mod process;
pub mod registry;
pub mod shell;

use serde::{Deserialize, Serialize};

/// What OVERLAY could determine about the machine it is running on. Gathered
/// once at startup and shown on the Integrations page, so the user can see why
/// a feature is offered or withheld rather than guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub windows_version: String,
    pub build: u32,
    /// Windows 11 taskbar settings (alignment, widgets button) only exist from
    /// build 22000. On Windows 10 those controls are hidden rather than shown
    /// doing nothing.
    pub is_windows_11: bool,
    pub shell_running: bool,
    pub taskbar_accent_supported: bool,
    pub monitors: Vec<shell::MonitorInfo>,
    pub elevated: bool,
}

pub fn environment() -> Environment {
    let (version, build) = os_version();
    Environment {
        is_windows_11: build >= 22000,
        windows_version: version,
        build,
        shell_running: shell::shell_present(),
        taskbar_accent_supported: accent::available(),
        monitors: shell::monitors().unwrap_or_default(),
        elevated: process::is_elevated(),
    }
}

/// Reads the real build number from `ntoskrnl`'s version resource.
///
/// `GetVersionExW` lies to unmanifested processes and `RtlGetVersion` needs a
/// driver-kit import, so the file version of the kernel is the reliable
/// user-mode answer — it is what Windows' own `winver` reports.
#[cfg(windows)]
fn os_version() -> (String, u32) {
    use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

    let mut buf = [0u16; 260];
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    let sys_dir = String::from_utf16_lossy(&buf[..len.min(buf.len())]);
    let kernel = std::path::Path::new(&sys_dir).join("ntoskrnl.exe");

    if let Some((major, minor, build)) = file_version(&kernel) {
        let name = if build >= 22000 {
            "Windows 11"
        } else if major >= 10 {
            "Windows 10"
        } else {
            "Windows"
        };
        return (format!("{name} ({major}.{minor}.{build})"), build);
    }
    ("Windows (version unavailable)".into(), 0)
}

#[cfg(windows)]
fn file_version(path: &std::path::Path) -> Option<(u32, u32, u32)> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(
            PCWSTR(wide.as_ptr()),
            Some(0),
            size,
            data.as_mut_ptr() as *mut _,
        )
        .ok()?;

        let mut info: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        // Returns BOOL, not a Result: false means the "\\" root block is absent.
        if !VerQueryValueW(
            data.as_ptr() as *const _,
            windows::core::w!("\\"),
            &mut info,
            &mut len,
        )
        .as_bool()
        {
            return None;
        }
        if info.is_null() || (len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>() {
            return None;
        }
        let fixed = &*(info as *const VS_FIXEDFILEINFO);
        Some((
            fixed.dwFileVersionMS >> 16,
            fixed.dwFileVersionMS & 0xFFFF,
            fixed.dwFileVersionLS >> 16,
        ))
    }
}

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(not(windows))]
fn os_version() -> (String, u32) {
    ("not Windows".into(), 0)
}
