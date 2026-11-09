//! Typed registry access built around one idea: **absence is a value**.
//!
//! Most Windows personalisation settings are "missing until first changed" —
//! `TaskbarAl`, `EnableTransparency` and `AppsUseLightTheme` do not exist on a
//! fresh profile, and Explorer falls back to a built-in default. A restore that
//! writes `0` back over an absent value is *not* a restore: it pins a default
//! that was previously free to change. So `RegValue::Absent` round-trips
//! through capture, storage and restore, and restoring it deletes the value.

use crate::error::{OverlayError, Result};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
#[cfg(windows)]
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ, REG_VALUE_TYPE,
};

/// Which root hive a path is relative to. Deliberately only the two OVERLAY
/// ever touches; there is no variant that could reach HKCR or HKU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

impl Hive {
    #[cfg(windows)]
    fn handle(self) -> HKEY {
        match self {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Hive::CurrentUser => "HKCU",
            Hive::LocalMachine => "HKLM",
        }
    }
}

/// A registry value as OVERLAY models it, including the "not there" case.
/// Adjacently tagged rather than internally tagged: serde cannot write a
/// newtype variant wrapping a sequence under an internal tag, and `Binary`
/// wraps a `Vec<u8>` — which is how the taskbar's auto-hide blob is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RegValue {
    /// The value name does not exist. Restoring this deletes the value.
    Absent,
    Dword(u32),
    Str(String),
    Binary(Vec<u8>),
}

impl RegValue {
    /// Short, stable rendering used in journal rows, conflict reports and the
    /// restoration report shown to the user.
    pub fn describe(&self) -> String {
        match self {
            RegValue::Absent => "(not set)".into(),
            RegValue::Dword(v) => v.to_string(),
            RegValue::Str(s) => format!("\"{s}\""),
            RegValue::Binary(b) => format!("{} bytes", b.len()),
        }
    }

    pub fn as_dword(&self) -> Option<u32> {
        match self {
            RegValue::Dword(v) => Some(*v),
            _ => None,
        }
    }

    /// Treats absence as the supplied Windows default, for display only —
    /// never for deciding what to write back.
    pub fn dword_or(&self, default: u32) -> u32 {
        self.as_dword().unwrap_or(default)
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, RegValue::Absent)
    }
}

/// A fully qualified value location. Cheap to clone and `Eq`, so it doubles as
/// the journal's target identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegTarget {
    pub hive: Hive,
    pub path: String,
    pub name: String,
}

impl RegTarget {
    pub fn hkcu(path: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            hive: Hive::CurrentUser,
            path: path.into(),
            name: name.into(),
        }
    }

    /// Opaque, stable string identity — the journal's primary key for a target.
    pub fn id(&self) -> String {
        format!("{}\\{}\\{}", self.hive.as_str(), self.path, self.name)
    }
}

impl std::fmt::Display for RegTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\\{} :: {}", self.hive.as_str(), self.path, self.name)
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
struct OwnedKey(HKEY);

#[cfg(windows)]
impl Drop for OwnedKey {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
}

/// Reads a value, mapping "no such value" and "no such key" to `Absent`
/// rather than to an error — both mean the same thing for our purposes.
#[cfg(windows)]
pub fn read(target: &RegTarget) -> Result<RegValue> {
    unsafe {
        let path = wide(&target.path);
        let mut key = HKEY::default();
        let status = RegOpenKeyExW(
            target.hive.handle(),
            PCWSTR(path.as_ptr()),
            Some(0),
            KEY_READ,
            &mut key,
        );
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(RegValue::Absent);
        }
        if status != ERROR_SUCCESS {
            return Err(OverlayError::Registry(format!(
                "cannot open {}: win32 error {}",
                target, status.0
            )));
        }
        let key = OwnedKey(key);

        let name = wide(&target.name);
        let mut kind = REG_VALUE_TYPE::default();
        let mut size: u32 = 0;
        let status = RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        );
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(RegValue::Absent);
        }
        if status != ERROR_SUCCESS {
            return Err(OverlayError::Registry(format!(
                "cannot query {}: win32 error {}",
                target, status.0
            )));
        }

        let mut buf = vec![0u8; size.max(4) as usize];
        let mut len = buf.len() as u32;
        let status = RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr()),
            Some(&mut len),
        );
        if status != ERROR_SUCCESS {
            return Err(OverlayError::Registry(format!(
                "cannot read {}: win32 error {}",
                target, status.0
            )));
        }
        buf.truncate(len as usize);

        Ok(decode(kind, buf))
    }
}

#[cfg(windows)]
fn decode(kind: REG_VALUE_TYPE, buf: Vec<u8>) -> RegValue {
    match kind {
        REG_DWORD => {
            let mut b = [0u8; 4];
            let n = 4.min(buf.len());
            b[..n].copy_from_slice(&buf[..n]);
            RegValue::Dword(u32::from_le_bytes(b))
        }
        REG_SZ => {
            let units: Vec<u16> = buf
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&u| u != 0)
                .collect();
            RegValue::Str(String::from_utf16_lossy(&units))
        }
        _ => RegValue::Binary(buf),
    }
}

