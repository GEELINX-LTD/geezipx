//! TTY-aware progress bar wrappers using `indicatif`.
//!
//! Automatically disables rendering when stderr is piped.
//! Provides [`ProgressBarWrapper`] for per-operation progress and
//! [`SharedCallback`] for sharing a single bar across multiple
//! [`geezipx_core::ProgressReader`] instances (e.g. multi-file archive compress).
//!
//! # Progress bar templates
//!
//! * **Determinate**: `{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})`
//! * **Spinner**:     `{spinner:.green} {msg} [{elapsed}]`

use std::io::IsTerminal;
use std::time::Duration;

use geezipx_core::{ProgressCallback, ProgressEvent};
use indicatif::{ProgressBar, ProgressStyle};

// ---------------------------------------------------------------------------
// TTY detection
// ---------------------------------------------------------------------------

/// Returns `true` when stderr is a terminal (i.e. progress bars can render).
///
/// Uses [`std::io::IsTerminal`] (stable since Rust 1.70).  Does NOT require
/// crossterm for this check.
pub fn progress_bar_enabled() -> bool {
    std::io::stderr().is_terminal()
}

// ---------------------------------------------------------------------------
// ProgressBarWrapper
// ---------------------------------------------------------------------------

/// A progress bar that wraps `indicatif::ProgressBar` and implements
/// [`ProgressCallback`] so it can be attached to a [`geezipx_core::ProgressReader`] or
/// [`geezipx_core::ProgressWriter`].
pub struct ProgressBarWrapper {
    pb: ProgressBar,
}

impl ProgressBarWrapper {
    /// Create a determinate progress bar with a known total byte count.
    ///
    /// Template: `{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})`
    pub fn determinate(total: u64) -> Self {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
            )
            .unwrap()
            .progress_chars("=>-"),
        );
        pb.enable_steady_tick(Duration::from_millis(250));
        ProgressBarWrapper { pb }
    }

    /// Create an indeterminate spinner.
    ///
    /// Template: `{spinner:.green} {msg} [{elapsed}]`
    pub fn spinner(msg: &str) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg} [{elapsed}]").unwrap());
        pb.set_message(msg.to_string());
        pb.enable_steady_tick(Duration::from_millis(80));
        ProgressBarWrapper { pb }
    }

    /// Create a hidden progress bar that does not render to the terminal.
    ///
    /// Useful for background tasks or when progress display is disabled
    /// but cancellation checks are still needed.
    pub fn hidden() -> Self {
        let pb = ProgressBar::hidden();
        ProgressBarWrapper { pb }
    }

    /// Update the message shown on the bar (e.g. the current filename).
    pub fn set_message(&self, msg: &str) {
        self.pb.set_message(msg.to_string());
    }

    /// Finish the bar with a message, then clear/dispose.
    pub fn finish(&self, msg: &str) {
        self.pb.finish_with_message(msg.to_string());
    }
}

impl ProgressCallback for ProgressBarWrapper {
    fn update(&mut self, event: ProgressEvent) {
        // If a total becomes known (spinner -> determinate transition), set it.
        if let Some(total) = event.total {
            if self.pb.length().is_none() || self.pb.length() == Some(0) {
                self.pb.set_length(total);
            }
        }
        self.pb.set_position(event.current);
    }
}

// ---------------------------------------------------------------------------
// SharedCallback
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Thin adapter that shares a single [`ProgressBarWrapper`] across multiple
/// [`geezipx_core::ProgressReader`] instances (one per file in a multi-file archive).
///
/// Internally uses `Arc<Mutex<ProgressBarWrapper>>` so that `&mut self` on the
/// trait is satisfied.
pub struct SharedCallback {
    pub(crate) inner: Arc<Mutex<ProgressBarWrapper>>,
    pub(crate) cancelled: Arc<AtomicBool>,
}

