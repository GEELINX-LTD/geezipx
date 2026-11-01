use super::*;

impl App {
    pub(super) fn status_bar(&mut self, parent: &mut egui::Ui) -> egui::Rect {
        let language = self.preferences.language;
        let controls_enabled = !self.modal_open();
        let radius = self
            .window_frame
            .corner_radius(parent.ctx())
            .saturating_sub(1);
        egui::TopBottomPanel::bottom("task-status")
            .min_height(44.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(self.preferences.dark).canvas)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: radius,
                        se: radius,
                    })
                    .inner_margin(egui::Margin::symmetric(14, 7)),
            )
            .show_inside(parent, |ui| {
                ui.set_min_height(30.0);
                if let Some(active) = &self.active {
                    let progress = active.job.control.snapshot();
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(active.kind.title(language)).strong());
                        let cancelled = active.job.control.is_cancelled();
                        let stage_width = (ui.available_width() - 180.0).max(100.0);
                        ui.add_sized(
                            [stage_width, 24.0],
                            egui::Label::new(if cancelled {
                                tr!(language, Cancelling)
                            } else {
                                &progress.stage
                            })
                            .truncate(),
                        );
                        ui.add_enabled_ui(controls_enabled, |ui| {
                            if widgets::subtle_button(ui, tr!(language, Details)).clicked() {
                                self.tasks_open = true;
                            }
                            if active.kind != Kind::Dialog
                                && ui
                                    .add_enabled(
                                        !cancelled,
                                        egui::Button::new(tr!(language, CancelTask)).small(),
                                    )
                                    .clicked()
                            {
                                active.job.control.cancel();
                            }
                        });
                    });
                    if active.kind != Kind::Dialog {
                        let fraction = if progress.total > 0 {
                            (progress.bytes as f64 / progress.total as f64).clamp(0.0, 0.99) as f32
                        } else {
                            0.0
                        };
                        ui.add(
                            egui::ProgressBar::new(fraction)
                                .desired_width(ui.available_width())
                                .desired_height(5.0)
                                .animate(progress.total == 0),
                        );
                        ui.horizontal(|ui| {
                            let entry_width = (ui.available_width() - 250.0).max(120.0);
                            ui.add_sized(
                                [entry_width, 18.0],
                                egui::Label::new(
                                    RichText::new(&progress.entry)
                                        .size(12.0)
                                        .color(theme::palette(ui).muted),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&progress.entry);
                            let elapsed = active.job.started.elapsed().as_secs_f64();
                            let speed = progress.bytes as f64 / elapsed.max(0.01);
                            let remaining = if speed > 0.0 && progress.total > progress.bytes {
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
                                if progress.total > 0 {
                                    format!(
                                        "{:.0}% · {}/s{remaining}",
                                        fraction * 100.0,
                                        theme::bytes(speed as u64)
                                    )
                                } else {
                                    tr!(language, Elapsed, elapsed)
                                },
                            );
                        });
                    }
                } else if self.export.receiver.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr!(language, DragHint));
                    });
                } else {
                    ui.horizontal(|ui| {
                        if let Some(archive) = &self.archive {
                            let browser = &archive.browser;
                            theme::caption(ui, tr!(language, ItemCount, browser.visible.len()));
                            if !browser.selection.is_empty() {
                                theme::caption(
                                    ui,
                                    tr!(
                                        language,
                                        SelectedCount,
                                        browser.selection.len(),
                                        theme::bytes(browser.selected_bytes)
                                    ),
                                );
                                if browser.hidden_selected > 0 {
                                    ui.label(
                                        RichText::new(tr!(
                                            language,
                                            HiddenCount,
                                            browser.hidden_selected
                                        ))
                                        .size(12.0)
                                        .color(theme::palette(ui).accent),
                                    );
                                }
                            }
                        } else {
                            theme::caption(ui, tr!(language, Ready));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(archive) = &self.archive {
                                theme::caption(
                                    ui,
                                    tr!(
                                        language,
                                        TotalEntries,
                                        archive.entries.len(),
                                        theme::bytes(archive.bytes)
                                    ),
                                );
                            } else {
                                widgets::shortcut_hint(
                                    ui,
                                    tr!(language, New),
                                    widgets::command(egui::Key::N),
                                );
                                ui.add_space(16.0);
                                widgets::shortcut_hint(
                                    ui,
                                    tr!(language, Open),
                                    widgets::command(egui::Key::O),
                                );
                            }
                        });
                    });
                }
            })
            .response
            .rect
    }
    pub(super) fn tasks_window(&mut self, context: &egui::Context) {
        if !self.tasks_open {
            return;
        }
        let language = self.preferences.language;
        let mut open_output = None;
        let response = theme::modal(
            context,
            "task-history",
            self.window_frame.corner_radius(context),
        )
        .show(context, |ui| {
            ui.set_width(580.0_f32.min(context.content_rect().width() - 80.0));
            ui.horizontal(|ui| {
                ui.heading(tr!(language, Tasks));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.history.is_empty()
                        && widgets::subtle_button(ui, tr!(language, ClearHistory)).clicked()
                    {
                        self.history.clear();
                    }
                });
            });
            egui::ScrollArea::vertical()
                .id_salt("history")
                .max_height((context.content_rect().height() - 210.0).max(150.0))
                .show(ui, |ui| {
                    if let Some(active) = &self.active {
                        let progress = active.job.control.snapshot();
                        theme::card(ui).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(active.kind.title(language)).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if active.kind != Kind::Dialog
                                            && ui
                                                .add_enabled(
                                                    !active.job.control.is_cancelled(),
                                                    egui::Button::new(tr!(language, CancelTask)),
                                                )
                                                .clicked()
                                        {
                                            active.job.control.cancel();
                                        }
                                    },
                                );
                            });
                            ui.label(&progress.stage);
                            ui.add(egui::Label::new(&progress.entry).truncate())
                                .on_hover_text(&progress.entry);
                            if progress.total > 0 {
                                ui.add(
                                    egui::ProgressBar::new(
                                        (progress.bytes as f64 / progress.total as f64)
                                            .clamp(0.0, 0.99)
                                            as f32,
                                    )
                                    .desired_width(f32::INFINITY)
                                    .show_percentage(),
                                );
                            }
                            theme::caption(
                                ui,
                                tr!(
                                    language,
                                    TaskProgress,
                                    theme::bytes(progress.bytes),
                                    theme::bytes(progress.total),
                                    progress.completed,
                                    progress.entries,
                                    active.job.started.elapsed().as_secs_f64()
                                ),
                            );
                            if active.job.control.is_cancelled() {
                                theme::caption(ui, tr!(language, CancelWait));
                            }
                        });
                    }
                    if self.history.is_empty() && self.active.is_none() {
                        ui.add_space(24.0);
                        theme::caption(ui, tr!(language, NoTasks));
                        ui.add_space(24.0);
                    }
                    for (index, history) in self.history.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            theme::card(ui).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (status, color) = if history.cancelled {
                                        (tr!(language, Cancelled), theme::palette(ui).muted)
                                    } else if history.failed {
                                        (tr!(language, Failed), Color32::from_rgb(194, 81, 70))
                                    } else {
                                        (tr!(language, Completed), theme::palette(ui).accent)
                                    };
                                    ui.label(RichText::new(history.kind.title(language)).strong());
                                    ui.label(RichText::new(status).size(12.0).color(color));
                                    theme::caption(
                                        ui,
                                        tr!(language, Seconds, history.elapsed.as_secs_f64()),
                                    );
                                });
                                ui.label(&history.message);
                                if let Some(path) = &history.output {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(path.display().to_string())
                                                    .size(12.0)
                                                    .color(theme::palette(ui).muted),
                                            )
                                            .truncate(),
                                        );
                                        if widgets::icon_button(
                                            ui,
                                            Icon::Folder,
                                            tr!(language, OpenOutputFolder),
                                        )
                                        .clicked()
                                        {
                                            open_output = Some(if history.kind == Kind::Compress {
                                                path.parent()
                                                    .unwrap_or(std::path::Path::new("."))
                                                    .to_owned()
                                            } else {
                                                path.clone()
                                            });
                                        }
                                    });
                                }
                            });
                        });
                    }
                });
            ui.add_space(6.0);
            theme::rule(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::Button::new(tr!(language, Close))
                        .min_size(egui::vec2(64.0, theme::CONTROL_HEIGHT)),
                )
                .clicked()
            })
            .inner
        });
        if response.inner || response.should_close() {
            self.tasks_open = false;
        }
        if let Some(path) = open_output {
            self.open_output(path);
        }
    }
}
