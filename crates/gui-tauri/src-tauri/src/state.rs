//! Application state shared across Tauri commands.

/// Shared application state.
///
/// Future fields will include cancellation tokens for in-flight tasks
/// and a connection to the core progress channel.
#[derive(Default)]
pub struct AppState {
    // Placeholder for future task-cancellation support.
}

impl AppState {
    /// Create a new empty application state.
    pub fn new() -> Self {
        Self {}
    }
}
