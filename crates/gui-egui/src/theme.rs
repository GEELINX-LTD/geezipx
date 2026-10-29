use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, Stroke, TextStyle,
};
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(82, 99, 224);
pub const MUTED: Color32 = Color32::from_rgb(133, 143, 161);

/// Semantic colors shared by standard inputs and our painted components.
pub struct Palette {
    pub canvas: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub soft: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub tint: Color32,
}

pub fn palette(ui: &egui::Ui) -> Palette {
    colors(ui.visuals().dark_mode)
}

pub fn colors(dark: bool) -> Palette {
    if dark {
        Palette {
            canvas: Color32::from_rgb(23, 26, 34),
            sidebar: Color32::from_rgb(28, 31, 41),
            surface: Color32::from_rgb(32, 36, 47),
            soft: Color32::from_rgb(40, 44, 58),
            border: Color32::from_rgb(49, 54, 69),
            text: Color32::from_rgb(233, 236, 245),
            muted: Color32::from_rgb(151, 161, 183),
            accent: Color32::from_rgb(155, 167, 255),
            tint: Color32::from_rgb(47, 53, 86),
        }
    } else {
        Palette {
            canvas: Color32::from_rgb(248, 249, 252),
            sidebar: Color32::from_rgb(241, 243, 249),
            surface: Color32::WHITE,
            soft: Color32::from_rgb(243, 245, 250),
            border: Color32::from_rgb(228, 232, 241),
            text: Color32::from_rgb(38, 45, 64),
            muted: Color32::from_rgb(117, 128, 150),
            accent: ACCENT,
            tint: Color32::from_rgb(234, 237, 255),
        }
    }
}

pub fn fonts(context: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "cjk".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/DroidSansFallbackFull.ttf"
        ))),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".into());
    }
    context.set_fonts(fonts);
}

pub fn apply(context: &egui::Context, dark: bool) {
    let p = colors(dark);
    let mut style = egui::Style {
        visuals: if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        },
        ..Default::default()
    };
    style.visuals.panel_fill = p.canvas;
    style.visuals.window_fill = p.surface;
    style.visuals.extreme_bg_color = p.soft;
    style.visuals.faint_bg_color = p.soft;
    style.visuals.override_text_color = Some(p.text);
    style.visuals.selection.bg_fill = p.tint;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, p.accent);
    style.visuals.hyperlink_color = p.accent;
    style.visuals.window_corner_radius = 14.into();
    for (state, fill) in [
        (&mut style.visuals.widgets.noninteractive, p.surface),
        (&mut style.visuals.widgets.inactive, p.soft),
        (&mut style.visuals.widgets.hovered, p.tint),
        (&mut style.visuals.widgets.active, p.tint),
        (&mut style.visuals.widgets.open, p.tint),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0_f32, p.border);
        state.fg_stroke = Stroke::new(1.0_f32, p.text);
        state.corner_radius = 7.into();
        state.expansion = 0.0;
    }
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    style.spacing.text_edit_width = 220.0;
    style.spacing.combo_height = 280.0;
    style.spacing.scroll = egui::style::ScrollStyle::thin();
    for (kind, size) in [
        (TextStyle::Body, 14.0),
        (TextStyle::Button, 14.0),
        (TextStyle::Heading, 18.0),
        (TextStyle::Small, 12.0),
        (TextStyle::Monospace, 13.0),
    ] {
        let font = if kind == TextStyle::Monospace {
            FontId::monospace(size)
        } else {
            FontId::proportional(size)
        };
        style.text_styles.insert(kind, font);
    }
    context.set_style(style);
}

pub fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    crate::widgets::button(
        ui,
        text,
        crate::widgets::Icon::ArrowRight,
        true,
        ui.available_width(),
    )
}

pub fn card(ui: &egui::Ui) -> egui::Frame {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(14)
        .inner_margin(20)
}

pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).size(12.0).color(palette(ui).muted));
}

pub fn title(ui: &mut egui::Ui, eyebrow: &str, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(eyebrow)
            .size(11.0)
            .color(palette(ui).accent)
            .extra_letter_spacing(1.8),
    );
    ui.add_space(2.0);
    ui.label(RichText::new(title).size(27.0).strong());
    caption(ui, subtitle);
}

pub fn rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0_f32, palette(ui).border),
    );
}

pub fn logo(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint_logo(ui.painter(), rect);
}

fn paint_logo(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_filled(rect, rect.width() * 0.28, ACCENT);
    let s = rect.width() / 32.0;
    let origin = rect.min;
    for index in 0..5 {
        let x = if index % 2 == 0 { 12.0 } else { 16.0 };
        painter.rect_filled(
            egui::Rect::from_min_size(
                origin + egui::vec2(x * s, (5.0 + index as f32 * 3.5) * s),
                egui::vec2(4.0 * s, 2.5 * s),
            ),
            0.5,
            Color32::WHITE,
        );
    }
    painter.rect_stroke(
        egui::Rect::from_min_size(
            origin + egui::vec2(12.0 * s, 23.0 * s),
            egui::vec2(8.0 * s, 5.0 * s),
        ),
        1.5,
        Stroke::new(1.7 * s, Color32::WHITE),
        egui::StrokeKind::Inside,
    );
}

pub fn bytes(value: u64) -> String {
    let value = value as f64;
    for (unit, scale) in [("TB", 1e12), ("GB", 1e9), ("MB", 1e6), ("KB", 1e3)] {
        if value >= scale {
            return format!("{:.1} {unit}", value / scale);
        }
    }
    format!("{value:.0} B")
}

pub fn icon() -> Arc<egui::IconData> {
    let mut rgba = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let offset = (y * 32 + x) * 4;
            let teeth = (5..22).contains(&y)
                && if ((y - 5) / 4) % 2 == 0 {
                    (12..16).contains(&x)
                } else {
                    (16..20).contains(&x)
                };
            let pull = (23..28).contains(&y) && (12..20).contains(&x);
            rgba[offset..offset + 4].copy_from_slice(&if teeth || pull {
                [255; 4]
            } else {
                ACCENT.to_array()
            });
        }
    }
    Arc::new(egui::IconData {
        rgba,
        width: 32,
        height: 32,
    })
}
