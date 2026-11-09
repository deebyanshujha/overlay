//! Where OVERLAY keeps its files.
//!
//! Resolved without a Tauri `AppHandle`, because the emergency recovery path
//! runs before — and independently of — the Tauri runtime. That is the whole
//! reason this is a free function over `dirs` rather than a method on app
//! state: `overlay.exe --restore-original` must be able to find the journal
//! even if the GUI cannot start at all.

use crate::error::{OverlayError, Result};
use std::path::PathBuf;

/// Matches `identifier` in tauri.conf.json, so the GUI and the recovery CLI
/// agree on one directory.
pub const APP_DIR: &str = "com.overlay.studio";

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub db: PathBuf,
    /// Timestamped copies of the database, written before each profile switch
    /// and each restoration.
    pub backups: PathBuf,
    /// Plain-JSON mirror of the captured original state. Redundant with the
    /// database on purpose: if SQLite is ever unreadable, this file still says
    /// what the desktop looked like, in a format a human can act on.
    pub original_snapshot: PathBuf,
    pub exports: PathBuf,
    pub log: PathBuf,
}

impl Paths {
    pub fn resolve() -> Result<Self> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| OverlayError::other("could not locate the local app data folder"))?;
        Self::under(base.join(APP_DIR))
    }

    pub fn under(root: PathBuf) -> Result<Self> {
        let backups = root.join("backups");
        let exports = root.join("exports");
        std::fs::create_dir_all(&backups)?;
        std::fs::create_dir_all(&exports)?;
        Ok(Self {
            db: root.join("overlay.db"),
            original_snapshot: root.join("original-windows.json"),
            log: root.join("overlay.log"),
            backups,
            exports,
            root,
        })
    }

    /// A dated backup filename. Second resolution plus a counter suffix so two
    /// switches in the same second cannot collide.
    pub fn backup_file(&self, label: &str) -> PathBuf {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let safe: String = label
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let mut candidate = self.backups.join(format!("{stamp}-{safe}.db"));
        let mut n = 1;
        while candidate.exists() {
            candidate = self.backups.join(format!("{stamp}-{safe}-{n}.db"));
            n += 1;
        }
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!("overlay-paths-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn under_creates_the_directories_it_promises() {
        let root = temp_root();
        let p = Paths::under(root.clone()).unwrap();
        assert!(p.backups.is_dir());
        assert!(p.exports.is_dir());
        assert_eq!(p.db, root.join("overlay.db"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn backup_names_never_collide_within_one_second() {
        let root = temp_root();
        let p = Paths::under(root.clone()).unwrap();
        let a = p.backup_file("switch to developer");
        std::fs::write(&a, b"x").unwrap();
        let b = p.backup_file("switch to developer");
        assert_ne!(a, b);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn backup_labels_are_sanitised_into_the_filename() {
        let root = temp_root();
        let p = Paths::under(root.clone()).unwrap();
        let f = p.backup_file("../../etc/passwd");
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        assert!(!name.contains(".."), "path traversal leaked into {name}");
        assert!(!name.contains('/'));
        assert_eq!(f.parent().unwrap(), p.backups.as_path());
        std::fs::remove_dir_all(&root).ok();
    }
}
