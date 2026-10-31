use super::*;

impl App {
    pub(super) fn create_window(&mut self, context: &egui::Context) {
        if !self.create_open {
            return;
        }
        let busy = self.busy();
        let had_popup = egui::Popup::is_any_open(context);
        let mut cancel = false;
        let response = theme::modal(
            context,
            "create-archive",
            self.window_frame.corner_radius(context),
        )
        .show(context, |ui| {
            ui.set_width(560.0_f32.min(context.content_rect().width() - 80.0));
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                widgets::paint_icon(ui.painter(), rect, Icon::Archive, theme::palette(ui).accent);
                ui.heading("新建压缩包");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        cancel = widgets::icon_button(ui, Icon::Close, "关闭").clicked();
                    });
                });
            });
            ui.add_space(6.0);
            egui::ScrollArea::vertical()
                .id_salt("create-options")
                .max_height((context.content_rect().height() - 210.0).max(160.0))
                .show(ui, |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("待压缩内容").strong());
                            widgets::badge(ui, &format!("{} 项", self.draft.sources.len()));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !self.draft.sources.is_empty()
                                        && widgets::subtle_button(
                                            ui,
                                            if self.draft.show_sources {
                                                "收起列表"
                                            } else {
                                                "查看 / 编辑"
                                            },
                                        )
                                        .clicked()
                                    {
                                        self.draft.show_sources = !self.draft.show_sources;
                                    }
                                },
                            );
                        });
                        if let Some(first) = self.draft.sources.first() {
                            let name = first.file_name().unwrap_or_default().to_string_lossy();
                            let summary = if self.draft.sources.len() > 1 {
                                format!("{name} 等 {} 项", self.draft.sources.len())
                            } else {
                                name.into_owned()
                            };
                            ui.add(egui::Label::new(summary).truncate())
                                .on_hover_text(first.display().to_string());
                        } else {
                            egui::Frame::new()
                                .fill(theme::palette(ui).soft)
                                .corner_radius(6)
                                .inner_margin(14)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    theme::caption(
                                        ui,
                                        "拖入文件或文件夹，也可以使用下方按钮添加。",
                                    );
                                });
                        }
                        if self.draft.show_sources && !self.draft.sources.is_empty() {
                            let mut remove = None;
                            egui::ScrollArea::vertical()
                                .id_salt("source-list")
                                .max_height(116.0)
                                .show_rows(ui, 48.0, self.draft.sources.len(), |ui, range| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    for index in range {
                                        let path = &self.draft.sources[index];
                                        ui.push_id(index, |ui| {
                                            if widgets::source_row(
                                                ui,
                                                &path
                                                    .file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy(),
                                                &path.display().to_string(),
                                            ) {
                                                remove = Some(index);
                                            }
                                        });
                                    }
                                });
                            if let Some(index) = remove {
                                self.draft.sources.remove(index);
                            }
                        }
                        ui.horizontal(|ui| {
                            if widgets::button(ui, "添加文件", Icon::Plus, false, 0.0).clicked()
                            {
                                self.dialog(context, Dialog::Files);
                            }
                            if widgets::button(ui, "添加文件夹", Icon::Folder, false, 0.0).clicked()
                            {
                                self.dialog(context, Dialog::Folders);
                            }
                            if !self.draft.sources.is_empty()
                                && widgets::subtle_button(ui, "清空").clicked()
                            {
                                self.draft.sources.clear();
                            }
                        });
                        ui.add_space(5.0);
                        theme::rule(ui);
                        ui.add_space(3.0);
                        ui.label(RichText::new("保存为").strong());
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 38.0).max(120.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    self.draft.output.edit(ui, "选择压缩包的保存位置…");
                                },
                            );
                            if widgets::icon_button(ui, Icon::Folder, "浏览保存位置").clicked()
                            {
                                self.dialog(context, Dialog::Save);
                            }
                        });
                        let formats = engine::formats::WRITE_FORMATS;
                        let previous = self.draft.format;
                        ui.columns(2, |columns| {
                            theme::caption(&mut columns[0], "格式");
                            let width = columns[0].available_width();
                            egui::ComboBox::from_id_salt("compression-format")
                                .width(width)
                                .selected_text(formats[self.draft.format].1)
                                .show_ui(&mut columns[0], |ui| {
                                    for (index, (_, name)) in formats.iter().enumerate() {
                                        ui.selectable_value(&mut self.draft.format, index, *name);
                                    }
                                });
                            theme::caption(&mut columns[1], "压缩偏好");
                            let adjustable = !matches!(
                                formats[self.draft.format].0,
                                ArchiveFormat::Zip | ArchiveFormat::Tar | ArchiveFormat::TarLz4
                            );
                            columns[1].add_enabled_ui(adjustable, |ui| {
                                egui::ComboBox::from_id_salt("compression-preset")
                                    .width(ui.available_width())
                                    .selected_text(if adjustable {
                                        ["优先速度", "标准", "优先体积"][self.draft.preset]
                                    } else {
                                        "标准"
                                    })
                                    .show_ui(ui, |ui| {
                                        for (index, name) in
                                            ["优先速度", "标准", "优先体积"].iter().enumerate()
                                        {
                                            ui.selectable_value(
                                                &mut self.draft.preset,
                                                index,
                                                *name,
                                            );
                                        }
                                    });
                            });
                        });
                        if previous != self.draft.format {
                            self.change_format(previous);
                        }
                        let encryption = matches!(
                            formats[self.draft.format].0,
                            ArchiveFormat::Zip | ArchiveFormat::SevenZip
                        );
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(encryption, |ui| {
                                if widgets::check(ui, &mut self.draft.show_password, "密码保护")
                                    .changed()
                                    && !self.draft.show_password
                                {
                                    self.draft.password.clear();
                                    self.draft.confirm.clear();
                                    self.form_error = None;
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::subtle_button(
                                        ui,
                                        if self.draft.show_advanced {
                                            "收起选项"
                                        } else {
                                            "更多选项…"
                                        },
                                    )
                                    .clicked()
                                    {
                                        self.draft.show_advanced = !self.draft.show_advanced;
                                    }
                                },
                            );
                        });
                        if self.draft.show_password && encryption {
                            ui.columns(2, |columns| {
                                columns[0].add(
                                    widgets::text_edit(&mut self.draft.password)
                                        .password(true)
                                        .hint_text("设置密码"),
                                );
                                columns[1].add(
                                    widgets::text_edit(&mut self.draft.confirm)
                                        .password(true)
                                        .hint_text("再次输入密码"),
                                );
                            });
                            theme::caption(ui, "密码仅用于本次任务，不会保存。");
                        }
                        if self.draft.show_advanced {
                            widgets::check(ui, &mut self.draft.replace, "替换已有同名压缩包");
                            theme::caption(
                                ui,
                                "保留目录结构；链接和特殊文件会跳过，结果中报告数量。",
                            );
                        }
                        if let Some(error) = &self.form_error {
                            ui.label(RichText::new(error).color(Color32::from_rgb(194, 81, 70)));
                        }
                    });
                });
            ui.add_space(8.0);
            theme::rule(ui);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                theme::caption(ui, "拖放可继续添加来源");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = !busy
                        && !self.draft.sources.is_empty()
                        && !self.draft.output.path.as_os_str().is_empty();
                    ui.add_enabled_ui(ready, |ui| {
                        if widgets::button(ui, "开始压缩", Icon::ArrowRight, true, 0.0).clicked()
                        {
                            self.create_archive(context);
                        }
                    });
                    if ui
                        .add_enabled(
                            !busy,
                            egui::Button::new("取消")
                                .min_size(egui::vec2(60.0, theme::CONTROL_HEIGHT)),
                        )
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        });
        if !busy && (cancel || response.should_close()) {
            self.create_open = false;
            self.draft.password.clear();
            self.draft.confirm.clear();
            self.draft.show_password = false;
            self.form_error = None;
        } else if self.create_open
            && !busy
            && !had_popup
            && !self.draft.sources.is_empty()
            && !self.draft.output.path.as_os_str().is_empty()
            && context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            self.create_archive(context);
        }
    }
    fn change_format(&mut self, previous: usize) {
        let formats = engine::formats::WRITE_FORMATS;
        let old_suffix = format!(".{}", formats[previous].0);
        if !self.draft.output.path.as_os_str().is_empty() {
            let name = self
                .draft
                .output
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let stem = name.strip_suffix(&old_suffix).unwrap_or(&name);
            self.draft.output.set(
                self.draft
                    .output
                    .path
                    .with_file_name(format!("{stem}.{}", formats[self.draft.format].0)),
            );
        }
        if !matches!(
            formats[self.draft.format].0,
            ArchiveFormat::Zip | ArchiveFormat::SevenZip
        ) {
            self.draft.password.clear();
            self.draft.confirm.clear();
            self.draft.show_password = false;
        }
    }
}
