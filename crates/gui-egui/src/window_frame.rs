use crate::theme;
use eframe::egui::{self, Align2, Color32, CursorIcon, FontId, Rect, Sense, Stroke, Vec2};
use egui::viewport::ResizeDirection;

#[cfg(any(test, target_os = "macos"))]
mod resize;
#[cfg(test)]
mod tests;

pub const MIN_INNER_SIZE: [f32; 2] = [760.0, 480.0];
pub const TITLE_HEIGHT: f32 = 32.0;
const BORDER: f32 = 1.0;
const RADIUS: u8 = 12;
const EDGE: f32 = 5.0;
const CORNER: f32 = 14.0;
const BUTTON_WIDTH: f32 = 34.0;

#[derive(Default)]
pub struct WindowFrame {
    maximized: bool,
    reported_maximized: Option<bool>,
    last_title_click: Option<(f64, egui::Pos2)>,
    #[cfg(target_os = "macos")]
    resize_drag: Option<resize::ResizeDrag>,
}

#[derive(Clone, Copy, Debug, PartialEq, Hash)]
enum Control {
    Minimize,
    Maximize,
    Close,
}

impl WindowFrame {
    pub fn margin(&mut self, context: &egui::Context) -> i8 {
        let reported = context.input(|input| input.viewport().maximized);
        if reported != self.reported_maximized {
            self.maximized = reported.unwrap_or(false);
            self.reported_maximized = reported;
        }
        if self.expanded(context) {
            0
        } else {
            BORDER as i8
        }
    }

    pub fn corner_radius(&self, context: &egui::Context) -> u8 {
        if self.expanded(context) {
            0
        } else {
            RADIUS
        }
    }

    fn expanded(&self, context: &egui::Context) -> bool {
        self.maximized || context.input(|input| input.viewport().fullscreen.unwrap_or(false))
    }

    fn toggle_maximized(&mut self, context: &egui::Context) {
        if context.input(|input| input.viewport().fullscreen.unwrap_or(false)) {
            context.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        } else {
            self.maximized = !self.maximized;
            context.send_viewport_cmd(egui::ViewportCommand::Maximized(self.maximized));
        }
    }

