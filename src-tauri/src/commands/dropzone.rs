//! Dropzone shelf commands.
//!
//! The shelf lives in memory for the session. That is deliberate: it is a
//! holding area for files in transit, and persisting it across restarts would
//! accumulate references to files that have since moved or been deleted.
//!
//! Every destructive operation requires explicit confirmation from the
//! caller — see [`crate::modules::dropzone::move_into`].

use crate::error::{OverlayError, Result};
use crate::modules::dropzone::{self, Collision, ShelfItem};
use parking_lot::Mutex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Upper bound on shelf size, so a dropped folder of ten thousand files
/// cannot exhaust memory or make the surface unusable.
const MAX_ITEMS: usize = 500;

fn shelf() -> &'static Mutex<Vec<ShelfItem>> {
    static CELL: OnceLock<Mutex<Vec<ShelfItem>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Debug, Serialize)]
pub struct ShelfState {
    pub items: Vec<ShelfItem>,
    pub total_bytes: u64,
    pub missing: usize,
}

fn state_of(items: &[ShelfItem]) -> ShelfState {
    ShelfState {
        total_bytes: items.iter().map(|i| i.size_bytes).sum(),
        missing: items.iter().filter(|i| !i.exists).count(),
        items: items.to_vec(),
    }
}

/// Adds files to the shelf as references. Nothing is copied or moved.
#[tauri::command(async)]
pub fn shelf_add(paths: Vec<String>) -> Result<ShelfState> {
    let mut items = shelf().lock();
    for raw in paths {
        let path = PathBuf::from(&raw);
        // Dropping the same file twice is a no-op rather than a duplicate row.
        if items.iter().any(|i| i.path == path) {
            continue;
        }
        if items.len() >= MAX_ITEMS {
            return Err(OverlayError::other(format!(
                "the Dropzone holds at most {MAX_ITEMS} items"
            )));
        }
        items.push(ShelfItem::describe(&path));
    }
    Ok(state_of(&items))
}

/// Re-checks every item against the disk, so files deleted elsewhere show as
/// missing instead of silently failing later.
#[tauri::command(async)]
pub fn shelf_refresh() -> ShelfState {
    let mut items = shelf().lock();
    for item in items.iter_mut() {
        *item = ShelfItem {
            id: item.id.clone(),
            added_at: item.added_at,
            ..ShelfItem::describe(&item.path)
        };
    }
    state_of(&items)
}

#[tauri::command(async)]
pub fn shelf_list() -> ShelfState {
    state_of(&shelf().lock())
}

#[tauri::command(async)]
pub fn shelf_remove(ids: Vec<String>) -> ShelfState {
    let mut items = shelf().lock();
    items.retain(|i| !ids.contains(&i.id));
    state_of(&items)
}

/// Empties the shelf. Removes references only — no file is deleted.
#[tauri::command(async)]
pub fn shelf_clear() -> ShelfState {
    let mut items = shelf().lock();
    items.clear();
    state_of(&items)
}

