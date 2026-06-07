//! Shared task-progress payloads, throttling, and Tauri event emit helpers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use geezipx_core::{GeeZipError, Phase, ProgressCallback, ProgressEvent};

pub const TASK_PROGRESS_EVENT: &str = "task:progress";
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(120);
const PROGRESS_EMIT_STEP_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Compress,
    Extract,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Started,
    Progress,
    Finished,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStage {
    Scanning,
    Compressing,
    Extracting,
    Finalizing,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    Reading,
    Writing,
    Hashing,
}

impl From<Phase> for TaskPhase {
    fn from(value: Phase) -> Self {
        match value {
            Phase::Reading => TaskPhase::Reading,
            Phase::Writing => TaskPhase::Writing,
            Phase::Hashing => TaskPhase::Hashing,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskProgressPayload {
    pub task_id: String,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub stage: TaskStage,
    pub phase: Option<TaskPhase>,
    pub message: String,
    pub current: u64,
    pub total: Option<u64>,
    pub percent: Option<f64>,
    pub bytes_per_second: Option<f64>,
    pub current_entry: Option<String>,
    pub completed_entries: u64,
    pub total_entries: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct ProgressThrottle {
    last_emitted_at: Option<Duration>,
    last_current: u64,
    last_percent: Option<u8>,
}

impl ProgressThrottle {
    fn should_emit(
        &mut self,
        elapsed: Duration,
        current: u64,
        total: Option<u64>,
        force: bool,
    ) -> bool {
        if force || self.last_emitted_at.is_none() {
            self.record(elapsed, current, total);
            return true;
        }

        let elapsed_since_last = elapsed
            .checked_sub(self.last_emitted_at.unwrap_or_default())
            .unwrap_or_default();
        let percent = percentage_bucket(current, total);
        let percent_changed = percent != self.last_percent;
        let bytes_changed = current.saturating_sub(self.last_current) >= PROGRESS_EMIT_STEP_BYTES;

        if percent_changed || bytes_changed || elapsed_since_last >= PROGRESS_EMIT_INTERVAL {
            self.record(elapsed, current, total);
            true
        } else {
            false
        }
    }

    fn record(&mut self, elapsed: Duration, current: u64, total: Option<u64>) {
        self.last_emitted_at = Some(elapsed);
        self.last_current = current;
        self.last_percent = percentage_bucket(current, total);
    }
}

#[derive(Debug)]
struct EmitterState {
    total: Option<u64>,
    total_entries: Option<u64>,
    last_current_seen: u64,
    last_completed_entries: u64,
    throttle: ProgressThrottle,
}

#[derive(Clone)]
pub struct TaskProgressEmitter {
    app: AppHandle,
    task_id: String,
    kind: TaskKind,
    started_at: Instant,
    state: Arc<Mutex<EmitterState>>,
}

impl TaskProgressEmitter {
    pub fn new(app: AppHandle, task_id: impl Into<String>, kind: TaskKind) -> Self {
        Self {
            app,
            task_id: task_id.into(),
            kind,
            started_at: Instant::now(),
            state: Arc::new(Mutex::new(EmitterState {
                total: None,
                total_entries: None,
                last_current_seen: 0,
                last_completed_entries: 0,
                throttle: ProgressThrottle::default(),
            })),
        }
    }

    pub fn set_totals(&self, total: Option<u64>, total_entries: Option<u64>) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.total = total;
        state.total_entries = total_entries;
    }

    pub fn latest_snapshot(&self) -> (u64, u64) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.last_current_seen, state.last_completed_entries)
    }

    pub fn emit_started(&self, message: impl Into<String>) {
        self.emit_inner(
            TaskStatus::Started,
            TaskStage::Scanning,
            None,
            0,
            None,
            0,
            Some(message.into()),
            true,
        );
    }

    pub fn emit_progress(
        &self,
        stage: TaskStage,
        phase: Option<Phase>,
        current: u64,
        current_entry: Option<&str>,
        completed_entries: u64,
        force: bool,
    ) {
        self.emit_inner(
            TaskStatus::Progress,
            stage,
            phase.map(TaskPhase::from),
            current,
            current_entry.map(|value| value.to_string()),
            completed_entries,
            None,
            force,
        );
    }

    pub fn emit_finished(&self, current: u64, completed_entries: u64) {
        self.emit_inner(
            TaskStatus::Finished,
            TaskStage::Completed,
            None,
            current,
            None,
            completed_entries,
            Some(match self.kind {
                TaskKind::Compress => "Compression completed.".to_string(),
                TaskKind::Extract => "Extraction completed.".to_string(),
            }),
            true,
        );
    }

    pub fn emit_cancelled(&self, current: u64, completed_entries: u64) {
        self.emit_inner(
            TaskStatus::Cancelled,
            TaskStage::Cancelled,
            None,
            current,
            None,
            completed_entries,
            Some("Operation cancelled by user.".to_string()),
            true,
        );
    }

    pub fn emit_failed(&self, current: u64, completed_entries: u64, message: impl Into<String>) {
        self.emit_inner(
            TaskStatus::Failed,
            TaskStage::Failed,
            None,
            current,
            None,
            completed_entries,
            Some(message.into()),
            true,
        );
    }

    pub fn emit_finalizing(&self, current: u64, completed_entries: u64) {
        self.emit_inner(
            TaskStatus::Progress,
            TaskStage::Finalizing,
            None,
            current,
            None,
            completed_entries,
            Some(match self.kind {
                TaskKind::Compress => "Finalizing archive...".to_string(),
                TaskKind::Extract => "Finishing extraction...".to_string(),
            }),
            true,
        );
    }

    pub fn reader_callback(
        &self,
        cancel_token: Arc<AtomicBool>,
        stage: TaskStage,
        base_current: u64,
        current_entry: String,
        completed_entries: u64,
    ) -> GuiProgressCallback {
        GuiProgressCallback {
            emitter: self.clone(),
            cancel_token,
            stage,
            base_current,
            current_entry,
            completed_entries,
        }
    }

    pub fn writer_callback(
        &self,
        cancel_token: Arc<AtomicBool>,
        stage: TaskStage,
        base_current: u64,
        current_entry: String,
        completed_entries: u64,
    ) -> GuiProgressCallback {
        GuiProgressCallback {
            emitter: self.clone(),
            cancel_token,
            stage,
            base_current,
            current_entry,
            completed_entries,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_inner(
        &self,
        status: TaskStatus,
        stage: TaskStage,
        phase: Option<TaskPhase>,
        current: u64,
        current_entry: Option<String>,
        completed_entries: u64,
        explicit_message: Option<String>,
        force: bool,
    ) {
        let elapsed = self.started_at.elapsed();
        let (total, total_entries, should_emit) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.last_current_seen = current;
            state.last_completed_entries = completed_entries;
            let total = state.total;
            let total_entries = state.total_entries;
            let should_emit = state.throttle.should_emit(elapsed, current, total, force);
            (total, total_entries, should_emit)
        };

        if !should_emit {
            return;
        }

        let percent = percentage_value(current, total);
        let bytes_per_second = if current > 0 && elapsed.as_secs_f64() > 0.0 {
            Some(current as f64 / elapsed.as_secs_f64())
        } else {
            None
        };

        let payload = TaskProgressPayload {
            task_id: self.task_id.clone(),
            kind: self.kind,
            status,
            stage,
            phase,
            message: explicit_message
                .unwrap_or_else(|| default_message(self.kind, stage, current_entry.as_deref())),
            current,
            total,
            percent,
            bytes_per_second,
            current_entry,
            completed_entries,
            total_entries,
        };

        let _ = self.app.emit(TASK_PROGRESS_EVENT, &payload);
    }
}

pub struct GuiProgressCallback {
    emitter: TaskProgressEmitter,
    cancel_token: Arc<AtomicBool>,
    stage: TaskStage,
    base_current: u64,
    current_entry: String,
    completed_entries: u64,
}

impl ProgressCallback for GuiProgressCallback {
    fn update(&mut self, event: ProgressEvent) {
        self.emitter.emit_progress(
            self.stage,
            Some(event.phase),
            self.base_current.saturating_add(event.current),
            Some(&self.current_entry),
            self.completed_entries,
            false,
        );
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_token.load(Ordering::SeqCst)
    }
}

pub fn is_cancelled_error(err: &GeeZipError) -> bool {
    matches!(err, GeeZipError::Cancelled)
        || matches!(
            err,
            GeeZipError::Io { source, .. } if source.kind() == std::io::ErrorKind::Interrupted
        )
}

fn percentage_bucket(current: u64, total: Option<u64>) -> Option<u8> {
    total.and_then(|value| {
        current
            .saturating_mul(100)
            .checked_div(value)
            .map(|percent| percent.min(100) as u8)
    })
}

fn percentage_value(current: u64, total: Option<u64>) -> Option<f64> {
    total.and_then(|value| {
        if value == 0 {
            None
        } else {
            Some(((current as f64 / value as f64) * 100.0).clamp(0.0, 100.0))
        }
    })
}

fn default_message(kind: TaskKind, stage: TaskStage, current_entry: Option<&str>) -> String {
    match stage {
        TaskStage::Scanning => match kind {
            TaskKind::Compress => "Scanning input files...".to_string(),
            TaskKind::Extract => "Reading archive entries...".to_string(),
        },
        TaskStage::Compressing => current_entry
            .map(|entry| format!("Compressing {entry}"))
            .unwrap_or_else(|| "Compressing files...".to_string()),
        TaskStage::Extracting => current_entry
            .map(|entry| format!("Extracting {entry}"))
            .unwrap_or_else(|| "Extracting files...".to_string()),
        TaskStage::Finalizing => match kind {
            TaskKind::Compress => "Finalizing archive...".to_string(),
            TaskKind::Extract => "Finishing extraction...".to_string(),
        },
        TaskStage::Completed => match kind {
            TaskKind::Compress => "Compression completed.".to_string(),
            TaskKind::Extract => "Extraction completed.".to_string(),
        },
        TaskStage::Cancelled => "Operation cancelled by user.".to_string(),
        TaskStage::Failed => "Operation failed.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_maps_to_serializable_variant() {
        assert_eq!(TaskPhase::from(Phase::Reading), TaskPhase::Reading);
        assert_eq!(TaskPhase::from(Phase::Writing), TaskPhase::Writing);
        assert_eq!(TaskPhase::from(Phase::Hashing), TaskPhase::Hashing);
    }

    #[test]
    fn throttle_emits_first_update_and_then_suppresses_small_repeats() {
        let mut throttle = ProgressThrottle::default();
        assert!(throttle.should_emit(Duration::from_millis(0), 0, Some(100_000), false));
        assert!(!throttle.should_emit(Duration::from_millis(10), 500, Some(100_000), false));
    }

    #[test]
    fn throttle_emits_on_percent_change() {
        let mut throttle = ProgressThrottle::default();
        assert!(throttle.should_emit(Duration::from_millis(0), 0, Some(100), false));
        assert!(throttle.should_emit(Duration::from_millis(10), 1, Some(100), false));
        assert!(!throttle.should_emit(Duration::from_millis(20), 1, Some(100), false));
    }

    #[test]
    fn throttle_emits_on_time_interval_for_unknown_total() {
        let mut throttle = ProgressThrottle::default();
        assert!(throttle.should_emit(Duration::from_millis(0), 0, None, false));
        assert!(!throttle.should_emit(Duration::from_millis(50), 1, None, false));
        assert!(throttle.should_emit(Duration::from_millis(150), 1, None, false));
    }

    #[test]
    fn throttle_force_emit_always_wins() {
        let mut throttle = ProgressThrottle::default();
        assert!(throttle.should_emit(Duration::from_millis(0), 0, Some(100), false));
        assert!(throttle.should_emit(Duration::from_millis(1), 0, Some(100), true));
    }

    #[test]
    fn throttle_force_emit_after_suppressed_repeat() {
        let mut throttle = ProgressThrottle::default();
        assert!(throttle.should_emit(Duration::from_millis(0), 0, Some(10_000), false));
        assert!(!throttle.should_emit(Duration::from_millis(5), 1, Some(10_000), false));
        assert!(throttle.should_emit(Duration::from_millis(6), 1, Some(10_000), true));
    }

    #[test]
    fn percentage_is_clamped() {
        assert_eq!(percentage_bucket(150, Some(100)), Some(100));
        assert_eq!(percentage_bucket(0, Some(0)), None);
        assert!(
            matches!(percentage_value(150, Some(100)), Some(value) if (value - 100.0).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn percentage_value_reports_fractional_progress() {
        assert!(
            matches!(percentage_value(25, Some(200)), Some(value) if (value - 12.5).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn interrupted_io_is_treated_as_cancellation() {
        let err = GeeZipError::io(
            std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"),
            "reading entry",
        );
        assert!(is_cancelled_error(&err));
    }
}
