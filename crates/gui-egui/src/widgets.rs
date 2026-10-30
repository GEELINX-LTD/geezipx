//! Small, consistent painted controls. No font glyphs or platform icon dependencies.
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

#[derive(Clone, Copy)]
pub enum Icon {
    Archive,
    Folder,
    File,
    Plus,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
    Chevron,
    ChevronDown,
    Check,
    Clock,
    Sun,
    Moon,
    Close,
    Shield,
    Search,
}

pub fn command(key: egui::Key) -> egui::KeyboardShortcut {
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key)
}

pub fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.item_spacing = egui::vec2(8.0, 2.0);
    style.spacing.interact_size.y = 32.0;
    for visuals in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        visuals.corner_radius = 5.into();
    }
}

pub fn menu(ui: &mut Ui) {
    ui.set_width(252.0);
    menu_style(ui.style_mut());
}

pub fn menu_item(
    ui: &mut Ui,
    label: &str,
    shortcut: Option<egui::KeyboardShortcut>,
    enabled: bool,
) -> Response {
    let mut button = egui::Button::new(label)
        .min_size(egui::vec2(252.0, 32.0))
        .truncate();
    if let Some(shortcut) = shortcut {
        button = button.shortcut_text(
            egui::RichText::new(ui.ctx().format_shortcut(&shortcut))
                .monospace()
                .size(12.0),
        );
    }
    ui.add_enabled(enabled, button)
}

fn paint_keycap(ui: &Ui, rect: Rect, text: &str) {
    let p = theme::palette(ui);
    ui.painter().rect_filled(rect, 4, p.soft);
    ui.painter()
        .rect_stroke(rect, 4, Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::monospace(11.0),
        p.muted,
    );
}

pub fn shortcut_hint(ui: &mut Ui, label: &str, shortcut: egui::KeyboardShortcut) {
    let p = theme::palette(ui);
    let text = ui.ctx().format_shortcut(&shortcut);
    let key = ui
        .painter()
        .layout_no_wrap(text.clone(), FontId::monospace(11.0), p.muted);
    let label = ui
        .painter()
        .layout_no_wrap(label.into(), FontId::proportional(12.0), p.muted);
    let key_width = key.size().x + 12.0;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(label.size().x + 12.0 + key_width, 22.0),
        Sense::hover(),
    );
    ui.painter().galley(
        rect.left_center() - egui::vec2(0.0, label.size().y / 2.0),
        label,
        p.muted,
    );
    paint_keycap(
        ui,
        Rect::from_min_max(egui::pos2(rect.right() - key_width, rect.top()), rect.max),
        &text,
    );
}

pub fn subtle_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(12.0)
                .color(theme::palette(ui).accent),
        )
        .small()
        .min_size(egui::vec2(0.0, 28.0))
        .frame_when_inactive(false),
    )
}

pub fn text_edit(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(egui::Margin::symmetric(10, 7))
        .desired_width(f32::INFINITY)
}

