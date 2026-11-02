use super::*;

pub const WINDOW_SIZE: [f32; 2] = [560.0, 320.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [480.0, 300.0];
const OPTIONS_SIZE: [f32; 2] = [560.0, 440.0];

pub(super) struct ShellTask {
    source: PathBuf,
    options_visible: bool,
}

impl ShellTask {
    pub(super) fn new(source: PathBuf) -> Self {
        Self {
            source,
            options_visible: false,
        }
    }
}

impl App {
    pub(super) fn close_shell_task(&mut self, context: &egui::Context) {
        self.read_password.clear();
        if let Some(active) = &self.active {
            active.job.control.cancel();
            self.close_when_finished = true;
        } else {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn shell_progress(&mut self, context: &egui::Context) {
        let Some(task) = &mut self.shell_task else {
            return;
        };
        let options_visible = self.extraction.is_some();
        let options_changed = task.options_visible != options_visible;
        task.options_visible = options_visible;
        let source = task.source.clone();
        let language = self.preferences.language;
        let mut close = false;
        let mut retry = false;
        self.window_frame.set_min_inner_size(
            context,
            if options_visible {
                [MIN_WINDOW_SIZE[0], OPTIONS_SIZE[1]]
            } else {
                MIN_WINDOW_SIZE
            },
        );
        if options_changed {
            context.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                if options_visible {
                    OPTIONS_SIZE
                } else {
                    WINDOW_SIZE
                }
                .into(),
            ));
        }
        let margin = self.window_frame.margin(context);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(self.preferences.dark).surface)
                    .corner_radius(self.window_frame.corner_radius(context))
                    .inner_margin(margin),
            )
            .show(context, |ui| {
                ui.add_space(crate::window_frame::TITLE_HEIGHT);
                egui::Frame::new().inner_margin(20).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    egui::TopBottomPanel::bottom("shell-task-actions")
                        .min_height(64.0)
                        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 8)))
                        .show_inside(ui, |ui| {
                            theme::rule(ui);
                            ui.add_space(8.0);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if let Some(active) = &self.active {
                                        close = ui
                                            .add_enabled(
                                                !active.job.control.is_cancelled(),
                                                egui::Button::new(tr!(language, CancelTask))
                                                    .min_size(egui::vec2(
                                                        90.0,
                                                        theme::CONTROL_HEIGHT,
                                                    )),
                                            )
                                            .clicked();
                                    } else {
                                        close = ui.button(tr!(language, Close)).clicked();
                                        if self.retry_path.is_some() {
                                            retry =
                                                theme::primary(ui, tr!(language, Reopen)).clicked();
                                        }
                                    }
                                },
                            );
                        });
                    ui.add(
                        egui::Label::new(source.file_name().unwrap_or_default().to_string_lossy())
                            .truncate(),
                    )
                    .on_hover_text(source.display().to_string());
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("shell-progress")
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            if let Some(active) = &self.active {
                                let progress = active.job.control.snapshot();
                                let cancelled = active.job.control.is_cancelled();
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(if cancelled {
                                        tr!(language, Cancelling)
                                    } else if progress.stage.is_empty() {
                                        active.kind.title(language)
                                    } else {
                                        &progress.stage
                                    });
                                });
                                let fraction = if progress.total > 0 {
                                    (progress.bytes as f64 / progress.total as f64).clamp(0.0, 0.99)
                                        as f32
                                } else {
                                    0.0
                                };
                                let mut bar = egui::ProgressBar::new(fraction)
                                    .desired_width(ui.available_width())
                                    .animate(progress.total == 0);
                                if progress.total > 0 {
                                    bar = bar.show_percentage();
                                }
                                ui.add_space(6.0);
                                ui.add(bar);
                                ui.add(egui::Label::new(&progress.entry).truncate())
                                    .on_hover_text(&progress.entry);
                                let elapsed = active.job.started.elapsed().as_secs_f64();
                                theme::caption(
                                    ui,
                                    tr!(
                                        language,
                                        TaskProgress,
                                        theme::bytes(progress.bytes),
                                        theme::bytes(progress.total),
                                        progress.completed,
                                        progress.entries,
                                        elapsed
                                    ),
                                );
                                let speed = progress.bytes as f64 / elapsed.max(0.01);
                                if speed > 0.0 {
                                    let remaining = if progress.total > progress.bytes {
                                        tr!(
                                            language,
                                            Remaining,
                                            (progress.total - progress.bytes) as f64 / speed
                                        )
                                    } else {
                                        String::new()
                                    };
                                    theme::caption(
                                        ui,
                                        format!("{}/s{remaining}", theme::bytes(speed as u64)),
                                    );
                                }
                                if cancelled {
                                    theme::caption(ui, tr!(language, CancelWait));
                                }
                            } else if let Some(notice) = &self.notice {
                                ui.label(
                                    RichText::new(&notice.message)
                                        .color(Color32::from_rgb(194, 81, 70)),
                                );
                            }
                            if self.retry_path.is_some() && self.active.is_none() {
                                ui.add_space(8.0);
                                ui.add(
                                    widgets::text_edit(&mut self.read_password)
                                        .password(true)
                                        .hint_text(tr!(language, ExtractionPasswordHint)),
                                );
                                theme::caption(ui, tr!(language, PasswordHint));
                                retry |= context.input_mut(|input| {
                                    input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                });
                            }
                        });
                });
            });
        if close {
            self.close_shell_task(context);
        } else if retry {
            if let Some(path) = self.retry_path.clone() {
                let password = std::mem::take(&mut self.read_password);
                self.open_archive(context, path, (!password.is_empty()).then_some(password));
            }
        }
        // Reuse the normal extraction form and worker, with no manager behind it.
        self.extract_window(context);
        // Keep window controls above destination/password dialogs, like the manager.
        if self.window_frame.show(
            context,
            self.preferences.dark,
            tr!(language, ExtractFiles),
            language,
        ) {
            self.close_shell_task(context);
        }
    }
}
