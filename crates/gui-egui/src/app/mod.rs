mod chrome;
mod create;
mod extract;
mod folders;
mod interaction;
mod native_drag;
mod notices;
mod settings;
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
    browser::SortColumn,
    engine::{self, Archive, CompressRequest, Preview, ReadRequest, Summary},
    job::{Job, CANCELLED},
};
use notices::Notice;
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Duration, Instant},
};

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
    font: crate::fonts::Selection,
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
    fn edit(&mut self, ui: &mut egui::Ui, hint: &str) -> egui::Response {
        let response = ui.add(widgets::text_edit(&mut self.text).hint_text(hint));
        if response.changed() {
            self.path = PathBuf::from(&self.text);
        }
        response
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
    show_sources: bool,
    show_password: bool,
    show_advanced: bool,
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
            show_sources: false,
            show_password: false,
            show_advanced: false,
        }
    }
}
struct Extraction {
    selection: BTreeSet<String>,
    destination: PathField,
    overwrite: bool,
    open_after: bool,
    show_password: bool,
}
struct Unlock {
    path: PathBuf,
    password: String,
}

pub struct App {
    preferences: Preferences,
    settings: Settings,
    archive: Option<Archive>,
    active: Option<Active>,
    history: Vec<History>,
    draft: Draft,
    create_open: bool,
    extraction: Option<Extraction>,
    pending_extraction: Option<Extraction>,
    tasks_open: bool,
    read_password: String,
    notice: Option<Notice>,
    form_error: Option<String>,
    retry_path: Option<PathBuf>,
    opening_path: Option<PathBuf>,
    unlock: Option<Unlock>,
    preview: Option<Preview>,
    about: bool,
    close_prompt: bool,
    close_when_finished: bool,
    open_after_extract: bool,
    typeahead: String,
    last_typed: Instant,
    column_widths: [f32; 3],
    list_offset: f32,
    tree_offset: f32,
    marquee: Option<interaction::Marquee>,
    row_activation: interaction::RowActivation,
    export: interaction::ExportState,
}
impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>) -> Self {
        interaction::configure_pointer(&cc.egui_ctx);
        let preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "native-preferences"))
            .unwrap_or_default();
        let mut app = Self::empty(preferences);
        app.settings.available = theme::fonts(&cc.egui_ctx, &app.preferences.font);
        theme::apply(&cc.egui_ctx, app.preferences.dark);
        app.accept_paths(&cc.egui_ctx, paths);
        app
    }
    fn empty(preferences: Preferences) -> Self {
        Self {
            preferences,
            settings: Settings::default(),
            archive: None,
            active: None,
            history: Vec::new(),
            draft: Draft::default(),
            create_open: false,
            extraction: None,
            pending_extraction: None,
            tasks_open: false,
            read_password: String::new(),
            notice: None,
            form_error: None,
            retry_path: None,
            opening_path: None,
            unlock: None,
            preview: None,
            about: false,
            close_prompt: false,
            close_when_finished: false,
            open_after_extract: false,
            typeahead: String::new(),
            last_typed: Instant::now(),
            column_widths: [110.0, 110.0, 90.0],
            list_offset: 0.0,
            tree_offset: 0.0,
            marquee: None,
            row_activation: interaction::RowActivation::default(),
            export: interaction::ExportState::default(),
        }
    }
    fn busy(&self) -> bool {
        self.active.is_some() || self.export.receiver.is_some()
    }
    fn modal_open(&self) -> bool {
        self.create_open
            || self.extraction.is_some()
            || self.tasks_open
            || self.unlock.is_some()
            || self.preview.is_some()
            || self.close_prompt
            || self.about
            || self.settings.open
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
        self.start(context, Kind::List, move |control| {
            engine::list(path, password, control).map(|archive| Outcome::Archive(Box::new(archive)))
        });
    }
    fn new_archive(&mut self) {
        self.draft = Draft::default();
        self.form_error = None;
        self.create_open = true;
    }
    fn add_sources(&mut self, paths: Vec<PathBuf>) {
        if !self.create_open {
            self.draft = Draft::default();
        }
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
                let format = engine::formats::WRITE_FORMATS[self.draft.format].0;
                let suggested = first.with_file_name(format!("{stem}.{format}"));
                self.draft.output.set(if suggested == *first {
                    first.with_file_name(format!("{stem}-archive.{format}"))
                } else {
                    suggested
                });
            }
        }
        self.form_error = None;
        self.create_open = true;
    }
    fn accept_paths(&mut self, context: &egui::Context, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if self.busy() {
            self.notice = Some(Notice::new("请等待当前任务结束后再添加文件。", true));
            return;
        }
        if self.extraction.is_some()
            || self.unlock.is_some()
            || self.preview.is_some()
            || self.close_prompt
            || self.tasks_open
            || self.about
            || self.settings.open
        {
            return;
        }
        if paths.len() == 1 && detect_from_extension(&paths[0]).is_some() && !self.create_open {
            self.open_archive(context, paths[0].clone(), None);
        } else {
            self.add_sources(paths);
        }
    }
    fn dialog(&mut self, context: &egui::Context, dialog: Dialog) {
        let output = self.draft.output.path.clone();
        let destination = self
            .extraction
            .as_ref()
            .map(|options| options.destination.path.clone());
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
                    let mut picker = rfd::FileDialog::new().set_title("保存压缩包");
                    if let Some(name) = output.file_name() {
                        picker = picker.set_file_name(name.to_string_lossy());
                    }
                    if let Some(parent) = output.parent() {
                        picker = picker.set_directory(parent);
                    }
                    picker.save_file().into_iter().collect()
                }
                Dialog::Destination => {
                    let mut picker = rfd::FileDialog::new().set_title("选择解压位置");
                    if let Some(destination) = destination {
                        picker = picker.set_directory(destination);
                    }
                    picker.pick_folder().into_iter().collect()
                }
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
        let kind = active.kind;
        let mut record = History {
            kind,
            message: String::new(),
            output: None,
            failed: false,
            cancelled: false,
            elapsed: active.job.started.elapsed(),
        };
        match result {
            Ok(Outcome::Archive(archive)) => {
                self.preferences.recent.retain(|path| *path != archive.path);
                self.preferences.recent.insert(0, archive.path.clone());
                self.preferences.recent.truncate(8);
                context.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "{} — GeeZipX",
                    archive
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )));
                self.archive = Some(*archive);
                self.marquee = None;
                self.export.pending = None;
                self.row_activation.reset();
                self.retry_path = None;
                self.opening_path = None;
                self.read_password.clear();
                self.typeahead.clear();
                self.list_offset = 0.0;
                self.tree_offset = 0.0;
            }
            Ok(Outcome::Done(summary)) => {
                self.notice = Some(Notice::new(summary.message.clone(), false));
                record.message = summary.message;
                record.output = summary.output;
                if kind == Kind::Compress {
                    self.draft.show_password = false;
                }
                if kind == Kind::Extract && self.open_after_extract && !self.close_when_finished {
                    if let Some(path) = &record.output {
                        self.open_output(path.clone());
                    }
                }
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
                    if let (Some(options), Some(path)) =
                        (&mut self.extraction, paths.into_iter().next())
                    {
                        options.destination.set(path);
                    }
                }
            },
            Err(error) => {
                record.cancelled = error == CANCELLED;
                record.failed = !record.cancelled;
                record.message = error.clone();
                if record.failed && !self.close_when_finished {
                    if kind == Kind::Compress {
                        self.create_open = true;
                        self.tasks_open = false;
                        self.form_error = Some(error.clone());
                    } else if kind == Kind::Extract {
                        self.extraction = self.pending_extraction.take();
                        self.tasks_open = false;
                        self.form_error = Some(error.clone());
                    }
                }
                self.notice = Some(Notice::new(error, record.failed));
                if kind == Kind::List {
                    self.retry_path = self.opening_path.take();
                }
            }
        }
        if kind == Kind::Extract {
            self.pending_extraction = None;
            self.open_after_extract = false;
        }
        if kind.record() {
            self.history.insert(0, record);
            self.history.truncate(30);
        }
        if self.active.is_none() {
            self.close_prompt = false;
            if self.close_when_finished {
                context.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn read_request(&mut self, all: bool) -> Option<ReadRequest> {
        let archive = self.archive.as_ref()?;
        let password = std::mem::take(&mut self.read_password);
        Some(ReadRequest {
            archive: archive.path.clone(),
            password: (!password.is_empty()).then_some(password),
            selection: if all {
                BTreeSet::new()
            } else {
                archive.browser.selection.clone()
            },
            destination: engine::default_destination(&archive.path),
            overwrite: false,
        })
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
    fn preview_entry(&mut self, context: &egui::Context, index: usize) {
        if self.busy() {
            return;
        }
        let Some(archive) = &self.archive else {
            return;
        };
        let Some(entry) = archive.entries.get(index).cloned() else {
            return;
        };
        let path = archive.path.clone();
        let password = std::mem::take(&mut self.read_password);
        self.start(context, Kind::Preview, move |control| {
            engine::preview(
                path,
                (!password.is_empty()).then_some(password),
                entry,
                control,
            )
            .map(Outcome::Preview)
        });
    }
    fn create_archive(&mut self, context: &egui::Context) {
        if self.busy() {
            return;
        }
        if self.draft.show_password && self.draft.password.is_empty() {
            self.form_error = Some("请设置密码，或关闭密码保护。".into());
            return;
        }
        if self.draft.password != self.draft.confirm {
            self.form_error = Some("两次密码不一致，请重新输入。".into());
            return;
        }
        if self.draft.sources.is_empty() || self.draft.output.path.as_os_str().is_empty() {
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
                password: (!password.is_empty()).then_some(password),
                seven_zip: None,
            },
            replace: self.draft.replace,
        };
        self.create_open = false;
        self.form_error = None;
        self.start(context, Kind::Compress, move |control| {
            engine::compress(request, control).map(Outcome::Done)
        });
    }
    fn open_output(&mut self, path: PathBuf) {
        if let Err(error) = open::that_detached(path) {
            self.notice = Some(Notice::new(format!("无法打开文件夹：{error}"), true));
        }
    }
    fn shortcuts(&mut self, context: &egui::Context) {
        if self.modal_open() || egui::Popup::is_any_open(context) || self.export.receiver.is_some()
        {
            return;
        }
        if context.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.marquee = None;
            self.export.pending = None;
            if let Some(archive) = &mut self.archive {
                if !archive.browser.query.is_empty() {
                    archive.browser.query.clear();
                    archive.browser.refresh();
                    context.memory_mut(|m| m.request_focus(egui::Id::new("archive-list")));
                } else {
                    archive.browser.clear_selection();
                }
            }
            self.typeahead.clear();
            return;
        }
        if !self.busy() {
            if context.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
                self.dialog(context, Dialog::Open);
                return;
            }
            if context.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
                self.new_archive();
                return;
            }
            if context.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::E)) {
                self.show_extract(false);
                return;
            }
        }
        if self.archive.is_none() {
            return;
        }
        if context.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            context.memory_mut(|m| m.request_focus(egui::Id::new("archive-search")));
            return;
        }
        let list_focus = context.memory(|m| m.has_focus(egui::Id::new("archive-list")));
        if context.wants_keyboard_input() && !list_focus {
            return;
        }
        let mut preview = None;
        if let Some(archive) = &mut self.archive {
            let browser = &mut archive.browser;
            let input = |modifiers, key| context.input_mut(|i| i.consume_key(modifiers, key));
            if input(egui::Modifiers::ALT, egui::Key::ArrowLeft) {
                browser.back();
            }
            if input(egui::Modifiers::ALT, egui::Key::ArrowRight) {
                browser.forward();
            }
            if input(egui::Modifiers::NONE, egui::Key::Backspace)
                || input(egui::Modifiers::ALT, egui::Key::ArrowUp)
            {
                browser.parent();
            }
            if input(egui::Modifiers::COMMAND, egui::Key::A) {
                browser.select_all();
            }
            let modifiers = context.input(|i| i.modifiers);
            for (key, offset) in [
                (egui::Key::ArrowDown, 1),
                (egui::Key::ArrowUp, -1),
                (egui::Key::PageDown, 12),
                (egui::Key::PageUp, -12),
            ] {
                if input(modifiers, key) {
                    browser.move_focus(
                        offset,
                        modifiers.shift,
                        modifiers.command || modifiers.ctrl,
                    );
                }
            }
            for (key, offset) in [(egui::Key::Home, isize::MIN), (egui::Key::End, isize::MAX)] {
                if input(modifiers, key) {
                    if browser.focus.is_none() && !browser.visible.is_empty() {
                        browser.focus = Some(browser.visible[0].path.clone());
                    }
                    browser.move_focus(offset, modifiers.shift, false);
                }
            }
            if input(egui::Modifiers::NONE, egui::Key::Space) {
                if let Some(path) = browser.focus.clone() {
                    browser.toggle(&path);
                }
            }
            if input(egui::Modifiers::NONE, egui::Key::Enter) {
                if let Some(row) = browser
                    .visible
                    .iter()
                    .find(|row| Some(&row.path) == browser.focus.as_ref())
                    .cloned()
                {
                    if row.is_dir {
                        browser.navigate(row.path.clone());
                    } else {
                        preview = row.entry_index;
                    }
                }
            }
            if !modifiers.command && !modifiers.ctrl && !modifiers.alt {
                let typed = context.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|event| match event {
                            egui::Event::Text(text) if !text.chars().all(char::is_whitespace) => {
                                Some(text.clone())
                            }
                            _ => None,
                        })
                        .collect::<String>()
                });
                if !typed.is_empty() {
                    if self.last_typed.elapsed() > Duration::from_millis(900) {
                        self.typeahead.clear();
                    }
                    if self.typeahead == typed {
                        self.typeahead.clear();
                    }
                    self.typeahead.push_str(&typed);
                    self.last_typed = Instant::now();
                    browser.jump_to(&self.typeahead);
                }
            }
        }
        if let Some(index) = preview {
            self.preview_entry(context, index);
        }
    }
    fn windows(&mut self, context: &egui::Context) {
        self.create_window(context);
        self.extract_window(context);
        self.tasks_window(context);
        self.settings_window(context);
        if self.about {
            let response = theme::modal(context, "about").show(context, |ui| {
                ui.set_width(460.0_f32.min(context.content_rect().width() - 64.0));
                ui.horizontal(|ui| {
                    theme::logo(ui, 34.0);
                    ui.heading(format!("GeeZipX {}", env!("CARGO_PKG_VERSION")));
                });
                theme::caption(ui, "轻巧、专注的原生归档工具");
                ui.hyperlink_to("项目主页与问题反馈", env!("CARGO_PKG_REPOSITORY"));
                ui.label(
                    "GeeZipX 采用 MIT 许可。优先使用系统字体，中文缺字由内置 Noto Sans SC 补齐（SIL OFL 1.1 许可）。",
                );
                ui.collapsing("字体版权与许可", |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            ui.label(include_str!("../../assets/FONT-NOTICE.txt"));
                            ui.label(include_str!("../../assets/FONT-LICENSE.txt"));
                            ui.label(include_str!("../../assets/EGUI-FONTS-LICENSES.txt"));
                        });
                });
                ui.button("关闭").clicked()
            });
            if response.inner || response.should_close() {
                self.about = false;
            }
        }
        if let Some(unlock) = &mut self.unlock {
            let response = theme::modal(context, "unlock").show(context, |ui| {
                ui.set_width(360.0);
                ui.heading("使用密码打开归档");
                ui.label(
                    unlock
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                );
                ui.add(
                    widgets::text_edit(&mut unlock.password)
                        .password(true)
                        .hint_text("归档密码"),
                );
                theme::primary(ui, "重新打开").clicked()
            });
            if response.inner {
                if let Some(unlock) = self.unlock.take() {
                    self.open_archive(context, unlock.path, Some(unlock.password));
                }
            } else if response.should_close() {
                self.unlock = None;
            }
        }
        if let Some(preview) = &mut self.preview {
            let response = theme::modal(context, "preview").show(context, |ui| {
                ui.set_width(640.0_f32.min(context.content_rect().width() - 80.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&preview.name).strong());
                });
                if preview.truncated {
                    theme::caption(ui, "仅显示前 512 KiB；完整内容请解压后查看。");
                }
                egui::ScrollArea::both()
                    .max_height((context.content_rect().height() - 200.0).max(120.0))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut preview.text.as_str())
                                .code_editor()
                                .desired_width(f32::INFINITY),
                        );
                    });
                ui.button("关闭").clicked()
            });
            if response.inner || response.should_close() {
                self.preview = None;
            }
        }
        if self.close_prompt {
            theme::modal(context, "close-confirmation").show(context, |ui| {
                ui.set_width(380.0);
                ui.heading("任务仍在进行");
                ui.label("关闭前会安全取消任务并清理临时文件。已经解压完成的文件会保留。");
                ui.horizontal(|ui| {
                    if ui.button("继续处理").clicked() {
                        self.close_prompt = false;
                    }
                    if ui.button("取消任务并退出").clicked() {
                        if let Some(cancel) = &self.export.cancel {
                            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        if let Some(export) = self.export.retained.last() {
                            export.control.cancel();
                        }
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
    fn render(&mut self, context: &egui::Context) {
        self.shortcuts(context);
        self.chrome(context);
        let footer = self.status_bar(context);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(self.preferences.dark).surface)
                    .inner_margin(12),
            )
            .show(context, |ui| {
                self.workspace(ui, context);
            });
        self.notices(context, footer);
        self.windows(context);
    }
}
impl eframe::App for App {
    fn update(&mut self, context: &egui::Context, frame: &mut eframe::Frame) {
        self.poll_native_drag(context);
        self.poll(context);
        if context.input(|i| i.viewport().close_requested()) && self.busy() {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_prompt = true;
        }
        let drops = context.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });
        if !drops.is_empty() && self.export.receiver.is_none() && self.export.pending.is_none() {
            self.accept_paths(context, drops);
        }
        self.render(context);
        self.start_native_drag(context, frame);
        if !context.input(|i| i.raw.hovered_files.is_empty())
            && (!self.modal_open() || self.create_open)
        {
            let painter = context.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop-overlay"),
            ));
            let rect = context.content_rect();
            painter.rect_filled(
                rect.shrink(10.0),
                10,
                Color32::from_rgba_unmultiplied(39, 47, 84, 238),
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                if self.create_open {
                    "松开鼠标，添加到压缩列表"
                } else {
                    "松开鼠标，打开归档或创建压缩包"
                },
                egui::FontId::proportional(22.0),
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