pub fn search(ui: &mut Ui, query: &mut String, width: f32) -> Response {
    let p = theme::palette(ui);
    let id = egui::Id::new("archive-search");
    let shortcut = ui.ctx().format_shortcut(&command(egui::Key::F));
    let key = ui
        .painter()
        .layout_no_wrap(shortcut.clone(), FontId::monospace(11.0), p.muted);
    let empty = query.is_empty();
    let trailing = if empty { key.size().x + 12.0 } else { 24.0 };
    let (rect, frame_response) = ui.allocate_exact_size(egui::vec2(width, 32.0), Sense::click());
    let focused = ui.memory(|memory| memory.has_focus(id));
    ui.painter().rect_filled(rect, 6, p.surface);
    ui.painter().rect_stroke(
        rect,
        6,
        Stroke::new(
            1.0_f32,
            if focused || frame_response.hovered() {
                p.accent
            } else {
                p.border
            },
        ),
        StrokeKind::Inside,
    );
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(16.0, 0.0),
            Vec2::splat(16.0),
        ),
        Icon::Search,
        p.muted,
    );
    let content = Rect::from_min_max(
        rect.min + egui::vec2(32.0, 7.0),
        rect.max - egui::vec2(trailing + 16.0, 7.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(content)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let mut response = child.add_sized(
        content.size(),
        egui::TextEdit::singleline(query)
            .id(id)
            .frame(false)
            .margin(egui::Margin::ZERO)
            .hint_text("搜索此目录"),
    );
    if frame_response.clicked() {
        response.request_focus();
    }
    let right = Rect::from_center_size(
        rect.right_center() - egui::vec2(trailing / 2.0 + 8.0, 0.0),
        egui::vec2(trailing, 22.0),
    );
    if empty {
        paint_keycap(ui, right, &shortcut);
    } else {
        let clear = ui
            .interact(right, id.with("clear"), Sense::click())
            .on_hover_text("清除搜索");
        if clear.hovered() {
            ui.painter().rect_filled(right, 4, p.tint);
        }
        paint_icon(ui.painter(), right.shrink(5.0), Icon::Close, p.muted);
        if clear.clicked() {
            query.clear();
            response.mark_changed();
            response.request_focus();
        }
    }
    response
}

pub fn paint_icon(p: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let r = Rect::from_center_size(rect.center(), Vec2::splat(rect.width().min(rect.height())));
    let point = |x, y| r.min + egui::vec2(x, y) * (r.width() / 24.0);
    let stroke = Stroke::new(1.6_f32, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([point(a.0, a.1), point(b.0, b.1)], stroke);
    };
    match icon {
        Icon::Folder => {
            p.add(egui::Shape::closed_line(
                vec![
                    point(3.0, 7.0),
                    point(3.0, 5.0),
                    point(9.0, 5.0),
                    point(11.0, 8.0),
                    point(21.0, 8.0),
                    point(21.0, 19.0),
                    point(3.0, 19.0),
                ],
                stroke,
            ));
            line((3.0, 9.0), (21.0, 9.0));
        }
        Icon::File | Icon::Archive => {
            p.add(egui::Shape::closed_line(
                vec![
                    point(5.0, 3.0),
                    point(14.0, 3.0),
                    point(19.0, 8.0),
                    point(19.0, 21.0),
                    point(5.0, 21.0),
                ],
                stroke,
            ));
            line((14.0, 3.0), (14.0, 8.0));
            line((14.0, 8.0), (19.0, 8.0));
            if matches!(icon, Icon::Archive) {
                for y in [9.0, 12.0, 15.0] {
                    line((10.0, y), (13.0, y));
                }
            } else {
                line((9.0, 13.0), (15.0, 13.0));
                line((9.0, 17.0), (15.0, 17.0));
            }
        }
        Icon::Search => {
            p.circle_stroke(point(10.0, 10.0), r.width() * 0.27, stroke);
            line((15.0, 15.0), (21.0, 21.0));
        }
        Icon::Plus => {
            line((5.0, 12.0), (19.0, 12.0));
            line((12.0, 5.0), (12.0, 19.0));
        }
        Icon::Close => {
            line((6.0, 6.0), (18.0, 18.0));
            line((18.0, 6.0), (6.0, 18.0));
        }
        Icon::ArrowRight => {
            line((4.0, 12.0), (20.0, 12.0));
            line((14.0, 6.0), (20.0, 12.0));
            line((20.0, 12.0), (14.0, 18.0));
        }
        Icon::ArrowLeft => {
            line((20.0, 12.0), (4.0, 12.0));
            line((10.0, 6.0), (4.0, 12.0));
            line((4.0, 12.0), (10.0, 18.0));
        }
        Icon::ArrowUp => {
            line((12.0, 20.0), (12.0, 4.0));
            line((6.0, 10.0), (12.0, 4.0));
            line((12.0, 4.0), (18.0, 10.0));
        }
        Icon::ArrowDown => {
            line((12.0, 4.0), (12.0, 20.0));
            line((6.0, 14.0), (12.0, 20.0));
            line((12.0, 20.0), (18.0, 14.0));
        }
        Icon::ChevronDown => {
            line((6.0, 9.0), (12.0, 15.0));
            line((12.0, 15.0), (18.0, 9.0));
        }
        Icon::Chevron => {
            line((9.0, 6.0), (15.0, 12.0));
            line((15.0, 12.0), (9.0, 18.0));
        }
        Icon::Check => {
            line((5.0, 12.0), (10.0, 17.0));
            line((10.0, 17.0), (20.0, 6.0));
        }
        Icon::Clock => {
            p.circle_stroke(point(12.0, 12.0), r.width() * 0.38, stroke);
            line((12.0, 6.0), (12.0, 12.0));
            line((12.0, 12.0), (16.0, 14.0));
        }
        Icon::Sun => {
            p.circle_stroke(point(12.0, 12.0), r.width() * 0.18, stroke);
            for angle in 0..8 {
                let a = angle as f32 * std::f32::consts::TAU / 8.0;
                let v = egui::vec2(a.cos(), a.sin());
                p.line_segment(
                    [
                        r.center() + v * r.width() * 0.32,
                        r.center() + v * r.width() * 0.43,
                    ],
                    stroke,
                );
            }
        }
        Icon::Moon => {
            p.add(egui::Shape::line(
                vec![
                    point(17.0, 3.0),
                    point(12.0, 4.0),
                    point(7.0, 8.0),
                    point(5.0, 14.0),
                    point(8.0, 19.0),
                    point(14.0, 21.0),
                    point(20.0, 17.0),
                    point(14.0, 16.0),
                    point(11.0, 12.0),
                    point(12.0, 7.0),
                    point(17.0, 3.0),
                ],
                stroke,
            ));
        }
        Icon::Shield => {
            p.add(egui::Shape::closed_line(
                vec![
                    point(12.0, 3.0),
                    point(20.0, 6.0),
                    point(19.0, 15.0),
                    point(16.0, 19.0),
                    point(12.0, 22.0),
                    point(8.0, 19.0),
                    point(5.0, 15.0),
                    point(4.0, 6.0),
                ],
                stroke,
            ));
            line((8.0, 12.0), (11.0, 15.0));
            line((11.0, 15.0), (16.0, 9.0));
        }
    }
}

fn accessible(response: &Response, label: &str, selected: Option<bool>) {
    response.widget_info(|| match selected {
        Some(value) => egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            response.enabled(),
            value,
            label,
        ),
        None => egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label),
    });
}

