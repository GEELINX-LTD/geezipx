use super::*;

const LIFETIME: Duration = Duration::from_secs(2);
const AREA_ID: &str = "notice-toast";

pub(super) struct Notice {
    message: String,
    error: bool,
    expires_at: Option<f64>,
    hovered: bool,
}

impl Notice {
    pub(super) fn new(message: impl Into<String>, error: bool) -> Self {
        Self {
            message: message.into(),
            error,
            expires_at: None,
            hovered: false,
        }
    }
}

impl App {
    pub(super) fn notices(&mut self, context: &egui::Context, footer: egui::Rect) {
        let Some(notice) = &mut self.notice else {
            return;
        };
        let now = context.input(|input| input.time);
        let expires_at = notice
            .expires_at
            .get_or_insert(now + LIFETIME.as_secs_f64());
        let screen = context.content_rect();
        let bounds = egui::Rect::from_min_max(
            screen.left_top() + egui::vec2(12.0, 12.0),
            egui::pos2(screen.right() - 12.0, footer.top() - 12.0),
        );
        let max_width = bounds.width().min(420.0);
        let area_id = egui::Id::new(AREA_ID);
        let previous_size =
            context.memory(|memory| memory.area_rect(area_id).map(|rect| rect.size()));
        let toast = egui::Area::new(area_id)
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(bounds.right_bottom())
            .constrain_to(bounds)
            .default_width(max_width)
            .sense(egui::Sense::hover())
            .fade_in(false)
            .show(context, |ui| {
                let palette = theme::palette(ui);
                let color = if notice.error {
                    Color32::from_rgb(194, 81, 70)
                } else {
                    palette.accent
                };
                egui::Frame::popup(ui.style())
                    .fill(palette.surface)
                    .stroke(egui::Stroke::new(1.0_f32, palette.border))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        // Measure against a fixed limit, independent of the previous toast's size.
                        // A short message stays compact; a longer replacement can grow and wrap.
                        let text =
                            egui::WidgetText::from(RichText::new(&notice.message).color(color))
                                .into_galley(
                                    ui,
                                    Some(egui::TextWrapMode::Wrap),
                                    (max_width
                                        - 26.0
                                        - theme::CONTROL_HEIGHT
                                        - ui.spacing().item_spacing.x)
                                        .max(1.0),
                                    egui::TextStyle::Body.resolve(ui.style()),
                                );
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(text));
                            widgets::icon_button(ui, Icon::Close, "关闭消息").clicked()
                        })
                        .inner
                    })
                    .inner
            });
        if previous_size != Some(toast.response.rect.size()) {
            // Resolve the anchored position before presenting a new or resized message.
            context.request_discard("notice size changed");
        }
        let hovered = toast.response.contains_pointer();
        if hovered || notice.hovered {
            // Leaving after a stationary hover also starts a full countdown.
            *expires_at = now + LIFETIME.as_secs_f64();
        }
        notice.hovered = hovered;
        if toast.inner || now >= *expires_at {
            self.notice = None;
            context.request_repaint();
        } else {
            // Wake only when the deadline arrives, including when the pointer stays still.
            context.request_repaint_after(Duration::from_secs_f64(*expires_at - now));
        }
    }
}
