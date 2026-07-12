//! `get_opened_archives` / `get_shell_action` commands — retrieve file paths
//! and shell context menu actions received at startup.
//!
//! The frontend calls these once on startup to pull pending data that arrived via:
//! - Cold-start argv (Windows/Linux double-click file association).
//! - Single-instance second-argv (app already running, user opens another file).
//! - macOS `RunEvent::Opened` (macOS double-click / drag-to-dock).
//!
//! After retrieval the pending data is cleared.

use crate::state::AppState;
use crate::ShellActionPayload;

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

/// Retrieve the shell context menu action (if any) from cold start.
/// The frontend calls this once on startup. After retrieval, the value is cleared.
#[tauri::command]
pub async fn get_shell_action(
    state: tauri::State<'_, AppState>,
) -> Result<Option<ShellActionPayload>, String> {
    let mut pending = state
        .pending_shell_action
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    Ok(pending.take())
}
