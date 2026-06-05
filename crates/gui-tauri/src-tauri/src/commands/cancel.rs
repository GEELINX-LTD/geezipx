//! `cancel_task` command — cancel a running task.

use std::sync::atomic::Ordering;
use tauri::State;

use crate::state::AppState;

/// Cancel a running task by its task id.
///
/// Looks up the cancellation token registered by the task and sets it to
/// `true`. The running `spawn_blocking` closure checks this flag and
/// returns early.
///
/// If no task with the given id is found, returns an error (the task may
/// have already completed or never existed).
#[tauri::command]
pub fn cancel_task(task_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut tokens = state
        .cancel_tokens
        .lock()
        .map_err(|e| format!("Internal error: {}", e))?;

    match tokens.remove(&task_id) {
        Some(token) => {
            token.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err(format!(
            "No active task with id '{}' (it may have already completed)",
            task_id
        )),
    }
}
