use super::*;
use crate::fonts::{self, Selection};
use geezipx_egui::integration::{self, Operation, Snapshot, FORMATS};
type IntegrationResult = (Option<Snapshot>, Option<String>);

pub(super) struct Settings {
    pub(super) open: bool,
    pub(super) available: bool,
    search: String,
    pub(super) integration_job: Option<Job<IntegrationResult>>,
    integration: Option<Snapshot>,
    selected: Vec<bool>,
    menu: bool,
    reload: bool,
    integration_message: Option<Result<(), String>>,
    integration_report: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            open: false,
            available: true,
            search: String::new(),
            integration_job: None,
            integration: None,
            selected: vec![false; FORMATS.len()],
            menu: false,
            reload: false,
            integration_message: None,
            integration_report: false,
        }
    }
}

impl App {
    pub(super) fn show_settings(&mut self) {
        self.settings.search.clear();
        self.settings.open = true;
        self.settings.reload = true;
    }

    fn start_integration(&mut self, context: &egui::Context, operation: Operation) {
        if self.settings.integration_job.is_some() {
            return;
        }
        self.settings.integration_message = None;
        self.settings.integration_report =
            matches!(&operation, Operation::Apply { .. } | Operation::Remove);
        self.settings.integration_job =
            Some(Job::spawn(context, move |_| {
                match integration::execute(operation) {
                    Ok(snapshot) => Ok((Some(snapshot), None)),
                    Err(error) => Ok((integration::execute(Operation::Inspect).ok(), Some(error))),
                }
            }));
    }
    pub(super) fn poll_integration(&mut self, context: &egui::Context) {
        let Some(result) = self.settings.integration_job.as_ref().and_then(Job::poll) else {
            return;
        };
        self.settings.integration_job = None;
        match result {
            Ok((snapshot, error)) => {
                if let Some(snapshot) = snapshot {
                    self.settings.selected = snapshot
                        .associations
                        .iter()
                        .map(|association| {
                            if cfg!(target_os = "windows") {
                                association.registered
                            } else {
                                association.is_default
                            }
                        })
                        .collect();
                    self.settings.menu = snapshot.menu;
                    self.settings.integration = Some(snapshot);
                }
                self.settings.integration_message =
                    if error.is_some() || self.settings.integration_report {
                        Some(error.map_or(Ok(()), Err))
                    } else {
                        None
                    };
            }
            Err(error) => self.settings.integration_message = Some(Err(error)),
        }
        if self.close_when_finished && !self.busy() {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn integration_settings(&mut self, context: &egui::Context, ui: &mut egui::Ui) {
        if self.settings.reload && self.settings.integration_job.is_none() {
            self.settings.reload = false;
            self.start_integration(context, Operation::Inspect);
        }
        let language = self.preferences.language;
        theme::caption(ui, tr!(language, AssociationHint));
        theme::caption(
            ui,
            if cfg!(target_os = "windows") {
                tr!(language, WindowsAssociationHint)
            } else if cfg!(target_os = "macos") {
                tr!(language, MacAssociationHint)
            } else {
                tr!(language, LinuxAssociationHint)
            },
        );
        let working = self.settings.integration_job.is_some();
        if working {
            ui.label(tr!(language, IntegrationWorking));
        }
        let mut operation = None;
        ui.add_enabled_ui(!working, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr!(language, SelectAllFormats)).clicked() {
                    self.settings.selected = FORMATS.iter().map(|f| f.available()).collect();
                }
                if ui.button(tr!(language, SelectNoFormats)).clicked() {
                    self.settings.selected.fill(false);
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("integration-formats")
                .max_height(180.0)
                .show(ui, |ui| {
                    for (index, format) in FORMATS.iter().enumerate() {
                        if !format.available() {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.settings.selected[index], format.name)
                                .on_hover_text(
                                    format
                                        .extensions
                                        .iter()
                                        .map(|ext| format!(".{ext}"))
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                );
                            let status = self
                                .settings
                                .integration
                                .as_ref()
                                .and_then(|s| s.associations.get(index));
                            theme::caption(
                                ui,
                                match status {
                                    Some(s) if s.is_default => tr!(language, AssociationDefault),
                                    Some(s) if s.registered => tr!(language, AssociationRegistered),
                                    _ => tr!(language, AssociationUnbound),
                                },
                            );
                        });
                    }
                });
            ui.checkbox(&mut self.settings.menu, tr!(language, IntegrationMenu));
            ui.horizontal_wrapped(|ui| {
                if theme::primary(ui, tr!(language, ApplyIntegration)).clicked() {
                    operation = Some(Operation::Apply {
                        selected: self.settings.selected.clone(),
                        menu: self.settings.menu,
                        language,
                    });
                }
                if ui.button(tr!(language, RefreshIntegration)).clicked() {
                    operation = Some(Operation::Inspect);
                }
                if ui.button(tr!(language, RemoveIntegration)).clicked() {
                    operation = Some(Operation::Remove);
                }
                if cfg!(target_os = "windows") && ui.button(tr!(language, SystemDefaults)).clicked()
                {
                    operation = Some(Operation::Defaults);
                }
            });
        });
        if let Some(message) = &self.settings.integration_message {
            match message {
                Ok(()) => {
                    theme::caption(ui, tr!(language, IntegrationApplied));
                }
                Err(error) => {
                    ui.colored_label(
                        theme::palette(ui).accent,
                        tr!(language, IntegrationFailed, error),
                    );
                }
            }
        }
        if let Some(operation) = operation {
            self.start_integration(context, operation);
        }
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
                    ui.add_space(12.0);
                    ui.collapsing(tr!(language, SystemIntegration), |ui| {
                        self.integration_settings(context, ui);
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
