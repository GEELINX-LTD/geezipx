mod create;
mod tasks;
#[cfg(test)]
mod tests;
mod workspace;
use crate::{
    theme,
    widgets::{self, Icon},
};
use eframe::egui::{self, Color32, RichText};
use geezipx_core::{
    config::CompressOptions,
    detect::{detect_from_extension, ArchiveFormat},
};
use geezipx_egui::{
    engine::{self, Archive, CompressRequest, Preview, ReadRequest, Summary},
    job::{Job, CANCELLED},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Archive,
    Create,
    Tasks,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    List,
    Compress,
    Extract,
    Verify,
    Preview,
    Dialog,
}
impl Kind {
    fn title(self) -> &'static str {
        match self {
            Self::List => "打开归档",
            Self::Compress => "创建压缩包",
            Self::Extract => "解压文件",
            Self::Verify => "完整性测试",
            Self::Preview => "文本预览",
            Self::Dialog => "选择文件",
        }
    }
    fn record(self) -> bool {
        matches!(self, Self::Compress | Self::Extract | Self::Verify)
    }
}
#[derive(Clone, Copy)]
enum Dialog {
    Open,
    Files,
    Folders,
    Save,
    Destination,
}
enum Outcome {
    Archive(Box<Archive>),
    Done(Summary),
    Preview(Preview),
    Paths(Dialog, Vec<PathBuf>),
}
struct Active {
    kind: Kind,
    job: Job<Outcome>,
}
struct History {
    kind: Kind,
    message: String,
    output: Option<PathBuf>,
    failed: bool,
    cancelled: bool,
    elapsed: Duration,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    dark: bool,
    recent: Vec<PathBuf>,
}

#[derive(Default)]
struct PathField {
    path: PathBuf,
    text: String,
}
impl PathField {
    fn set(&mut self, path: PathBuf) {
        self.text = path.to_string_lossy().into_owned();
        self.path = path;
    }
    fn edit(&mut self, ui: &mut egui::Ui, hint: &str) {
        if ui
            .add(
                egui::TextEdit::singleline(&mut self.text)
                    .margin(egui::Margin::symmetric(10, 8))
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            )
            .changed()
        {
            self.path = PathBuf::from(&self.text);
        }
    }
}
struct Draft {
    sources: Vec<PathBuf>,
    output: PathField,
    format: usize,
    preset: usize,
    password: String,
    confirm: String,
    replace: bool,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            output: PathField::default(),
            format: 0,
            preset: 1,
            password: String::new(),
            confirm: String::new(),
            replace: false,
        }
    }
}
struct Unlock {
    path: PathBuf,
    password: String,
}

