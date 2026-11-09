//! The change journal: the single gate every system modification passes
//! through, and the only thing restoration needs in order to undo them.
//!
//! # The contract
//!
//! For each change, in this order:
//!
//! 1. **Capture** the original value, once and forever, before the first
//!    write. Later changes to the same target never overwrite that record.
//! 2. **Check ownership.** If OVERLAY already owns the target but the live
//!    value is not what OVERLAY last wrote, something else changed it; that is
//!    reported as a conflict instead of being silently steamrolled.
//! 3. **Write a `pending` row** describing the intended change *before*
//!    touching Windows.
//! 4. **Apply** the change.
//! 5. **Verify** by reading back.
//! 6. **Settle** the row to `applied` or `failed`, and record ownership.
//!
//! Step 3 is what makes a crash recoverable: a process killed between steps 3
//! and 6 leaves a `pending` row naming exactly one target, and
//! [`Journal::unsettled`] finds it on the next start.
//!
//! # What restoration may touch
//!
//! Only rows in `owned_settings`. A setting OVERLAY never changed has no row,
//! so there is no code path by which restore can reset an unrelated Windows
//! preference — that is enforced by the data model, not by discipline.

pub mod target;

use crate::error::{OverlayError, Result};
use crate::storage::now;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub use target::{SettingTarget, SettingValue};

/// Lifecycle of one journal row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    /// Written before the change was attempted. Seeing this at startup means
    /// the process died mid-change.
    Pending,
    Applied,
    Failed,
    /// The original value was put back and verified.
    Reverted,
}

impl ChangeStatus {
    fn as_str(self) -> &'static str {
        match self {
            ChangeStatus::Pending => "pending",
            ChangeStatus::Applied => "applied",
            ChangeStatus::Failed => "failed",
            ChangeStatus::Reverted => "reverted",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "applied" => ChangeStatus::Applied,
            "failed" => ChangeStatus::Failed,
            "reverted" => ChangeStatus::Reverted,
            _ => ChangeStatus::Pending,
        }
    }
}

/// A journal row as the UI sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub id: i64,
    pub target_id: String,
    pub module_id: String,
    pub label: String,
    pub before: String,
    pub after: String,
    pub status: ChangeStatus,
    pub detail: String,
    pub started_at: i64,
    pub settled_at: Option<i64>,
}

/// An interrupted change found at startup, after it has been reconciled
/// against what the machine actually looks like.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRow {
    pub id: i64,
    pub target_id: String,
    pub module_id: String,
}

/// A setting OVERLAY has changed and not yet put back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedSetting {
    pub target: SettingTarget,
    pub module_id: String,
    /// What OVERLAY last wrote. Drift from this is what flags a conflict.
    pub applied: SettingValue,
    pub applied_at: i64,
}

/// What happened to one target during a restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOutcome {
    pub target_id: String,
    pub label: String,
    pub module_id: String,
    pub restored_to: String,
    pub status: RestoreStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreStatus {
    /// Put back and verified.
    Restored,
    /// Already held its original value; nothing to do.
    AlreadyOriginal,
    /// Someone changed it after OVERLAY did. Left alone and reported, because
    /// overwriting would silently discard the user's own later choice.
    ConflictSkipped,
    Failed,
}

/// Shared handle to the journal. Cheap to clone; the mutex is held only for
/// the duration of a statement.
#[derive(Clone)]
pub struct Journal {
    conn: Arc<Mutex<Connection>>,
    /// Where to mirror captured originals as plain JSON. `None` in tests and
    /// in-memory use.
    snapshot: Option<std::path::PathBuf>,
}

