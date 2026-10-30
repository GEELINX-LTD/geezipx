use super::*;
use crate::fonts::{self, Selection};

pub(super) struct Settings {
    pub(super) open: bool,
    pub(super) available: bool,
    search: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open: false,
            available: true,
            search: String::new(),
        }
    }
}

impl App {
    pub(super) fn show_settings(&mut self) {
        self.settings.search.clear();
        self.settings.open = true;
    }

    fn change_font(&mut self, context: &egui::Context, selection: Selection) {
        self.settings.available = theme::fonts(context, &selection);
        self.preferences.font = selection;
    }

    pub(super) fn settings_window(&mut self, context: &egui::Context) {
        if !self.settings.open {
            return;
        }
        let popup_was_open = egui::Popup::is_any_open(context);
        let mut selected = self.preferences.font.clone();
        let response = theme::modal(context, "settings").show(context, |ui| {
            ui.set_width(440.0_f32.min(context.content_rect().width() - 64.0));
            ui.heading("设置");
            theme::caption(
                ui,
                "选择后立即生效，下次启动时保留。\n中文缺字会自动使用内置 Noto Sans SC。",
            );
            ui.add_space(12.0);
            ui.label(RichText::new("界面字体").strong());
            egui::ComboBox::from_id_salt("ui-font")
                .width(ui.available_width())
                .height(220.0)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .truncate()
                .selected_text(selected.label())
                .show_ui(ui, |ui| {
                    ui.add(
                        widgets::text_edit(&mut self.settings.search)
                            .hint_text("搜索已安装字体…")
                            .desired_width(ui.available_width()),
                    );
                    if ui
                        .selectable_value(
                            &mut selected,
                            Selection::System,
                            Selection::System.label(),
                        )
                        .clicked()
                    {
                        ui.close();
                    }
                    if ui
                        .selectable_value(&mut selected, Selection::Noto, Selection::Noto.label())
                        .clicked()
                    {
                        ui.close();
                    }
                    ui.separator();
                    let filter = self.settings.search.to_lowercase();
                    let mut found = false;
                    for name in fonts::families() {
                        if !name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        found = true;
                        if ui
                            .selectable_value(
                                &mut selected,
                                Selection::Installed(name.clone()),
                                name,
                            )
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                    if !found {
                        theme::caption(ui, "没有匹配的已安装字体");
                    }
                });
            if !self.settings.available && selected == self.preferences.font {
                ui.colored_label(
                    theme::palette(ui).accent,
                    "此字体当前不可用，已使用系统字体及中文后备。",
                );
            }
            ui.add_space(12.0);
            theme::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                theme::caption(ui, "预览");
                ui.label(RichText::new("轻巧、专注的归档工具").size(20.0));
                ui.label("中文文件夹 / 繁體中文 / Archive 0123456789");
                ui.monospace("README.md  128 KiB");
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("恢复默认").clicked() {
                    selected = Selection::System;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::primary(ui, "完成").clicked()
                })
                .inner
            })
            .inner
        });
        if selected != self.preferences.font {
            self.change_font(context, selected);
        }
        if response.inner || (!popup_was_open && response.should_close()) {
            self.settings.open = false;
        }
    }
}