    /// Draw after application modals so OS-level window actions stay available.
    pub fn show(&mut self, context: &egui::Context, dark: bool, title: &str) -> bool {
        #[cfg(target_os = "macos")]
        self.update_resize(context);
        let p = theme::colors(dark);
        let screen = context.content_rect();
        let expanded = self.expanded(context);
        let inset = if expanded { 0.0 } else { BORDER };
        let header = Rect::from_min_size(
            screen.min + Vec2::splat(inset),
            egui::vec2(screen.width() - inset * 2.0, TITLE_HEIGHT),
        );
        let area = egui::Area::new(egui::Id::new("native-title-bar"))
            .order(egui::Order::Foreground)
            .fixed_pos(header.min)
            .default_size(header.size())
            .constrain(false)
            .movable(false)
            .fade_in(false)
            .sense(Sense::hover());
        context.move_to_top(area.layer());
        let close = area
            .show(context, |ui| {
                ui.set_min_size(header.size());
                ui.set_max_size(header.size());
                ui.set_clip_rect(header);
                let radius = self.corner_radius(context).saturating_sub(inset as u8);
                ui.painter().rect_filled(
                    header,
                    egui::CornerRadius {
                        nw: radius,
                        ne: radius,
                        sw: 0,
                        se: 0,
                    },
                    p.canvas,
                );
                ui.painter().hline(
                    header.x_range(),
                    header.bottom() - 0.5,
                    Stroke::new(1.0_f32, p.border),
                );

                let controls = [Control::Minimize, Control::Maximize, Control::Close];
                let controls_left = control_rect(header, Control::Minimize).left();
                let drag_rect = Rect::from_min_max(
                    header.min,
                    egui::pos2(controls_left - 4.0, header.bottom()),
                );
                let drag = ui.interact(drag_rect, ui.id().with("drag"), Sense::click_and_drag());
                if ui.input(|input| input.pointer.primary_pressed()) && !drag.contains_pointer() {
                    self.last_title_click = None;
                }
                if drag.clicked_by(egui::PointerButton::Primary) {
                    if let Some(position) = drag.interact_pointer_pos() {
                        let now = ui.input(|input| input.time);
                        let double = self.last_title_click.is_some_and(|(time, previous)| {
                            context.options(|options| {
                                now - time <= options.input_options.max_double_click_delay
                                    && previous.distance(position)
                                        <= options.input_options.max_click_dist
                            })
                        });
                        self.last_title_click = if double { None } else { Some((now, position)) };
                        if double {
                            self.toggle_maximized(context);
                        }
                    }
                } else if drag.drag_started_by(egui::PointerButton::Primary) {
                    self.last_title_click = None;
                    // Wait for movement so the OS drag loop cannot swallow title-bar clicks.
                    context.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }

                let logo = Rect::from_center_size(
                    egui::pos2(header.left() + 21.0, header.center().y),
                    Vec2::splat(18.0),
                );
                theme::paint_logo(ui.painter(), logo);
                let brand_left = logo.right() + 8.0;
                ui.painter().text(
                    egui::pos2(brand_left, header.center().y),
                    Align2::LEFT_CENTER,
                    "GeeZipX",
                    FontId::proportional(13.0),
                    p.text,
                );
                let title_left = brand_left + 68.0;
                let title_rect = Rect::from_min_max(
                    egui::pos2(title_left, header.top()),
                    egui::pos2(controls_left - 16.0, header.bottom()),
                );
                if title_rect.width() > 0.0 {
                    let mut text_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(title_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    text_ui.add(
                        egui::Label::new(egui::RichText::new(title).size(12.0).color(p.muted))
                            .selectable(false)
                            .truncate(),
                    );
                }

                let mut close = false;
                for control in controls {
                    let rect = control_rect(header, control);
                    let response = ui.interact(rect, ui.id().with(control), Sense::click());
                    let label = match control {
                        Control::Minimize => "最小化",
                        Control::Maximize if expanded => "还原窗口",
                        Control::Maximize => "最大化",
                        Control::Close => "关闭窗口",
                    };
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label)
                    });
                    let active = response.hovered() || response.has_focus();
                    let close_hover = active && control == Control::Close;
                    if active || response.is_pointer_button_down_on() {
                        ui.painter().rect_filled(
                            rect.shrink(1.0),
                            5,
                            if close_hover {
                                Color32::from_rgb(216, 67, 79)
                            } else {
                                p.tint
                            },
                        );
                    }
                    paint_control(
                        ui.painter(),
                        rect,
                        control,
                        expanded,
                        if close_hover { Color32::WHITE } else { p.muted },
                    );
                    if response.clicked() {
                        match control {
                            Control::Minimize => {
                                context.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
                            }
                            Control::Maximize => self.toggle_maximized(context),
                            Control::Close => close = true,
                        }
                    }
                    response.on_hover_text(label);
                }
                close
            })
            .inner;
        if !expanded {
            self.resize_edges(context, screen);
            context
                .layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("native-window-border"),
                ))
                .rect_stroke(
                    screen.shrink(0.5),
                    self.corner_radius(context),
                    Stroke::new(BORDER, p.border),
                    egui::StrokeKind::Inside,
                );
        }
        close
    }

    fn resize_edges(&mut self, context: &egui::Context, screen: Rect) {
        for (index, (rect, direction, cursor)) in resize_regions(screen).into_iter().enumerate() {
            let area = egui::Area::new(egui::Id::new(("native-resize", index)))
                .order(egui::Order::Foreground)
                .fixed_pos(rect.min)
                .default_size(rect.size())
                .constrain(false)
                .movable(false)
                .fade_in(false)
                .sense(Sense::click_and_drag());
            context.move_to_top(area.layer());
            let response = area
                .show(context, |ui| {
                    ui.set_min_size(rect.size());
                    ui.set_max_size(rect.size());
                })
                .response
                .on_hover_cursor(cursor);
            if response.is_pointer_button_down_on()
                && context.input(|input| input.pointer.primary_pressed())
            {
                self.begin_resize(context, direction);
            }
        }
    }
    fn begin_resize(&mut self, context: &egui::Context, direction: ResizeDirection) {
        #[cfg(not(target_os = "macos"))]
        context.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        // winit has no native drag_resize_window implementation on macOS.
        #[cfg(target_os = "macos")]
        {
            self.resize_drag = resize::ResizeDrag::begin(context, direction);
        }
    }

    #[cfg(target_os = "macos")]
    fn update_resize(&mut self, context: &egui::Context) {
        if self
            .resize_drag
            .as_ref()
            .is_some_and(|drag| !drag.update(context))
        {
            self.resize_drag = None;
        }
    }
}

