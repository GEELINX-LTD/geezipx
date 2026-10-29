//! Window-scoped startup delivery. Main-window launches are read without
//! removing them and acknowledged after the frontend handles each request.
//! Quick-extract windows retain their one-shot, per-window shell action.

use crate::state::{AppState, LaunchRequest};
use crate::ShellActionPayload;

/// Return a non-destructive snapshot for the calling window. The frontend
/// subscribes to `launch-requests-pending` before its first read, so a lost
/// notification during webview startup never loses the actual request.
#[tauri::command]
pub async fn get_launch_requests(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<LaunchRequest>, String> {
    state.launch_requests(window.label())
}

/// Acknowledge a handled request, without clearing any other pending launch.
#[tauri::command]
pub async fn acknowledge_launch_request(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.acknowledge_launch(window.label(), &id)
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
