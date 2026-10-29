use super::*;

impl App {
    pub(super) fn create_page(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        theme::title(
            ui,
            "CREATE / NEW ARCHIVE",
            "新建压缩",
            "把文件收进一个压缩包，轻松保存与分享。",
        );
        ui.add_space(22.0);
        let busy = self.busy();
        let height = ui.available_height();
        ui.columns(2, |columns| {
            let ui = &mut columns[0];
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(height - 42.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("待压缩内容").size(16.0).strong());
                    widgets::badge(ui, &format!("{} 项", self.draft.sources.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled_ui(!busy && !self.draft.sources.is_empty(), |ui| {
                            if widgets::icon_button(ui, Icon::Close, "清空来源").clicked() {
                                self.draft.sources.clear();
                            }
                        });
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 10.0) / 2.0;
                    ui.add_enabled_ui(!busy, |ui| {
                        if widgets::button(ui, "添加文件", Icon::Plus, false, width).clicked() {
                            self.dialog(context, Dialog::Files);
                        }
                        if widgets::button(ui, "文件夹", Icon::Folder, false, width).clicked() {
                            self.dialog(context, Dialog::Folders);
                        }
                    });
                });
                ui.add_space(12.0);
                theme::rule(ui);
                let list_height = (height - 205.0).max(130.0);
                let mut remove = None;
                egui::ScrollArea::vertical()
                    .id_salt("source-list")
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show_rows(ui, 66.0, self.draft.sources.len(), |ui, range| {
                        for index in range {
                            let path = &self.draft.sources[index];
                            ui.push_id(index, |ui| {
                                ui.add_enabled_ui(!busy, |ui| {
                                    if widgets::source_row(
                                        ui,
                                        &path.file_name().unwrap_or_default().to_string_lossy(),
                                        &path.display().to_string(),
                                    ) {
                                        remove = Some(index);
                                    }
                                });
                            });
                        }
                        if self.draft.sources.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                if list_height >= 300.0 {
                                    widgets::illustration(ui);
                                } else {
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(64.0, 64.0),
                                        egui::Sense::hover(),
                                    );
                                    widgets::paint_icon(
                                        ui.painter(),
                                        rect.shrink(8.0),
                                        Icon::Folder,
                                        theme::palette(ui).accent,
                                    );
                                }
                                ui.label("把文件或文件夹拖到这里");
                                theme::caption(ui, "也可以使用上方按钮添加内容");
                            });
                        }
                    });
                if let Some(index) = remove {
                    self.draft.sources.remove(index);
                }
                theme::rule(ui);
                theme::caption(ui, "保留文件夹结构 · 支持多个来源");
            });
            let ui = &mut columns[1];
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(height - 42.0);
                ui.label(RichText::new("归档设置").size(16.0).strong());
                theme::caption(ui, "常用选项，一步完成。");
                ui.add_space(12.0);
                egui::ScrollArea::vertical()
                    .id_salt("compression-settings")
                    .max_height((height - 168.0).max(150.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!busy, |ui| {
                            ui.label(RichText::new("归档格式").strong());
                            let formats = engine::formats::WRITE_FORMATS;
                            let previous = self.draft.format;
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 20.0) / 3.0;
                                for (index, (_, name)) in formats.iter().take(3).enumerate() {
                                    if widgets::choice(ui, name, self.draft.format == index, width)
                                        .clicked()
                                    {
                                        self.draft.format = index;
                                    }
                                }
                            });
                            egui::ComboBox::from_id_salt("compression-format")
                                .width(ui.available_width() - 12.0)
                                .selected_text(if self.draft.format < 3 {
                                    "更多格式…"
                                } else {
                                    formats[self.draft.format].1
                                })
                                .show_ui(ui, |ui| {
                                    for (index, (_, name)) in formats.iter().enumerate() {
                                        ui.selectable_value(&mut self.draft.format, index, *name);
                                    }
                                });
                            if previous != self.draft.format {
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
                                    self.draft.output.set(self.draft.output.path.with_file_name(
                                        format!("{stem}.{}", formats[self.draft.format].0),
                                    ));
                                }
                                if !matches!(
                                    formats[self.draft.format].0,
                                    ArchiveFormat::Zip | ArchiveFormat::SevenZip
                                ) {
                                    self.draft.password.clear();
                                    self.draft.confirm.clear();
                                }
                            }
                            let format = formats[self.draft.format].0;
                            theme::caption(
                                ui,
                                match format {
                                    ArchiveFormat::Zip => "兼容性好 · Deflate 压缩 · 支持密码",
                                    ArchiveFormat::SevenZip => "高压缩率 · 支持 AES-256 密码",
                                    ArchiveFormat::Tar => "只打包，保留文件内容",
                                    _ => "优先体积会增加处理时间",
                                },
                            );
                            if !matches!(
                                format,
                                ArchiveFormat::Zip | ArchiveFormat::Tar | ArchiveFormat::TarLz4
                            ) {
                                ui.add_space(10.0);
                                ui.label("压缩偏好");
                                ui.horizontal(|ui| {
                                    let width = (ui.available_width() - 20.0) / 3.0;
                                    for (index, name) in
                                        ["优先速度", "均衡", "优先体积"].iter().enumerate()
                                    {
                                        if widgets::choice(
                                            ui,
                                            name,
                                            self.draft.preset == index,
                                            width,
                                        )
                                        .clicked()
                                        {
                                            self.draft.preset = index;
                                        }
                                    }
                                });
                            }
                            ui.add_space(12.0);
                            ui.label(RichText::new("保存为").strong());
                            self.draft.output.edit(ui, "选择输出文件路径…");
                            ui.horizontal(|ui| {
                                if widgets::icon_button(ui, Icon::Folder, "浏览保存位置").clicked()
                                {
                                    self.dialog(context, Dialog::Save);
                                }
                                widgets::check(ui, &mut self.draft.replace, "替换同名压缩包");
                            });
                            if matches!(format, ArchiveFormat::Zip | ArchiveFormat::SevenZip) {
                                ui.add_space(8.0);
                                ui.label(RichText::new("密码保护").strong());
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.draft.password)
                                        .margin(egui::Margin::symmetric(10, 8))
                                        .password(true)
                                        .hint_text("设置密码（可选）")
                                        .desired_width(f32::INFINITY),
                                );
                                if !self.draft.password.is_empty() {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.draft.confirm)
                                            .margin(egui::Margin::symmetric(10, 8))
                                            .password(true)
                                            .hint_text("再次输入密码")
                                            .desired_width(f32::INFINITY),
                                    );
                                }
                                theme::caption(ui, "密码仅用于本次任务，不会保存。");
                            }
                        });
                    });
                ui.add_space(8.0);
                let ready = !busy
                    && !self.draft.sources.is_empty()
                    && !self.draft.output.path.as_os_str().is_empty();
                ui.add_enabled_ui(ready, |ui| {
                    if theme::primary(ui, "开始压缩").clicked() {
                        self.create_archive(context);
                    }
                });
            });
        });
    }
}
