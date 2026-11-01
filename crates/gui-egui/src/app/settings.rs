use super::*;
use crate::fonts::{self, Selection};
use geezipx_egui::integration::{self, Operation, Snapshot, FORMATS};
type IntegrationResult = (Option<Snapshot>, Option<String>);
#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    Appearance,
    Integration,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirmation {
    Close,
    Remove,
}

pub(super) struct Settings {
    pub(super) open: bool,
    pub(super) available: bool,
    pub(super) page: Page,
    search: String,
    format_search: String,
    pub(super) integration_job: Option<Job<IntegrationResult>>,
    integration: Option<Snapshot>,
    selected: Vec<bool>,
    menu: bool,
    reload: bool,
    integration_message: Option<Result<(), String>>,
    integration_report: bool,
    replace_draft: bool,
    confirmation: Option<Confirmation>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            open: false,
            available: true,
            page: Page::default(),
            search: String::new(),
            format_search: String::new(),
            integration_job: None,
            integration: None,
            selected: vec![false; FORMATS.len()],
            menu: false,
            reload: false,
            integration_message: None,
            integration_report: false,
            replace_draft: true,
            confirmation: None,
        }
    }
}
impl Settings {
    fn registered_selection(snapshot: &Snapshot) -> Vec<bool> {
        FORMATS
            .iter()
            .zip(&snapshot.associations)
            .map(|(format, association)| {
                format.available()
                    && if cfg!(target_os = "windows") {
                        association.registered
                    } else {
                        association.is_default
                    }
            })
            .collect()
    }
    fn dirty(&self) -> bool {
        self.integration.as_ref().is_some_and(|snapshot| {
            self.selected != Self::registered_selection(snapshot) || self.menu != snapshot.menu
        })
    }
    fn restore_draft(&mut self) {
        if let Some(snapshot) = &self.integration {
            self.selected = Self::registered_selection(snapshot);
            self.menu = snapshot.menu;
        }
    }
    fn accept_snapshot(&mut self, snapshot: Snapshot, success: bool) {
        let replace = self.integration.is_none() || (success && self.replace_draft);
        self.integration = Some(snapshot);
        if replace {
            self.restore_draft();
        }
    }
    fn has_integration(&self) -> bool {
        self.integration.as_ref().is_some_and(|snapshot| {
            snapshot.menu
                || snapshot
                    .associations
                    .iter()
                    .any(|a| a.registered || a.is_default)
        })
    }
    fn request_close(&mut self) {
        if self.integration_job.is_some() {
            return;
        }
        if self.dirty() {
            self.confirmation = Some(Confirmation::Close);
        } else {
            self.open = false;
        }
    }
}