fn control_rect(header: Rect, control: Control) -> Rect {
    let offset = match control {
        Control::Minimize => 3.0,
        Control::Maximize => 2.0,
        Control::Close => 1.0,
    };
    Rect::from_min_size(
        egui::pos2(
            header.right() - 5.0 - BUTTON_WIDTH * offset,
            header.top() + 3.0,
        ),
        egui::vec2(BUTTON_WIDTH, TITLE_HEIGHT - 6.0),
    )
}

fn paint_control(
    painter: &egui::Painter,
    rect: Rect,
    control: Control,
    maximized: bool,
    color: Color32,
) {
    let icon = Rect::from_center_size(rect.center(), Vec2::splat(10.0));
    let stroke = Stroke::new(1.2_f32, color);
    match control {
        Control::Minimize => {
            painter.hline(icon.x_range(), icon.center().y + 2.0, stroke);
        }
        Control::Maximize if maximized => {
            let back = icon.translate(egui::vec2(2.0, -2.0)).shrink(1.0);
            painter.line_segment([back.left_top(), back.right_top()], stroke);
            painter.line_segment([back.right_top(), back.right_bottom()], stroke);
            painter.rect_stroke(
                icon.translate(egui::vec2(-1.0, 1.0)).shrink(1.0),
                1,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Control::Maximize => {
            painter.rect_stroke(icon, 1, stroke, egui::StrokeKind::Inside);
        }
        Control::Close => {
            painter.line_segment([icon.left_top(), icon.right_bottom()], stroke);
            painter.line_segment([icon.right_top(), icon.left_bottom()], stroke);
        }
    }
}

fn resize_regions(screen: Rect) -> [(Rect, ResizeDirection, CursorIcon); 8] {
    let rect = |x0, y0, x1, y1| {
        Rect::from_min_max(
            screen.min + egui::vec2(x0, y0),
            screen.min + egui::vec2(x1, y1),
        )
    };
    let w = screen.width();
    let h = screen.height();
    [
        (
            rect(0.0, 0.0, CORNER, CORNER),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNwSe,
        ),
        (
            rect(CORNER, 0.0, w - CORNER, EDGE),
            ResizeDirection::North,
            CursorIcon::ResizeVertical,
        ),
        (
            rect(w - CORNER, 0.0, w, CORNER),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNeSw,
        ),
        (
            rect(w - EDGE, CORNER, w, h - CORNER),
            ResizeDirection::East,
            CursorIcon::ResizeHorizontal,
        ),
        (
            rect(w - CORNER, h - CORNER, w, h),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeNwSe,
        ),
        (
            rect(CORNER, h - EDGE, w - CORNER, h),
            ResizeDirection::South,
            CursorIcon::ResizeVertical,
        ),
        (
            rect(0.0, h - CORNER, CORNER, h),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeNeSw,
        ),
        (
            rect(0.0, CORNER, EDGE, h - CORNER),
            ResizeDirection::West,
            CursorIcon::ResizeHorizontal,
        ),
    ]
}