fn focus(ui: &Ui, response: &Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.shrink(1.0),
            6,
            Stroke::new(2.0_f32, theme::palette(ui).accent),
            StrokeKind::Inside,
        );
    }
}

pub fn button(ui: &mut Ui, text: &str, icon: Icon, primary: bool, width: f32) -> Response {
    let p = theme::palette(ui);
    let font = FontId::proportional(14.0);
    let galley = ui.painter().layout_no_wrap(text.into(), font, p.text);
    let w = if width > 0.0 {
        width
    } else {
        galley.size().x + 46.0
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(w, 32.0), Sense::click());
    accessible(&response, text, None);
    let fill = if primary {
        if response.hovered() {
            Color32::from_rgb(67, 82, 202)
        } else {
            theme::ACCENT
        }
    } else if response.hovered() {
        p.tint
    } else {
        p.surface
    };
    ui.painter()
        .rect_filled(rect, 6, if ui.is_enabled() { fill } else { p.soft });
    if !primary || !ui.is_enabled() {
        ui.painter()
            .rect_stroke(rect, 6, Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
    }
    let color = if !ui.is_enabled() {
        p.muted
    } else if primary {
        Color32::WHITE
    } else {
        p.text
    };
    let left = rect.center().x - (galley.size().x + 28.0) / 2.0;
    paint_icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(left + 8.0, rect.center().y), Vec2::splat(18.0)),
        icon,
        color,
    );
    ui.painter().galley_with_override_text_color(
        egui::pos2(left + 28.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    focus(ui, &response);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn icon_control(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    let p = theme::palette(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    accessible(&response, label, None);
    if response.hovered() {
        ui.painter().rect_filled(rect, 7, p.tint);
    }
    paint_icon(
        ui.painter(),
        rect.shrink(8.0),
        icon,
        if ui.is_enabled() {
            p.muted
        } else {
            p.muted.gamma_multiply(0.4)
        },
    );
    focus(ui, &response);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    icon_control(ui, icon, label).on_hover_text(label)
}

pub fn navigation_button(ui: &mut Ui, icon: Icon, label: &str, extra: bool) -> Response {
    icon_control(ui, icon, label).on_hover_ui(|ui| {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.monospace(if extra { "Backspace / Alt +" } else { "Alt +" });
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
            paint_icon(ui.painter(), rect, icon, theme::palette(ui).text);
        });
    })
}

pub fn nav(ui: &mut Ui, label: &str, icon: Icon, selected: bool, detail: &str) -> Response {
    let p = theme::palette(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 43.0), Sense::click());
    accessible(&response, label, Some(selected));
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, 9, if selected { p.tint } else { p.soft });
    }
    let color = if selected { p.accent } else { p.muted };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(20.0, 0.0),
            Vec2::splat(19.0),
        ),
        icon,
        color,
    );
    let detail_width = ui
        .painter()
        .layout_no_wrap(detail.into(), FontId::proportional(11.0), p.muted)
        .size()
        .x;
    let label = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (rect.width() - detail_width - 84.0).max(0.0),
        FontId::proportional(14.0),
    );
    ui.painter().galley_with_override_text_color(
        rect.left_center() + egui::vec2(40.0, -label.size().y / 2.0),
        label,
        if selected { p.accent } else { p.text },
    );
    ui.painter().text(
        rect.right_center() - egui::vec2(14.0, 0.0),
        Align2::RIGHT_CENTER,
        detail,
        FontId::proportional(11.0),
        p.muted,
    );
    focus(ui, &response);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn paint_check(ui: &Ui, rect: Rect, selected: bool, hovered: bool) {
    let p = theme::palette(ui);
    ui.painter()
        .rect_filled(rect, 4, if selected { theme::ACCENT } else { p.surface });
    ui.painter().rect_stroke(
        rect,
        4,
        Stroke::new(
            1.0_f32,
            if selected || hovered {
                p.accent
            } else {
                p.border
            },
        ),
        StrokeKind::Inside,
    );
    if selected {
        paint_icon(ui.painter(), rect.shrink(2.0), Icon::Check, Color32::WHITE);
    }
}

