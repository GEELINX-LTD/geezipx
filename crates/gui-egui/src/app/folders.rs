use super::*;

impl App {
    pub(super) fn folder_tree(&mut self, ui: &mut egui::Ui) {
        let max_width = (ui.available_width() * 0.38).clamp(156.0, 300.0);
        egui::SidePanel::left("archive-folders")
            .default_width(200.0)
            .width_range(156.0..=max_width)
            .resizable(true)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                right: 8,
                ..Default::default()
            }))
            .show_inside(ui, |ui| {
                let Some(archive) = self.archive.as_mut() else {
                    return;
                };
                let browser = &mut archive.browser;
                let palette = theme::palette(ui);
                ui.spacing_mut().item_spacing.y = 0.0;
                let (header, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 29.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(header, 4, palette.soft);
                ui.painter().text(
                    header.left_center() + egui::vec2(12.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "文件夹",
                    egui::FontId::proportional(12.0),
                    palette.muted,
                );
                let height = ui.available_height().max(29.0);
                if let Some(index) = browser.tree_reveal.take() {
                    let top = index as f32 * 29.0;
                    if top < self.tree_offset {
                        self.tree_offset = top;
                    }
                    if top + 29.0 > self.tree_offset + height {
                        self.tree_offset = (top + 29.0 - height).max(0.0);
                    }
                }
                let mut navigate = None;
                let mut toggle = None;
                let output = egui::ScrollArea::vertical()
                    .id_salt(("folder-tree-rows", &archive.path))
                    .auto_shrink([false, false])
                    .vertical_scroll_offset(self.tree_offset)
                    .show_rows(ui, 29.0, browser.folder_tree.len(), |ui, range| {
                        for index in range {
                            let node = &browser.folder_tree[index];
                            let (_, rect) =
                                ui.allocate_space(egui::vec2(ui.available_width(), 29.0));
                            let indent =
                                (node.depth as f32 * 14.0).min((rect.width() - 80.0).max(0.0));
                            let arrow = egui::Rect::from_min_size(
                                rect.min + egui::vec2(indent, 0.0),
                                egui::vec2(22.0, 29.0),
                            );
                            let id =
                                egui::Id::new(("folder-tree-entry", &archive.path, &node.path));
                            let response = ui.interact(
                                egui::Rect::from_min_max(arrow.right_top(), rect.right_bottom()),
                                id,
                                egui::Sense::click(),
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::SelectableLabel,
                                    ui.is_enabled(),
                                    node.path == browser.directory,
                                    &node.name,
                                )
                            });
                            let expand = ui.interact(
                                arrow,
                                id.with("expand"),
                                if node.has_children {
                                    egui::Sense::click()
                                } else {
                                    egui::Sense::hover()
                                },
                            );
                            let selected = node.path == browser.directory;
                            if selected || response.hovered() || expand.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    3,
                                    if selected { palette.tint } else { palette.soft },
                                );
                            }
                            if node.has_children {
                                widgets::paint_icon(
                                    ui.painter(),
                                    egui::Rect::from_center_size(
                                        arrow.center(),
                                        egui::Vec2::splat(12.0),
                                    ),
                                    if node.expanded {
                                        Icon::ArrowDown
                                    } else {
                                        Icon::Chevron
                                    },
                                    palette.muted,
                                );
                                if expand.clicked() {
                                    toggle = Some(node.path.clone());
                                }
                            }
                            widgets::paint_icon(
                                ui.painter(),
                                egui::Rect::from_center_size(
                                    egui::pos2(arrow.right() + 9.0, rect.center().y),
                                    egui::Vec2::splat(16.0),
                                ),
                                Icon::Folder,
                                palette.accent,
                            );
                            let text_rect = egui::Rect::from_min_max(
                                egui::pos2(arrow.right() + 23.0, rect.top()),
                                rect.right_bottom() - egui::vec2(5.0, 0.0),
                            );
                            let mut job = egui::text::LayoutJob::simple(
                                node.name.clone(),
                                egui::FontId::proportional(13.0),
                                palette.text,
                                text_rect.width().max(0.0),
                            );
                            job.wrap.max_rows = 1;
                            let galley = ui.painter().layout_job(job);
                            ui.painter()
                                .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                                .galley(
                                    text_rect.left_center()
                                        - egui::vec2(0.0, galley.size().y / 2.0),
                                    galley,
                                    palette.text,
                                );
                            if response.clicked() {
                                navigate = Some(node.path.clone());
                            }
                            response.on_hover_text(if node.path.is_empty() {
                                archive.path.display().to_string()
                            } else {
                                node.path.clone()
                            });
                        }
                    });
                self.tree_offset = output.state.offset.y;
                if let Some(path) = toggle {
                    browser.toggle_folder(&path);
                }
                if let Some(path) = navigate {
                    browser.navigate(path);
                    self.row_activation.reset();
                    self.marquee = None;
                    ui.ctx()
                        .memory_mut(|memory| memory.request_focus(egui::Id::new("archive-list")));
                }
            });
    }
}
