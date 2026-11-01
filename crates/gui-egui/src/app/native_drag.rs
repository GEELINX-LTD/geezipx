use geezipx_egui::export::DeferredExport;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
use geezipx_egui::tr;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

pub enum DropOutcome {
    #[cfg(not(target_os = "windows"))]
    Delivered,
    Cancelled,
    #[cfg(target_os = "windows")]
    ExtractTo(PathBuf),
}
impl DropOutcome {
    #[cfg(not(target_os = "windows"))]
    fn from_delivered(delivered: bool) -> Self {
        if delivered {
            Self::Delivered
        } else {
            Self::Cancelled
        }
    }
}

/// Begin dragging descriptors immediately; native backends authorize data only on drop.
pub fn start(
    frame: &eframe::Frame,
    export: Arc<DeferredExport>,
    finished: impl Fn(Result<DropOutcome, String>) + Send + 'static,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        windows::start(frame, export, finished, cancel)
    }
    #[cfg(target_os = "macos")]
    {
        macos::start(
            frame,
            export,
            move |result| finished(result.map(DropOutcome::from_delivered)),
            cancel,
        )
    }
    #[cfg(target_os = "linux")]
    {
        linux::start(
            frame,
            export,
            move |result| finished(result.map(DropOutcome::from_delivered)),
            cancel,
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let language = export.control.language;
        let _ = (frame, export, finished, cancel);
        Err(tr!(language, DragUnsupported).into())
    }
}
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
