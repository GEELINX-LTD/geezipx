use super::*;
use geezipx_egui::export::{self, ExportKey, PreparedExport};
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
pub(super) struct ExportState {
    pub ready: Option<PreparedExport>,
    // OS receivers may open files after the drag callback. Keep them until exit.
    pub retained: Vec<PreparedExport>,
    pub requested: bool,
    pub receiver: Option<mpsc::Receiver<Result<bool, String>>>,
    pub cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl App {
    fn export_key(&self) -> Option<ExportKey> {
        let archive = self.archive.as_ref()?;
        (!archive.browser.selection.is_empty()).then(|| ExportKey {
            archive: archive.path.clone(),
            selection: archive.browser.selection.clone(),
        })
    }
    pub(super) fn request_export(&mut self, context: &egui::Context) {
        if self.busy() || self.modal_open() {
            return;
        }
        let Some(key) = self.export_key() else {
            return;
        };
        self.export.requested = true;
        if self
            .export
            .ready
            .as_ref()
            .is_some_and(|ready| ready.key == key)
        {
            return;
        }
        self.export.ready = None;
        let password = std::mem::take(&mut self.read_password);
        self.start(context, Kind::Export, move |control| {
            export::prepare(key, (!password.is_empty()).then_some(password), control)
                .map(|ready| Outcome::Export(Box::new(ready)))
        });
    }
    pub(super) fn start_native_drag(&mut self, context: &egui::Context, frame: &eframe::Frame) {
        if !self.export.requested || self.export.ready.is_none() {
            return;
        }
        if !context.input(|i| i.pointer.primary_down()) || self.modal_open() {
            self.export.requested = false;
            return;
        }
        if self.export.ready.as_ref().map(|ready| &ready.key) != self.export_key().as_ref() {
            self.export.requested = false;
            return;
        }
        let ready = self.export.ready.take().expect("ready checked above");
        let paths = ready.paths.clone();
        self.export.retained.push(ready);
        self.export.requested = false;
        let (sender, receiver) = mpsc::channel();
        let repaint = context.clone();
        self.export.receiver = Some(receiver);
        context.stop_dragging();
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.export.cancel = Some(cancel.clone());
        if let Err(error) = native_drag::start(
            frame,
            paths,
            move |result| {
                let _ = sender.send(result);
                repaint.request_repaint();
            },
            cancel,
        ) {
            self.export.receiver = None;
            self.export.cancel = None;
            self.export.ready = self.export.retained.pop();
            self.notice = Some((error, true));
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
        if self.close_when_finished {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Native drag loops consume the release event, especially OLE on Windows.
        context.stop_dragging();
        context.input_mut(|input| input.pointer = Default::default());
        match result {
            Ok(true) => self.notice = Some(("已将所选文件交给目标位置。".into(), false)),
            Ok(false) => {}
            Err(error) => self.notice = Some((format!("拖出未完成：{error}"), true)),
        }
    }
}
