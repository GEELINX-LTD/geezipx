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

/// Retrieve the pending action for the calling window exactly once.
#[tauri::command]
pub async fn get_shell_action(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ShellActionPayload>, String> {
    take_shell_action(&state, window.label())
}

fn take_shell_action(state: &AppState, label: &str) -> Result<Option<ShellActionPayload>, String> {
    let mut pending = state
        .pending_shell_actions
        .lock()
        .map_err(|e| format!("Internal error: {e}"))?;
    Ok(pending.remove(label))
}

/// Return the application version string from Cargo.toml.
#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_actions_are_consumed_only_by_their_own_window() {
        let state = AppState::new();
        for (label, action, path) in [
            ("main", "compress", "input.txt"),
            ("extract-here-1", "extract-here", "first.7z"),
            ("extract-here-2", "extract-here", "second.zip"),
        ] {
            state.pending_shell_actions.lock().unwrap().insert(
                label.into(),
                ShellActionPayload {
                    action: action.into(),
                    format: None,
                    paths: vec![path.into()],
                },
            );
        }
        assert!(take_shell_action(&state, "unknown").unwrap().is_none());
        assert_eq!(
            take_shell_action(&state, "extract-here-2")
                .unwrap()
                .unwrap()
                .paths,
            ["second.zip"]
        );
        assert!(take_shell_action(&state, "extract-here-2")
            .unwrap()
            .is_none());
        assert_eq!(
            take_shell_action(&state, "main").unwrap().unwrap().action,
            "compress"
        );
        assert_eq!(
            take_shell_action(&state, "extract-here-1")
                .unwrap()
                .unwrap()
                .paths,
            ["first.7z"]
        );
        assert!(state.pending_shell_actions.lock().unwrap().is_empty());
    }
}
