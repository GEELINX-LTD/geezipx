//! Application state shared across Tauri commands.

use crate::ShellActionPayload;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A main-window launch received from argv, a second instance or macOS.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "kebab-case")]
pub enum LaunchAction {
    OpenArchives(Vec<String>),
    ShellAction(ShellActionPayload),
}

/// Retained until the receiving window acknowledges this individual request.
#[derive(Debug, Clone, Serialize)]
pub struct LaunchRequest {
    pub id: String,
    pub action: LaunchAction,
}

/// Unique identifier for an in-flight task.
pub type TaskId = String;

/// Shared application state.
pub struct AppState {
    /// Cancellation tokens for in-flight tasks.
    /// Each task stores an `Arc<AtomicBool>`; when `cancel_task` sets it to
    /// `true`, the running `spawn_blocking` closure checks this flag and
    /// returns early.
    pub cancel_tokens: Arc<Mutex<HashMap<TaskId, Arc<AtomicBool>>>>,
    /// Launches remain available across event loss and webview initialization.
    /// Reading a snapshot never removes requests; acknowledgement is per window.
    pending_launches: Mutex<HashMap<String, Vec<LaunchRequest>>>,
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
    /// Queue before creating or notifying the webview. IDs are strings so the
    /// JavaScript bridge never loses precision after many launches.
    pub fn enqueue_launch(&self, label: &str, action: LaunchAction) -> String {
        static NEXT_LAUNCH: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_LAUNCH.fetch_add(1, Ordering::Relaxed).to_string();
        self.pending_launches
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(label.into())
            .or_default()
            .push(LaunchRequest {
                id: id.clone(),
                action,
            });
        id
    }

    /// Non-destructive snapshot: even a response received after unmount can be
    /// recovered by the next frontend without losing the user's action.
    pub fn launch_requests(&self, label: &str) -> Result<Vec<LaunchRequest>, String> {
        let pending = self
            .pending_launches
            .lock()
            .map_err(|e| format!("Internal error: {e}"))?;
        Ok(pending.get(label).cloned().unwrap_or_default())
    }

    /// Idempotently acknowledge only the calling window's request. A snapshot
    /// cannot erase newer arrivals, and another window cannot steal an action.
    pub fn acknowledge_launch(&self, label: &str, id: &str) -> Result<(), String> {
        let mut pending = self
            .pending_launches
            .lock()
            .map_err(|e| format!("Internal error: {e}"))?;
        if let Some(requests) = pending.get_mut(label) {
            requests.retain(|request| request.id != id);
            if requests.is_empty() {
                pending.remove(label);
            }
        }
        Ok(())
    }

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
            pending_launches: Mutex::new(HashMap::new()),
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

    fn open_action(path: &str) -> LaunchAction {
        LaunchAction::OpenArchives(vec![path.into()])
    }

    #[test]
    fn launches_survive_repeated_reads_until_individually_acknowledged() {
        let state = AppState::new();
        let first = state.enqueue_launch("main", open_action("第一.zip"));
        let second = state.enqueue_launch(
            "main",
            LaunchAction::ShellAction(ShellActionPayload {
                action: "compress".into(),
                format: None,
                paths: vec!["第二.txt".into()],
            }),
        );
        let third = state.enqueue_launch("main", open_action("第一.zip"));
        assert_ne!(first, third, "identical payloads are independent launches");
        for _ in 0..2 {
            let requests = state.launch_requests("main").unwrap();
            assert_eq!(requests.len(), 3);
            assert_eq!(requests[0].id, first);
            assert_eq!(requests[1].id, second);
            assert_eq!(requests[2].id, third);
        }
        state.acknowledge_launch("main", &second).unwrap();
        state.acknowledge_launch("main", &second).unwrap();
        let remaining = state.launch_requests("main").unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(remaining[0].id, first);
        assert_eq!(remaining[1].id, third);
    }

    #[test]
    fn launch_snapshots_and_acknowledgements_are_window_scoped() {
        let state = AppState::new();
        let main_id = state.enqueue_launch("main", open_action("main.zip"));
        let other_id = state.enqueue_launch("other", open_action("other.zip"));
        assert!(state.launch_requests("unknown").unwrap().is_empty());
        state.acknowledge_launch("other", &main_id).unwrap();
        state.acknowledge_launch("unknown", &main_id).unwrap();
        assert_eq!(state.launch_requests("main").unwrap()[0].id, main_id);
        assert_eq!(state.launch_requests("other").unwrap()[0].id, other_id);
        state.acknowledge_launch("main", &main_id).unwrap();
        assert!(state.launch_requests("main").unwrap().is_empty());
        assert_eq!(state.launch_requests("other").unwrap().len(), 1);
    }

    #[test]
    fn acknowledging_a_snapshot_preserves_later_arrivals() {
        let state = AppState::new();
        let first = state.enqueue_launch("main", open_action("first.zip"));
        let snapshot = state.launch_requests("main").unwrap();
        let later = state.enqueue_launch("main", open_action("later.zip"));
        state.acknowledge_launch("main", &snapshot[0].id).unwrap();
        assert_ne!(first, later);
        let remaining = state.launch_requests("main").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, later);
    }

    #[test]
    fn launch_serialization_matches_the_frontend_contract() {
        let state = AppState::new();
        let id = state.enqueue_launch("main", open_action("文件.zip"));
        let requests = state.launch_requests("main").unwrap();
        assert_eq!(
            serde_json::to_value(&requests[0]).unwrap(),
            serde_json::json!({
                "id": id,
                "action": { "kind": "open-archives", "payload": ["文件.zip"] }
            })
        );
        state.enqueue_launch(
            "main",
            LaunchAction::ShellAction(ShellActionPayload {
                action: "compress-zip".into(),
                format: None,
                paths: vec!["input.txt".into()],
            }),
        );
        let requests = state.launch_requests("main").unwrap();
        assert_eq!(
            serde_json::to_value(&requests[1].action).unwrap(),
            serde_json::json!({
                "kind": "shell-action",
                "payload": { "action": "compress-zip", "format": null, "paths": ["input.txt"] }
            })
        );
    }

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
