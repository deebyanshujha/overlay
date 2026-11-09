//! WallFlow adapter.
//!
//! WallFlow is a wallpaper manager with its own scheduler, provider list and
//! image cache. OVERLAY does not reimplement any of that — this adapter
//! detects WallFlow, reads its state, and hands control back to it.
//!
//! # How the integration actually works
//!
//! WallFlow exposes no IPC server, no HTTP endpoint and no documented CLI.
//! What it *does* have, verified by reading its source at `D:\wallflow`:
//!
//! * a `tauri-plugin-single-instance` registration, so launching the
//!   executable a second time is forwarded to the running copy and raises its
//!   window instead of starting a duplicate — this is the supported way to
//!   "open WallFlow";
//! * a SQLite database at `%LOCALAPPDATA%\com.wallflow.app\wallflow.db`
//!   holding its settings blob and wallpaper table, which we read **read-only**
//!   to report the current wallpaper and schedule.
//!
//! Requesting a wallpaper change is therefore *not* supported against the
//! stock build, and this adapter says so rather than pretending. Launching
//! WallFlow so the user can press "Change wallpaper now" is offered instead.
//!
//! # Why the database is copied before reading
//!
//! WallFlow runs in WAL mode and keeps the database open. A read-only
//! connection from another process needs to map the `-shm` file, which fails
//! in some configurations. Rather than risk interfering with a running
//! WallFlow, the three WAL files are snapshotted to a temporary directory and
//! read there. OVERLAY never opens WallFlow's live database for writing.

use super::{Capability, DetectionSource, IntegrationStatus};
use crate::error::{OverlayError, Result};
use crate::sys::process;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DISPLAY_NAME: &str = "WallFlow";
const EXE: &str = "wallflow";
const DATA_DIR: &str = "com.wallflow.app";

/// The slice of WallFlow's settings blob OVERLAY cares about. Deliberately
/// `#[serde(default)]` on everything: WallFlow owns this schema and may add
/// or rename fields, and a changed sibling field must not break detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WallFlowSettings {
    pub provider: String,
    pub category: String,
    pub search_query: String,
    pub source_mode: String,
    pub interval_hours: u32,
    pub next_change_at: i64,
    pub current_wallpaper_id: Option<String>,
    pub theme: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurrentWallpaper {
    pub id: String,
    pub provider: String,
    pub category: String,
    pub author_name: Option<String>,
    pub source_url: Option<String>,
    pub width: u32,
    pub height: u32,
    pub file_name: String,
    /// Absolute path to the image, when the file is still in the cache.
    pub path: Option<PathBuf>,
}

pub struct WallFlow;

impl WallFlow {
    pub fn data_dir() -> Option<PathBuf> {
        let dir = dirs::data_local_dir()?.join(DATA_DIR);
        dir.is_dir().then_some(dir)
    }

    /// Locates the executable: uninstall registry first, then the default
    /// per-user install path WallFlow's NSIS `currentUser` mode produces.
    pub fn locate() -> (Option<PathBuf>, Option<String>, DetectionSource) {
        if let Some((dir, version)) = super::find_installed(DISPLAY_NAME) {
            let exe = dir.join(format!("{EXE}.exe"));
            if exe.is_file() {
                return (Some(exe), version, DetectionSource::UninstallRegistry);
            }
        }
        if let Some(local) = dirs::data_local_dir() {
            let exe = local.join(DISPLAY_NAME).join(format!("{EXE}.exe"));
            if exe.is_file() {
                return (Some(exe), None, DetectionSource::KnownPath);
            }
        }
        if Self::data_dir().is_some() {
            return (None, None, DetectionSource::DataDirectoryOnly);
        }
        (None, None, DetectionSource::NotFound)
    }

    pub fn is_running() -> bool {
        process::is_running_cached(EXE)
    }

    /// Opens WallFlow, or raises it if it is already running.
    pub fn launch() -> Result<()> {
        let (Some(exe), _, _) = Self::locate() else {
            return Err(OverlayError::Integration(
                "WallFlow is not installed on this machine.".into(),
            ));
        };
        process::launch_detached(&exe, &[])
    }

