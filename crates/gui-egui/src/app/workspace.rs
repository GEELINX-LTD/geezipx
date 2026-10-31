use super::*;

impl App {
    pub(super) fn workspace(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let busy = self.busy();
        if let Some(path) = self.retry_path.clone() {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(format!("未能打开：{}", path.display())).truncate());
                if ui
                    .add_enabled(!busy, egui::Button::new("输入密码重试"))
                    .clicked()
                {
                    self.unlock = Some(Unlock {
                        path,
                        password: String::new(),
                    });
                }
            });
        }
        if self.archive.is_none() {
            self.welcome(ui, context);
            return;
        }
        let archive = self.archive.as_mut().expect("archive checked above");
        let browser = &mut archive.browser;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_enabled_ui(browser.can_back(), |ui| {
                if widgets::navigation_button(ui, Icon::ArrowLeft, "返回", false).clicked() {
                    browser.back();
                }
            });
            ui.add_enabled_ui(browser.can_forward(), |ui| {
                if widgets::navigation_button(ui, Icon::ArrowRight, "前进", false).clicked() {
                    browser.forward();
                }
            });
            ui.add_enabled_ui(!browser.directory.is_empty(), |ui| {
                if widgets::navigation_button(ui, Icon::ArrowUp, "上一级", true).clicked() {
                    browser.parent();
                }
            });
            let search_width = (ui.available_width() * 0.30).clamp(180.0, 260.0);
            let path_width = (ui.available_width() - search_width - 12.0).max(180.0);
            ui.allocate_ui_with_layout(
                egui::vec2(path_width, 30.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(path_width);
                    egui::ScrollArea::horizontal()
                        .id_salt("breadcrumbs")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let name = archive
                                    .path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new(name).strong())
                                            .frame(false),
                                    )
                                    .on_hover_text(archive.path.display().to_string())
                                    .clicked()
                                {
                                    browser.navigate(String::new());
                                }
                                let parts: Vec<_> = browser
                                    .directory
                                    .split('/')
                                    .filter(|part| !part.is_empty())
                                    .map(str::to_owned)
                                    .collect();
                                let mut prefix = String::new();
                                for part in parts {
                                    if !prefix.is_empty() {
                                        prefix.push('/');
                                    }
                                    prefix.push_str(&part);
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(12.0, 16.0),
                                        egui::Sense::hover(),
                                    );
                                    widgets::paint_icon(
                                        ui.painter(),
                                        rect,
                                        Icon::Chevron,
                                        theme::palette(ui).muted,
                                    );
                                    if ui.add(egui::Button::new(part).frame(false)).clicked() {
                                        browser.navigate(prefix.clone());
                                    }
                                }
                            });
                        });
                },
            );
            ui.add_space(4.0);
            if widgets::search(ui, &mut browser.query, search_width).changed() {
                browser.refresh();
            }
        });
        ui.add_space(5.0);
        self.folder_tree(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(ui, |ui| self.archive_contents(ui, context));
    }
    fn archive_contents(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let busy = self.busy();
        let mut preview = None;
        let mut extract = false;
        let mut drag_out = false;
        let mut start_marquee = None;
        let archive = self.archive.as_mut().expect("archive checked above");
        let browser = &mut archive.browser;
        let parent_offset = usize::from(!browser.directory.is_empty());
        let row_count = browser.visible.len() + parent_offset;
        let columns = widgets::Columns::new(ui.available_width(), self.column_widths);
        let (header, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 29.0), egui::Sense::hover());
        let palette = theme::palette(ui);
        ui.painter().rect_filled(header, 4, palette.soft);
        for (index, (label, sort)) in [
            ("名称", Some(SortColumn::Name)),
            ("大小", Some(SortColumn::Size)),
            ("压缩后", Some(SortColumn::Packed)),
            ("类型", Some(SortColumn::Type)),
        ]
        .into_iter()
        .enumerate()
        {
            let cell = columns.cell(header, index);
            let response = ui.interact(
                cell.shrink2(egui::vec2(4.0, 0.0)),
                ui.id().with(("header", index)),
                egui::Sense::click(),
            );
            if let Some(sort) = sort {
                if response.clicked() {
                    browser.sort_by(sort);
                }
                response.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
            let selected = sort == Some(browser.sort);
            let galley = ui.painter().layout_no_wrap(
                label.into(),
                egui::FontId::proportional(12.0),
                if selected {
                    palette.accent
                } else {
                    palette.muted
                },
            );
            let origin = cell.left_center() + egui::vec2(if index == 0 { 12.0 } else { 10.0 }, 0.0);
            if selected {
                widgets::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(
                        origin + egui::vec2(galley.size().x + 10.0, 0.0),
                        egui::vec2(12.0, 12.0),
                    ),
                    if browser.descending {
                        Icon::ArrowDown
                    } else {
                        Icon::ArrowUp
                    },
                    palette.accent,
                );
            }
            ui.painter().galley(
                origin - egui::vec2(0.0, galley.size().y / 2.0),
                galley,
                palette.text,
            );
        }
        for boundary in 1..4 {
            let x = header.left() + columns.edges[boundary];
            let hit = egui::Rect::from_center_size(
                egui::pos2(x, header.center().y),
                egui::vec2(8.0, header.height()),
            );
            let response = ui
                .interact(
                    hit,
                    ui.id().with(("column-resize", boundary)),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            ui.painter().vline(
                x,
                header.y_range().shrink(7.0),
                egui::Stroke::new(1.0_f32, palette.border),
            );
            if response.dragged() {
                let delta = response.drag_delta().x;
                let next = boundary - 1;
                let max_growth = if next == 0 {
                    (columns.edges[1] - 180.0).max(0.0)
                } else {
                    self.column_widths[next - 1] - 64.0
                };
                let delta = delta.clamp(-max_growth, (self.column_widths[next] - 64.0).max(0.0));
                self.column_widths[next] -= delta;
                if next > 0 {
                    self.column_widths[next - 1] += delta;
                }
            }
        }
        let list_height = ui.available_height().max(60.0);
        if let Some(index) = browser.reveal.take() {
            let top = (index + parent_offset) as f32 * 29.0;
            if top < self.list_offset {
                self.list_offset = top;
            }
            if top + 29.0 > self.list_offset + list_height {
                self.list_offset = (top + 29.0 - list_height).max(0.0);
            }
        }
        let list = ui.interact(
            ui.available_rect_before_wrap(),
            egui::Id::new("archive-list"),
            egui::Sense::click_and_drag(),
        );
        if list.clicked_by(egui::PointerButton::Primary) {
            browser.clear_selection();
            list.request_focus();
        }
        if list.drag_started_by(egui::PointerButton::Primary) {
            start_marquee = ui.input(|i| i.pointer.press_origin());
            list.request_focus();
        }
        let list_focused = list.has_focus();
        let mut navigate = None;
        let mut go_parent = false;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let output = egui::ScrollArea::vertical()
                .id_salt("archive-rows")
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: false,
                    ..Default::default()
                })
                .auto_shrink([false, false])
                .max_height(list_height)
                .vertical_scroll_offset(self.list_offset)
                .show_rows(ui, 29.0, row_count, |ui, range| {
                    let columns = widgets::Columns::new(ui.available_width(), self.column_widths);
                    for visual_index in range {
                        if parent_offset == 1 && visual_index == 0 {
                            let response = widgets::file_row(
                                ui,
                                egui::Id::new((
                                    "archive-parent",
                                    &archive.path,
                                    &browser.directory,
                                )),
                                widgets::FileRow {
                                    name: "..",
                                    size: "",
                                    packed: "",
                                    kind: "上一级",
                                    folder: true,
                                    selected: false,
                                    focused: false,
                                },
                                columns,
                            );
                            if response.clicked()
                                && !ui.input(|i| i.modifiers.any())
                                && response.interact_pointer_pos().is_some_and(|pos| {
                                    self.row_activation.click(context, response.id, pos)
                                })
                            {
                                go_parent = true;
                            }
                            if response.drag_started() {
                                self.row_activation.reset();
                            }
                            response.on_hover_text("双击返回上一级");
                            continue;
                        }
                        let index = visual_index - parent_offset;
                        let row = browser.visible[index].clone();
                        ui.push_id(&row.path, |ui| {
                            let size = if row.is_dir {
                                String::new()
                            } else {
                                theme::bytes(row.size)
                            };
                            let packed = row
                                .entry_index
                                .and_then(|index| archive.entries.get(index))
                                .map_or_else(String::new, |entry| {
                                    theme::bytes(entry.compressed_size)
                                });
                            let response = widgets::file_row(
                                ui,
                                egui::Id::new(("archive-entry", &archive.path, &row.path)),
                                widgets::FileRow {
                                    name: &row.name,
                                    size: &size,
                                    packed: &packed,
                                    kind: &row.kind,
                                    folder: row.is_dir,
                                    selected: browser.selection.contains(&row.path),
                                    focused: list_focused
                                        && browser.focus.as_ref() == Some(&row.path),
                                },
                                columns,
                            );
                            if response.clicked() {
                                let modifiers = ui.input(|i| i.modifiers);
                                if !modifiers.any()
                                    && response.interact_pointer_pos().is_some_and(|pos| {
                                        self.row_activation.click(context, response.id, pos)
                                    })
                                {
                                    if row.is_dir {
                                        navigate = Some(row.path.clone());
                                    } else if !busy {
                                        preview = row.entry_index;
                                    }
                                }
                                browser.select(
                                    index,
                                    modifiers.shift,
                                    modifiers.command || modifiers.ctrl,
                                );
                                list.request_focus();
                            }
                            if response.drag_started_by(egui::PointerButton::Primary) {
                                self.row_activation.reset();
                                let origin = ui.input(|i| i.pointer.press_origin());
                                let from_name = origin.is_some_and(|pos| {
                                    pos.x < response.rect.left() + columns.edges[1]
                                });
                                if from_name || browser.selection.contains(&row.path) {
                                    if !browser.selection.contains(&row.path) {
                                        let modifiers = ui.input(|i| i.modifiers);
                                        browser.select(
                                            index,
                                            modifiers.shift,
                                            modifiers.command || modifiers.ctrl,
                                        );
                                    }
                                    drag_out = !busy;
                                } else {
                                    start_marquee = origin;
                                }
                                list.request_focus();
                            }
                            if response.secondary_clicked() {
                                if !browser.selection.contains(&row.path) {
                                    browser.select(index, false, false);
                                }
                                list.request_focus();
                            }
                            response.context_menu(|ui| {
                                widgets::menu(ui);
                                if row.is_dir && ui.button("打开文件夹").clicked() {
                                    navigate = Some(row.path.clone());
                                    ui.close();
                                }
                                if !row.is_dir
                                    && ui
                                        .add_enabled(!busy, egui::Button::new("预览文本"))
                                        .clicked()
                                {
                                    preview = row.entry_index;
                                    ui.close();
                                }
                                ui.separator();
                                if ui
                                    .add_enabled(
                                        !busy,
                                        egui::Button::new(format!(
                                            "解压所选 ({})…",
                                            browser.selection.len()
                                        )),
                                    )
                                    .clicked()
                                {
                                    extract = true;
                                    ui.close();
                                }
                                if ui.button("复制归档内路径").clicked() {
                                    context.copy_text(
                                        browser
                                            .selection
                                            .iter()
                                            .cloned()
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    );
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("全选").clicked() {
                                    browser.select_all();
                                    ui.close();
                                }
                                if ui.button("清空选择").clicked() {
                                    browser.clear_selection();
                                    ui.close();
                                }
                            });
                        });
                    }
                    if browser.visible.is_empty() {
                        ui.add_space(28.0);
                        ui.vertical_centered(|ui| {
                            theme::caption(
                                ui,
                                if browser.query.is_empty() {
                                    "此文件夹为空"
                                } else {
                                    "没有匹配的文件 · 按 Esc 清除搜索"
                                },
                            );
                        });
                    }
                });
            self.list_offset = output.state.offset.y;
            let viewport = output.inner_rect;
            if let Some(origin) = start_marquee {
                let modifiers = ui.input(|i| i.modifiers);
                self.marquee = Some(interaction::Marquee {
                    archive: archive.path.clone(),
                    directory: browser.directory.clone(),
                    origin: origin - viewport.min.to_vec2() + output.state.offset,
                    original: if modifiers.command || modifiers.ctrl || modifiers.shift {
                        browser.selection.clone()
                    } else {
                        BTreeSet::new()
                    },
                    toggle: modifiers.command || modifiers.ctrl,
                });
            }
            if let Some(drag) = &self.marquee {
                if drag.archive != archive.path || drag.directory != browser.directory {
                    self.marquee = None;
                } else if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                    let point = pointer - viewport.min.to_vec2() + output.state.offset;
                    let rect = egui::Rect::from_two_pos(drag.origin, point);
                    let mut selection = drag.original.clone();
                    let first = ((rect.top().max(0.0) / 29.0).floor() as usize)
                        .saturating_sub(parent_offset);
                    let end = ((rect.bottom().max(0.0) / 29.0).ceil() as usize)
                        .saturating_sub(parent_offset)
                        .min(browser.visible.len());
                    if rect.right() >= 0.0 && rect.left() <= viewport.width() {
                        for row in browser.visible.iter().take(end).skip(first) {
                            if drag.toggle && !selection.insert(row.path.clone()) {
                                selection.remove(&row.path);
                            } else {
                                selection.insert(row.path.clone());
                            }
                        }
                    }
                    browser.set_selection(selection);
                    let painter = ui.painter().with_clip_rect(viewport);
                    let screen = rect.translate(viewport.min.to_vec2() - output.state.offset);
                    painter.rect_filled(screen, 0, palette.accent.gamma_multiply(0.10));
                    painter.rect_stroke(
                        screen,
                        0,
                        egui::Stroke::new(1.0_f32, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                    if ui.input(|i| i.pointer.primary_down()) {
                        // Scroll near the edges, keeping the content-space anchor fixed.
                        let delta = if pointer.y < viewport.top() + 20.0 {
                            -10.0
                        } else if pointer.y > viewport.bottom() - 20.0 {
                            10.0
                        } else {
                            0.0
                        };
                        if delta != 0.0 {
                            self.list_offset = (self.list_offset + delta)
                                .clamp(0.0, (row_count as f32 * 29.0 - viewport.height()).max(0.0));
                            context.request_repaint_after(Duration::from_millis(16));
                        }
                    }
                }
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.marquee = None;
            }
        });
        if go_parent {
            browser.parent();
            self.marquee = None;
            self.list_offset = 0.0;
        } else if let Some(path) = navigate {
            browser.navigate(path);
            self.marquee = None;
            self.list_offset = 0.0;
        }
        if drag_out {
            self.request_export(context);
        }
        if let Some(index) = preview {
            self.preview_entry(context, index);
        }
        if extract {
            self.show_extract(false);
        }
    }
    fn welcome(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let opening = self
            .active
            .as_ref()
            .is_some_and(|active| active.kind == Kind::List);
        let busy = self.busy();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space((ui.available_height() * 0.10).clamp(12.0, 48.0));
                ui.vertical_centered(|ui| {
                    if opening {
                        ui.spinner();
                        ui.add_space(10.0);
                        ui.heading("正在打开归档…");
                        theme::caption(ui, "归档目录在后台读取，你可以随时取消。");
                        return;
                    }
                    if ui.available_height() > 560.0 {
                        widgets::illustration(ui);
                    } else {
                        theme::logo(ui, 48.0);
                        ui.add_space(12.0);
                    }
                    ui.label(RichText::new("文件归档，轻松处理").size(23.0).strong());
                    theme::caption(ui, "拖入压缩包直接浏览，拖入文件或文件夹开始压缩。");
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        let button_width = 118.0;
                        let group_width = button_width * 2.0 + ui.spacing().item_spacing.x;
                        ui.add_space(((ui.available_width() - group_width) / 2.0).max(0.0));
                        ui.add_enabled_ui(!busy, |ui| {
                            if widgets::button(ui, "打开归档", Icon::Folder, true, button_width)
                                .clicked()
                            {
                                self.dialog(context, Dialog::Open);
                            }
                            if widgets::button(ui, "新建压缩包", Icon::Plus, false, button_width)
                                .clicked()
                            {
                                self.new_archive();
                            }
                        });
                    });
                });
                if opening {
                    return;
                }
                ui.add_space(26.0);
                let recent_width = 520.0_f32.min(ui.available_width() - 24.0);
                ui.horizontal(|ui| {
                    ui.add_space(((ui.available_width() - recent_width) / 2.0).max(0.0));
                    ui.vertical(|ui| {
                        ui.set_width(recent_width);
                        ui.horizontal(|ui| {
                            theme::caption(ui, "最近打开");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !self.preferences.recent.is_empty()
                                        && widgets::subtle_button(ui, "清空").clicked()
                                    {
                                        self.preferences.recent.clear();
                                    }
                                },
                            );
                        });
                        theme::rule(ui);
                        let mut open = None;
                        for path in self.preferences.recent.iter().take(4) {
                            ui.add_enabled_ui(!busy, |ui| {
                                if widgets::nav(
                                    ui,
                                    &path.file_name().unwrap_or_default().to_string_lossy(),
                                    Icon::Archive,
                                    false,
                                    "打开",
                                )
                                .on_hover_text(path.display().to_string())
                                .clicked()
                                {
                                    open = Some(path.clone());
                                }
                            });
                        }
                        if self.preferences.recent.is_empty() {
                            ui.add_space(8.0);
                            theme::caption(ui, "打开过的归档会显示在这里。");
                        }
                        if let Some(path) = open {
                            self.open_archive(context, path, None);
                        }
                    });
                });
            });
    }
}
