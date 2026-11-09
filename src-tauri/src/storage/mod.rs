//! SQLite persistence: schema, migrations and the connection the rest of the
//! backend shares.
//!
//! Everything lives in one database under the app's local data directory, so a
//! backup is a file copy and a reset is a file delete. Profiles additionally
//! export to standalone JSON for portability.

pub mod paths;

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

/// Bumped whenever `migrate` gains a step. Stored in `user_version` so an
/// older build opening a newer database can say so instead of corrupting it.
pub const SCHEMA_VERSION: i64 = 1;

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    // WAL keeps the widget metric writes from blocking UI reads.
    let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Opens an in-memory database with the full schema. Used by the test suite so
/// journal and profile logic is exercised against real SQL rather than a mock.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Applies every migration step the database has not seen yet.
///
/// Steps are append-only and each is wrapped in its own transaction, so an
/// interrupted upgrade leaves the database at the last fully applied version
/// rather than halfway through one.
pub fn migrate(conn: &Connection) -> Result<()> {
    let from = user_version(conn)?;

    if from < 1 {
        conn.execute_batch(
            r#"
            BEGIN;

            -- Profiles. `builtin` rows ship with OVERLAY and cannot be deleted;
            -- `original_windows` is the one captured from this machine.
            CREATE TABLE IF NOT EXISTS profiles (
                id            TEXT PRIMARY KEY,
                name          TEXT NOT NULL,
                description   TEXT NOT NULL DEFAULT '',
                builtin       INTEGER NOT NULL DEFAULT 0,
                theme         TEXT NOT NULL DEFAULT 'minimal',
                accent        TEXT NOT NULL DEFAULT 'slate',
                created_at    INTEGER NOT NULL,
                updated_at    INTEGER NOT NULL
            );

            -- One row per module per profile: whether it is on, and its config.
            CREATE TABLE IF NOT EXISTS profile_modules (
                profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
                module_id  TEXT NOT NULL,
                enabled    INTEGER NOT NULL DEFAULT 0,
                config     TEXT NOT NULL DEFAULT '{}',
                PRIMARY KEY (profile_id, module_id)
            );

            -- Exactly one row, id = 1. Holds which profile is live and whether
            -- a switch is in flight (see `switch_state`).
            CREATE TABLE IF NOT EXISTS active_profile (
                id           INTEGER PRIMARY KEY CHECK (id = 1),
                profile_id   TEXT,
                switch_state TEXT NOT NULL DEFAULT 'idle',
                switch_from  TEXT,
                switch_to    TEXT,
                switch_began INTEGER
            );
            INSERT OR IGNORE INTO active_profile (id, profile_id) VALUES (1, NULL);

            -- The original value of every setting OVERLAY has ever touched.
            -- Written once, on first capture, and never updated afterwards:
            -- this is the only record of what the machine looked like before.
            CREATE TABLE IF NOT EXISTS original_state (
                target_id   TEXT PRIMARY KEY,
                module_id   TEXT NOT NULL,
                kind        TEXT NOT NULL,
                payload     TEXT NOT NULL,
                human       TEXT NOT NULL DEFAULT '',
                captured_at INTEGER NOT NULL
            );

            -- The change journal. One row per attempted system modification,
            -- written *before* the change and completed after verification, so
            -- a crash leaves a row in `pending` that recovery can find.
            CREATE TABLE IF NOT EXISTS applied_changes (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                target_id   TEXT NOT NULL,
                module_id   TEXT NOT NULL,
                kind        TEXT NOT NULL,
                before_val  TEXT NOT NULL,
                after_val   TEXT NOT NULL,
                status      TEXT NOT NULL,
                detail      TEXT NOT NULL DEFAULT '',
                started_at  INTEGER NOT NULL,
                settled_at  INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_changes_target ON applied_changes(target_id);
            CREATE INDEX IF NOT EXISTS idx_changes_status ON applied_changes(status);

            -- What OVERLAY currently believes it owns. A row here means "we
            -- changed this and have not put it back". Restoration works from
            -- this table, so it can never touch a setting we did not modify.
            CREATE TABLE IF NOT EXISTS owned_settings (
                target_id  TEXT PRIMARY KEY,
                module_id  TEXT NOT NULL,
                kind       TEXT NOT NULL,
                applied    TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS restoration_history (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                scope      TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                ended_at   INTEGER,
                outcome    TEXT NOT NULL,
                report     TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS integration_registry (
                id          TEXT PRIMARY KEY,
                exe_path    TEXT,
                data_dir    TEXT,
                version     TEXT,
                last_seen   INTEGER,
                notes       TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Widget placement is per widget per monitor, kept out of the
            -- profile blob so dragging a widget is a cheap single-row write.
            CREATE TABLE IF NOT EXISTS widget_layout (
                profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
                widget_id  TEXT NOT NULL,
                monitor    TEXT NOT NULL DEFAULT '',
                x          INTEGER NOT NULL DEFAULT 80,
                y          INTEGER NOT NULL DEFAULT 80,
                width      INTEGER NOT NULL DEFAULT 320,
                height     INTEGER NOT NULL DEFAULT 160,
                locked     INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (profile_id, widget_id)
            );

            PRAGMA user_version = 1;
            COMMIT;
            "#,
        )?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Small key/value helpers for app-level settings.
// ---------------------------------------------------------------------------

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_the_schema_and_stamps_the_version() {
        let conn = open_in_memory().unwrap();
        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);

        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type = 'table' AND name IN
                   ('profiles','profile_modules','active_profile','original_state',
                    'applied_changes','owned_settings','restoration_history',
                    'integration_registry','app_settings','widget_layout')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 10);
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn active_profile_is_a_singleton_row() {
        let conn = open_in_memory().unwrap();
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM active_profile", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        // The CHECK constraint is what stops a second "active profile".
        assert!(conn
            .execute("INSERT INTO active_profile (id, profile_id) VALUES (2, 'x')", [])
            .is_err());
    }

    #[test]
    fn settings_round_trip() {
        let conn = open_in_memory().unwrap();
        assert_eq!(get_setting(&conn, "theme").unwrap(), None);
        set_setting(&conn, "theme", "futuristic").unwrap();
        set_setting(&conn, "theme", "minimal").unwrap();
        assert_eq!(get_setting(&conn, "theme").unwrap().unwrap(), "minimal");
    }

    #[test]
    fn deleting_a_profile_takes_its_module_rows_with_it() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO profiles (id,name,created_at,updated_at) VALUES ('p','P',0,0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO profile_modules (profile_id,module_id,enabled) VALUES ('p','taskbar',1)",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM profiles WHERE id = 'p'", []).unwrap();
        let left: i64 = conn
            .query_row("SELECT count(*) FROM profile_modules", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "cascade should have removed the module rows");
    }
}