    /// Snapshots the WAL database and reads WallFlow's settings and current
    /// wallpaper from the copy.
    pub fn read_state() -> Result<(WallFlowSettings, Option<CurrentWallpaper>)> {
        let dir = Self::data_dir()
            .ok_or_else(|| OverlayError::Integration("WallFlow has no data directory yet".into()))?;
        let db = dir.join("wallflow.db");
        if !db.is_file() {
            return Err(OverlayError::Integration(
                "WallFlow has not created its database yet".into(),
            ));
        }

        let snapshot = snapshot_wal_database(&db)?;
        let result = read_from_snapshot(&snapshot.path, &dir);
        // Best effort; a leftover temp copy is harmless but we would rather not.
        let _ = std::fs::remove_dir_all(&snapshot.dir);
        result
    }

    pub fn status() -> IntegrationStatus {
        let (exe_path, version, detected_via) = Self::locate();
        let data_dir = Self::data_dir();
        let installed = exe_path.is_some();

        if !installed && data_dir.is_none() {
            return IntegrationStatus::missing(
                "wallflow",
                DISPLAY_NAME,
                "Wallpaper manager. OVERLAY shows its status and can open it; it does not \
                 replace its wallpaper engine.",
                "WallFlow was not found. Install it and OVERLAY will pick it up automatically.",
            );
        }

        let mut details = Vec::new();
        let mut limitations = vec![
            "OVERLAY cannot ask WallFlow to change the wallpaper: the installed build exposes \
             no command-line or IPC interface for it. Use \"Open WallFlow\" and press \
             \"Change wallpaper now\" there."
                .to_string(),
        ];

        match Self::read_state() {
            Ok((settings, current)) => {
                if !settings.provider.is_empty() {
                    details.push(("Provider".into(), settings.provider.clone()));
                }
                let source = if settings.source_mode == "search" && !settings.search_query.is_empty()
                {
                    format!("search: {}", settings.search_query)
                } else if settings.category.is_empty() {
                    "anything".into()
                } else {
                    settings.category.clone()
                };
                details.push(("Source".into(), source));
                if settings.interval_hours > 0 {
                    details.push((
                        "Changes every".into(),
                        format!("{} hours", settings.interval_hours),
                    ));
                }
                if settings.next_change_at > 0 {
                    if let Some(t) = chrono::DateTime::from_timestamp(settings.next_change_at, 0) {
                        details.push((
                            "Next change".into(),
                            t.with_timezone(&chrono::Local)
                                .format("%a %d %b, %H:%M")
                                .to_string(),
                        ));
                    }
                }
                match current {
                    Some(w) => {
                        let mut label = w.category.clone();
                        if let Some(a) = &w.author_name {
                            label = format!("{label} — {a}");
                        }
                        details.push(("Current wallpaper".into(), label));
                        details.push(("Resolution".into(), format!("{}×{}", w.width, w.height)));
                    }
                    None => {
                        details.push(("Current wallpaper".into(), "WallFlow's default".into()));
                    }
                }
            }
            Err(e) => limitations.push(format!("Could not read WallFlow's library: {e}")),
        }

        let capabilities = vec![
            Capability::yes(
                "detect",
                "Detect installation",
                "Found through the Windows uninstall registry.",
            ),
            Capability::yes(
                "status",
                "Report status",
                "Reads WallFlow's own database read-only, from a temporary snapshot.",
            ),
            Capability::yes(
                "launch",
                "Open WallFlow",
                "Starts it, or raises the running copy via its single-instance handler.",
            ),
            Capability::yes(
                "wallpaper_info",
                "Read current wallpaper",
                "Provider, topic, author and resolution.",
            ),
            Capability::no(
                "request_change",
                "Request a wallpaper change",
                "The installed WallFlow build has no command-line or IPC entry point for this.",
            ),
            Capability::yes(
                "profile_preference",
                "Per-profile preference",
                "A profile can record whether WallFlow should be running, and start it on switch.",
            ),
        ];

        IntegrationStatus {
            id: "wallflow".into(),
            name: DISPLAY_NAME.into(),
            description:
                "Wallpaper manager. OVERLAY reports its state and can launch it; WallFlow keeps \
                 full ownership of wallpapers, its schedule and its settings."
                    .into(),
            installed,
            running: Self::is_running(),
            detected_via,
            exe_path,
            data_dir,
            version,
            capabilities,
            details,
            limitations,
        }
    }
}

