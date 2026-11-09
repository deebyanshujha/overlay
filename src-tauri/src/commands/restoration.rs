//! Restoration commands — the "Restore Original Desktop" path.

use crate::error::Result;
use crate::journal::{ChangeRecord, RestoreOutcome};
use crate::restoration::RestorationReport;
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Debug, Serialize)]
pub struct RestorePreview {
    /// Exactly what would change, read live.
    pub settings: Vec<RestoreOutcome>,
    /// OVERLAY windows that would be closed.
    pub surfaces: Vec<String>,
    /// Plain-language statements of what will *not* happen, shown in the
    /// confirmation dialog because they are the user's real worry.
    pub guarantees: Vec<String>,
}

/// What a full restore would do. Changes nothing.
#[tauri::command(async)]
pub fn preview_restore(state: State<'_, AppState>) -> Result<RestorePreview> {
    Ok(RestorePreview {
        settings: state.restoration.preview()?,
        surfaces: state.surfaces.open_labels(),
        guarantees: vec![
            "No desktop shortcut, file or folder is touched.".into(),
            "WallFlow and QuickDock keep running, with their settings unchanged.".into(),
            "Only settings OVERLAY itself changed are restored — nothing else is reset.".into(),
            "A setting you changed yourself after OVERLAY did is left as you set it."
                .into(),
        ],
    })
}

/// Restores the original desktop. The user has confirmed by the time this
/// runs; the dialog is the frontend's job, the verification is ours.
#[tauri::command(async)]
pub fn restore_original_desktop(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RestorationReport> {
    let ctx = state.ctx(&app);
    state.restoration.restore_all(&state.registry, &ctx)
}

#[tauri::command(async)]
pub fn restore_module(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: String,
) -> Result<RestorationReport> {
    let ctx = state.ctx(&app);
    state
        .restoration
        .restore_module(&module_id, &state.registry, &ctx)
}

#[tauri::command(async)]
pub fn restoration_history(state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<RestorationReport>> {
    state.restoration.history(limit.unwrap_or(20))
}

/// The change journal, newest first — the audit trail of every system change.
#[tauri::command(async)]
pub fn change_journal(state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<ChangeRecord>> {
    state.journal.recent(limit.unwrap_or(200))
}

#[derive(Debug, Serialize)]
pub struct OwnedSummary {
    pub target_id: String,
    pub label: String,
    pub module_id: String,
    pub applied: String,
    pub original: String,
    pub applied_at: i64,
}

/// Everything OVERLAY currently owns, with its original value beside it.
#[tauri::command(async)]
pub fn owned_settings(state: State<'_, AppState>) -> Result<Vec<OwnedSummary>> {
    let owned = state.journal.owned()?;
    let mut out = Vec::with_capacity(owned.len());
    for setting in owned {
        let original = state
            .journal
            .original_of(&setting.target)?
            .map(|v| v.describe())
            .unwrap_or_else(|| "(not recorded)".into());
        out.push(OwnedSummary {
            target_id: setting.target.id(),
            label: setting.target.label(),
            module_id: setting.module_id,
            applied: setting.applied.describe(),
            original,
            applied_at: setting.applied_at,
        });
    }
    Ok(out)
}

/// Settles any change interrupted by a crash, on demand from the Recovery
/// page. Startup does this automatically; this is for the user who wants to
/// see it happen.
#[tauri::command(async)]
pub fn reconcile_interrupted(state: State<'_, AppState>) -> Result<usize> {
    Ok(state.journal.reconcile_unsettled()?.len())
}

#[derive(Debug, Serialize)]
pub struct RecoveryStatus {
    pub unsettled_changes: usize,
    pub switch_in_flight: bool,
    pub owned_settings: usize,
    pub database: String,
    pub backups: Vec<BackupFile>,
    /// The exact command to run if OVERLAY's window will not open at all.
    pub recovery_command: String,
}

#[derive(Debug, Serialize)]
pub struct BackupFile {
    pub name: String,
    pub bytes: u64,
    pub modified: i64,
}

#[tauri::command(async)]
pub fn recovery_status(state: State<'_, AppState>) -> Result<RecoveryStatus> {
    let mut backups: Vec<BackupFile> = std::fs::read_dir(&state.paths.backups)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some(BackupFile {
                name: e.file_name().to_string_lossy().to_string(),
                bytes: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            })
        })
        .collect();
    backups.sort_by(|a, b| b.modified.cmp(&a.modified));
    backups.truncate(20);

    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "overlay.exe".into());

    Ok(RecoveryStatus {
        unsettled_changes: state.journal.unsettled().map(|u| u.len()).unwrap_or(0),
        switch_in_flight: matches!(
            state.switcher.state(),
            Ok(crate::profiles::switch::SwitchState::InFlight)
        ),
        owned_settings: state.journal.owned().map(|o| o.len()).unwrap_or(0),
        database: state.paths.db.to_string_lossy().to_string(),
        backups,
        recovery_command: format!("\"{exe}\" --restore-original"),
    })
}