#[derive(Debug, Serialize)]
pub struct TransferReport {
    pub moved: bool,
    pub succeeded: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Copies shelf items into a folder.
#[tauri::command(async)]
pub fn shelf_copy_to(ids: Vec<String>, destination: String) -> Result<TransferReport> {
    transfer(ids, destination, false, false)
}

/// Moves shelf items into a folder.
///
/// `confirmed` must be true. The frontend sets it only after the user has
/// agreed to a dialog naming the files, because this is the one Dropzone
/// operation that removes the original.
#[tauri::command(async)]
pub fn shelf_move_to(
    ids: Vec<String>,
    destination: String,
    confirmed: bool,
) -> Result<TransferReport> {
    if !confirmed {
        return Err(OverlayError::other(
            "moving files needs confirmation first",
        ));
    }
    transfer(ids, destination, true, true)
}

fn transfer(
    ids: Vec<String>,
    destination: String,
    moving: bool,
    confirmed: bool,
) -> Result<TransferReport> {
    let destination = PathBuf::from(destination);
    if !destination.is_dir() {
        return Err(OverlayError::other(format!(
            "{} is not a folder",
            destination.display()
        )));
    }

    let selected: Vec<ShelfItem> = {
        let items = shelf().lock();
        items.iter().filter(|i| ids.contains(&i.id)).cloned().collect()
    };
    if selected.is_empty() {
        return Err(OverlayError::other("nothing was selected"));
    }

    let mut report = TransferReport {
        moved: moving,
        succeeded: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };

    for item in &selected {
        if item.is_dir {
            report.failed.push((
                item.name.clone(),
                "folders are not supported yet — only files".into(),
            ));
            continue;
        }
        let result = if moving {
            dropzone::move_into(&item.path, &destination, Collision::Rename, confirmed)
        } else {
            dropzone::copy_into(&item.path, &destination, Collision::Rename)
        };
        match result {
            Ok(Some(path)) => report
                .succeeded
                .push(path.file_name().unwrap_or_default().to_string_lossy().to_string()),
            Ok(None) => report.skipped.push(item.name.clone()),
            Err(e) => report.failed.push((item.name.clone(), e.to_string())),
        }
    }

    // A moved file no longer exists where the shelf points, so its reference
    // goes; a copied one stays available for another destination.
    if moving && !report.succeeded.is_empty() {
        let mut items = shelf().lock();
        items.retain(|i| !ids.contains(&i.id) || i.path.exists());
    }
    Ok(report)
}

/// Opens a shelf item with its registered application.
#[tauri::command(async)]
pub fn shelf_open(id: String) -> Result<()> {
    let item = {
        let items = shelf().lock();
        items.iter().find(|i| i.id == id).cloned()
    }
    .ok_or_else(|| OverlayError::other("that item is no longer on the shelf"))?;

    if !item.path.exists() {
        return Err(OverlayError::other(format!(
            "{} is no longer on disk",
            item.name
        )));
    }
    open_path(&item.path)
}

/// Shows a shelf item in File Explorer.
#[tauri::command(async)]
pub fn shelf_reveal(id: String) -> Result<()> {
    let item = {
        let items = shelf().lock();
        items.iter().find(|i| i.id == id).cloned()
    }
    .ok_or_else(|| OverlayError::other("that item is no longer on the shelf"))?;

    let folder = item
        .path
        .parent()
        .ok_or_else(|| OverlayError::other("that item has no containing folder"))?;
    open_path(folder)
}

/// Opens a path with the shell.
///
/// Converted to a `file:///` URL so it goes through the same allowlisted
/// entry point as every other external open, rather than being a second,
/// looser path into `ShellExecute`.
fn open_path(path: &Path) -> Result<()> {
    let url = format!("file:///{}", path.to_string_lossy().replace('\\', "/"));
    crate::sys::process::open_external(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shelf is deliberately one per process — there is one Dropzone on
    /// screen, not one per caller — which means these tests share it and must
    /// not run concurrently. Each takes this lock for its duration.
    fn exclusive() -> parking_lot::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock()
    }

    #[test]
    fn adding_the_same_file_twice_does_not_duplicate_it() {
        let _guard = exclusive();
        shelf_clear();
        let f = std::env::temp_dir().join(format!("overlay-shelf-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().to_string();

        shelf_add(vec![p.clone()]).unwrap();
        let state = shelf_add(vec![p]).unwrap();
        assert_eq!(state.items.len(), 1);

        shelf_clear();
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn clearing_the_shelf_deletes_no_files() {
        let _guard = exclusive();
        // The Dropzone's central safety promise.
        shelf_clear();
        let f = std::env::temp_dir().join(format!("overlay-shelf-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&f, b"precious").unwrap();

        shelf_add(vec![f.to_string_lossy().to_string()]).unwrap();
        shelf_clear();

        assert!(f.exists(), "clearing the shelf must never delete a file");
        assert_eq!(std::fs::read(&f).unwrap(), b"precious");
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn a_move_without_confirmation_is_refused() {
        let _guard = exclusive();
        shelf_clear();
        let f = std::env::temp_dir().join(format!("overlay-shelf-{}.txt", uuid::Uuid::new_v4()));
        std::fs::write(&f, b"data").unwrap();
        let state = shelf_add(vec![f.to_string_lossy().to_string()]).unwrap();
        let id = state.items[0].id.clone();

        let dest = std::env::temp_dir().to_string_lossy().to_string();
        assert!(shelf_move_to(vec![id], dest, false).is_err());
        assert!(f.exists());

        shelf_clear();
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn a_missing_file_is_tracked_rather_than_dropped() {
        let _guard = exclusive();
        shelf_clear();
        shelf_add(vec!["Z:\\nope\\gone.txt".into()]).unwrap();
        let state = shelf_refresh();
        assert_eq!(state.items.len(), 1);
        assert_eq!(state.missing, 1);
        shelf_clear();
    }

    #[test]
    fn transferring_nothing_is_an_error_not_a_silent_success() {
        let _guard = exclusive();
        shelf_clear();
        let dest = std::env::temp_dir().to_string_lossy().to_string();
        assert!(shelf_copy_to(vec!["no-such-id".into()], dest).is_err());
    }
}
