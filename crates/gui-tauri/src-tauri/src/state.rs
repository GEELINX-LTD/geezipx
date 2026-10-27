//! Application state shared across Tauri commands.

use crate::ShellActionPayload;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// Unique identifier for an in-flight task.
pub type TaskId = String;

/// Shared application state.
pub struct AppState {
    /// Cancellation tokens for in-flight tasks.
    /// Each task stores an `Arc<AtomicBool>`; when `cancel_task` sets it to
    /// `true`, the running `spawn_blocking` closure checks this flag and
    /// returns early.
    pub cancel_tokens: Arc<Mutex<HashMap<TaskId, Arc<AtomicBool>>>>,
    /// Archive file paths received via file association / open-with.
    /// The frontend pulls these on startup via `get_opened_archives`.
    pub pending_archives: Mutex<Vec<String>>,
    /// Previous OS default handler per MIME type, captured when the user binds a
    /// format on Linux so we can restore it on unbind.
    pub assoc_backup: Mutex<HashMap<String, String>>,
    /// Shell actions waiting for their own webview to initialize. Quick-extract
    /// windows must never consume the main window's pending action.
    pub pending_shell_actions: Mutex<HashMap<String, ShellActionPayload>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Reserve an ID until the blocking worker actually terminates.
    pub fn register_task(&self, id: &str) -> Result<TaskRegistration, String> {
        if id.trim().is_empty() {
            return Err("Task ID cannot be empty".into());
        }
        let mut tokens = self
            .cancel_tokens
            .lock()
            .map_err(|e| format!("Internal error: {e}"))?;
        if tokens.contains_key(id) {
            return Err(format!("Task '{id}' is already running"));
        }
        let token = Arc::new(AtomicBool::new(false));
        tokens.insert(id.to_owned(), token.clone());
        Ok(TaskRegistration {
            registry: self.cancel_tokens.clone(),
            id: id.to_owned(),
            token,
        })
    }

    /// Create a new empty application state.
    pub fn new() -> Self {
        Self {
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            pending_archives: Mutex::new(Vec::new()),
            assoc_backup: Mutex::new(HashMap::new()),
            pending_shell_actions: Mutex::new(HashMap::new()),
        }
    }
}

/// Owned by the blocking worker so panic or an abandoned IPC future cannot leak
/// registrations or free an ID while its old worker is still writing output.
pub struct TaskRegistration {
    registry: Arc<Mutex<HashMap<TaskId, Arc<AtomicBool>>>>,
    id: TaskId,
    pub token: Arc<AtomicBool>,
}

impl Drop for TaskRegistration {
    fn drop(&mut self) {
        let mut tokens = self
            .registry
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if tokens
            .get(&self.id)
            .is_some_and(|token| Arc::ptr_eq(token, &self.token))
        {
            tokens.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn duplicate_ids_do_not_replace_cancel_targets() {
        let state = AppState::new();
        let first = state.register_task("task-a").unwrap();
        let second = state.register_task("task-b").unwrap();
        assert!(state.register_task("task-a").is_err());
        assert!(state.register_task(" ").is_err());
        state
            .cancel_tokens
            .lock()
            .unwrap()
            .get("task-a")
            .unwrap()
            .store(true, Ordering::SeqCst);
        assert!(first.token.load(Ordering::SeqCst));
        assert!(!second.token.load(Ordering::SeqCst));
        // Cancellation cannot release the ID before the worker finishes.
        assert!(state.register_task("task-a").is_err());
        drop(first);
        assert!(state.register_task("task-a").is_ok());
        drop(second);
        assert!(state.cancel_tokens.lock().unwrap().is_empty());
    }

    #[test]
    fn worker_unwind_cleans_up_its_own_registration() {
        let state = AppState::new();
        let registration = state.register_task("panic").unwrap();
        assert!(std::thread::spawn(move || {
            let _registration = registration;
            panic!("worker failed");
        })
        .join()
        .is_err());
        assert!(state.cancel_tokens.lock().unwrap().is_empty());
    }
}
