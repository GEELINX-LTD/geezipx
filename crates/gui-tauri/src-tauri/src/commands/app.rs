//! `get_opened_archives` command — retrieve file paths received via
//! file association / open-with.
//!
//! The frontend calls this once on startup (or any time) to pull pending
//! archive paths that arrived via:
//! - Cold-start argv (Windows/Linux double-click file association).
//! - Single-instance second-argv (app already running, user opens another file).
//! - macOS `RunEvent::Opened` (macOS double-click / drag-to-dock).
//!
//! After retrieval the pending list is cleared.

use crate::state::AppState;

/// Retrieve all pending archive paths and clear them from state.
///
/// The frontend should call this on startup and/or subscribe to the
/// `opened-archives` event for live updates during the session.
#[tauri::command]
pub async fn get_opened_archives(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut pending = state
        .pending_archives
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    let result = pending.clone();
    pending.clear();
    Ok(result)
}