/// Writes a value, or deletes it when `value` is `Absent`. Creates the key path
/// if needed; deleting a value from a key that is already gone is a success,
/// because the end state is what was asked for.
#[cfg(windows)]
pub fn write(target: &RegTarget, value: &RegValue) -> Result<()> {
    unsafe {
        let path = wide(&target.path);
        let name = wide(&target.name);

        if matches!(value, RegValue::Absent) {
            let mut key = HKEY::default();
            let status = RegOpenKeyExW(
                target.hive.handle(),
                PCWSTR(path.as_ptr()),
                Some(0),
                KEY_SET_VALUE,
                &mut key,
            );
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            if status != ERROR_SUCCESS {
                return Err(OverlayError::Registry(format!(
                    "cannot open {} to clear it: win32 error {}",
                    target, status.0
                )));
            }
            let key = OwnedKey(key);
            let status = RegDeleteValueW(key.0, PCWSTR(name.as_ptr()));
            if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            return Err(OverlayError::Registry(format!(
                "cannot clear {}: win32 error {}",
                target, status.0
            )));
        }

        let mut key = HKEY::default();
        let status = RegCreateKeyExW(
            target.hive.handle(),
            PCWSTR(path.as_ptr()),
            Some(0),
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        );
        if status != ERROR_SUCCESS {
            return Err(OverlayError::Registry(format!(
                "cannot open {} for writing: win32 error {}",
                target, status.0
            )));
        }
        let key = OwnedKey(key);

        let (kind, bytes) = match value {
            RegValue::Absent => unreachable!("handled above"),
            RegValue::Dword(v) => (REG_DWORD, v.to_le_bytes().to_vec()),
            RegValue::Str(s) => {
                let mut b = Vec::with_capacity((s.len() + 1) * 2);
                for u in s.encode_utf16().chain(std::iter::once(0)) {
                    b.extend_from_slice(&u.to_le_bytes());
                }
                (REG_SZ, b)
            }
            RegValue::Binary(b) => (REG_BINARY, b.clone()),
        };

        let status = RegSetValueExW(key.0, PCWSTR(name.as_ptr()), Some(0), kind, Some(&bytes));
        if status != ERROR_SUCCESS {
            return Err(OverlayError::Registry(format!(
                "cannot write {}: win32 error {}",
                target, status.0
            )));
        }
        Ok(())
    }
}

/// Lists the immediate subkey names of a key.
///
/// Used to walk the uninstall registry when detecting companion apps. A key
/// that does not exist yields an empty list rather than an error, since
/// "nothing installed there" is a normal answer.
#[cfg(windows)]
pub fn subkeys(hive: Hive, path: &str) -> Result<Vec<String>> {
    use windows::Win32::System::Registry::RegEnumKeyExW;

    unsafe {
        let wide_path = wide(path);
        let mut key = HKEY::default();
        let status = RegOpenKeyExW(
            hive.handle(),
            PCWSTR(wide_path.as_ptr()),
            Some(0),
            KEY_READ,
            &mut key,
        );
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(Vec::new());
        }
        if status != ERROR_SUCCESS {
            return Err(OverlayError::Registry(format!(
                "cannot enumerate {}\\{path}: win32 error {}",
                hive.as_str(),
                status.0
            )));
        }
        let key = OwnedKey(key);

        let mut out = Vec::new();
        // Key names are capped at 255 characters by the registry itself.
        let mut name = [0u16; 256];
        for index in 0.. {
            let mut len = name.len() as u32;
            let status = RegEnumKeyExW(
                key.0,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut len,
                None,
                None,
                None,
                None,
            );
            if status != ERROR_SUCCESS {
                break; // ERROR_NO_MORE_ITEMS, or anything else: stop cleanly.
            }
            out.push(String::from_utf16_lossy(&name[..len as usize]));
        }
        Ok(out)
    }
}

#[cfg(not(windows))]
pub fn subkeys(_hive: Hive, _path: &str) -> Result<Vec<String>> {
    Ok(Vec::new())
}

/// Writes then reads back, so a silent failure cannot be reported as success.
pub fn write_verified(target: &RegTarget, value: &RegValue) -> Result<()> {
    write(target, value)?;
    let found = read(target)?;
    if &found != value {
        return Err(OverlayError::VerificationFailed {
            target: target.to_string(),
            expected: value.describe(),
            found: found.describe(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Non-Windows stubs. OVERLAY only ships for Windows; these exist so the crate
// still type-checks elsewhere and so unit tests that never touch the registry
// can run anywhere.
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
pub fn read(_target: &RegTarget) -> Result<RegValue> {
    Err(OverlayError::Registry("registry is Windows-only".into()))
}

#[cfg(not(windows))]
pub fn write(_target: &RegTarget, _value: &RegValue) -> Result<()> {
    Err(OverlayError::Registry("registry is Windows-only".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_round_trips_through_json() {
        let v = RegValue::Absent;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<RegValue>(&json).unwrap(), v);
    }

    #[test]
    fn absent_is_not_zero() {
        // The whole point of the type: a missing value must never compare
        // equal to an explicit 0, or restore would pin Windows defaults.
        assert_ne!(RegValue::Absent, RegValue::Dword(0));
    }

    #[test]
    fn dword_or_only_substitutes_for_absence() {
        assert_eq!(RegValue::Absent.dword_or(1), 1);
        assert_eq!(RegValue::Dword(0).dword_or(1), 0);
    }

    #[test]
    fn target_id_is_stable_and_qualified() {
        let t = RegTarget::hkcu("Software\\X", "Y");
        assert_eq!(t.id(), "HKCU\\Software\\X\\Y");
    }

    #[test]
    fn describe_distinguishes_every_shape() {
        assert_eq!(RegValue::Absent.describe(), "(not set)");
        assert_eq!(RegValue::Dword(3).describe(), "3");
        assert_eq!(RegValue::Str("a".into()).describe(), "\"a\"");
        assert_eq!(RegValue::Binary(vec![1, 2]).describe(), "2 bytes");
    }

    #[test]
    fn binary_values_survive_a_json_round_trip() {
        // StuckRects3 auto-hide lives in a 40-byte blob; losing a byte here
        // would corrupt the user's taskbar on restore.
        let v = RegValue::Binary((0u8..40).collect());
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<RegValue>(&json).unwrap(), v);
    }
}