pub fn check(ui: &mut Ui, selected: &mut bool, label: &str) -> Response {
    let p = theme::palette(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), FontId::proportional(13.0), p.text);
    let (rect, mut response) = ui.allocate_exact_size(
        egui::vec2((galley.size().x + 28.0).min(ui.available_width()), 26.0),
        Sense::click(),
    );
    if response.clicked() {
        *selected = !*selected;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            response.enabled(),
            *selected,
            label,
        )
    });
    paint_check(
        ui,
        Rect::from_center_size(rect.left_center() + egui::vec2(9.0, 0.0), Vec2::splat(18.0)),
        *selected,
        response.hovered(),
    );
    ui.painter().with_clip_rect(rect).galley(
        rect.left_center() + egui::vec2(28.0, -galley.size().y / 2.0),
        galley,
        p.text,
    );
    focus(ui, &response);
    response
}

pub fn badge(ui: &mut Ui, text: &str) {
    let p = theme::palette(ui);
    let galley = ui
        .painter()
        .layout_no_wrap(text.into(), FontId::proportional(11.0), p.accent);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(galley.size().x + 16.0, 24.0), Sense::hover());
    ui.painter().rect_filled(rect, 5, p.tint);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, p.accent);
}

/// Column boundaries shared by the header and every virtualized row.
#[derive(Clone, Copy)]
pub struct Columns {
    pub edges: [f32; 5],
}
impl Columns {
    pub fn new(width: f32, trailing: [f32; 3]) -> Self {
        let width = width.max(320.0);
        let scale = ((width - 180.0) / trailing.iter().sum::<f32>()).min(1.0);
        let [size, packed, kind] = trailing.map(|value| value * scale);
        Self {
            edges: [
                0.0,
                width - size - packed - kind,
                width - packed - kind,
                width - kind,
                width,
            ],
        }
    }
    pub fn cell(&self, rect: Rect, index: usize) -> Rect {
        Rect::from_min_max(
            egui::pos2(rect.left() + self.edges[index], rect.top()),
            egui::pos2(rect.left() + self.edges[index + 1], rect.bottom()),
        )
    }
}
pub struct FileRow<'a> {
    pub name: &'a str,
    pub size: &'a str,
    pub packed: &'a str,
    pub kind: &'a str,
    pub folder: bool,
    pub selected: bool,
    pub focused: bool,
}
pub fn file_row(ui: &mut Ui, id: egui::Id, row: FileRow<'_>, columns: Columns) -> Response {
    let p = theme::palette(ui);
    let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), 29.0));
    let response = ui.interact(rect, id, Sense::click_and_drag());
    accessible(&response, row.name, Some(row.selected));
    if row.selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, 3, if row.selected { p.tint } else { p.soft });
    }
    if row.focused {
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            3,
            Stroke::new(1.0_f32, p.accent.gamma_multiply(0.7)),
            StrokeKind::Inside,
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(17.0, 0.0),
            Vec2::splat(17.0),
        ),
        if row.folder { Icon::Folder } else { Icon::File },
        if row.folder { p.accent } else { p.muted },
    );
    for (index, text) in [row.name, row.size, row.packed, row.kind]
        .into_iter()
        .enumerate()
    {
        let cell = columns.cell(rect, index);
        let text_rect = Rect::from_min_max(
            cell.min + egui::vec2(if index == 0 { 33.0 } else { 10.0 }, 0.0),
            cell.max - egui::vec2(10.0, 0.0),
        );
        let galley = ui.painter().layout_no_wrap(
            text.into(),
            FontId::proportional(13.0),
            if index == 0 { p.text } else { p.muted },
        );
        let pos = if index == 1 || index == 2 {
            text_rect.right_center() - egui::vec2(galley.size().x, galley.size().y / 2.0)
        } else {
            text_rect.left_center() - egui::vec2(0.0, galley.size().y / 2.0)
        };
        ui.painter()
            .with_clip_rect(text_rect)
            .galley(pos, galley, p.text);
    }
    response
}

