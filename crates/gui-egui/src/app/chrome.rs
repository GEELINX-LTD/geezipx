use super::*;

impl App {
    pub(super) fn chrome(&mut self, context: &egui::Context) {
        let enabled = !self.modal_open();
        let idle = !self.busy();
        let has_archive = self.archive.is_some();
        egui::TopBottomPanel::top("main-toolbar")
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(self.preferences.dark).canvas)
                    .inner_margin(egui::Margin::symmetric(12, 5)),
            )
            .show(context, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    egui::MenuBar::new()
                        .style(widgets::menu_bar_style)
                        .config(
                            egui::containers::menu::MenuConfig::new().style(widgets::menu_style),
                        )
                        .ui(ui, |ui| {
                            widgets::menu_bar_button(ui, "文件", |ui| {
                                widgets::menu(ui);
                                if widgets::menu_item(
                                    ui,
                                    "打开归档…",
                                    Some(widgets::command(egui::Key::O)),
                                    idle,
                                )
                                .clicked()
                                {
                                    self.dialog(context, Dialog::Open);
                                    ui.close();
                                }
                                if widgets::menu_item(
                                    ui,
                                    "新建压缩包…",
                                    Some(widgets::command(egui::Key::N)),
                                    idle,
                                )
                                .clicked()
                                {
                                    self.new_archive();
                                    ui.close();
                                }
                                ui.separator();
                                ui.menu_button("最近打开", |ui| {
                                    widgets::menu(ui);
                                    if self.preferences.recent.is_empty() {
                                        theme::caption(ui, "暂无最近归档");
                                    }
                                    let mut open = None;
                                    for path in &self.preferences.recent {
                                        if widgets::menu_item(
                                            ui,
                                            &path.file_name().unwrap_or_default().to_string_lossy(),
                                            None,
                                            idle,
                                        )
                                        .on_hover_text(path.display().to_string())
                                        .clicked()
                                        {
                                            open = Some(path.clone());
                                        }
                                    }
                                    if let Some(path) = open {
                                        self.open_archive(context, path, None);
                                        ui.close();
                                    }
                                    if !self.preferences.recent.is_empty() {
                                        ui.separator();
                                        if widgets::menu_item(ui, "清空记录", None, true).clicked()
                                        {
                                            self.preferences.recent.clear();
                                            ui.close();
                                        }
                                    }
                                });
                            });
                            widgets::menu_bar_button(ui, "编辑", |ui| {
                                widgets::menu(ui);
                                if widgets::menu_item(
                                    ui,
                                    "全选",
                                    Some(widgets::command(egui::Key::A)),
                                    has_archive,
                                )
                                .clicked()
                                {
                                    if let Some(archive) = &mut self.archive {
                                        archive.browser.select_all();
                                    }
                                    ui.close();
                                }
                                if widgets::menu_item(
                                    ui,
                                    "清空选择",
                                    Some(egui::KeyboardShortcut::new(
                                        egui::Modifiers::NONE,
                                        egui::Key::Escape,
                                    )),
                                    has_archive,
                                )
                                .clicked()
                                {
                                    if let Some(archive) = &mut self.archive {
                                        archive.browser.clear_selection();
                                    }
                                    ui.close();
                                }
                                if widgets::menu_item(
                                    ui,
                                    "搜索当前目录…",
                                    Some(widgets::command(egui::Key::F)),
                                    has_archive,
                                )
                                .clicked()
                                {
                                    context.memory_mut(|m| {
                                        m.request_focus(egui::Id::new("archive-search"))
                                    });
                                    ui.close();
                                }
                            });
                            widgets::menu_bar_button(ui, "查看", |ui| {
                                widgets::menu(ui);
                                if widgets::menu_item(
                                    ui,
                                    "上一级",
                                    Some(egui::KeyboardShortcut::new(
                                        egui::Modifiers::ALT,
                                        egui::Key::ArrowUp,
                                    )),
                                    has_archive,
                                )
                                .clicked()
                                {
                                    if let Some(archive) = &mut self.archive {
                                        archive.browser.parent();
                                    }
                                    ui.close();
                                }
                                if widgets::menu_item(ui, "重新读取归档", None, has_archive && idle)
                                    .clicked()
                                {
                                    if let Some(path) =
                                        self.archive.as_ref().map(|a| a.path.clone())
                                    {
                                        self.open_archive(context, path, None);
                                    }
                                    ui.close();
                                }
                                ui.separator();
                                if widgets::menu_item(
                                    ui,
                                    if self.preferences.dark {
                                        "切换到浅色外观"
                                    } else {
                                        "切换到深色外观"
                                    },
                                    None,
                                    true,
                                )
                                .clicked()
                                {
                                    self.preferences.dark = !self.preferences.dark;
                                    theme::apply(context, self.preferences.dark);
                                    ui.close();
                                }
                            });
                            widgets::menu_bar_button(ui, "工具", |ui| {
                                widgets::menu(ui);
                                if widgets::menu_item(
                                    ui,
                                    "测试归档完整性",
                                    None,
                                    idle && has_archive,
                                )
                                .clicked()
                                {
                                    self.verify(context);
                                    ui.close();
                                }
                                if widgets::menu_item(ui, "任务记录…", None, true).clicked() {
                                    self.tasks_open = true;
                                    ui.close();
                                }
                                ui.separator();
                                if widgets::menu_item(ui, "设置…", None, true).clicked() {
                                    self.show_settings();
                                    ui.close();
                                }
                                if has_archive {
                                    ui.separator();
                                    theme::caption(ui, "下一次读取使用的密码");
                                    ui.add(
                                        widgets::text_edit(&mut self.read_password)
                                            .password(true)
                                            .background_color(theme::palette(ui).soft)
                                            .desired_width(232.0)
                                            .hint_text("仅用于下一次操作"),
                                    );
                                }
                            });
                            widgets::menu_bar_button(ui, "帮助", |ui| {
                                widgets::menu(ui);
                                if widgets::menu_item(ui, "关于 GeeZipX…", None, true).clicked()
                                {
                                    self.about = true;
                                    ui.close();
                                }
                            });
                        });
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.add_enabled_ui(idle, |ui| {
                            if widgets::button(ui, "新建", Icon::Plus, false, 0.0)
                                .on_hover_ui(|ui| {
                                    widgets::shortcut_hint(
                                        ui,
                                        "新建压缩包",
                                        widgets::command(egui::Key::N),
                                    )
                                })
                                .clicked()
                            {
                                self.new_archive();
                            }
                            if widgets::button(ui, "打开", Icon::Folder, false, 0.0)
                                .on_hover_ui(|ui| {
                                    widgets::shortcut_hint(
                                        ui,
                                        "打开归档",
                                        widgets::command(egui::Key::O),
                                    )
                                })
                                .clicked()
                            {
                                self.dialog(context, Dialog::Open);
                            }
                        });
                        ui.separator();
                        ui.add_enabled_ui(idle && has_archive, |ui| {
                            let count = self
                                .archive
                                .as_ref()
                                .map_or(0, |a| a.browser.selection.len());
                            let label = if count == 0 {
                                "解压全部…".into()
                            } else {
                                format!("解压所选 ({count})…")
                            };
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if widgets::button(ui, &label, Icon::ArrowRight, true, 0.0)
                                .on_hover_ui(|ui| {
                                    widgets::shortcut_hint(
                                        ui,
                                        "选择目标位置",
                                        widgets::command(egui::Key::E),
                                    )
                                })
                                .clicked()
                            {
                                self.show_extract(false);
                            }
                            let fill = if ui.is_enabled() {
                                theme::ACCENT
                            } else {
                                theme::palette(ui).soft
                            };
                            let dropdown = egui::containers::menu::MenuButton::from_button(
                                egui::Button::new("")
                                    .min_size(egui::vec2(24.0, theme::CONTROL_HEIGHT))
                                    .fill(fill)
                                    .stroke(egui::Stroke::NONE),
                            )
                            .config(
                                egui::containers::menu::MenuConfig::new()
                                    .style(widgets::menu_style),
                            )
                            .ui(ui, |ui| {
                                widgets::menu(ui);
                                if widgets::menu_item(
                                    ui,
                                    "解压所选…",
                                    if count > 0 {
                                        Some(widgets::command(egui::Key::E))
                                    } else {
                                        None
                                    },
                                    count > 0,
                                )
                                .clicked()
                                {
                                    self.show_extract(false);
                                    ui.close();
                                }
                                if widgets::menu_item(
                                    ui,
                                    "解压全部…",
                                    if count == 0 {
                                        Some(widgets::command(egui::Key::E))
                                    } else {
                                        None
                                    },
                                    true,
                                )
                                .clicked()
                                {
                                    self.show_extract(true);
                                    ui.close();
                                }
                                ui.separator();
                                if widgets::menu_item(ui, "全部解压到同名文件夹", None, true)
                                    .clicked()
                                {
                                    self.quick_extract(context, false);
                                    ui.close();
                                }
                                if widgets::menu_item(ui, "全部解压到当前文件夹", None, true)
                                    .clicked()
                                {
                                    self.quick_extract(context, true);
                                    ui.close();
                                }
                                ui.separator();
                                theme::caption(ui, "快捷解压保留已有同名文件");
                            });
                            let response = dropdown.0;
                            if response.hovered() && ui.is_enabled() {
                                ui.painter().rect_filled(
                                    response.rect.shrink(1.0),
                                    6,
                                    egui::Color32::from_rgb(67, 82, 202),
                                );
                            }
                            widgets::paint_icon(
                                ui.painter(),
                                response.rect.shrink(6.0),
                                Icon::ChevronDown,
                                if ui.is_enabled() {
                                    egui::Color32::WHITE
                                } else {
                                    theme::palette(ui).muted
                                },
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    "更多解压方式",
                                )
                            });
                            response.on_hover_text("更多解压方式");
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if widgets::button(ui, "测试", Icon::Shield, false, 0.0)
                                .on_hover_text("读取并验证全部文件的完整性")
                                .clicked()
                            {
                                self.verify(context);
                            }
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(
                                ui,
                                if self.preferences.dark {
                                    Icon::Sun
                                } else {
                                    Icon::Moon
                                },
                                "切换浅色 / 深色",
                            )
                            .clicked()
                            {
                                self.preferences.dark = !self.preferences.dark;
                                theme::apply(context, self.preferences.dark);
                            }
                            if widgets::icon_button(ui, Icon::Clock, "任务记录").clicked() {
                                self.tasks_open = true;
                            }
                            if let Some(archive) = &self.archive {
                                widgets::badge(ui, &archive.format.to_string().to_uppercase());
                            }
                        });
                    });
                });
            });
    }
}
