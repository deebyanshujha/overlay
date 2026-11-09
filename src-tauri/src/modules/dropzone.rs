//! Dropzone — a floating shelf for files you are moving between places.
//!
//! # The data-safety rule
//!
//! Dropping a file into the Dropzone creates a **reference**. Nothing is
//! copied, nothing is moved, nothing is written. The shelf is a list of paths
//! with metadata; clearing it deletes nothing. Copy and move are explicit,
//! separate actions, and move — the only destructive one — requires a
//! confirmation flag the caller must pass deliberately.
//!
//! That ordering is why [`ShelfItem`] has no "pending operation" state: there
//! is nothing in flight to get wrong if OVERLAY exits with items on the shelf.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::windows::{SurfaceManager, SurfaceSpec, ZMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Manager;

pub const LABEL: &str = "dropzone";

/// One file on the shelf. Always a reference to a file in its original place.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShelfItem {
    pub id: String,
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub is_dir: bool,
    /// False when the file has been deleted or unmounted since it was added.
    pub exists: bool,
    pub extension: String,
    pub added_at: i64,
}

impl ShelfItem {
    pub fn describe(path: &Path) -> Self {
        let meta = std::fs::metadata(path);
        let exists = meta.is_ok();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string()),
            size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            is_dir: meta.as_ref().map(|m| m.is_dir()).unwrap_or(false),
            exists,
            extension: path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
            added_at: crate::storage::now(),
            path: path.to_path_buf(),
        }
    }
}

/// What to do when the destination already has a file of that name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Collision {
    /// Write `name (2).ext` instead. The safe default: nothing is lost.
    #[default]
    Rename,
    Skip,
    Overwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DropzoneConfig {
    pub opacity: f32,
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    /// Shrink to a small puck when the pointer is elsewhere.
    pub collapse_when_idle: bool,
    pub show_previews: bool,
    pub collision: Collision,
    /// Drop items that no longer exist on disk when the shelf is reopened.
    pub prune_missing: bool,
}

impl Default for DropzoneConfig {
    fn default() -> Self {
        Self {
            opacity: 0.96,
            width: 300.0,
            height: 380.0,
            // Clear of the column widgets default into, so switching on
            // both at once does not stack them.
            x: 560.0,
            y: 140.0,
            collapse_when_idle: true,
            show_previews: true,
            collision: Collision::Rename,
            prune_missing: true,
        }
    }
}

impl DropzoneConfig {
    fn normalize(mut self) -> Result<Self> {
        for (label, v) in [
            ("width", self.width),
            ("height", self.height),
            ("x", self.x),
            ("y", self.y),
        ] {
            if !v.is_finite() {
                return Err(OverlayError::Config(format!(
                    "dropzone {label} must be a number"
                )));
            }
        }
        self.width = self.width.clamp(220.0, 900.0);
        self.height = self.height.clamp(240.0, 1_400.0);
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.4, 1.0)
        } else {
            0.96
        };
        Ok(self)
    }
}