impl App {
    pub(super) fn show_settings(&mut self) {
        self.settings.search.clear();
        self.settings.confirmation = None;
        self.settings.open = true;
        self.settings.reload = true;
    }
    fn start_integration(&mut self, context: &egui::Context, operation: Operation) {
        if self.settings.integration_job.is_some() {
            return;
        }
        self.settings.integration_message = None;
        self.settings.confirmation = None;
        self.settings.integration_report =
            matches!(&operation, Operation::Apply { .. } | Operation::Remove);
        self.settings.replace_draft = self.settings.integration_report || !self.settings.dirty();
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
                    self.settings.accept_snapshot(snapshot, error.is_none());
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
    fn change_font(&mut self, context: &egui::Context, selection: Selection) {
        self.settings.available = theme::fonts(context, &selection);
        self.preferences.font = selection;
    }
    fn appearance_settings(&mut self, ui: &mut egui::Ui, selected: &mut Selection) {
        let language = self.preferences.language;
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            setting_row(ui, tr!(language, Language), |ui| {
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
            });
            ui.add_space(6.0);
            setting_row(ui, tr!(language, Appearance), |ui| {
                ui.horizontal(|ui| {
                    for (dark, text) in [(false, tr!(language, Light)), (true, tr!(language, Dark))]
                    {
                        ui.selectable_value(&mut self.preferences.dark, dark, text);
                    }
                });
            });
        });
        ui.add_space(8.0);
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            setting_row(ui, tr!(language, Font), |ui| {
                egui::ComboBox::from_id_salt("ui-font")
                    .width(ui.available_width())
                    .height(220.0)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .truncate()
                    .selected_text(selected.label_in(language))
                    .show_ui(ui, |ui| {
                        ui.add(
                            widgets::text_edit(&mut self.settings.search)
                                .hint_text(tr!(language, SearchFonts)),
                        );
                        for choice in [Selection::System, Selection::Noto] {
                            let label = choice.label_in(language).to_owned();
                            if ui.selectable_value(selected, choice, label).clicked() {
                                ui.close();
                            }
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
                                    selected,
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
            });
            if !self.settings.available && *selected == self.preferences.font {
                ui.colored_label(theme::palette(ui).accent, tr!(language, FontUnavailable));
            }
            theme::caption(ui, tr!(language, FontFallback));
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(theme::palette(ui).soft)
                .corner_radius(6)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    theme::caption(ui, tr!(language, Preview));
                    ui.label(RichText::new(tr!(language, Tagline)).size(18.0));
                    ui.label("中文文件夹 / 繁體中文 / Archive 0123456789");
                });
        });
    }
    fn integration_settings(&mut self, ui: &mut egui::Ui, height: f32) {
        let body_top = ui.cursor().top();
        let language = self.preferences.language;
        let working = self.settings.integration_job.is_some();
        let loaded = self.settings.integration.is_some();
        let hint = if cfg!(target_os = "windows") {
            tr!(language, WindowsAssociationHint)
        } else if cfg!(target_os = "macos") {
            tr!(language, MacAssociationHint)
        } else {
            tr!(language, LinuxAssociationHint)
        };
        let mut defaults = false;
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add_enabled_ui(loaded && !working, |ui| {
                    ui.checkbox(&mut self.settings.menu, tr!(language, IntegrationMenu));
                });
                if cfg!(target_os = "windows") {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        defaults = ui
                            .add_enabled(!working, egui::Button::new(tr!(language, SystemDefaults)))
                            .clicked();
                    });
                }
            });
            ui.add(egui::Label::new(RichText::new(hint).small().weak()).truncate())
                .on_hover_text(hint);
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr!(language, FileAssociations)).strong())
                .on_hover_text(tr!(language, AssociationHint));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !working,
                        egui::Button::new(tr!(language, RefreshIntegration)),
                    )
                    .clicked()
                {
                    self.settings.reload = true;
                }
                let count = self
                    .settings
                    .selected
                    .iter()
                    .filter(|selected| **selected)
                    .count();
                theme::caption(ui, tr!(language, SelectedFormats, count));
            });
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 200.0).max(120.0);
            ui.add_sized(
                [width, 32.0],
                widgets::text_edit(&mut self.settings.format_search)
                    .hint_text(tr!(language, SearchFormats)),
            );
            ui.add_enabled_ui(loaded && !working, |ui| {
                if ui.button(tr!(language, SelectAllFormats)).clicked() {
                    self.settings.selected = FORMATS.iter().map(|f| f.available()).collect();
                }
                if ui.button(tr!(language, SelectNoFormats)).clicked() {
                    self.settings.selected.fill(false);
                }
            });
        });
        let filter = self
            .settings
            .format_search
            .trim()
            .trim_start_matches('.')
            .to_lowercase();
        let list_height = (height - (ui.cursor().top() - body_top) - 8.0).max(36.0);
        egui::Frame::new()
            .fill(theme::palette(ui).surface)
            .stroke(egui::Stroke::new(1.0_f32, theme::palette(ui).border))
            .corner_radius(6)
            .inner_margin(4)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt(("integration-formats", &filter))
                    .min_scrolled_height(list_height)
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        let mut found = false;
                        for (index, format) in FORMATS.iter().enumerate() {
                            if !format.available() || !matches_format(format, &filter) {
                                continue;
                            }
                            found = true;
                            let status = self
                                .settings
                                .integration
                                .as_ref()
                                .and_then(|s| s.associations.get(index));
                            let status = match status {
                                Some(s) if s.is_default => tr!(language, AssociationDefault),
                                Some(s) if s.registered => tr!(language, AssociationRegistered),
                                Some(_) => tr!(language, AssociationUnbound),
                                None => "—",
                            };
                            ui.push_id(index, |ui| {
                                ui.add_enabled_ui(loaded && !working, |ui| {
                                    format_row(
                                        ui,
                                        format,
                                        &mut self.settings.selected[index],
                                        status,
                                    );
                                });
                            });
                        }
                        if !found {
                            theme::caption(ui, tr!(language, NoFormats));
                        }
                    });
            });
        if defaults {
            self.start_integration(ui.ctx(), Operation::Defaults);
        }
    }
    fn settings_footer(&mut self, ui: &mut egui::Ui, selected: &mut Selection) {
        let language = self.preferences.language;
        let working = self.settings.integration_job.is_some();
        let mut operation = None;
        let mut close = false;
        ui.horizontal(|ui| match self.settings.confirmation {
            Some(confirmation) => {
                if ui.button(tr!(language, Cancel)).clicked() {
                    self.settings.confirmation = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = if confirmation == Confirmation::Close {
                        tr!(language, DiscardAndClose)
                    } else {
                        tr!(language, RemoveIntegration)
                    };
                    if ui.button(text).clicked() {
                        if confirmation == Confirmation::Close {
                            self.settings.restore_draft();
                            self.settings.confirmation = None;
                            self.settings.open = false;
                        } else {
                            operation = Some(Operation::Remove);
                        }
                    }
                });
            }
            None => {
                if self.settings.page == Page::Appearance {
                    if ui.button(tr!(language, Reset)).clicked() {
                        *selected = Selection::System;
                        self.preferences.language = Language::default();
                        self.preferences.dark = false;
                    }
                } else if ui
                    .add_enabled(
                        !working && self.settings.has_integration(),
                        egui::Button::new(tr!(language, RemoveIntegration)),
                    )
                    .clicked()
                {
                    self.settings.confirmation = Some(Confirmation::Remove);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui
                        .add_enabled_ui(!working, |ui| {
                            if self.settings.page == Page::Appearance {
                                theme::primary(ui, tr!(language, Done)).clicked()
                            } else {
                                ui.button(tr!(language, Done)).clicked()
                            }
                        })
                        .inner;
                    if self.settings.page == Page::Integration
                        && ui
                            .add_enabled_ui(!working && self.settings.dirty(), |ui| {
                                theme::primary(ui, tr!(language, ApplyIntegration)).clicked()
                            })
                            .inner
                    {
                        operation = Some(Operation::Apply {
                            selected: self.settings.selected.clone(),
                            menu: self.settings.menu,
                            language,
                        });
                    }
                });
            }
        });
        if let Some(operation) = operation {
            self.start_integration(ui.ctx(), operation);
        }
        if close {
            self.settings.request_close();
        }
    }
    pub(super) fn settings_window(&mut self, context: &egui::Context) {
        if !self.settings.open {
            return;
        }
        if self.settings.page == Page::Integration
            && self.settings.reload
            && self.settings.integration_job.is_none()
        {
            self.settings.reload = false;
            self.start_integration(context, Operation::Inspect);
        }
        let language = self.preferences.language;
        let dark = self.preferences.dark;
        let popup_was_open = egui::Popup::is_any_open(context);
        let confirmation_was_open = self.settings.confirmation.is_some();
        let mut selected = self.preferences.font.clone();
        let response = theme::modal(
            context,
            "settings",
            self.window_frame.corner_radius(context),
        )
        .show(context, |ui| {
            let width = (context.content_rect().width() - 80.0).clamp(320.0, 720.0);
            let body_height = if self.settings.confirmation.is_some() {
                120.0
            } else {
                (context.content_rect().height() - 248.0).clamp(196.0, 380.0)
            };
            ui.set_width(width);
            ui.heading(tr!(language, Settings));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                for (page, text) in [
                    (Page::Appearance, tr!(language, Appearance)),
                    (Page::Integration, tr!(language, SystemIntegration)),
                ] {
                    if ui
                        .add_sized(
                            [156.0, 32.0],
                            egui::Button::new(text).selected(self.settings.page == page),
                        )
                        .clicked()
                    {
                        self.settings.page = page;
                        self.settings.confirmation = None;
                    }
                }
            });
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::vec2(width, body_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_height(body_height);
                    if let Some(confirmation) = self.settings.confirmation {
                        ui.add_space(24.0);
                        ui.heading(if confirmation == Confirmation::Close {
                            tr!(language, UnappliedChanges)
                        } else {
                            tr!(language, RemoveIntegration)
                        });
                        ui.add_space(8.0);
                        ui.label(if confirmation == Confirmation::Close {
                            tr!(language, DiscardIntegrationHint)
                        } else {
                            tr!(language, RemoveIntegrationHint)
                        });
                    } else if self.settings.page == Page::Appearance {
                        egui::ScrollArea::vertical()
                            .id_salt("settings-appearance")
                            .min_scrolled_height(body_height)
                            .max_height(body_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.appearance_settings(ui, &mut selected);
                            });
                    } else {
                        self.integration_settings(ui, body_height);
                    }
                },
            );
            ui.add_space(6.0);
            let message = if self.settings.integration_job.is_some() {
                tr!(language, IntegrationWorking).to_owned()
            } else if let Some(Err(error)) = &self.settings.integration_message {
                tr!(language, IntegrationFailed, error)
            } else if self.settings.dirty() {
                tr!(language, UnappliedChanges).to_owned()
            } else if self.settings.page == Page::Appearance {
                tr!(language, SettingsHint).to_owned()
            } else if self.settings.integration_message == Some(Ok(())) {
                tr!(language, IntegrationApplied).to_owned()
            } else {
                tr!(language, IntegrationHint).to_owned()
            };
            ui.add(
                egui::Label::new(
                    RichText::new(&message)
                        .small()
                        .color(theme::palette(ui).muted),
                )
                .truncate(),
            )
            .on_hover_text(&message);
            theme::rule(ui);
            ui.add_space(6.0);
            self.settings_footer(ui, &mut selected);
        });
        if language != self.preferences.language {
            context.request_repaint();
        }
        if dark != self.preferences.dark {
            theme::apply(context, self.preferences.dark);
        }
        if selected != self.preferences.font {
            self.change_font(context, selected);
        }
        if !popup_was_open && response.should_close() {
            if confirmation_was_open {
                self.settings.confirmation = None;
            } else {
                self.settings.request_close();
            }
        }
    }
}

