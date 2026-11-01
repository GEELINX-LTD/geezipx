use geezipx_egui::export::DeferredExport;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
use geezipx_egui::tr;
use std::sync::{atomic::AtomicBool, Arc};

/// Begin dragging descriptors immediately; native backends authorize data only on drop.
pub fn start(
    frame: &eframe::Frame,
    export: Arc<DeferredExport>,
    finished: impl Fn(Result<bool, String>) + Send + 'static,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let _ = frame;
        windows::start(export, finished, cancel)
    }
    #[cfg(target_os = "macos")]
    {
        macos::start(frame, export, finished, cancel)
    }
    #[cfg(target_os = "linux")]
    {
        linux::start(frame, export, finished, cancel)
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
