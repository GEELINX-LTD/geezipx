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
        let language = self.preferences.language;
        let popup_was_open = egui::Popup::is_any_open(context);
        let mut selected = self.preferences.font.clone();
        let response = theme::modal(
            context,
            "settings",
            self.window_frame.corner_radius(context),
        )
        .show(context, |ui| {
            ui.set_width(440.0_f32.min(context.content_rect().width() - 64.0));
            ui.heading(tr!(language, Settings));
            theme::caption(ui, tr!(language, SettingsHint));
            ui.add_space(12.0);
            egui::ScrollArea::vertical()
                .id_salt("settings-body")
                .max_height((context.content_rect().height() - 210.0).max(120.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(tr!(language, Language)).strong());
                    egui::ComboBox::from_id_salt("ui-language")
                        .width(ui.available_width())
                        .selected_text(self.preferences.language.label())
                        .show_ui(ui, |ui| {
                            for choice in Language::ALL {
                                ui.selectable_value(
                                    &mut self.preferences.language,
                                    choice,
                                    choice.label(),
                                );
                            }
                        });
                    ui.add_space(12.0);
                    ui.label(RichText::new(tr!(language, Font)).strong());
                    egui::ComboBox::from_id_salt("ui-font")
                        .width(ui.available_width())
                        .height(220.0)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .truncate()
                        .selected_text(selected.label_in(language))
                        .show_ui(ui, |ui| {
                            ui.add(
                                widgets::text_edit(&mut self.settings.search)
                                    .hint_text(tr!(language, SearchFonts))
                                    .desired_width(ui.available_width()),
                            );
                            if ui
                                .selectable_value(
                                    &mut selected,
                                    Selection::System,
                                    Selection::System.label_in(language),
                                )
                                .clicked()
                            {
                                ui.close();
                            }
                            if ui
                                .selectable_value(
                                    &mut selected,
                                    Selection::Noto,
                                    Selection::Noto.label_in(language),
                                )
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
                                theme::caption(ui, tr!(language, NoFonts));
                            }
                        });
                    if !self.settings.available && selected == self.preferences.font {
                        ui.colored_label(theme::palette(ui).accent, tr!(language, FontUnavailable));
                    }
                    theme::caption(ui, tr!(language, FontFallback));
                    ui.add_space(12.0);
                    theme::card(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        theme::caption(ui, tr!(language, Preview));
                        ui.label(RichText::new(tr!(language, Tagline)).size(20.0));
                        ui.label("中文文件夹 / 繁體中文 / Archive 0123456789");
                        ui.monospace("README.md  128 KiB");
                    });
                });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(tr!(language, Reset)).clicked() {
                    selected = Selection::System;
                    self.preferences.language = Language::default();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::primary(ui, tr!(language, Done)).clicked()
                })
                .inner
            })
            .inner
        });
        if language != self.preferences.language {
            context.request_repaint();
        }
        if selected != self.preferences.font {
            self.change_font(context, selected);
        }
        if response.inner || (!popup_was_open && response.should_close()) {
            self.settings.open = false;
        }
    }
}
