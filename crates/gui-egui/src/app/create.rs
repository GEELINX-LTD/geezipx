use super::*;

impl App {
    pub(super) fn create_window(&mut self, context: &egui::Context) {
        if !self.create_open {
            return;
        }
        let language = self.preferences.language;
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
                ui.heading(tr!(language, NewArchive));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        cancel =
                            widgets::icon_button(ui, Icon::Close, tr!(language, Close)).clicked();
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
                            ui.label(RichText::new(tr!(language, Contents)).strong());
                            widgets::badge(ui, &tr!(language, ItemCount, self.draft.sources.len()));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !self.draft.sources.is_empty()
                                        && widgets::subtle_button(
                                            ui,
                                            if self.draft.show_sources {
                                                tr!(language, CollapseList)
                                            } else {
                                                tr!(language, EditSources)
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
                                tr!(language, SourceSummary, name, self.draft.sources.len())
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
                                    theme::caption(ui, tr!(language, AddHint));
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
                                                language,
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
                            if widgets::button(ui, tr!(language, AddFiles), Icon::Plus, false, 0.0)
                                .clicked()
                            {
                                self.dialog(context, Dialog::Files);
                            }
                            if widgets::button(
                                ui,
                                tr!(language, AddFolders),
                                Icon::Folder,
                                false,
                                0.0,
                            )
                            .clicked()
                            {
                                self.dialog(context, Dialog::Folders);
                            }
                            if !self.draft.sources.is_empty()
                                && widgets::subtle_button(ui, tr!(language, Clear)).clicked()
                            {
                                self.draft.sources.clear();
                            }
                        });
                        ui.add_space(5.0);
                        theme::rule(ui);
                        ui.add_space(3.0);
                        ui.label(RichText::new(tr!(language, SaveAs)).strong());
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 38.0).max(120.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    self.draft.output.edit(ui, tr!(language, SaveLocationHint));
                                },
                            );
                            if widgets::icon_button(
                                ui,
                                Icon::Folder,
                                tr!(language, BrowseSaveLocation),
                            )
                            .clicked()
                            {
                                self.dialog(context, Dialog::Save);
                            }
                        });
                        let formats = engine::formats::WRITE_FORMATS;
                        let previous = self.draft.format;
                        ui.columns(2, |columns| {
                            theme::caption(&mut columns[0], tr!(language, Format));
                            let width = columns[0].available_width();
                            let parent_layer = columns[0].layer_id();
                            egui::ComboBox::from_id_salt("compression-format")
                                .width(width)
                                .height(200.0)
                                .selected_text(engine::formats::label(
                                    formats[self.draft.format].0,
                                    language,
                                ))
                                .show_ui(&mut columns[0], |ui| {
                                    context.set_sublayer(parent_layer, ui.layer_id());
                                    for (index, (format, _)) in formats.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut self.draft.format,
                                            index,
                                            engine::formats::label(*format, language),
                                        );
                                    }
                                });
                            theme::caption(&mut columns[1], tr!(language, CompressionPreference));
                            let adjustable =
                                engine::formats::adjustable(formats[self.draft.format].0);
                            columns[1].add_enabled_ui(adjustable, |ui| {
                                egui::ComboBox::from_id_salt("compression-preset")
                                    .width(ui.available_width())
                                    .selected_text(if adjustable {
                                        [
                                            tr!(language, Fast),
                                            tr!(language, Standard),
                                            tr!(language, Small),
                                        ][self.draft.preset]
                                    } else {
                                        tr!(language, Standard)
                                    })
                                    .show_ui(ui, |ui| {
                                        for (index, name) in [
                                            tr!(language, Fast),
                                            tr!(language, Standard),
                                            tr!(language, Small),
                                        ]
                                        .iter()
                                        .enumerate()
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
                        let format = formats[self.draft.format].0;
                        if format == ArchiveFormat::Zip {
                            let previous = self.draft.sfx_target;
                            let mut enabled = previous.is_some();
                            if widgets::check(ui, &mut enabled, tr!(language, CreateSfx)).changed()
                            {
                                self.draft.sfx_target = if enabled {
                                    [
                                        geezipx_core::sfx::SfxTarget::host(),
                                        geezipx_core::sfx::SfxTarget::Windows,
                                        geezipx_core::sfx::SfxTarget::Linux,
                                        geezipx_core::sfx::SfxTarget::MacOS,
                                    ]
                                    .into_iter()
                                    .find(|target| target.is_available())
                                } else {
                                    None
                                };
                            }
                            if let Some(target) = &mut self.draft.sfx_target {
                                egui::ComboBox::from_id_salt("sfx-target")
                                    .selected_text(format!("{target:?} x86_64"))
                                    .show_ui(ui, |ui| {
                                        for (value, label) in [
                                            (
                                                geezipx_core::sfx::SfxTarget::Windows,
                                                "Windows x86_64",
                                            ),
                                            (geezipx_core::sfx::SfxTarget::Linux, "Linux x86_64"),
                                            (geezipx_core::sfx::SfxTarget::MacOS, "macOS x86_64"),
                                        ] {
                                            ui.add_enabled_ui(value.is_available(), |ui| {
                                                ui.selectable_value(target, value, label);
                                            });
                                        }
                                    });
                            }
                            if previous != self.draft.sfx_target {
                                let old_suffix =
                                    previous.map_or(".zip", |target| target.extension());
                                self.replace_output_suffix(old_suffix);
                            }
                        }
                        if engine::streams::single_input(format) {
                            theme::caption(ui, tr!(language, SingleInputHint));
                        }
                        if matches!(format, ArchiveFormat::Img | ArchiveFormat::Bin) {
                            theme::caption(ui, tr!(language, CopyImageHint));
                        }
                        if format == ArchiveFormat::Lzh {
                            theme::caption(ui, tr!(language, LzhHint));
                        }
                        if format == ArchiveFormat::Cab {
                            theme::caption(ui, tr!(language, CabHint));
                        }
                        if format == ArchiveFormat::Wim {
                            theme::caption(ui, tr!(language, WimHint));
                        }
                        if format == ArchiveFormat::Isz {
                            theme::caption(ui, tr!(language, IszHint));
                        }
                        let encryption = engine::formats::write_password(format);
                        if format == ArchiveFormat::Aes {
                            self.draft.show_password = true;
                        }
                        ui.add_space(3.0);
                        ui.horizontal(|ui| {
                            ui.add_enabled_ui(encryption && format != ArchiveFormat::Aes, |ui| {
                                if widgets::check(
                                    ui,
                                    &mut self.draft.show_password,
                                    tr!(language, PasswordProtection),
                                )
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
                                            tr!(language, CollapseOptions)
                                        } else {
                                            tr!(language, MoreOptions)
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
                                        .hint_text(tr!(language, SetPassword)),
                                );
                                columns[1].add(
                                    widgets::text_edit(&mut self.draft.confirm)
                                        .password(true)
                                        .hint_text(tr!(language, ConfirmPassword)),
                                );
                            });
                            theme::caption(ui, tr!(language, PasswordHint));
                        }
                        if self.draft.show_advanced {
                            widgets::check(
                                ui,
                                &mut self.draft.replace,
                                tr!(language, ReplaceArchive),
                            );
                            theme::caption(ui, tr!(language, StructureHint));
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
                theme::caption(ui, tr!(language, DropMore));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = !busy
                        && !self.draft.sources.is_empty()
                        && !self.draft.output.path.as_os_str().is_empty();
                    ui.add_enabled_ui(ready, |ui| {
                        if widgets::button(
                            ui,
                            tr!(language, StartCompression),
                            Icon::ArrowRight,
                            true,
                            0.0,
                        )
                        .clicked()
                        {
                            self.create_archive(context);
                        }
                    });
                    if ui
                        .add_enabled(
                            !busy,
                            egui::Button::new(tr!(language, Cancel))
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
    pub(super) fn change_format(&mut self, previous: usize) {
        let formats = engine::formats::WRITE_FORMATS;
        let old_suffix = self.draft.sfx_target.map_or_else(
            || format!(".{}", engine::formats::extension(formats[previous].0)),
            |target| target.extension().to_owned(),
        );
        let automatic = self
            .suggested_output(formats[previous].0, &old_suffix)
            .as_ref()
            == Some(&self.draft.output.path);
        self.draft.sfx_target = None;
        if automatic {
            if let Some(path) =
                self.suggested_output(formats[self.draft.format].0, &self.output_suffix())
            {
                self.draft.output.set(path);
            }
        } else {
            self.replace_output_suffix(&old_suffix);
        }
        if !engine::formats::write_password(formats[self.draft.format].0) {
            self.draft.password.clear();
            self.draft.confirm.clear();
            self.draft.show_password = false;
        } else if formats[self.draft.format].0 == ArchiveFormat::Aes {
            self.draft.show_password = true;
        }
    }
    pub(super) fn suggested_output(
        &self,
        format: ArchiveFormat,
        suffix: &str,
    ) -> Option<std::path::PathBuf> {
        let first = self.draft.sources.first()?;
        let name = if engine::streams::is_stream(format) {
            first.file_name()
        } else {
            first.file_stem()
        }?
        .to_string_lossy();
        let suggested = first.with_file_name(format!("{name}{suffix}"));
        Some(if suggested == *first {
            first.with_file_name(format!("{name}-archive{suffix}"))
        } else {
            suggested
        })
    }
    pub(super) fn output_suffix(&self) -> String {
        self.draft.sfx_target.map_or_else(
            || {
                format!(
                    ".{}",
                    engine::formats::extension(engine::formats::WRITE_FORMATS[self.draft.format].0)
                )
            },
            |target| target.extension().to_owned(),
        )
    }
    fn replace_output_suffix(&mut self, previous: &str) {
        if self.draft.output.path.as_os_str().is_empty() {
            return;
        }
        let name = self
            .draft
            .output
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let stem = name.strip_suffix(previous).unwrap_or(&name);
        let path = self
            .draft
            .output
            .path
            .with_file_name(format!("{stem}{}", self.output_suffix()));
        self.draft.output.set(path);
    }
}
