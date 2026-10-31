use super::*;

impl App {
    pub(super) fn show_extract(&mut self, all: bool) {
        if self.busy() {
            return;
        }
        let Some(archive) = &self.archive else {
            return;
        };
        let mut destination = PathField::default();
        destination.set(engine::default_destination(&archive.path));
        self.extraction = Some(Extraction {
            selection: if all {
                BTreeSet::new()
            } else {
                archive.browser.selection.clone()
            },
            destination,
            overwrite: false,
            open_after: false,
            show_password: !self.read_password.is_empty(),
        });
        self.form_error = None;
    }
    pub(super) fn quick_extract(&mut self, context: &egui::Context, in_place: bool) {
        if self.busy() {
            return;
        }
        if let Some(mut request) = self.read_request(true) {
            if in_place {
                request.destination = request
                    .archive
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new("."))
                    .to_owned();
            }
            let mut destination = PathField::default();
            destination.set(request.destination.clone());
            self.pending_extraction = Some(Extraction {
                selection: request.selection.clone(),
                destination,
                overwrite: request.overwrite,
                open_after: false,
                show_password: request.password.is_some(),
            });
            self.form_error = None;
            self.start(context, Kind::Extract, move |control| {
                engine::extract(request, control).map(Outcome::Done)
            });
        }
    }
    pub(super) fn begin_extraction(&mut self, context: &egui::Context) {
        if self.busy() {
            return;
        }
        let Some(options) = self.extraction.take() else {
            return;
        };
        if options.show_password && self.read_password.is_empty() {
            self.form_error = Some("请输入归档密码，或关闭密码输入。".into());
            self.extraction = Some(options);
            return;
        }
        let Some(archive) = &self.archive else {
            return;
        };
        let password = std::mem::take(&mut self.read_password);
        let request = ReadRequest {
            archive: archive.path.clone(),
            password: (!password.is_empty()).then_some(password),
            selection: options.selection.clone(),
            destination: options.destination.path.clone(),
            overwrite: options.overwrite,
        };
        self.open_after_extract = options.open_after;
        self.pending_extraction = Some(options);
        self.form_error = None;
        self.start(context, Kind::Extract, move |control| {
            engine::extract(request, control).map(Outcome::Done)
        });
    }
    pub(super) fn extract_window(&mut self, context: &egui::Context) {
        if self.extraction.is_none() {
            return;
        }
        let busy = self.busy();
        let had_popup = egui::Popup::is_any_open(context);
        let mut cancel = false;
        let mut browse = false;
        let mut start = false;
        let response = theme::modal(context, "extract-archive").show(context, |ui| {
            ui.set_width(510.0_f32.min(context.content_rect().width() - 80.0));
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
                widgets::paint_icon(ui.painter(), rect, Icon::Folder, theme::palette(ui).accent);
                ui.heading("解压到");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        cancel = widgets::icon_button(ui, Icon::Close, "关闭").clicked();
                    });
                });
            });
            let options = self.extraction.as_mut().expect("extraction checked above");
            let archive = self.archive.as_ref();
            let name = archive
                .map(|archive| {
                    archive
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                })
                .unwrap_or_default();
            theme::caption(
                ui,
                if options.selection.is_empty() {
                    format!("{name} · 全部内容")
                } else {
                    format!(
                        "{name} · 已选 {} 项，包含文件夹内全部内容",
                        options.selection.len()
                    )
                },
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("extraction-settings")
                .max_height((context.content_rect().height() - 220.0).max(160.0))
                .show(ui, |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        if let Some(error) = &self.form_error {
                            ui.label(RichText::new(error).color(Color32::from_rgb(194, 81, 70)));
                            ui.add_space(4.0);
                        }
                        ui.label(RichText::new("目标位置").strong());
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 38.0).max(120.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    options.destination.edit(ui, "选择目标文件夹…");
                                },
                            );
                            browse =
                                widgets::icon_button(ui, Icon::Folder, "浏览目标文件夹").clicked();
                        });
                        theme::caption(ui, "保留压缩包内的目录结构。");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("遇到同名文件");
                            egui::ComboBox::from_id_salt("extraction-overwrite")
                                .width(180.0)
                                .selected_text(if options.overwrite {
                                    "替换已有文件"
                                } else {
                                    "跳过已有文件"
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut options.overwrite,
                                        false,
                                        "跳过已有文件",
                                    );
                                    ui.selectable_value(
                                        &mut options.overwrite,
                                        true,
                                        "替换已有文件",
                                    );
                                });
                        });
                        widgets::check(ui, &mut options.open_after, "完成后打开目标文件夹");
                        if widgets::check(ui, &mut options.show_password, "使用归档密码").changed()
                            && !options.show_password
                        {
                            self.read_password.clear();
                        }
                        if options.show_password {
                            ui.add(
                                widgets::text_edit(&mut self.read_password)
                                    .password(true)
                                    .hint_text("仅用于本次解压"),
                            );
                        }
                    });
                });
            ui.add_space(8.0);
            theme::rule(ui);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                theme::caption(ui, "跳过和完成数量会显示在结果中");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(
                        !busy && !options.destination.path.as_os_str().is_empty(),
                        |ui| {
                            start = widgets::button(ui, "开始解压", Icon::ArrowRight, true, 0.0)
                                .clicked();
                        },
                    );
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
        start |= !busy
            && !had_popup
            && self
                .extraction
                .as_ref()
                .is_some_and(|options| !options.destination.path.as_os_str().is_empty())
            && context
                .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        if !busy && (cancel || response.should_close()) {
            self.extraction = None;
            self.read_password.clear();
            self.form_error = None;
        } else if browse {
            self.dialog(context, Dialog::Destination);
        } else if start {
            self.begin_extraction(context);
        }
    }
}
