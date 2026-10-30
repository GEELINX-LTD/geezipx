use std::path::PathBuf;

/// Start a copy drag on the UI thread. The callback marks OS drag completion;
/// callers keep staged files alive longer because receivers can copy later.
pub fn start(
    frame: &eframe::Frame,
    paths: Vec<PathBuf>,
    finished: impl Fn(Result<bool, String>) + Send + 'static,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let _ = cancel;
        drag::start_drag(
            frame,
            drag::DragItem::Files(paths),
            drag::Image::Raw(include_bytes!("../../assets/drag-file.png").to_vec()),
            move |result, _| finished(Ok(matches!(result, drag::DragResult::Dropped))),
            drag::Options::default(),
        )
        .map_err(|e| e.to_string())
    }
    #[cfg(target_os = "linux")]
    {
        linux::start(frame, paths, finished, cancel)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (frame, paths, finished, cancel);
        Err("此平台尚不支持拖出文件，请使用解压操作。".into())
    }
}

#[cfg(target_os = "linux")]
mod linux;
