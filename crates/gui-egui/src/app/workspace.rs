use super::*;

impl App {
    pub(super) fn workspace(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let busy = self.busy();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                theme::title(
                    ui,
                    "ARCHIVE / WORKSPACE",
                    "归档浏览",
                    "浏览、选择，解压所需的内容。",
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if widgets::button(ui, "打开归档", Icon::Folder, false, 130.0)
                        .on_hover_text("Ctrl/⌘ O")
                        .clicked()
                    {
                        self.dialog(context, Dialog::Open);
                    }
                });
            });
        });
        ui.add_space(22.0);
        if let Some(path) = self.retry_path.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("未能打开：{}", path.display()));
                ui.add_enabled_ui(!busy, |ui| {
                    if widgets::button(ui, "输入密码重试", Icon::Lock, false, 0.0).clicked() {
                        self.unlock = Some(Unlock {
                            path,
                            password: String::new(),
                        });
                    }
                });
            });
            ui.add_space(12.0);
        }
        if self.archive.is_none() {
            self.welcome(ui, context);
            return;
        }
        let mut verify = false;
        let mut reload = None;
        let mut preview_entry = None;
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(archive) = &mut self.archive else {
                return;
            };
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 10, theme::palette(ui).tint);
                widgets::paint_icon(
                    ui.painter(),
                    rect.shrink(10.0),
                    Icon::Archive,
                    theme::palette(ui).accent,
                );
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 155.0).max(100.0));
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                archive
                                    .path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            )
                            .size(17.0)
                            .strong(),
                        )
                        .truncate(),
                    )
                    .on_hover_text(archive.path.display().to_string());
                    theme::caption(
                        ui,
                        format!(
                            "{} 个条目  ·  {}",
                            archive.entries.len(),
                            theme::bytes(archive.bytes)
                        ),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        if widgets::icon_button(ui, Icon::Shield, "测试完整性").clicked() {
                            verify = true;
                        }
                        if widgets::icon_button(ui, Icon::Refresh, "重新读取").clicked() {
                            reload = Some(archive.path.clone());
                        }
                    });
                    widgets::badge(ui, &archive.format.to_string().to_uppercase());
                });
            });
        });
        if verify {
            self.verify(context);
        }
        if let Some(path) = reload {
            let password = std::mem::take(&mut self.read_password);
            self.open_archive(
                context,
                path,
                if password.is_empty() {
                    None
                } else {
                    Some(password)
                },
            );
        }
        ui.add_space(16.0);
        let Some(archive) = &mut self.archive else {
            return;
        };
        let browser = &mut archive.browser;
        theme::card(ui).inner_margin(12).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled_ui(!browser.directory.is_empty(), |ui| {
                    if widgets::icon_button(ui, Icon::ArrowUp, "上一级 / Backspace").clicked() {
                        browser.parent();
                        self.range_anchor = None;
                    }
                });
                if ui.add(egui::Button::new("根目录").frame(false)).clicked() {
                    browser.navigate(String::new());
                    self.range_anchor = None;
                }
                let parts: Vec<_> = browser
                    .directory
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                let mut prefix = String::new();
                for part in parts {
                    if !prefix.is_empty() {
                        prefix.push('/');
                    }
                    prefix.push_str(&part);
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
                    widgets::paint_icon(
                        ui.painter(),
                        rect,
                        Icon::Chevron,
                        theme::palette(ui).muted,
                    );
                    if ui.add(egui::Button::new(&part).frame(false)).clicked() {
                        browser.navigate(prefix.clone());
                        self.range_anchor = None;
                    }
                }
            });
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(20.0, 24.0), egui::Sense::hover());
                widgets::paint_icon(ui.painter(), rect, Icon::Search, theme::palette(ui).muted);
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut browser.query)
                            .margin(egui::Margin::symmetric(10, 8))
                            .id(egui::Id::new("archive-search"))
                            .hint_text("搜索当前文件夹…   Ctrl/⌘ F")
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    browser.refresh();
                    self.range_anchor = None;
                }
            });
            ui.horizontal(|ui| {
                let mut all = !browser.visible.is_empty()
                    && browser
                        .visible
                        .iter()
                        .all(|row| browser.selection.contains(&row.path));
                if widgets::check(ui, &mut all, "全选").changed() {
                    for row in &browser.visible {
                        if all {
                            browser.selection.insert(row.path.clone());
                        } else {
                            browser.selection.remove(&row.path);
                        }
                    }
                }
                if ui
                    .add(egui::Button::new("名称 A–Z").small().frame(false))
                    .clicked()
                {
                    browser.size_descending = false;
                    browser.refresh();
                    self.range_anchor = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new("大小优先").small().frame(false))
                        .clicked()
                    {
                        browser.size_descending = true;
                        browser.refresh();
                        self.range_anchor = None;
                    }
                });
            });
            theme::rule(ui);
            let height = (ui.available_height() - 55.0).max(80.0);
            let mut navigate = None;
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                egui::ScrollArea::vertical()
                    .id_salt("archive-rows")
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show_rows(ui, 44.0, browser.visible.len(), |ui, range| {
                        for index in range {
                            let row = browser.visible[index].clone();
                            ui.push_id(&row.path, |ui| {
                                let checked = browser.selection.contains(&row.path);
                                let detail = if row.is_dir {
                                    "文件夹".into()
                                } else {
                                    theme::bytes(row.size)
                                };
                                let (response, toggle) =
                                    widgets::file_row(ui, &row.name, &detail, row.is_dir, checked);
                                if toggle {
                                    browser.toggle(&row.path);
                                    self.range_anchor = Some(index);
                                } else if response.double_clicked() && !busy {
                                    if row.is_dir {
                                        navigate = Some(row.path.clone());
                                    } else {
                                        preview_entry = row.entry_index;
                                    }
                                } else if response.clicked() {
                                    let modifiers = ui.input(|input| input.modifiers);
                                    if modifiers.shift {
                                        let anchor = self
                                            .range_anchor
                                            .unwrap_or(index)
                                            .min(browser.visible.len().saturating_sub(1));
                                        for row in
                                            &browser.visible[anchor.min(index)..=anchor.max(index)]
                                        {
                                            browser.selection.insert(row.path.clone());
                                        }
                                    } else if modifiers.command || modifiers.ctrl {
                                        browser.toggle(&row.path);
                                        self.range_anchor = Some(index);
                                    } else {
                                        browser.selection.clear();
                                        browser.selection.insert(row.path.clone());
                                        self.range_anchor = Some(index);
                                    }
                                }
                                response.on_hover_text(format!(
                                    "{}\n{}",
                                    row.name,
                                    if row.is_dir {
                                        "双击进入；勾选可提取全部内容"
                                    } else {
                                        "双击预览文本；Ctrl/⌘ 多选，Shift 连选"
                                    }
                                ));
                            });
                        }
                        if browser.visible.is_empty() {
                            theme::caption(ui, "此文件夹为空，或没有匹配的项目。");
                        }
                    });
            });
            theme::rule(ui);
            ui.horizontal(|ui| {
                theme::caption(
                    ui,
                    format!(
                        "{} 项  ·  已选 {} 项",
                        browser.visible.len(),
                        browser.selection.len()
                    ),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !browser.selection.is_empty()
                        && ui
                            .add(egui::Button::new("清空选择").small().frame(false))
                            .clicked()
                    {
                        browser.selection.clear();
                        self.range_anchor = None;
                    }
                });
            });
            if let Some(path) = navigate {
                browser.navigate(path);
                self.range_anchor = None;
            }
        });
        if let Some(index) = preview_entry {
            if let Some(entry) = archive.entries.get(index).cloned() {
                let path = archive.path.clone();
                let password = std::mem::take(&mut self.read_password);
                self.start(context, Kind::Preview, move |control| {
                    engine::preview(
                        path,
                        if password.is_empty() {
                            None
                        } else {
                            Some(password)
                        },
                        entry,
                        control,
                    )
                    .map(Outcome::Preview)
                });
            }
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let height = ui.available_height();
        let width = ui.available_width();
        theme::card(ui).show(ui, |ui| {
            ui.set_min_size(egui::vec2(width - 42.0, (height - 70.0).max(320.0)));
            ui.add_space(((height - 480.0) / 2.0).max(8.0));
            ui.vertical_centered(|ui| {
                widgets::illustration(ui);
                ui.add_space(12.0);
                ui.label(RichText::new("让文件轻装上阵").size(28.0).strong());
                theme::caption(ui, "拖入压缩包浏览内容，或添加文件开始压缩。");
                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - 310.0) / 2.0).max(0.0));
                    ui.add_enabled_ui(!self.busy(), |ui| {
                        if widgets::button(ui, "打开归档", Icon::Folder, true, 150.0).clicked()
                        {
                            self.dialog(context, Dialog::Open);
                        }
                    });
                    if widgets::button(ui, "新建压缩", Icon::Plus, false, 150.0).clicked() {
                        self.page = Page::Create;
                    }
                });
                ui.add_space(24.0);
                theme::caption(ui, "ZIP  /  7z  /  TAR 及常见归档格式");
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            widgets::paint_icon(ui.painter(), rect, Icon::Shield, theme::palette(ui).muted);
            theme::caption(ui, "文件在本地处理，密码仅用于本次操作。");
        });
    }

    pub(super) fn extract_panel(&mut self, context: &egui::Context) {
        egui::SidePanel::right("extract-options")
            .exact_width(260.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(context.style().visuals.window_fill)
                    .inner_margin(20),
            )
            .show(context, |ui| {
                let count = self
                    .archive
                    .as_ref()
                    .map_or(0, |a| a.browser.selection.len());
                let reserved = if count > 0 { 110.0 } else { 60.0 };
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - reserved).max(100.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(12.0);
                        theme::title(ui, "EXTRACT", "提取文件", "只取所需，保留目录结构。");
                        ui.add_space(20.0);
                        theme::rule(ui);
                        ui.add_space(12.0);
                        let count = self
                            .archive
                            .as_ref()
                            .map_or(0, |a| a.browser.selection.len());
                        widgets::badge(
                            ui,
                            &if count > 0 {
                                format!("已选择 {count} 项")
                            } else {
                                "整个归档".into()
                            },
                        );
                        ui.add_space(14.0);
                        ui.label(RichText::new("保存位置").strong());
                        let busy = self.busy();
                        ui.add_enabled_ui(!busy, |ui| {
                            self.destination.edit(ui, "输出文件夹");
                            if widgets::button(
                                ui,
                                "选择文件夹",
                                Icon::Folder,
                                false,
                                ui.available_width(),
                            )
                            .clicked()
                            {
                                self.dialog(context, Dialog::Destination);
                            }
                            ui.add_space(14.0);
                            widgets::check(ui, &mut self.overwrite, "替换同名文件");
                        });
                        theme::caption(
                            ui,
                            if self.overwrite {
                                "成功提取后替换同名文件。"
                            } else {
                                "默认跳过同名文件，保留原有内容。"
                            },
                        );
                        let encrypted = self.archive.as_ref().is_some_and(|a| {
                            matches!(
                                a.format,
                                ArchiveFormat::Zip | ArchiveFormat::SevenZip | ArchiveFormat::Rar
                            )
                        });
                        if encrypted {
                            ui.add_space(20.0);
                            ui.label("归档密码（如需要）");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.read_password)
                                    .margin(egui::Margin::symmetric(10, 8))
                                    .password(true)
                                    .hint_text("仅供下一次操作")
                                    .desired_width(f32::INFINITY),
                            );
                        }
                        theme::caption(ui, "勾选文件夹将包含其全部内容。可跨目录选择多个项目。");
                    });
                ui.add_space(12.0);
                let enabled = !self.busy() && !self.destination.path.as_os_str().is_empty();
                ui.add_enabled_ui(enabled, |ui| {
                    let label = if count > 0 {
                        format!("解压选中项（{count}）")
                    } else {
                        "解压全部".into()
                    };
                    if theme::primary(ui, &label).clicked() {
                        self.extract(context, count == 0);
                    }
                    if count > 0
                        && widgets::button(
                            ui,
                            "解压全部",
                            Icon::Archive,
                            false,
                            ui.available_width(),
                        )
                        .clicked()
                    {
                        self.extract(context, true);
                    }
                });
            });
    }
}
