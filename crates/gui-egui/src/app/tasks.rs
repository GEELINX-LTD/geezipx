use super::*;

impl App {
    pub(super) fn status_bar(&mut self, context: &egui::Context) {
        egui::TopBottomPanel::bottom("task-status")
            .min_height(45.0)
            .frame(
                egui::Frame::new()
                    .fill(context.style().visuals.window_fill)
                    .stroke(egui::Stroke::new(
                        1.0,
                        context
                            .style()
                            .visuals
                            .widgets
                            .noninteractive
                            .bg_stroke
                            .color,
                    ))
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    if let Some(active) = &self.active {
                        let progress = active.job.control.snapshot();
                        ui.spinner();
                        ui.label(RichText::new(active.kind.title()).strong());
                        if progress.total > 0 {
                            let fraction = (progress.bytes as f64 / progress.total as f64)
                                .clamp(0.0, 0.99) as f32;
                            ui.add(
                                egui::ProgressBar::new(fraction)
                                    .desired_width(120.0)
                                    .show_percentage(),
                            );
                        }
                        let label = if active.job.control.is_cancelled() {
                            "正在取消，等待当前编解码步骤结束…".into()
                        } else {
                            progress.stage
                        };
                        ui.add(egui::Label::new(label).truncate());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if active.kind != Kind::Dialog
                                && ui
                                    .add_enabled(
                                        !active.job.control.is_cancelled(),
                                        egui::Button::new("取消任务").small(),
                                    )
                                    .clicked()
                            {
                                active.job.control.cancel();
                            }
                            if ui.small_button("查看任务").clicked() {
                                self.page = Page::Tasks;
                            }
                        });
                    } else {
                        let (status, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(status.center(), 3.0, theme::ACCENT);
                        ui.label("准备就绪");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            theme::caption(
                                ui,
                                "拖放文件以打开或压缩  ·  Ctrl/⌘ O 打开  ·  Ctrl/⌘ N 新建",
                            );
                        });
                    }
                });
            });
    }

    pub(super) fn tasks_page(&mut self, ui: &mut egui::Ui) {
        theme::title(
            ui,
            "ACTIVITY / THIS SESSION",
            "任务中心",
            "当前进度与本次操作的结果，尽在此处。",
        );
        ui.add_space(22.0);
        if let Some(active) = &self.active {
            let progress = active.job.control.snapshot();
            theme::card(ui).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(active.kind.title()).size(20.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                    });
                });
                ui.label(&progress.stage);
                if !progress.entry.is_empty() {
                    ui.add(egui::Label::new(&progress.entry).truncate())
                        .on_hover_text(&progress.entry);
                }
                if progress.total > 0 {
                    let fraction =
                        (progress.bytes as f64 / progress.total as f64).clamp(0.0, 0.99) as f32;
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .desired_width(f32::INFINITY)
                            .show_percentage(),
                    );
                    theme::caption(
                        ui,
                        format!(
                            "{} / {}  ·  {} / {} 项  ·  {}/s  ·  已用时 {:.1} 秒",
                            theme::bytes(progress.bytes),
                            theme::bytes(progress.total),
                            progress.completed,
                            progress.entries,
                            theme::bytes(
                                (progress.bytes as f64
                                    / active.job.started.elapsed().as_secs_f64().max(0.01))
                                    as u64
                            ),
                            active.job.started.elapsed().as_secs_f64()
                        ),
                    );
                }
                if active.job.control.is_cancelled() {
                    ui.label(
                        "正在安全取消。部分格式的目录读取或最终封装需要完成当前步骤后才能停止。",
                    );
                }
            });
            ui.add_space(18.0);
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("最近完成").strong());
            if !self.history.is_empty() && ui.small_button("清空记录").clicked() {
                self.history.clear();
            }
        });
        if self.history.is_empty() {
            theme::card(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.add_space(36.0);
                ui.vertical_centered(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
                    widgets::paint_icon(
                        ui.painter(),
                        rect.shrink(6.0),
                        Icon::Clock,
                        theme::palette(ui).accent,
                    );
                    ui.label(RichText::new("暂时没有任务").size(18.0).strong());
                    theme::caption(ui, "压缩、解压和完整性测试的结果会显示在这里。");
                });
                ui.add_space(36.0);
            });
        }
        let mut open = None;
        egui::ScrollArea::vertical()
            .id_salt("history")
            .show(ui, |ui| {
                for (index, history) in self.history.iter().enumerate() {
                    ui.push_id(index, |ui| {
                        theme::card(ui).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(history.kind.title()).strong());
                                let (status, color) = if history.cancelled {
                                    ("已取消", theme::MUTED)
                                } else if history.failed {
                                    ("未完成", Color32::from_rgb(184, 69, 59))
                                } else {
                                    ("已完成", theme::ACCENT)
                                };
                                ui.label(RichText::new(status).color(color));
                                theme::caption(
                                    ui,
                                    format!("{:.1} 秒", history.elapsed.as_secs_f64()),
                                );
                            });
                            ui.label(&history.message);
                            if let Some(path) = &history.output {
                                theme::caption(ui, path.display().to_string());
                                if widgets::button(ui, "打开输出文件夹", Icon::Folder, false, 0.0)
                                    .clicked()
                                {
                                    open = Some(if history.kind == Kind::Compress {
                                        path.parent()
                                            .unwrap_or(std::path::Path::new("."))
                                            .to_owned()
                                    } else {
                                        path.clone()
                                    });
                                }
                            }
                        });
                        ui.add_space(10.0);
                    });
                }
            });
        if let Some(path) = open {
            self.open_output(path);
        }
    }
}