impl SharedCallback {
    /// Wrap an owned `ProgressBarWrapper` for sharing.
    pub fn new(wrapper: ProgressBarWrapper, cancelled: Arc<AtomicBool>) -> Self {
        SharedCallback {
            inner: Arc::new(Mutex::new(wrapper)),
            cancelled,
        }
    }

    pub fn clone_inner(&self) -> Arc<Mutex<ProgressBarWrapper>> {
        Arc::clone(&self.inner)
    }
}

impl ProgressCallback for SharedCallback {
    fn update(&mut self, event: ProgressEvent) {
        self.inner.lock().unwrap().update(event);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
impl ProgressBarWrapper {
    /// Create a hidden determinate bar for use in tests (no terminal output).
    pub fn determinate_hidden(total: u64) -> Self {
        let pb = ProgressBar::hidden();
        pb.set_length(total);
        ProgressBarWrapper { pb }
    }

    /// Create a hidden spinner for use in tests (no terminal output).
    pub fn spinner_hidden(msg: &str) -> Self {
        let pb = ProgressBar::hidden();
        pb.set_message(msg.to_string());
        ProgressBarWrapper { pb }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use geezipx_core::Phase;

    #[test]
    fn test_determinate_wrapper_update() {
        let wrapper = ProgressBarWrapper::determinate_hidden(100);
        let event = ProgressEvent {
            current: 50,
            total: Some(100),
            phase: Phase::Reading,
        };
        // Should not panic
        let mut w = wrapper;
        w.update(event);
    }

    #[test]
    fn test_spinner_wrapper_update() {
        let wrapper = ProgressBarWrapper::spinner_hidden("test");
        let event = ProgressEvent {
            current: 10,
            total: Some(50),
            phase: Phase::Writing,
        };
        let mut w = wrapper;
        w.update(event);
    }

    #[test]
    fn test_shared_callback_multiple_updates() {
        let wrapper = ProgressBarWrapper::determinate_hidden(200);
        let shared = SharedCallback::new(wrapper, Arc::new(AtomicBool::new(false)));
        let inner = shared.clone_inner();

        // Simulate two files updating the same bar
        {
            let mut cb1 = SharedCallback {
                inner: inner.clone(),
                cancelled: Arc::new(AtomicBool::new(false)),
            };
            cb1.update(ProgressEvent {
                current: 50,
                total: Some(200),
                phase: Phase::Reading,
            });
        }
        {
            let mut cb2 = SharedCallback {
                inner: inner.clone(),
                cancelled: Arc::new(AtomicBool::new(false)),
            };
            cb2.update(ProgressEvent {
                current: 100,
                total: Some(200),
                phase: Phase::Reading,
            });
        }

        let final_val = inner.lock().unwrap().pb.position();
        assert_eq!(final_val, 100);
    }

    #[test]
    fn test_shared_callback_is_cancelled() {
        let cancelled = Arc::new(AtomicBool::new(true));
        let wrapper = ProgressBarWrapper::hidden();
        let cb = SharedCallback::new(wrapper, cancelled);
        assert!(cb.is_cancelled());
    }

    #[test]
    fn test_progress_bar_enabled() {
        // Just verify the function returns a bool (no panic).
        let _enabled = progress_bar_enabled();
    }

    #[test]
    fn test_set_message_and_finish() {
        let wrapper = ProgressBarWrapper::determinate_hidden(50);
        wrapper.set_message("processing...");
        wrapper.finish("done!");
        // No assertion — just verify no panic
    }

    #[test]
    fn test_spinner_no_total() {
        let wrapper = ProgressBarWrapper::spinner_hidden("waiting");
        let mut w = wrapper;
        w.update(ProgressEvent {
            current: 0,
            total: None,
            phase: Phase::Reading,
        });
    }

    #[test]
    fn test_determinate_zero_total() {
        let wrapper = ProgressBarWrapper::determinate_hidden(0);
        let mut w = wrapper;
        w.update(ProgressEvent {
            current: 0,
            total: Some(0),
            phase: Phase::Reading,
        });
    }
}