pub fn source_row(ui: &mut Ui, name: &str, path: &str) -> bool {
    let p = theme::palette(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 48.0), Sense::hover());
    ui.painter().rect_filled(rect, 8, p.soft);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            rect.left_center() + egui::vec2(24.0, 0.0),
            Vec2::splat(22.0),
        ),
        Icon::File,
        p.accent,
    );
    let label_rect = Rect::from_min_max(
        rect.min + egui::vec2(47.0, 0.0),
        rect.max - egui::vec2(38.0, 0.0),
    );
    let painter = ui.painter().with_clip_rect(label_rect);
    painter.text(
        label_rect.left_center() - egui::vec2(0.0, 10.0),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(14.0),
        p.text,
    );
    painter.text(
        label_rect.left_center() + egui::vec2(0.0, 12.0),
        Align2::LEFT_CENTER,
        path,
        FontId::proportional(11.0),
        p.muted,
    );
    let remove = Rect::from_center_size(
        rect.right_center() - egui::vec2(20.0, 0.0),
        Vec2::splat(28.0),
    );
    let response = ui.interact(remove, ui.id().with("remove"), Sense::click());
    accessible(&response, &format!("移除 {name}"), None);
    paint_icon(ui.painter(), remove.shrink(6.0), Icon::Close, p.muted);
    focus(ui, &response);
    response.on_hover_text(path).clicked()
}

pub fn illustration(ui: &mut Ui) {
    let p = theme::palette(ui);
    let (r, _) = ui.allocate_exact_size(egui::vec2(240.0, 142.0), Sense::hover());
    let c = r.center();
    ui.painter().circle_filled(c, 65.0, p.tint);
    for (offset, icon, size) in [
        (egui::vec2(-65.0, 10.0), Icon::File, 48.0),
        (egui::vec2(62.0, -18.0), Icon::Folder, 46.0),
        (Vec2::ZERO, Icon::Archive, 86.0),
    ] {
        let rect = Rect::from_center_size(c + offset, Vec2::splat(size));
        ui.painter().rect_filled(
            rect.translate(egui::vec2(0.0, 4.0)),
            14,
            p.border.gamma_multiply(0.4),
        );
        ui.painter().rect_filled(rect, 14, p.surface);
        ui.painter()
            .rect_stroke(rect, 14, Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
        paint_icon(ui.painter(), rect.shrink(size * 0.23), icon, p.accent);
    }
    for offset in [egui::vec2(-92.0, -42.0), egui::vec2(92.0, 48.0)] {
        ui.painter()
            .circle_filled(c + offset, 3.0, p.accent.gamma_multiply(0.35));
    }
}