/// Picks a destination path that will not overwrite anything, by inserting
/// ` (n)` before the extension.
///
/// Used for [`Collision::Rename`]. The counter is bounded so a directory with
/// pathological contents cannot spin here forever.
pub fn non_colliding_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return Some(candidate);
    }
    let path = Path::new(name);
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let ext = path.extension().map(|e| e.to_string_lossy().to_string());

    for n in 2..10_000 {
        let next = match &ext {
            Some(e) => format!("{stem} ({n}).{e}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(next);
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Copies a file to a directory, honouring the collision policy.
pub fn copy_into(item: &Path, dir: &Path, collision: Collision) -> Result<Option<PathBuf>> {
    if !item.is_file() {
        return Err(OverlayError::other(format!(
            "{} is no longer on disk",
            item.display()
        )));
    }
    if !dir.is_dir() {
        return Err(OverlayError::other(format!(
            "{} is not a folder",
            dir.display()
        )));
    }
    let name = item
        .file_name()
        .ok_or_else(|| OverlayError::other("that file has no name"))?
        .to_string_lossy()
        .to_string();

    let destination = match collision {
        Collision::Skip if dir.join(&name).exists() => return Ok(None),
        Collision::Overwrite => dir.join(&name),
        _ => non_colliding_path(dir, &name)
            .ok_or_else(|| OverlayError::other("could not find a free filename"))?,
    };

    // Refuse a copy onto itself, which would truncate the source.
    if same_file(item, &destination) {
        return Ok(None);
    }
    std::fs::copy(item, &destination)?;
    Ok(Some(destination))
}

/// Moves a file. `confirmed` must be true — the destructive operation is
/// never reachable by accident from a caller that forgot a flag.
pub fn move_into(
    item: &Path,
    dir: &Path,
    collision: Collision,
    confirmed: bool,
) -> Result<Option<PathBuf>> {
    if !confirmed {
        return Err(OverlayError::other(
            "moving a file needs explicit confirmation",
        ));
    }
    let Some(destination) = copy_into(item, dir, collision)? else {
        return Ok(None);
    };
    // Copy-then-delete rather than rename: rename fails across volumes, and
    // deleting only after a verified copy means an interrupted move loses
    // nothing.
    let copied = std::fs::metadata(&destination).map(|m| m.len()).unwrap_or(0);
    let original = std::fs::metadata(item).map(|m| m.len()).unwrap_or(u64::MAX);
    if copied != original {
        return Err(OverlayError::other(
            "the copy did not match the original, so the original was kept",
        ));
    }
    std::fs::remove_file(item)?;
    Ok(Some(destination))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

pub struct DropzoneModule;

impl DropzoneModule {
    fn surfaces(ctx: &ModuleCtx) -> Result<Arc<SurfaceManager>> {
        Ok(ctx.app()?.state::<Arc<SurfaceManager>>().inner().clone())
    }
}

impl CustomizationModule for DropzoneModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "dropzone",
            name: "Dropzone",
            description:
                "A floating shelf for files in transit. Dropping a file references it; nothing \
                 is copied or moved until you say so.",
            version: "1.0.0",
            category: Category::Surfaces,
            requires: &[],
            conflicts_with: &[],
            touches_system: false,
        }
    }

    fn availability(&self, _ctx: &ModuleCtx) -> Availability {
        Availability::Available
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(DropzoneConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: DropzoneConfig = serde_json::from_value(config.clone())
            .map_err(|e| OverlayError::Config(format!("dropzone settings are invalid: {e}")))?;
        Ok(serde_json::to_value(cfg.normalize()?)?)
    }

    fn capture_original(&self, _ctx: &ModuleCtx) -> Result<()> {
        Ok(())
    }

    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: DropzoneConfig = serde_json::from_value(self.validate_config(config)?)?;
        let app = ctx.app()?;
        let (x, y) = crate::windows::clamp_to_visible(
            cfg.x,
            cfg.y,
            cfg.width,
            cfg.height,
            &ctx.env.monitors,
        );
        Self::surfaces(ctx)?.show(
            app,
            SurfaceSpec::new(LABEL, "dropzone")
                .at(x, y)
                .sized(cfg.width, cfg.height)
                // Normal, not always-on-top. The Dropzone is a panel you
                // work in, not a readout you glance at, and a 300×380
                // always-on-top window parked over whatever you are doing —
                // including OVERLAY's own window — swallows the clicks meant
                // for what is underneath.
                .z(ZMode::Normal),
        )
    }

    fn disable(&self, ctx: &ModuleCtx) -> Result<()> {
        let app = ctx.app()?;
        Self::surfaces(ctx)?.hide(app, LABEL)
    }

    fn is_active(&self, ctx: &ModuleCtx) -> bool {
        Self::surfaces(ctx).map(|s| s.is_open(LABEL)).unwrap_or(false)
    }

    fn notes(&self, _ctx: &ModuleCtx) -> Vec<String> {
        vec![
            "Clearing the shelf removes references only — your files stay exactly where they are."
                .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("overlay-dz-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_free_name_is_used_as_is() {
        let d = temp_dir();
        assert_eq!(non_colliding_path(&d, "a.txt").unwrap(), d.join("a.txt"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_taken_name_gets_a_counter_before_the_extension() {
        let d = temp_dir();
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        assert_eq!(non_colliding_path(&d, "a.txt").unwrap(), d.join("a (2).txt"));
        std::fs::write(d.join("a (2).txt"), b"x").unwrap();
        assert_eq!(non_colliding_path(&d, "a.txt").unwrap(), d.join("a (3).txt"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn extensionless_names_still_get_a_counter() {
        let d = temp_dir();
        std::fs::write(d.join("README"), b"x").unwrap();
        assert_eq!(
            non_colliding_path(&d, "README").unwrap(),
            d.join("README (2)")
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn copying_never_destroys_an_existing_file_by_default() {
        let src = temp_dir();
        let dst = temp_dir();
        std::fs::write(src.join("a.txt"), b"new").unwrap();
        std::fs::write(dst.join("a.txt"), b"precious").unwrap();

        copy_into(&src.join("a.txt"), &dst, Collision::Rename).unwrap();

        assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"precious");
        assert_eq!(std::fs::read(dst.join("a (2).txt")).unwrap(), b"new");
        std::fs::remove_dir_all(&src).ok();
        std::fs::remove_dir_all(&dst).ok();
    }

    #[test]
    fn skip_leaves_both_files_untouched() {
        let src = temp_dir();
        let dst = temp_dir();
        std::fs::write(src.join("a.txt"), b"new").unwrap();
        std::fs::write(dst.join("a.txt"), b"precious").unwrap();

        assert_eq!(
            copy_into(&src.join("a.txt"), &dst, Collision::Skip).unwrap(),
            None
        );
        assert_eq!(std::fs::read(dst.join("a.txt")).unwrap(), b"precious");
        assert!(src.join("a.txt").exists());
        std::fs::remove_dir_all(&src).ok();
        std::fs::remove_dir_all(&dst).ok();
    }

    #[test]
    fn moving_without_confirmation_is_refused_and_changes_nothing() {
        let src = temp_dir();
        let dst = temp_dir();
        let file = src.join("a.txt");
        std::fs::write(&file, b"data").unwrap();

        assert!(move_into(&file, &dst, Collision::Rename, false).is_err());
        assert!(file.exists(), "the source must survive a refused move");
        assert!(!dst.join("a.txt").exists());
        std::fs::remove_dir_all(&src).ok();
        std::fs::remove_dir_all(&dst).ok();
    }

    #[test]
    fn a_confirmed_move_relocates_the_file_intact() {
        let src = temp_dir();
        let dst = temp_dir();
        let file = src.join("a.txt");
        std::fs::write(&file, b"data").unwrap();

        let to = move_into(&file, &dst, Collision::Rename, true).unwrap().unwrap();
        assert!(!file.exists());
        assert_eq!(std::fs::read(&to).unwrap(), b"data");
        std::fs::remove_dir_all(&src).ok();
        std::fs::remove_dir_all(&dst).ok();
    }

    #[test]
    fn copying_a_file_onto_itself_does_not_truncate_it() {
        let d = temp_dir();
        let file = d.join("a.txt");
        std::fs::write(&file, b"data").unwrap();
        copy_into(&file, &d, Collision::Overwrite).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"data");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_missing_source_is_an_error_not_a_silent_no_op() {
        let d = temp_dir();
        assert!(copy_into(Path::new("Z:\\nope\\a.txt"), &d, Collision::Rename).is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn describing_a_missing_file_records_it_as_missing_rather_than_failing() {
        // Items can outlive the files they point at; the shelf must cope.
        let item = ShelfItem::describe(Path::new("Z:\\nope\\gone.txt"));
        assert!(!item.exists);
        assert_eq!(item.name, "gone.txt");
        assert_eq!(item.extension, "txt");
    }

    #[test]
    fn dropzone_geometry_is_clamped_and_non_finite_values_rejected() {
        let cfg = DropzoneConfig {
            width: 5.0,
            height: 99_999.0,
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(cfg.width, 220.0);
        assert_eq!(cfg.height, 1_400.0);

        assert!(DropzoneConfig {
            x: f64::NAN,
            ..Default::default()
        }
        .normalize()
        .is_err());
    }

    #[test]
    fn the_default_collision_policy_is_the_one_that_loses_nothing() {
        assert_eq!(DropzoneConfig::default().collision, Collision::Rename);
    }
}