/// A temporary copy of a WAL database and its sidecar files.
struct Snapshot {
    dir: PathBuf,
    path: PathBuf,
}

/// Copies `db`, `db-wal` and `db-shm` into a temporary directory.
///
/// All three are needed: without the `-wal` file the snapshot would show a
/// stale database missing everything WallFlow has written since its last
/// checkpoint.
fn snapshot_wal_database(db: &Path) -> Result<Snapshot> {
    let dir = std::env::temp_dir().join(format!("overlay-wf-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;

    let name = db
        .file_name()
        .ok_or_else(|| OverlayError::Integration("bad database path".into()))?;
    let target = dir.join(name);
    std::fs::copy(db, &target)?;

    for suffix in ["-wal", "-shm"] {
        let mut from = db.as_os_str().to_os_string();
        from.push(suffix);
        let from = PathBuf::from(from);
        if from.is_file() {
            let mut to = target.as_os_str().to_os_string();
            to.push(suffix);
            // A sidecar that vanishes mid-copy just means a checkpoint
            // happened; the main file is still readable.
            let _ = std::fs::copy(&from, PathBuf::from(to));
        }
    }
    Ok(Snapshot { dir, path: target })
}

fn read_from_snapshot(
    db: &Path,
    data_dir: &Path,
) -> Result<(WallFlowSettings, Option<CurrentWallpaper>)> {
    use rusqlite::{Connection, OpenFlags, OptionalExtension};

    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| OverlayError::Integration(format!("cannot read WallFlow's database: {e}")))?;

    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| OverlayError::Integration(format!("WallFlow settings unreadable: {e}")))?;

    let settings: WallFlowSettings = match raw {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => WallFlowSettings::default(),
    };

    let Some(id) = settings.current_wallpaper_id.clone() else {
        return Ok((settings, None));
    };

    let current = conn
        .query_row(
            "SELECT id, provider, category, author_name, source_url, width, height, file_name
             FROM wallpapers WHERE id = ?1",
            [&id],
            |r| {
                Ok(CurrentWallpaper {
                    id: r.get(0)?,
                    provider: r.get(1)?,
                    category: r.get(2)?,
                    author_name: r.get(3)?,
                    source_url: r.get(4)?,
                    width: r.get(5)?,
                    height: r.get(6)?,
                    file_name: r.get(7)?,
                    path: None,
                })
            },
        )
        .optional()
        .map_err(|e| OverlayError::Integration(format!("WallFlow library unreadable: {e}")))?
        .map(|mut w| {
            let path = data_dir.join("wallpapers").join(&w.file_name);
            w.path = path.is_file().then_some(path);
            w
        });

    Ok((settings, current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_parsing_tolerates_fields_wallflow_may_add_or_remove() {
        // WallFlow owns this schema; our adapter must not break when it grows.
        let json = r#"{"provider":"wallhaven","category":"nature","interval_hours":24,
                       "a_future_field":123}"#;
        let s: WallFlowSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.provider, "wallhaven");
        assert_eq!(s.interval_hours, 24);
        assert_eq!(s.next_change_at, 0, "missing fields fall back to defaults");
    }

    #[test]
    fn an_empty_settings_blob_still_parses() {
        let s: WallFlowSettings = serde_json::from_str("{}").unwrap();
        assert!(s.current_wallpaper_id.is_none());
    }

    #[test]
    fn status_is_honest_about_not_being_able_to_change_the_wallpaper() {
        // The capability the stock build genuinely does not offer must never
        // be advertised as supported.
        let s = WallFlow::status();
        if let Some(c) = s.capabilities.iter().find(|c| c.id == "request_change") {
            assert!(
                !c.supported,
                "must not claim a wallpaper-change capability that does not exist"
            );
        }
    }
}
