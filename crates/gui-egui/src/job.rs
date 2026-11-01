//! One worker owns each operation. Progress is a single coalesced snapshot.
use crate::{i18n::Language, tr};
use geezipx_core::{io::ProgressEvent, ProgressCallback};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, Instant};

pub const CANCELLED: &str = "操作已取消。已完成的提取文件会保留。";

#[derive(Clone, Default)]
pub struct Progress {
    pub stage: String,
    pub entry: String,
    pub bytes: u64,
    pub total: u64,
    pub completed: usize,
    pub entries: usize,
}

#[derive(Clone)]
pub struct Control {
    pub language: Language,
    cancelled: Arc<AtomicBool>,
    progress: Arc<Mutex<Progress>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Default for Control {
    fn default() -> Self {
        Self::new(|| {})
    }
}

impl Control {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            language: Language::default(),
            cancelled: Arc::default(),
            progress: Arc::default(),
            wake: Arc::new(wake),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        (self.wake)();
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err(tr!(self.language, OperationCancelled).into())
        } else {
            Ok(())
        }
    }
    pub fn snapshot(&self) -> Progress {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn update(&self, update: impl FnOnce(&mut Progress)) {
        update(&mut self.progress.lock().unwrap_or_else(|e| e.into_inner()));
        (self.wake)();
    }
    pub fn callback(&self, base: u64) -> Box<dyn ProgressCallback> {
        Box::new(Callback {
            control: self.clone(),
            base,
            last: Instant::now(),
        })
    }
}

struct Callback {
    control: Control,
    base: u64,
    last: Instant,
}
impl ProgressCallback for Callback {
    fn update(&mut self, event: ProgressEvent) {
        if self.last.elapsed() >= Duration::from_millis(40) {
            self.control
                .update(|p| p.bytes = self.base.saturating_add(event.current));
            self.last = Instant::now();
        }
    }
    fn is_cancelled(&self) -> bool {
        self.control.is_cancelled()
    }
}

pub struct Job<T> {
    pub control: Control,
    pub started: Instant,
    receiver: mpsc::Receiver<Result<T, String>>,
}

impl<T: Send + 'static> Job<T> {
    pub fn spawn(
        context: &eframe::egui::Context,
        work: impl FnOnce(&Control) -> Result<T, String> + Send + 'static,
    ) -> Self {
        Self::spawn_in(context, Language::default(), work)
    }

    pub fn spawn_in(
        context: &eframe::egui::Context,
        language: Language,
        work: impl FnOnce(&Control) -> Result<T, String> + Send + 'static,
    ) -> Self {
        let context = context.clone();
        let mut control = Control::new(move || context.request_repaint());
        control.language = language;
        let worker_control = control.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let error_sender = sender.clone();
        let spawn = std::thread::Builder::new()
            .name("geezipx-worker".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    work(&worker_control)
                }))
                .unwrap_or_else(|_| Err(tr!(language, WorkerPanic).into()));
                let _ = sender.send(result);
                (worker_control.wake)();
            });
        if let Err(error) = spawn {
            let _ = error_sender.send(Err(tr!(language, WorkerStartError, error)));
        }
        Self {
            control,
            started: Instant::now(),
            receiver,
        }
    }
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(tr!(self.control.language, WorkerDisconnected).into()))
            }
        }
    }
}

impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