impl Journal {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            conn,
            snapshot: None,
        }
    }

    /// Mirrors every captured original to a plain-JSON file as well as the
    /// database.
    ///
    /// The database is the working record, but it is a single binary file:
    /// if it is deleted or corrupted, the only knowledge of what the desktop
    /// looked like before OVERLAY touched it goes with it, and the user is
    /// left with changes nothing can undo. This file is redundant on purpose
    /// — it is readable, diffable, and enough for a person to put things
    /// back by hand.
    pub fn with_snapshot(mut self, path: std::path::PathBuf) -> Self {
        self.snapshot = Some(path);
        self
    }

    // -----------------------------------------------------------------
    // Capture
    // -----------------------------------------------------------------

    /// Records a target's original value if it has never been recorded.
    ///
    /// `INSERT OR IGNORE` is the whole safety property: the first capture
    /// wins, so enabling a module twice, or switching profiles ten times,
    /// cannot overwrite the pre-OVERLAY truth with an OVERLAY-written value.
    pub fn capture_original(&self, module_id: &str, target: &SettingTarget) -> Result<SettingValue> {
        if let Some(existing) = self.original_of(target)? {
            return Ok(existing);
        }
        let current = target.read()?;
        {
            let conn = self.conn.lock();
            conn.execute(
                "INSERT OR IGNORE INTO original_state
                    (target_id, module_id, kind, payload, human, captured_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    target.id(),
                    module_id,
                    target.kind(),
                    serde_json::to_string(&current)?,
                    current.describe(),
                    now(),
                ],
            )?;
        }
        // Best effort: a snapshot that cannot be written must never block a
        // capture, because failing here would mean refusing to record the
        // original at all.
        if let Err(e) = self.write_snapshot() {
            log::warn!("could not update the original-state snapshot: {e}");
        }
        Ok(current)
    }

    /// Rewrites the plain-JSON mirror of every captured original.
    fn write_snapshot(&self) -> Result<()> {
        let Some(path) = &self.snapshot else {
            return Ok(());
        };

        #[derive(Serialize)]
        struct Entry {
            target: String,
            description: String,
            original_value: String,
            module: String,
            captured_at: String,
            raw: serde_json::Value,
        }

        let conn = self.conn.lock();
        let rows: Vec<Entry> = {
            let mut stmt = conn.prepare(
                "SELECT target_id, module_id, payload, human, captured_at
                 FROM original_state ORDER BY captured_at",
            )?;
            let mapped = stmt.query_map([], |r| {
                let target_id: String = r.get(0)?;
                let payload: String = r.get(2)?;
                let captured: i64 = r.get(4)?;
                Ok(Entry {
                    description: decode_target_id(&target_id)
                        .map(|t| t.label())
                        .unwrap_or_else(|| target_id.clone()),
                    target: target_id,
                    original_value: r.get(3)?,
                    module: r.get(1)?,
                    captured_at: chrono::DateTime::from_timestamp(captured, 0)
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default(),
                    raw: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
                })
            })?;
            mapped.collect::<rusqlite::Result<Vec<_>>>()?
        };
        drop(conn);

        let document = serde_json::json!({
            "note": "What this machine's settings were before OVERLAY changed them. \
                     Written alongside OVERLAY's database so the original values \
                     survive even if that database is lost. OVERLAY restores from \
                     the database; this file is for you.",
            "overlay_version": env!("CARGO_PKG_VERSION"),
            "updated_at": chrono::Local::now().to_rfc3339(),
            "originals": rows,
        });

        // Written via a temporary file and renamed, so a crash mid-write
        // cannot replace a good snapshot with a truncated one.
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_string_pretty(&document)?)?;
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    /// The originally captured value for a target, if one was ever captured.
    pub fn original_of(&self, target: &SettingTarget) -> Result<Option<SettingValue>> {
        let conn = self.conn.lock();
        let raw: Option<String> = conn
            .query_row(
                "SELECT payload FROM original_state WHERE target_id = ?1",
                [target.id()],
                |r| r.get(0),
            )
            .optional()?;
        match raw {
            None => Ok(None),
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        }
    }

    // -----------------------------------------------------------------
    // Apply
    // -----------------------------------------------------------------

    /// Applies one change under the full journal contract.
    ///
    /// Returns `Ok(())` only once the new value has been read back and
    /// confirmed, so a caller may treat success as "the desktop really changed".
    pub fn apply(
        &self,
        module_id: &str,
        target: &SettingTarget,
        desired: &SettingValue,
    ) -> Result<()> {
        let original = self.capture_original(module_id, target)?;
        let verifiable = target.verifiable();

        // Ownership check: if we own this and it has drifted from what we
        // wrote, someone else has had their say. Report rather than
        // overwrite. Skipped where the value cannot be read, since drift is
        // undetectable there and a false conflict would be worse than none.
        if verifiable {
            if let Some(owned) = self.owned_one(target)? {
                let live = target.read()?;
                if !target.matches(&owned.applied, &live) {
                    return Err(OverlayError::OwnershipConflict {
                        target: target.label(),
                        expected: owned.applied.describe(),
                        found: live.describe(),
                    });
                }
            }
        }

        let before = target.read()?;
        if verifiable && target.matches(desired, &before) {
            // Already there. Still record ownership, so restore knows to put
            // it back if the *original* differs from where it sits now.
            self.record_ownership(module_id, target, desired)?;
            return Ok(());
        }

        let row = self.begin_row(module_id, target, &before, desired)?;

        match target.write(desired) {
            Ok(()) => {}
            Err(e) => {
                self.settle_row(row, ChangeStatus::Failed, &e.to_string())?;
                return Err(e);
            }
        }

        if verifiable {
            let found = target.read()?;
            if !target.matches(desired, &found) {
                let err = OverlayError::VerificationFailed {
                    target: target.label(),
                    expected: desired.describe(),
                    found: found.describe(),
                };
                self.settle_row(row, ChangeStatus::Failed, &err.to_string())?;
                return Err(err);
            }
            self.settle_row(row, ChangeStatus::Applied, "")?;
        } else {
            self.settle_row(
                row,
                ChangeStatus::Applied,
                "Windows accepted the change; this setting cannot be read back to confirm it",
            )?;
        }

        self.record_ownership(module_id, target, desired)?;
        let _ = original; // captured above; kept for clarity of the contract
        Ok(())
    }

    fn begin_row(
        &self,
        module_id: &str,
        target: &SettingTarget,
        before: &SettingValue,
        after: &SettingValue,
    ) -> Result<i64> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO applied_changes
                (target_id, module_id, kind, before_val, after_val, status, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)",
            params![
                target.id(),
                module_id,
                target.kind(),
                serde_json::to_string(before)?,
                serde_json::to_string(after)?,
                now(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Writes the `pending` row that precedes a change, *without* performing
    /// the change.
    ///
    /// Exposed only so the crash-recovery tests can reproduce a process that
    /// died between journalling its intent and finishing — the one state that
    /// cannot be reached through the normal API, and the one recovery exists
    /// to handle.
    #[doc(hidden)]
    pub fn begin_row_for_test(
        &self,
        module_id: &str,
        target: &SettingTarget,
        before: &SettingValue,
        after: &SettingValue,
    ) -> Result<i64> {
        self.begin_row(module_id, target, before, after)
    }

    fn settle_row(&self, id: i64, status: ChangeStatus, detail: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE applied_changes SET status = ?2, detail = ?3, settled_at = ?4 WHERE id = ?1",
            params![id, status.as_str(), detail, now()],
        )?;
        Ok(())
    }

    fn record_ownership(
        &self,
        module_id: &str,
        target: &SettingTarget,
        applied: &SettingValue,
    ) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO owned_settings (target_id, module_id, kind, applied, applied_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(target_id) DO UPDATE SET
                module_id = excluded.module_id,
                applied = excluded.applied,
                applied_at = excluded.applied_at",
            params![
                target.id(),
                module_id,
                target.kind(),
                serde_json::to_string(applied)?,
                now(),
            ],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Ownership
    // -----------------------------------------------------------------

    pub fn owned_one(&self, target: &SettingTarget) -> Result<Option<OwnedSetting>> {
        let all = self.owned()?;
        let id = target.id();
        Ok(all.into_iter().find(|o| o.target.id() == id))
    }

    /// Every setting OVERLAY currently owns. The complete list of what a
    /// restore is allowed to touch.
    pub fn owned(&self) -> Result<Vec<OwnedSetting>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT o.target_id, o.module_id, o.applied, o.applied_at, s.payload
             FROM owned_settings o
             LEFT JOIN original_state s ON s.target_id = o.target_id
             ORDER BY o.applied_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (target_id, module_id, applied, applied_at) = row?;
            // The target itself is reconstructed from the original_state row's
            // sibling column; see `target_for`.
            let Some(target) = Self::target_for(&conn, &target_id)? else {
                continue;
            };
            out.push(OwnedSetting {
                target,
                module_id,
                applied: serde_json::from_str(&applied)?,
                applied_at,
            });
        }
        Ok(out)
    }

    /// Rebuilds a [`SettingTarget`] from its stored id.
    ///
    /// Ids are reversible by construction, so no extra column is needed and an
    /// owned row can never dangle because of a missing join.
    fn target_for(_conn: &Connection, target_id: &str) -> Result<Option<SettingTarget>> {
        Ok(decode_target_id(target_id))
    }

    // -----------------------------------------------------------------
    // Restore
    // -----------------------------------------------------------------

    /// Puts one owned setting back to its captured original and gives up
    /// ownership. Never touches a target OVERLAY does not own.
    pub fn restore_one(&self, target: &SettingTarget) -> Result<RestoreOutcome> {
        let label = target.label();
        let id = target.id();

        let Some(owned) = self.owned_one(target)? else {
            return Ok(RestoreOutcome {
                target_id: id,
                label,
                module_id: String::new(),
                restored_to: String::new(),
                status: RestoreStatus::AlreadyOriginal,
                detail: "OVERLAY does not own this setting".into(),
            });
        };

        let Some(original) = self.original_of(target)? else {
            // Cannot happen through `apply`, which always captures first.
            return Ok(RestoreOutcome {
                target_id: id,
                label,
                module_id: owned.module_id,
                restored_to: String::new(),
                status: RestoreStatus::Failed,
                detail: "no original value was recorded for this setting".into(),
            });
        };

        let live = match target.read() {
            Ok(v) => v,
            Err(e) => {
                return Ok(RestoreOutcome {
                    target_id: id,
                    label,
                    module_id: owned.module_id,
                    restored_to: original.describe(),
                    status: RestoreStatus::Failed,
                    detail: format!("could not read the current value: {e}"),
                })
            }
        };

        let verifiable = target.verifiable();

        // Someone changed it behind us: their value stands, and we say so.
        // Drift cannot be detected on a target that does not read back, so
        // that check is skipped there rather than guessed at.
        if verifiable
            && !target.matches(&owned.applied, &live)
            && !target.matches(&original, &live)
        {
            return Ok(RestoreOutcome {
                target_id: id,
                label,
                module_id: owned.module_id,
                restored_to: original.describe(),
                status: RestoreStatus::ConflictSkipped,
                detail: format!(
                    "changed outside OVERLAY since we set it (we wrote {}, found {}); left as it is",
                    owned.applied.describe(),
                    live.describe()
                ),
            });
        }

        // An unverifiable target is always written back: "it already looks
        // original" is not something we can know about it, and re-applying
        // the captured policy is harmless.
        if verifiable && target.matches(&original, &live) {
            self.release_ownership(&id)?;
            return Ok(RestoreOutcome {
                target_id: id,
                label,
                module_id: owned.module_id,
                restored_to: original.describe(),
                status: RestoreStatus::AlreadyOriginal,
                detail: String::new(),
            });
        }

        let row = self.begin_row(&owned.module_id, target, &live, &original)?;
        if let Err(e) = target.write(&original) {
            self.settle_row(row, ChangeStatus::Failed, &e.to_string())?;
            return Ok(RestoreOutcome {
                target_id: id,
                label,
                module_id: owned.module_id,
                restored_to: original.describe(),
                status: RestoreStatus::Failed,
                detail: e.to_string(),
            });
        }

        if verifiable {
            let found = target.read()?;
            if !target.matches(&original, &found) {
                let detail = format!(
                    "wrote {} but found {} afterwards",
                    original.describe(),
                    found.describe()
                );
                self.settle_row(row, ChangeStatus::Failed, &detail)?;
                return Ok(RestoreOutcome {
                    target_id: id,
                    label,
                    module_id: owned.module_id,
                    restored_to: original.describe(),
                    status: RestoreStatus::Failed,
                    detail,
                });
            }
        }

        self.settle_row(row, ChangeStatus::Reverted, "")?;
        self.release_ownership(&id)?;
        Ok(RestoreOutcome {
            target_id: id,
            label,
            module_id: owned.module_id,
            restored_to: original.describe(),
            status: RestoreStatus::Restored,
            detail: String::new(),
        })
    }

    /// Restores every setting a module owns.
    pub fn restore_module(&self, module_id: &str) -> Result<Vec<RestoreOutcome>> {
        let targets: Vec<SettingTarget> = self
            .owned()?
            .into_iter()
            .filter(|o| o.module_id == module_id)
            .map(|o| o.target)
            .collect();
        targets.iter().map(|t| self.restore_one(t)).collect()
    }

    /// Restores everything OVERLAY owns — the "Restore Original Desktop"
    /// action. Keeps going after an individual failure so one stubborn
    /// setting cannot strand the rest.
    pub fn restore_all(&self) -> Result<Vec<RestoreOutcome>> {
        let targets: Vec<SettingTarget> = self.owned()?.into_iter().map(|o| o.target).collect();
        let mut out = Vec::with_capacity(targets.len());
        for t in &targets {
            out.push(match self.restore_one(t) {
                Ok(o) => o,
                Err(e) => RestoreOutcome {
                    target_id: t.id(),
                    label: t.label(),
                    module_id: String::new(),
                    restored_to: String::new(),
                    status: RestoreStatus::Failed,
                    detail: e.to_string(),
                },
            });
        }
        Ok(out)
    }

    fn release_ownership(&self, target_id: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM owned_settings WHERE target_id = ?1", [target_id])?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Crash recovery
    // -----------------------------------------------------------------

    /// Rows left `pending` — changes that were started but never settled,
    /// i.e. the process died mid-write. Formatted for display.
    pub fn unsettled(&self) -> Result<Vec<ChangeRecord>> {
        self.records_where("status = 'pending'", 200)
    }

    /// The same rows, with `after_val` left as stored JSON so it can be
    /// parsed back into a [`SettingValue`].
    fn unsettled_raw(&self) -> Result<Vec<(i64, String, String, String)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, target_id, module_id, after_val
             FROM applied_changes WHERE status = 'pending' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Reconciles interrupted changes against reality.
    ///
    /// For each `pending` row the live value is read and the row is settled to
    /// what actually happened — `applied` if the write landed before the
    /// crash, `failed` if it did not. Ownership is recorded for the ones that
    /// landed, so a change interrupted at the worst possible moment is still
    /// restorable afterwards.
    pub fn reconcile_unsettled(&self) -> Result<Vec<PendingRow>> {
        // Deliberately *not* `unsettled()`: that renders values for display,
        // and reconciliation needs the stored JSON to parse back into a
        // `SettingValue`. Reading the formatted text here would make every
        // interrupted change look unrecoverable.
        let pending = self.unsettled_raw()?;
        let mut settled = Vec::new();

        for (id, target_id, module_id, after_json) in pending {
            let Some(target) = decode_target_id(&target_id) else {
                self.settle_row(id, ChangeStatus::Failed, "unrecognised target")?;
                continue;
            };
            let intended: SettingValue = match serde_json::from_str(&after_json) {
                Ok(v) => v,
                Err(_) => {
                    self.settle_row(id, ChangeStatus::Failed, "unreadable intended value")?;
                    continue;
                }
            };
            let rec = PendingRow {
                id,
                target_id,
                module_id,
            };
            let live = match target.read() {
                Ok(v) => v,
                Err(e) => {
                    self.settle_row(rec.id, ChangeStatus::Failed, &e.to_string())?;
                    continue;
                }
            };

            if target.matches(&intended, &live) {
                self.settle_row(
                    rec.id,
                    ChangeStatus::Applied,
                    "completed before shutdown; confirmed on restart",
                )?;
                self.record_ownership(&rec.module_id, &target, &intended)?;
            } else {
                self.settle_row(
                    rec.id,
                    ChangeStatus::Failed,
                    "interrupted before it took effect",
                )?;
            }
            settled.push(rec);
        }
        Ok(settled)
    }

    // -----------------------------------------------------------------
    // Reads for the UI
    // -----------------------------------------------------------------

    pub fn recent(&self, limit: usize) -> Result<Vec<ChangeRecord>> {
        self.records_where("1 = 1", limit)
    }

    fn records_where(&self, predicate: &str, limit: usize) -> Result<Vec<ChangeRecord>> {
        let conn = self.conn.lock();
        let sql = format!(
            "SELECT id, target_id, module_id, before_val, after_val, status, detail,
                    started_at, settled_at
             FROM applied_changes WHERE {predicate}
             ORDER BY id DESC LIMIT {limit}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |r| {
            Ok(ChangeRecord {
                id: r.get(0)?,
                target_id: r.get(1)?,
                module_id: r.get(2)?,
                label: String::new(),
                before: r.get(3)?,
                after: r.get(4)?,
                status: ChangeStatus::parse(&r.get::<_, String>(5)?),
                detail: r.get(6)?,
                started_at: r.get(7)?,
                settled_at: r.get(8)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            let mut rec = row?;
            // Render the stored JSON into something readable, and label the
            // row from its target id.
            rec.label = decode_target_id(&rec.target_id)
                .map(|t| t.label())
                .unwrap_or_else(|| rec.target_id.clone());
            rec.before = describe_json(&rec.before);
            rec.after = describe_json(&rec.after);
            out.push(rec);
        }
        Ok(out)
    }
}

fn describe_json(raw: &str) -> String {
    serde_json::from_str::<SettingValue>(raw)
        .map(|v| v.describe())
        .unwrap_or_else(|_| raw.to_string())
}

/// Inverse of [`SettingTarget::id`].
///
/// Keeping ids reversible means the journal never has to store a second copy
/// of the target, and recovery can reconstruct what to undo from the primary
/// key alone.
pub fn decode_target_id(id: &str) -> Option<SettingTarget> {
    use crate::sys::registry::{Hive, RegTarget};

    if let Some(rest) = id.strip_prefix("reg:") {
        // "HKCU\<path...>\<name>" — the name is the final segment.
        let (hive, remainder) = rest.split_once('\\')?;
        let hive = match hive {
            "HKCU" => Hive::CurrentUser,
            "HKLM" => Hive::LocalMachine,
            _ => return None,
        };
        let split = remainder.rfind('\\')?;
        let (path, name) = remainder.split_at(split);
        return Some(SettingTarget::Registry(RegTarget {
            hive,
            path: path.to_string(),
            name: name[1..].to_string(),
        }));
    }
    match id {
        "shell:desktop-icons" => Some(SettingTarget::DesktopIcons),
        "shell:taskbar-accent" => Some(SettingTarget::TaskbarAccent),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::registry::RegTarget;

    #[test]
    fn every_target_id_decodes_back_to_itself() {
        // Recovery depends on this being lossless: the id in the database is
        // the only description of what to undo.
        let targets = [
            SettingTarget::Registry(RegTarget::hkcu(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
                "TaskbarAl",
            )),
            SettingTarget::Registry(RegTarget::hkcu(r"Control Panel\Desktop", "WallPaper")),
            SettingTarget::DesktopIcons,
            SettingTarget::TaskbarAccent,
        ];
        for t in targets {
            let decoded = decode_target_id(&t.id());
            assert_eq!(decoded.as_ref(), Some(&t), "failed to decode {}", t.id());
        }
    }

    #[test]
    fn an_unknown_target_id_decodes_to_nothing_rather_than_guessing() {
        assert!(decode_target_id("nonsense").is_none());
        assert!(decode_target_id("reg:HKXX\\Path\\Name").is_none());
        assert!(decode_target_id("shell:not-a-thing").is_none());
    }
}
