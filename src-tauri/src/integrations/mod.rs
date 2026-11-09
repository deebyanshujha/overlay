//! Adapters for the applications OVERLAY coexists with.
//!
//! Three rules hold for every adapter here:
//!
//! 1. **Read-only by default.** OVERLAY reads another app's configuration to
//!    show status; it does not rewrite it. WallFlow's wallpaper library and
//!    QuickDock's pinned items are theirs.
//! 2. **No duplication.** Where the other app already does the job — wallpaper
//!    rotation, keyboard-first launching — OVERLAY defers to it rather than
//!    shipping a worse second copy.
//! 3. **Honest capability reporting.** An adapter advertises only what it has
//!    actually been able to do on this machine. Anything it cannot do is
//!    surfaced as a limitation, never silently stubbed.

pub mod quickdock;
pub mod wallflow;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long a detection result stays fresh.
///
/// Building one costs real work — enumerating processes, walking the
/// uninstall registry, and snapshotting WallFlow's multi-megabyte WAL
/// database — about 90 ms in total on the machine this was built on. The UI
/// asks for it on every page load and several surfaces poll for it, so
/// without a cache that cost lands on the interface again and again. Two
/// seconds is short enough that starting WallFlow still shows up promptly
/// and long enough that nothing pays for it twice in one interaction.
const FRESH_FOR: Duration = Duration::from_secs(2);

fn cache() -> &'static Mutex<Option<(Instant, Vec<IntegrationStatus>)>> {
    static CELL: OnceLock<Mutex<Option<(Instant, Vec<IntegrationStatus>)>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(None))
}

/// Detection for every companion app, cached briefly.
pub fn statuses_cached() -> Vec<IntegrationStatus> {
    if let Some((at, cached)) = cache().lock().as_ref() {
        if at.elapsed() < FRESH_FOR {
            return cached.clone();
        }
    }
    let fresh = vec![wallflow::WallFlow::status(), quickdock::QuickDock::status()];
    *cache().lock() = Some((Instant::now(), fresh.clone()));
    fresh
}

/// Drops the cache so the next read re-detects. Called after OVERLAY starts
/// one of these applications, where waiting two seconds to notice would
/// make the UI look broken.
pub fn invalidate_cache() {
    *cache().lock() = None;
}

/// How OVERLAY found (or failed to find) a companion application.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    /// Found via its Windows uninstall registration — the most reliable.
    UninstallRegistry,
    /// Found at the conventional per-user install location.
    KnownPath,
    /// Only its data directory exists; the executable was not located.
    DataDirectoryOnly,
    NotFound,
}

/// What an adapter can actually do with the installed build, as opposed to
/// what the integration is designed to do. Rendered directly on the
/// Integrations page so the user is never told something works when it does
/// not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capability {
    pub id: String,
    pub label: String,
    pub supported: bool,
    /// Why it is unsupported, or how it is achieved when it is.
    pub detail: String,
}

impl Capability {
    pub fn yes(id: &str, label: &str, detail: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            supported: true,
            detail: detail.into(),
        }
    }

    pub fn no(id: &str, label: &str, detail: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            supported: false,
            detail: detail.into(),
        }
    }
}

/// Everything the UI shows about one companion app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub installed: bool,
    pub running: bool,
    pub detected_via: DetectionSource,
    pub exe_path: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub version: Option<String>,
    pub capabilities: Vec<Capability>,
    /// Live details — current wallpaper, bound hotkey, and so on.
    pub details: Vec<(String, String)>,
    pub limitations: Vec<String>,
}

impl IntegrationStatus {
    pub fn missing(id: &str, name: &str, description: &str, hint: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            installed: false,
            running: false,
            detected_via: DetectionSource::NotFound,
            exe_path: None,
            data_dir: None,
            version: None,
            capabilities: Vec::new(),
            details: Vec::new(),
            limitations: vec![hint.into()],
        }
    }
}

/// Looks an application up in the per-user and machine uninstall registries.
///
/// This is how Windows itself answers "is X installed", so it keeps working
/// when the user installed to a non-default location.
pub fn find_installed(display_name: &str) -> Option<(PathBuf, Option<String>)> {
    use crate::sys::registry::{self, Hive, RegTarget, RegValue};

    const ROOTS: [(Hive, &str); 3] = [
        (
            Hive::CurrentUser,
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            Hive::LocalMachine,
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            Hive::LocalMachine,
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];

    for (hive, root) in ROOTS {
        for key in registry::subkeys(hive, root).unwrap_or_default() {
            let path = format!("{root}\\{key}");
            let name = registry::read(&RegTarget {
                hive,
                path: path.clone(),
                name: "DisplayName".into(),
            })
            .ok();
            let matches = matches!(name, Some(RegValue::Str(ref s)) if s == display_name);
            if !matches {
                continue;
            }

            let version = match registry::read(&RegTarget {
                hive,
                path: path.clone(),
                name: "DisplayVersion".into(),
            }) {
                Ok(RegValue::Str(v)) => Some(v),
                _ => None,
            };
            if let Ok(RegValue::Str(loc)) = registry::read(&RegTarget {
                hive,
                path,
                name: "InstallLocation".into(),
            }) {
                let trimmed = loc.trim_matches('"');
                if !trimmed.is_empty() {
                    return Some((PathBuf::from(trimmed), version));
                }
            }
        }
    }
    None
}
