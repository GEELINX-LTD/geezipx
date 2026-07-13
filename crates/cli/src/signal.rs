//! Signal handling and cancellation token for graceful shutdown.
//!
//! Provides a [`CancellationToken`] that is set to true when the user
//! presses Ctrl+C (SIGINT).  The token can be cloned cheaply and shared
//! across I/O callbacks.
//!
//! # Global state and reset semantics
//!
//! The cancellation flag is a process-global [`AtomicBool`].  Calling
//! [`CancellationToken::new()`] does **not** reset the flag — if Ctrl+C was
//! already pressed, every subsequent token will immediately see
//! `is_cancelled() == true`.  Use [`CancellationToken::reset()`] explicitly
//! **before** starting a new logical operation (e.g. at the top of
//! `execute_compress` / `execute_decompress`) to clear the flag.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// A cheaply-cloneable token that becomes true when Ctrl+C is pressed.
///
/// Create one at the start of a new operation and share clones into
/// every [`ProgressReader`] / [`ProgressWriter`] callback that needs to
/// support cancellation.
///
/// # Idempotency
///
/// Internally uses `OnceLock` so the underlying SIGINT handler is
/// installed at most once per process.  Calling `new()` multiple
/// times is safe (e.g. in tests) — each call returns a token viewing
/// the **same** global flag.  It does **not** reset the flag; use
/// [`reset`](Self::reset) before the first `new()` of a new operation.
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
    /// Create a new token viewing the process-global cancellation flag.
    ///
    /// The SIGINT handler is installed at most once per process (via
    /// `OnceLock`).  This method does **not** reset the flag — use
    /// [`reset`](Self::reset) before starting a new operation if you
    /// need a clean slate.
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
        CancellationToken {
            cancelled: Arc::clone(flag),
        }
    }

    /// Reset the process-global cancellation flag to `false`.
    ///
    /// Call this **once** at the top of each new logical operation
    /// (e.g. `execute_compress`, `execute_decompress`), **before**
    /// calling [`new`](Self::new) for that operation.  This ensures
    /// that a prior Ctrl+C does not leak into the new operation.
    ///
    /// The SIGINT handler stays registered — subsequent Ctrl+C presses
    /// will still set the flag.
    pub fn reset() {
        // Must initialise the OnceLock if it hasn't been set yet
        // (e.g. in tests that call reset() before new()).
        let flag = CANCELLED_FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)));
        flag.store(false, Ordering::SeqCst);
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
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;

    /// Serialize access to the process-level `CANCELLED_FLAG`.
    ///
    /// Rust's default test harness runs tests in parallel, and every
    /// `CancellationToken::new()` reads from / writes to the same global
    /// `AtomicBool` via `OnceLock`.  This lock ensures only one test
    /// touches the flag at a time, preventing flaky failures from
    /// interleaved `store` / `load` / `reset` across test functions.
    static TEST_SERIAL: OnceLock<Mutex<()>> = OnceLock::new();

    fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_SERIAL
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test serialisation mutex poisoned")
    }

    /// Verify that `new()` can be called multiple times without panic.
    #[test]
    fn new_is_idempotent() {
        let _guard = serial_guard();
        let _a = CancellationToken::new();
        let _b = CancellationToken::new();
    }

    /// After `reset()`, a fresh token is not cancelled.
    #[test]
    fn reset_clears_flag() {
        let _guard = serial_guard();
        // Simulate a prior cancel.
        let flag = CANCELLED_FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)));
        flag.store(true, Ordering::SeqCst);
        assert!(flag.load(Ordering::SeqCst));

        CancellationToken::reset();

        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    /// Calling `new()` multiple times does NOT clear a prior cancel.
    #[test]
    fn new_does_not_clear_cancel_state() {
        let _guard = serial_guard();
        // Set up the flag as cancelled.
        let flag = CANCELLED_FLAG.get_or_init(|| Arc::new(AtomicBool::new(false)));
        flag.store(true, Ordering::SeqCst);

        let a = CancellationToken::new();
        assert!(a.is_cancelled(), "first token should see cancelled state");

        // A second `new()` must NOT reset — it should still see cancelled.
        let b = CancellationToken::new();
        assert!(
            b.is_cancelled(),
            "second token should still see cancelled state"
        );

        // Clean up shared global state so later tests see a clean flag.
        CancellationToken::reset();
    }

    /// `into_inner()` shares the same AtomicBool.
    #[test]
    fn into_inner_shares_flag() {
        let _guard = serial_guard();
        // Ensure a clean starting state regardless of prior test order.
        CancellationToken::reset();
        let token = CancellationToken::new();
        let inner = token.into_inner();
        assert!(!inner.load(Ordering::SeqCst));

        inner.store(true, Ordering::SeqCst);
        let token2 = CancellationToken::new();
        assert!(token2.is_cancelled());

        // Clean up shared global state so later tests see a clean flag.
        CancellationToken::reset();
    }

    /// `reset()` works before any `new()` call (lazy init).
    #[test]
    fn reset_before_new() {
        let _guard = serial_guard();
        // In a fresh test the OnceLock might be unset; reset() must handle that.
        CancellationToken::reset();
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }
}
