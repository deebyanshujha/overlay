//! The application state every command reaches through.
//!
//! One place that owns the database connection, the journal, the profile
//! store, the module registry and the detected environment — so a command is
//! a thin function over already-constructed services rather than wiring its
//! own.

use crate::error::Result;
use crate::journal::Journal;
use crate::modules::{ModuleCtx, Registry};
use crate::profiles::{switch::SwitchEngine, ProfileStore};
use crate::restoration::RestorationEngine;
use crate::storage::{self, paths::Paths};
use crate::sys::Environment;
use crate::windows::SurfaceManager;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::sync::Arc;

pub struct AppState {
    pub paths: Paths,
    pub conn: Arc<Mutex<Connection>>,
    pub journal: Journal,
    pub profiles: ProfileStore,
    pub switcher: SwitchEngine,
    pub restoration: RestorationEngine,
    pub registry: Registry,
    pub surfaces: Arc<SurfaceManager>,
    /// Detected once at startup; refreshed when displays change.
    pub env: Mutex<Arc<Environment>>,
}

impl AppState {
    pub fn new(paths: Paths, surfaces: Arc<SurfaceManager>) -> Result<Self> {
        let conn = Arc::new(Mutex::new(storage::open(&paths.db)?));
        let journal =
            Journal::new(conn.clone()).with_snapshot(paths.original_snapshot.clone());
        let profiles = ProfileStore::new(conn.clone());
        let registry = Registry::new();
        profiles.ensure_builtins(&registry)?;

        let switcher = SwitchEngine::new(conn.clone(), profiles.clone(), journal.clone());
        let restoration = RestorationEngine::new(
            conn.clone(),
            journal.clone(),
            profiles.clone(),
            paths.clone(),
        );

        Ok(Self {
            env: Mutex::new(Arc::new(crate::sys::environment())),
            paths,
            conn,
            journal,
            profiles,
            switcher,
            restoration,
            registry,
            surfaces,
        })
    }

    pub fn env(&self) -> Arc<Environment> {
        self.env.lock().clone()
    }

    /// Re-detects displays and shell state. Called when Windows reports a
    /// display change, so widgets clamp against the real layout rather than
    /// the one present at startup.
    pub fn refresh_env(&self) -> Arc<Environment> {
        let fresh = Arc::new(crate::sys::environment());
        *self.env.lock() = fresh.clone();
        fresh
    }

    pub fn ctx(&self, app: &tauri::AppHandle) -> ModuleCtx {
        ModuleCtx {
            journal: self.journal.clone(),
            env: self.env(),
            app: Some(app.clone()),
        }
    }

    /// A context with no Tauri handle, for work that must run without a UI.
    pub fn headless_ctx(&self) -> ModuleCtx {
        ModuleCtx::headless(self.journal.clone(), self.env())
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock();
        storage::get_setting(&conn, key).ok().flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock();
        storage::set_setting(&conn, key, value)
    }

    pub fn flag(&self, key: &str, default: bool) -> bool {
        match self.setting(key).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        }
    }

    pub fn set_flag(&self, key: &str, value: bool) -> Result<()> {
        self.set_setting(key, if value { "true" } else { "false" })
    }
}