fn setting_row(ui: &mut egui::Ui, label: &str, contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [140.0, 28.0],
            egui::Label::new(RichText::new(label).strong()).halign(egui::Align::Min),
        );
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 28.0),
            egui::Layout::top_down(egui::Align::Min),
            contents,
        );
    });
}
fn matches_format(format: &integration::Format, filter: &str) -> bool {
    format.name.to_lowercase().contains(filter)
        || format.extensions.iter().any(|ext| ext.contains(filter))
}
fn format_row(ui: &mut egui::Ui, format: &integration::Format, selected: &mut bool, status: &str) {
    let p = theme::palette(ui);
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::click());
    if response.clicked() {
        *selected = !*selected;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            ui.is_enabled(),
            *selected,
            format.name,
        )
    });
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 4, p.soft);
    }
    let checkbox = egui::Rect::from_center_size(
        rect.left_center() + egui::vec2(14.0, 0.0),
        egui::vec2(16.0, 16.0),
    );
    ui.painter()
        .rect_filled(checkbox, 4, if *selected { p.accent } else { p.surface });
    ui.painter().rect_stroke(
        checkbox,
        4,
        egui::Stroke::new(1.0_f32, p.border),
        egui::StrokeKind::Inside,
    );
    if *selected {
        widgets::paint_icon(ui.painter(), checkbox.shrink(1.0), Icon::Check, p.surface);
    }
    let text_color = if ui.is_enabled() { p.text } else { p.muted };
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.text(
        rect.left_center() + egui::vec2(32.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format.name,
        egui::FontId::proportional(13.0),
        text_color,
    );
    let extensions = format
        .extensions
        .iter()
        .map(|ext| format!(".{ext}"))
        .collect::<Vec<_>>()
        .join("  ");
    let extension_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 195.0, rect.top()),
        egui::pos2(
            (rect.right() - 118.0).max(rect.left() + 195.0),
            rect.bottom(),
        ),
    );
    painter
        .with_clip_rect(extension_rect.intersect(ui.clip_rect()))
        .text(
            extension_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            &extensions,
            egui::FontId::proportional(12.0),
            p.muted,
        );
    painter.text(
        rect.right_center() - egui::vec2(10.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        status,
        egui::FontId::proportional(12.0),
        p.muted,
    );
    response
        .on_hover_text(extensions)
        .on_hover_cursor(egui::CursorIcon::PointingHand);
}
