//! Process discovery and launching, used by the WallFlow and QuickDock
//! adapters and by the dock.
//!
//! Launching is deliberately narrow: callers pass a path and optional
//! arguments, and the path is always resolved and validated by the caller
//! first. There is no code path that takes a string from the frontend and
//! hands it to a shell — that is why there is no `cmd /c` anywhere in OVERLAY.

use crate::error::{OverlayError, Result};
use std::path::Path;

/// True when the current process is running with an elevated token. Shown in
/// diagnostics; OVERLAY never requires it.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            size,
            &mut size,
        )
        .is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Whether a process with this executable name is running, cached briefly.
///
/// The underlying snapshot walks every process on the machine and costs
/// ~20 ms. Module status is rebuilt often and asks about several
/// executables each time, so uncached this alone made listing modules take
/// longer than a frame. One second is far shorter than a person notices and
/// long enough to collapse a burst of questions into one answer.
pub fn is_running_cached(exe_stem: &str) -> bool {
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    const FRESH_FOR: Duration = Duration::from_secs(1);
    static CELL: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> = OnceLock::new();
    let cache = CELL.get_or_init(|| Mutex::new(HashMap::new()));

    if let Some((at, running)) = cache.lock().get(exe_stem) {
        if at.elapsed() < FRESH_FOR {
            return *running;
        }
    }
    let running = is_running(exe_stem);
    cache
        .lock()
        .insert(exe_stem.to_string(), (Instant::now(), running));
    running
}

/// Whether a process with this executable name (without `.exe`) is running.
///
/// Walks every process; prefer [`is_running_cached`] on any path the UI can
/// reach.
#[cfg(windows)]
pub fn is_running(exe_stem: &str) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let needle = format!("{}.exe", exe_stem.to_ascii_lowercase());
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return false;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = false;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(&entry.szExeFile)
                    .trim_end_matches('\0')
                    .to_ascii_lowercase();
                if name == needle {
                    found = true;
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
pub fn is_running(_exe_stem: &str) -> bool {
    false
}

/// Starts a program that OVERLAY has already located on disk.
///
/// Detached and with no window: these are tray applications, and inheriting
/// our console would keep a phantom handle alive after OVERLAY exits.
pub fn launch_detached(exe: &Path, args: &[&str]) -> Result<()> {
    if !exe.is_file() {
        return Err(OverlayError::Integration(format!(
            "{} is not on disk",
            exe.display()
        )));
    }
    let mut command = std::process::Command::new(exe);
    command.args(args);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| OverlayError::Integration(format!("could not start {}: {e}", exe.display())))
}

/// Opens a path, URL or `ms-settings:` page with its registered handler.
///
/// This is how Control Center reaches Windows settings pages it cannot drive
/// directly. The scheme is checked against an allowlist so a malformed profile
/// import can never turn this into arbitrary execution.
pub fn open_external(target: &str) -> Result<()> {
    const ALLOWED: [&str; 4] = ["ms-settings:", "https://", "http://", "file:///"];
    let is_allowed = ALLOWED.iter().any(|p| target.starts_with(p));
    if !is_allowed {
        return Err(OverlayError::Config(format!(
            "refusing to open `{target}`: only Windows settings pages and web links are allowed"
        )));
    }
    #[cfg(windows)]
    {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let target = HSTRING::from(target);
        let result = unsafe {
            ShellExecuteW(
                None,
                windows::core::w!("open"),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW returns a pseudo-HINSTANCE; <= 32 is an error code.
        if result.0 as isize <= 32 {
            return Err(OverlayError::Win32(format!(
                "Windows could not open {target}"
            )));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err(OverlayError::Win32("opening links is Windows-only".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_external_rejects_anything_not_allowlisted() {
        // The guard that stops an imported profile becoming code execution.
        for bad in [
            "cmd.exe /c calc",
            "powershell -enc ABC",
            "javascript:alert(1)",
            "C:\\Windows\\System32\\cmd.exe",
            "\\\\evil-host\\share\\payload.exe",
        ] {
            assert!(
                open_external(bad).is_err(),
                "should have refused to open {bad}"
            );
        }
    }

    #[test]
    fn launch_detached_refuses_a_path_that_is_not_there() {
        let missing = Path::new("Z:\\overlay-does-not-exist\\nope.exe");
        assert!(launch_detached(missing, &[]).is_err());
    }
}
