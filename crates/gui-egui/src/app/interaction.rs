use super::*;
use geezipx_egui::export::{DeferredExport, ExportKey};
use std::sync::{mpsc, Arc};

pub(super) struct Marquee {
    pub archive: PathBuf,
    pub directory: String,
    /// Coordinates relative to the scroll content, so scrolling keeps the anchor.
    pub origin: egui::Pos2,
    pub original: BTreeSet<String>,
    pub toggle: bool,
}
#[derive(Default)]
pub(super) struct RowActivation {
    last: Option<(egui::Id, f64, egui::Pos2)>,
}
impl RowActivation {
    pub fn click(&mut self, context: &egui::Context, id: egui::Id, position: egui::Pos2) -> bool {
        let time = context.input(|i| i.time);
        let (delay, distance) = context.options(|options| {
            (
                options.input_options.max_double_click_delay,
                options.input_options.max_click_dist,
            )
        });
        let activate = self.last.is_some_and(|(previous, at, pos)| {
            previous == id && time - at <= delay && pos.distance(position) <= distance
        });
        self.last = if activate {
            None
        } else {
            Some((id, time, position))
        };
        activate
    }
    pub fn reset(&mut self) {
        self.last = None;
    }
}
pub(super) fn configure_pointer(context: &egui::Context) {
    #[cfg(target_os = "windows")]
    // SAFETY: This read-only system setting requires no pointers or resources.
    let delay = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() } as f64
        / 1000.0;
    #[cfg(not(target_os = "windows"))]
    let delay = 0.5;
    context.options_mut(|options| options.input_options.max_double_click_delay = delay);
}
#[derive(Default)]
pub(super) struct ExportState {
    pub pending: Option<Arc<DeferredExport>>,
    // Some receivers read after the native callback; retain successful sources.
    pub retained: Vec<Arc<DeferredExport>>,
    pub receiver: Option<mpsc::Receiver<Result<bool, String>>>,
    pub cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl App {
    pub(super) fn request_export(&mut self, context: &egui::Context) {
        if self.busy() || self.modal_open() {
            return;
        }
        let Some(archive) = &self.archive else {
            return;
        };
        let key = ExportKey {
            archive: archive.path.clone(),
            selection: archive.browser.selection.clone(),
        };
        let repaint = context.clone();
        let control = geezipx_egui::job::Control::new(move || repaint.request_repaint());
        let password = std::mem::take(&mut self.read_password);
        match DeferredExport::new(
            key,
            &archive.entries,
            (!password.is_empty()).then_some(password),
            control,
        ) {
            Ok(export) => self.export.pending = Some(Arc::new(export)),
            Err(error) => self.notice = Some(Notice::new(error, true)),
        }
    }
    pub(super) fn start_native_drag(&mut self, context: &egui::Context, frame: &eframe::Frame) {
        let Some(export) = self.export.pending.take() else {
            return;
        };
        if !context.input(|i| i.pointer.primary_down()) || self.modal_open() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let repaint = context.clone();
        self.export.receiver = Some(receiver);
        self.export.retained.push(export.clone());
        context.stop_dragging();
        self.row_activation.reset();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.export.cancel = Some(cancel.clone());
        if let Err(error) = native_drag::start(
            frame,
            export,
            move |result| {
                let _ = sender.send(result);
                repaint.request_repaint();
            },
            cancel,
        ) {
            self.export.receiver = None;
            self.export.cancel = None;
            self.export.retained.pop();
            self.notice = Some(Notice::new(error, true));
        }
    }
    pub(super) fn poll_native_drag(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.export.receiver else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("系统拖放连接已中断。".into()),
        };
        self.export.receiver = None;
        self.export.cancel = None;
        if !matches!(result, Ok(true)) {
            self.export.retained.pop();
        }
        if self.close_when_finished {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Native loops consume mouse release; discard stale egui pointer/click state.
        context.stop_dragging();
        context.input_mut(|input| input.pointer = Default::default());
        self.row_activation.reset();
        match result {
            Ok(true) => self.notice = Some(Notice::new("已将所选文件交给目标位置。", false)),
            Ok(false) => {}
            Err(error) => self.notice = Some(Notice::new(format!("拖出未完成：{error}"), true)),
        }
    }
}
