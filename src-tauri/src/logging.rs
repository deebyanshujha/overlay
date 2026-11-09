//! A small file logger.
//!
//! OVERLAY runs without a console in release, so `log::warn!` would otherwise
//! go nowhere — and the things it reports (a shortcut that would not bind, a
//! module that failed to apply, a backup that could not be written) are
//! exactly what someone needs when something is not behaving.
//!
//! Deliberately not a logging framework: one file, appended, rotated when it
//! gets large, mirrored to stderr in debug builds. Diagnostics are a feature
//! of the product, not a dependency.

use crate::storage::paths::Paths;
use log::{Level, LevelFilter, Metadata, Record};
use parking_lot::Mutex;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

/// Rotate at 1 MB. Large enough to hold a long session, small enough that a
/// user can open it.
const MAX_BYTES: u64 = 1024 * 1024;

struct FileLogger {
    path: PathBuf,
    file: Mutex<Option<std::fs::File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Our own crate at debug; dependencies only when they warn, so a
        // chatty library cannot drown the log.
        if metadata.target().starts_with("overlay") {
            metadata.level() <= Level::Debug
        } else {
            metadata.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} [{}] {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.target(),
            record.args()
        );

        if cfg!(debug_assertions) {
            eprint!("{line}");
        }

        let mut guard = self.file.lock();
        if guard.is_none() {
            *guard = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .ok();
        }
        if let Some(file) = guard.as_mut() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }

    fn flush(&self) {
        if let Some(file) = self.file.lock().as_mut() {
            let _ = file.flush();
        }
    }
}

/// Installs the logger. Safe to call more than once; later calls are ignored.
pub fn init(paths: &Paths) {
    rotate(&paths.log);

    let logger = FileLogger {
        path: paths.log.clone(),
        file: Mutex::new(None),
    };
    // A failure here means a logger was already installed, which is fine.
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(LevelFilter::Debug);
        log::info!(
            "OVERLAY {} starting; data folder {}",
            env!("CARGO_PKG_VERSION"),
            paths.root.display()
        );
    }
}

/// Keeps one previous log alongside the current one, so the file from the
/// run *before* a problem is still there to read.
fn rotate(path: &PathBuf) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() < MAX_BYTES {
        return;
    }
    let previous = path.with_extension("log.1");
    let _ = std::fs::remove_file(&previous);
    let _ = std::fs::rename(path, &previous);
}
