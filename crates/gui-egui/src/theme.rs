use eframe::egui::{self, Color32, FontId, RichText, Stroke, TextStyle};
use std::sync::Arc;

pub const ACCENT: Color32 = Color32::from_rgb(82, 99, 224);
pub const CONTROL_HEIGHT: f32 = 28.0;

/// Semantic colors shared by standard inputs and our painted components.
pub struct Palette {
    pub canvas: Color32,
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

pub fn fonts(context: &egui::Context, selection: &crate::fonts::Selection) -> bool {
    crate::fonts::install(context, selection)
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
    style.visuals.window_corner_radius = 8.into();
    style.visuals.menu_corner_radius = 8.into();
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 5],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 64 } else { 24 }),
    };
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.window_margin = egui::Margin::same(16);
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
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = CONTROL_HEIGHT;
    style.spacing.text_edit_width = 220.0;
    style.spacing.combo_height = 280.0;
    style.visuals.text_edit_bg_color = Some(p.surface);
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

pub fn modal(context: &egui::Context, id: &str, corner_radius: u8) -> egui::Modal {
    let modal = egui::Modal::new(egui::Id::new(id))
        .frame(egui::Frame::popup(&context.style()).inner_margin(16))
        .backdrop_color(Color32::TRANSPARENT);
    // egui's default rectangular backdrop would fill the transparent window corners.
    context.layer_painter(modal.area.layer()).rect_filled(
        context.content_rect(),
        corner_radius,
        Color32::from_black_alpha(100),
    );
    modal
}

pub fn primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    crate::widgets::button(ui, text, crate::widgets::Icon::ArrowRight, true, 0.0)
}

pub fn card(ui: &egui::Ui) -> egui::Frame {
    let p = palette(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(8)
        .inner_margin(12)
}

pub fn caption(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).size(12.0).color(palette(ui).muted));
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

pub(crate) fn paint_logo(painter: &egui::Painter, rect: egui::Rect) {
    let context = painter.ctx();
    let id = egui::Id::new("geezipx-logo");
    let texture = context.data(|data| data.get_temp::<egui::TextureHandle>(id));
    let texture = texture.unwrap_or_else(|| {
        let icon = icon();
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        );
        let texture = context.load_texture("geezipx-logo", image, egui::TextureOptions::LINEAR);
        context.data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    painter.image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
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
    Arc::new(
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
            .expect("Bundled GeeZipX icon must be a valid PNG"),
    )
}