pub struct App {
    page: Page,
    preferences: Preferences,
    archive: Option<Archive>,
    active: Option<Active>,
    history: Vec<History>,
    draft: Draft,
    destination: PathField,
    overwrite: bool,
    read_password: String,
    notice: Option<(String, bool)>,
    retry_path: Option<PathBuf>,
    opening_path: Option<PathBuf>,
    unlock: Option<Unlock>,
    preview: Option<Preview>,
    about: bool,
    close_prompt: bool,
    close_when_finished: bool,
    range_anchor: Option<usize>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>) -> Self {
        theme::fonts(&cc.egui_ctx);
        let preferences: Preferences = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, "native-preferences"))
            .unwrap_or_default();
        theme::apply(&cc.egui_ctx, preferences.dark);
        let mut app = Self::empty(preferences);
        app.accept_paths(&cc.egui_ctx, paths);
        app
    }
    fn empty(preferences: Preferences) -> Self {
        Self {
            page: Page::Archive,
            preferences,
            archive: None,
            active: None,
            history: Vec::new(),
            draft: Draft::default(),
            destination: PathField::default(),
            overwrite: false,
            read_password: String::new(),
            notice: None,
            retry_path: None,
            opening_path: None,
            unlock: None,
            preview: None,
            about: false,
            close_prompt: false,
            close_when_finished: false,
            range_anchor: None,
        }
    }
    fn busy(&self) -> bool {
        self.active.is_some()
    }
    fn start(
        &mut self,
        context: &egui::Context,
        kind: Kind,
        work: impl FnOnce(&geezipx_egui::job::Control) -> Result<Outcome, String> + Send + 'static,
    ) {
        if self.busy() {
            return;
        }
        self.notice = None;
        self.active = Some(Active {
            kind,
            job: Job::spawn(context, work),
        });
    }
    fn open_archive(&mut self, context: &egui::Context, path: PathBuf, password: Option<String>) {
        if self.busy() {
            return;
        }
        self.opening_path = Some(path.clone());
        self.page = Page::Archive;
        self.start(context, Kind::List, move |control| {
            engine::list(path, password, control).map(|archive| Outcome::Archive(Box::new(archive)))
        });
    }
    fn add_sources(&mut self, paths: Vec<PathBuf>) {
        let mut seen = self
            .draft
            .sources
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        self.draft
            .sources
            .extend(paths.into_iter().filter(|path| seen.insert(path.clone())));
        if self.draft.output.path.as_os_str().is_empty() {
            if let Some(first) = self.draft.sources.first() {
                let stem = first.file_stem().unwrap_or_default().to_string_lossy();
                let suggested = first.with_file_name(format!("{stem}.zip"));
                self.draft.output.set(if suggested == *first {
                    first.with_file_name(format!("{stem}-archive.zip"))
                } else {
                    suggested
                });
            }
        }
        self.page = Page::Create;
    }
    fn accept_paths(&mut self, context: &egui::Context, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if self.busy() {
            self.notice = Some(("请等待当前任务结束后再添加文件。".into(), true));
            return;
        }
        if paths.len() == 1
            && detect_from_extension(&paths[0]).is_some()
            && self.page != Page::Create
        {
            self.open_archive(context, paths[0].clone(), None);
        } else {
            self.add_sources(paths);
        }
    }
    fn dialog(&mut self, context: &egui::Context, dialog: Dialog) {
        let output = self.draft.output.path.clone();
        self.start(context, Kind::Dialog, move |_| {
            let paths = match dialog {
                Dialog::Open => rfd::FileDialog::new()
                    .set_title("打开归档")
                    .pick_file()
                    .into_iter()
                    .collect(),
                Dialog::Files => rfd::FileDialog::new()
                    .set_title("添加文件")
                    .pick_files()
                    .unwrap_or_default(),
                Dialog::Folders => rfd::FileDialog::new()
                    .set_title("添加文件夹")
                    .pick_folders()
                    .unwrap_or_default(),
                Dialog::Save => {
                    let mut dialog = rfd::FileDialog::new().set_title("保存压缩包");
                    if let Some(name) = output.file_name() {
                        dialog = dialog.set_file_name(name.to_string_lossy());
                    }
                    if let Some(parent) = output.parent() {
                        dialog = dialog.set_directory(parent);
                    }
                    dialog.save_file().into_iter().collect()
                }
                Dialog::Destination => rfd::FileDialog::new()
                    .set_title("选择解压位置")
                    .pick_folder()
                    .into_iter()
                    .collect(),
            };
            Ok(Outcome::Paths(dialog, paths))
        });
    }
    fn poll(&mut self, context: &egui::Context) {
        let Some(result) = self.active.as_ref().and_then(|active| active.job.poll()) else {
            return;
        };
        let Some(active) = self.active.take() else {
            return;
        };
        let elapsed = active.job.started.elapsed();
        let kind = active.kind;
        let mut record = History {
            kind,
            message: String::new(),
            output: None,
            failed: false,
            cancelled: false,
            elapsed,
        };
        match result {
            Ok(Outcome::Archive(archive)) => {
                self.preferences.recent.retain(|path| *path != archive.path);
                self.preferences.recent.insert(0, archive.path.clone());
                self.preferences.recent.truncate(8);
                self.destination
                    .set(engine::default_destination(&archive.path));
                self.archive = Some(*archive);
                self.retry_path = None;
                self.opening_path = None;
                self.range_anchor = None;
                self.read_password.clear();
            }
            Ok(Outcome::Done(summary)) => {
                self.notice = Some((summary.message.clone(), false));
                record.message = summary.message;
                record.output = summary.output;
            }
            Ok(Outcome::Preview(preview)) => self.preview = Some(preview),
            Ok(Outcome::Paths(_, _)) if self.close_when_finished => {}
            Ok(Outcome::Paths(dialog, paths)) => match dialog {
                Dialog::Open => {
                    if let Some(path) = paths.into_iter().next() {
                        self.open_archive(context, path, None);
                    }
                }
                Dialog::Files | Dialog::Folders => {
                    if !paths.is_empty() {
                        self.add_sources(paths);
                    }
                }
                Dialog::Save => {
                    if let Some(path) = paths.into_iter().next() {
                        self.draft.output.set(path);
                    }
                }
                Dialog::Destination => {
                    if let Some(path) = paths.into_iter().next() {
                        self.destination.set(path);
                    }
                }
            },
            Err(error) => {
                record.cancelled = error == CANCELLED;
                record.failed = !record.cancelled;
                record.message = error.clone();
                self.notice = Some((error, record.failed));
                if kind == Kind::List {
                    self.retry_path = self.opening_path.take();
                }
            }
        }
        if kind.record() {
            self.history.insert(0, record);
            self.history.truncate(30);
        }
        if self.close_when_finished && self.active.is_none() {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn read_request(&mut self, all: bool) -> Option<ReadRequest> {
        let archive = self.archive.as_ref()?;
        let password = std::mem::take(&mut self.read_password);
        Some(ReadRequest {
            archive: archive.path.clone(),
            password: if password.is_empty() {
                None
            } else {
                Some(password)
            },
            selection: if all {
                BTreeSet::new()
            } else {
                archive.browser.selection.clone()
            },
            destination: self.destination.path.clone(),
            overwrite: self.overwrite,
        })
    }
    fn extract(&mut self, context: &egui::Context, all: bool) {
        if self.busy() {
            return;
        }
        if let Some(request) = self.read_request(all) {
            self.start(context, Kind::Extract, move |control| {
                engine::extract(request, control).map(Outcome::Done)
            });
        }
    }
    fn verify(&mut self, context: &egui::Context) {
        if self.busy() {
            return;
        }
        if let Some(request) = self.read_request(true) {
            self.start(context, Kind::Verify, move |control| {
                engine::verify(request, control).map(Outcome::Done)
            });
        }
    }
    fn create_archive(&mut self, context: &egui::Context) {
        if self.busy() {
            return;
        }
        if self.draft.password != self.draft.confirm {
            self.notice = Some(("两次密码不一致，请重新输入。".into(), true));
            return;
        }
        let format = engine::formats::WRITE_FORMATS[self.draft.format].0;
        let level = match (format, self.draft.preset) {
            (ArchiveFormat::Zip | ArchiveFormat::Tar | ArchiveFormat::TarLz4, _) => None,
            (_, 1) => None,
            (ArchiveFormat::Zpaq, 0) => Some(1),
            (ArchiveFormat::Zpaq, _) => Some(5),
            (ArchiveFormat::TarZst, 0) => Some(1),
            (ArchiveFormat::TarZst, _) => Some(19),
            (ArchiveFormat::TarBr, 0) => Some(1),
            (ArchiveFormat::TarBr, _) => Some(11),
            (_, 0) => Some(1),
            (_, _) => Some(9),
        };
        let password = std::mem::take(&mut self.draft.password);
        self.draft.confirm.clear();
        let request = CompressRequest {
            sources: self.draft.sources.clone(),
            output: self.draft.output.path.clone(),
            format,
            options: CompressOptions {
                level,
                jobs: Some(
                    std::thread::available_parallelism()
                        .map_or(1, |count| count.get().min(8) as u32),
                ),
                password: if password.is_empty() {
                    None
                } else {
                    Some(password)
                },
                seven_zip: None,
            },
            replace: self.draft.replace,
        };
        self.start(context, Kind::Compress, move |control| {
            engine::compress(request, control).map(Outcome::Done)
        });
    }
    fn open_output(&mut self, path: PathBuf) {
        if let Err(error) = open::that_detached(path) {
            self.notice = Some((format!("无法打开文件夹：{error}"), true));
        }
    }
    fn shortcuts(&mut self, context: &egui::Context) {
        if self.unlock.is_none()
            && self.preview.is_none()
            && !self.close_prompt
            && !self.about
            && context
                .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            if let Some(active) = &self.active {
                if active.kind != Kind::Dialog {
                    active.job.control.cancel();
                }
            } else if !context.wants_keyboard_input() {
                if let Some(archive) = &mut self.archive {
                    archive.browser.selection.clear();
                }
                self.range_anchor = None;
            }
        }
        if self.busy()
            || self.unlock.is_some()
            || self.preview.is_some()
            || self.close_prompt
            || self.about
        {
            return;
        }
        if context.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
            self.dialog(context, Dialog::Open);
        }
        if context.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.page = Page::Create;
        }
        if self.page == Page::Archive
            && self.archive.is_some()
            && context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F))
        {
            context.memory_mut(|memory| memory.request_focus(egui::Id::new("archive-search")));
        }
        if !context.wants_keyboard_input() && self.page == Page::Archive {
            if context.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)) {
                if let Some(archive) = &mut self.archive {
                    archive.browser.parent();
                    self.range_anchor = None;
                }
            }
            if context.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
                if let Some(archive) = &mut self.archive {
                    for row in &archive.browser.visible {
                        archive.browser.selection.insert(row.path.clone());
                    }
                }
            }
        }
    }
    fn sidebar(&mut self, context: &egui::Context) {
        egui::SidePanel::left("navigation")
            .exact_width(204.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(self.preferences.dark).sidebar)
                    .inner_margin(16),
            )
            .show(context, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    theme::logo(ui, 35.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("GeeZipX").size(20.0).strong());
                        theme::caption(ui, "文件归档工作台");
                    });
                });
                ui.add_space(34.0);
                theme::caption(ui, "工作空间");
                for (page, label, icon, detail) in [
                    (Page::Archive, "归档浏览", Icon::Archive, "01"),
                    (Page::Create, "新建压缩", Icon::Plus, "02"),
                    (Page::Tasks, "任务中心", Icon::Clock, "03"),
                ] {
                    if widgets::nav(ui, label, icon, self.page == page, detail).clicked() {
                        self.page = page;
                    }
                }
                ui.add_space(28.0);
                ui.horizontal(|ui| {
                    theme::caption(ui, "最近打开");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.preferences.recent.is_empty()
                            && widgets::icon_button(ui, Icon::Close, "清空最近记录").clicked()
                        {
                            self.preferences.recent.clear();
                        }
                    });
                });
                let mut open = None;
                for path in self.preferences.recent.iter().take(4) {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    ui.add_enabled_ui(!self.busy(), |ui| {
                        if ui
                            .add_sized(
                                [ui.available_width(), 30.0],
                                egui::Button::new((
                                    RichText::new(name).size(12.0),
                                    egui::Atom::grow(),
                                ))
                                .frame(false)
                                .truncate(),
                            )
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            open = Some(path.clone());
                        }
                    });
                }
                if self.preferences.recent.is_empty() {
                    theme::caption(ui, "还没有打开过归档");
                }
                if let Some(path) = open {
                    self.open_archive(context, path, None);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(6.0);
                    theme::caption(ui, format!("NATIVE  /  {}", env!("CARGO_PKG_VERSION")));
                    ui.horizontal(|ui| {
                        if widgets::icon_button(ui, Icon::Shield, "关于 GeeZipX 与字体许可")
                            .clicked()
                        {
                            self.about = true;
                        }
                        theme::caption(ui, "本地处理 · 安心归档");
                    });
                    ui.horizontal(|ui| {
                        for (dark, text, icon) in
                            [(false, "浅色", Icon::Sun), (true, "深色", Icon::Moon)]
                        {
                            ui.scope(|ui| {
                                ui.set_width(80.0);
                                if widgets::nav(ui, text, icon, self.preferences.dark == dark, "")
                                    .clicked()
                                {
                                    self.preferences.dark = dark;
                                    theme::apply(context, dark);
                                }
                            });
                        }
                    });
                    theme::rule(ui);
                });
            });
    }
    fn notices(&mut self, ui: &mut egui::Ui) {
        if let Some((message, error)) = self.notice.clone() {
            let color = if error {
                Color32::from_rgb(184, 69, 59)
            } else {
                theme::ACCENT
            };
            egui::Frame::new()
                .fill(color.gamma_multiply(0.10))
                .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.35)))
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_max_width((ui.available_width() - 42.0).max(80.0));
                            ui.label(RichText::new(message).color(color));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, Icon::Close, "关闭消息").clicked() {
                                self.notice = None;
                            }
                        });
                    });
                });
            ui.add_space(6.0);
        }
    }
    fn windows(&mut self, context: &egui::Context) {
        egui::Window::new("关于 GeeZipX Native")
            .open(&mut self.about)
            .default_width(520.0)
            .show(context, |ui| {
                ui.heading(format!("GeeZipX Native {}", env!("CARGO_PKG_VERSION")));
                ui.label("简洁、原生的归档工作空间。");
                ui.hyperlink_to("项目主页与问题反馈", env!("CARGO_PKG_REPOSITORY"));
                ui.label(
                    "GeeZipX 采用 MIT 许可。中文字体 Droid Sans Fallback 采用 Apache 2.0 许可。",
                );
                ui.collapsing("字体版权与许可", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            ui.label(include_str!("../../assets/FONT-NOTICE.txt"));
                            ui.label(include_str!("../../assets/FONT-LICENSE.txt"));
                            ui.label(include_str!("../../assets/FONT-APACHE-2.0.txt"));
                            ui.label(include_str!("../../assets/EGUI-FONTS-LICENSES.txt"));
                        });
                });
            });
        if let Some(unlock) = &mut self.unlock {
            let mut open = true;
            let mut retry = false;
            egui::Window::new("使用密码打开归档")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(context, |ui| {
                    ui.label(
                        unlock
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut unlock.password)
                            .margin(egui::Margin::symmetric(10, 8))
                            .password(true)
                            .hint_text("归档密码"),
                    );
                    retry = theme::primary(ui, "重新打开").clicked();
                });
            if retry {
                if let Some(unlock) = self.unlock.take() {
                    self.open_archive(context, unlock.path, Some(unlock.password));
                }
            } else if !open {
                self.unlock = None;
            }
        }
        if let Some(preview) = &mut self.preview {
            let mut open = true;
            egui::Window::new("文本预览")
                .collapsible(false)
                .default_size([680.0, 480.0])
                .open(&mut open)
                .show(context, |ui| {
                    ui.label(RichText::new(&preview.name).strong());
                    if preview.truncated {
                        ui.label("仅显示前 512 KiB；预览不代表完整性验证。完整内容请解压后查看。");
                    }
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut preview.text.as_str())
                                .code_editor()
                                .desired_width(f32::INFINITY),
                        );
                    });
                });
            if !open {
                self.preview = None;
            }
        }
        if self.close_prompt {
            egui::Modal::new(egui::Id::new("close-confirmation")).show(context, |ui| {
                ui.set_width(380.0);
                ui.heading("任务仍在进行");
                ui.label("关闭前会安全取消任务并清理临时文件。已经解压完成的文件会保留。");
                ui.horizontal(|ui| {
                    if ui.button("继续处理").clicked() {
                        self.close_prompt = false;
                    }
                    if ui.button("取消任务并退出").clicked() {
                        if let Some(active) = &self.active {
                            active.job.control.cancel();
                        }
                        self.close_when_finished = true;
                        self.close_prompt = false;
                    }
                });
            });
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(context);
        if context.input(|input| input.viewport().close_requested()) && self.busy() {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_prompt = true;
        }
        self.shortcuts(context);
        let drops = context.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if !drops.is_empty() {
            self.accept_paths(context, drops);
        }
        self.status_bar(context);
        self.sidebar(context);
        if self.page == Page::Archive && self.archive.is_some() {
            self.extract_panel(context);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(context.style().visuals.panel_fill)
                    .inner_margin(24),
            )
            .show(context, |ui| {
                self.notices(ui);
                match self.page {
                    Page::Archive => self.workspace(ui, context),
                    Page::Create => self.create_page(ui, context),
                    Page::Tasks => self.tasks_page(ui),
                }
            });
        self.windows(context);
        if !context.input(|i| i.raw.hovered_files.is_empty()) {
            let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop-overlay"));
            let painter = context.layer_painter(layer);
            let rect = context.content_rect();
            painter.rect_filled(
                rect.shrink(12.0),
                16,
                Color32::from_rgba_unmultiplied(46, 54, 103, 235),
            );
            let text = if self.page == Page::Create {
                "松开鼠标，添加到压缩列表"
            } else {
                "松开鼠标，打开归档或添加待压缩文件"
            };
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(24.0),
                Color32::WHITE,
            );
        }
        if self.busy() {
            context.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "native-preferences", &self.preferences);
    }
}
