//! Signal handling and cancellation token for graceful shutdown.
//!
//! Provides a [`CancellationToken`] that is set to true when the user
//! presses Ctrl+C (SIGINT).  The token can be cloned cheaply and shared
//! across I/O callbacks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// A cheaply-cloneable token that becomes true when Ctrl+C is pressed.
///
/// Create one at program startup and share clones into every
/// [`ProgressReader`] / [`ProgressWriter`] callback that needs to
/// support cancellation.
///
/// # Idempotency
///
/// Internally uses `OnceLock` so the underlying SIGINT handler is
/// installed at most once per process.  Calling `new()` multiple
/// times is safe (e.g. in tests) — each call resets the flag but
/// does not re-register the OS handler.
///
/// A second Ctrl+C triggers immediate process termination
/// (the `ctrlc` crate default for double-press).
///
/// [`ProgressReader`]: geezipx_core::ProgressReader
/// [`ProgressWriter`]: geezipx_core::ProgressWriter
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

static CANCELLED_FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();

impl CancellationToken {
    /// Create a new token and register the global SIGINT handler.
    ///
    /// The handler is installed at most once per process (via
    /// `OnceLock`).  The cancellation flag is reset on each call.
    pub fn new() -> Self {
        let flag = CANCELLED_FLAG.get_or_init(|| {
            let flag = Arc::new(AtomicBool::new(false));
            let handler_flag = Arc::clone(&flag);
            ctrlc::set_handler(move || {
                handler_flag.store(true, Ordering::SeqCst);
            })
            .expect("failed to install Ctrl+C handler");
            flag
        });
        flag.store(false, Ordering::SeqCst);
        CancellationToken {
            cancelled: Arc::clone(flag),
        }
    }

    /// Returns `true` if the user has pressed Ctrl+C.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Consume the token and return the inner `Arc<AtomicBool>`.
    pub fn into_inner(self) -> Arc<AtomicBool> {
        self.cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that `new()` can be called multiple times without panic.
    ///
    /// Before the `OnceLock` fix, a second call would panic because
    /// `ctrlc::set_handler` allows only one handler per process.
    #[test]
    fn cancellation_token_new_is_idempotent() {
        let _a = CancellationToken::new();
        let _b = CancellationToken::new();
    }

    #[test]
    fn cancellation_token_default_is_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_into_inner() {
        let token = CancellationToken::new();
        let inner = token.into_inner();
        assert!(!inner.load(Ordering::SeqCst));
    }
}
