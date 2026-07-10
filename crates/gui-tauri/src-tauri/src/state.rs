//! Application state shared across Tauri commands.

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
    pub cancel_tokens: Mutex<HashMap<TaskId, Arc<AtomicBool>>>,
    /// Archive file paths received via file association / open-with.
    /// The frontend pulls these on startup via `get_opened_archives`.
    pub pending_archives: Mutex<Vec<String>>,
    /// Previous OS default handler per MIME type, captured when the user binds a
    /// format on Linux so we can restore it on unbind.
    pub assoc_backup: Mutex<HashMap<String, String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Create a new empty application state.
    pub fn new() -> Self {
        Self {
            cancel_tokens: Mutex::new(HashMap::new()),
            pending_archives: Mutex::new(Vec::new()),
            assoc_backup: Mutex::new(HashMap::new()),
        }
    }
}
