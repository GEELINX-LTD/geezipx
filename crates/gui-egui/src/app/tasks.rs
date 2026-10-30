use super::*;

impl App {
    pub(super) fn status_bar(&mut self, context: &egui::Context) {
        let controls_enabled = !self.modal_open();
        egui::TopBottomPanel::bottom("task-status")
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(self.preferences.dark).canvas)
                    .inner_margin(egui::Margin::symmetric(14, 7)),
            )
            .show(context, |ui| {
                ui.set_min_height(30.0);
                if let Some(active) = &self.active {
                    let progress = active.job.control.snapshot();
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(active.kind.title()).strong());
                        let cancelled = active.job.control.is_cancelled();
                        let stage_width = (ui.available_width() - 180.0).max(100.0);
                        ui.add_sized(
                            [stage_width, 24.0],
                            egui::Label::new(if cancelled {
                                "正在安全取消…"
                            } else {
                                &progress.stage
                            })
                            .truncate(),
                        );
                        ui.add_enabled_ui(controls_enabled, |ui| {
                            if widgets::subtle_button(ui, "详情").clicked() {
                                self.tasks_open = true;
                            }
                            if active.kind != Kind::Dialog
                                && ui
                                    .add_enabled(!cancelled, egui::Button::new("取消任务").small())
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
                                format!(
                                    " · 约 {:.0} 秒",
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
                                    format!("已用时 {elapsed:.0} 秒")
                                },
                            );
                        });
                    }
                } else if self.export.receiver.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("拖到目标文件夹后松开鼠标 · Esc 取消");
                    });
                } else if self.notice.is_some() {
                    self.notices(ui);
                } else {
                    ui.horizontal(|ui| {
                        if let Some(archive) = &self.archive {
                            let browser = &archive.browser;
                            theme::caption(ui, format!("{} 项", browser.visible.len()));
                            if !browser.selection.is_empty() {
                                theme::caption(
                                    ui,
                                    format!(
                                        "已选 {} 项 · {}",
                                        browser.selection.len(),
                                        theme::bytes(browser.selected_bytes)
                                    ),
                                );
                                if browser.hidden_selected > 0 {
                                    ui.label(
                                        RichText::new(format!(
                                            "其中 {} 项被筛选隐藏",
                                            browser.hidden_selected
                                        ))
                                        .size(12.0)
                                        .color(theme::palette(ui).accent),
                                    );
                                }
                            }
                        } else {
                            theme::caption(ui, "准备就绪");
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(archive) = &self.archive {
                                theme::caption(
                                    ui,
                                    format!(
                                        "共 {} 个条目 · {}",
                                        archive.entries.len(),
                                        theme::bytes(archive.bytes)
                                    ),
                                );
                            } else {
                                widgets::shortcut_hint(ui, "新建", widgets::command(egui::Key::N));
                                ui.add_space(16.0);
                                widgets::shortcut_hint(ui, "打开", widgets::command(egui::Key::O));
                            }
                        });
                    });
                }
            });
    }
    pub(super) fn tasks_window(&mut self, context: &egui::Context) {
        if !self.tasks_open {
            return;
        }
        let mut open_output = None;
        let response = theme::modal(context, "task-history").show(context, |ui| {
            ui.set_width(580.0_f32.min(context.content_rect().width() - 80.0));
            ui.horizontal(|ui| {
                ui.heading("任务记录");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.history.is_empty() && widgets::subtle_button(ui, "清空记录").clicked()
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
                                ui.label(RichText::new(active.kind.title()).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if active.kind != Kind::Dialog
                                            && ui
                                                .add_enabled(
                                                    !active.job.control.is_cancelled(),
                                                    egui::Button::new("取消任务"),
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
                                format!(
                                    "{} / {} · {} / {} 项 · 已用时 {:.1} 秒",
                                    theme::bytes(progress.bytes),
                                    theme::bytes(progress.total),
                                    progress.completed,
                                    progress.entries,
                                    active.job.started.elapsed().as_secs_f64()
                                ),
                            );
                            if active.job.control.is_cancelled() {
                                theme::caption(ui, "正在安全取消，等待当前编解码步骤结束。");
                            }
                        });
                    }
                    if self.history.is_empty() && self.active.is_none() {
                        ui.add_space(24.0);
                        theme::caption(ui, "本次会话的压缩、解压与完整性测试结果会显示在这里。");
                        ui.add_space(24.0);
                    }
                    for (index, history) in self.history.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            theme::card(ui).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (status, color) = if history.cancelled {
                                        ("已取消", theme::palette(ui).muted)
                                    } else if history.failed {
                                        ("未完成", Color32::from_rgb(194, 81, 70))
                                    } else {
                                        ("已完成", theme::palette(ui).accent)
                                    };
                                    ui.label(RichText::new(history.kind.title()).strong());
                                    ui.label(RichText::new(status).size(12.0).color(color));
                                    theme::caption(
                                        ui,
                                        format!("{:.1} 秒", history.elapsed.as_secs_f64()),
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
                                        if widgets::icon_button(ui, Icon::Folder, "打开输出文件夹")
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
                ui.add(egui::Button::new("关闭").min_size(egui::vec2(80.0, 32.0)))
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
