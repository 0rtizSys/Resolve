//! Colors, typography and spacing. Everything visual that is shared lives here.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals,
    text::{LayoutJob, TextFormat},
};

pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0C, 0x0F);
pub const SURFACE: Color32 = Color32::from_rgb(0x10, 0x13, 0x18);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0x15, 0x19, 0x20);
pub const BORDER: Color32 = Color32::from_rgb(0x1E, 0x24, 0x2C);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x2C, 0x34, 0x3F);
pub const TEXT: Color32 = Color32::from_rgb(0xE4, 0xE7, 0xEB);
pub const MUTED: Color32 = Color32::from_rgb(0x7B, 0x84, 0x90);
pub const FAINT: Color32 = Color32::from_rgb(0x48, 0x50, 0x5A);
pub const ACCENT: Color32 = Color32::from_rgb(0x5E, 0xE6, 0xC9);
pub const NEGATIVE: Color32 = Color32::from_rgb(0xF2, 0x7A, 0x7D);

pub const RADIUS: u8 = 8;
/// Horizontal space between the two main columns.
pub const COLUMN_GAP: f32 = 36.0;

/// Color used to display a signed amount of points.
pub fn points_color(points: i64) -> Color32 {
    match points.signum() {
        1 => ACCENT,
        -1 => NEGATIVE,
        _ => MUTED,
    }
}

/// `+5`, `-10`, `±0`.
pub fn format_points(points: i64) -> String {
    if points == 0 {
        "±0".to_owned()
    } else {
        format!("{points:+}")
    }
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Small, letter-spaced uppercase label used for section titles and captions.
pub fn caption(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(caption_job(text, MUTED))
}

pub fn caption_job(text: &str, color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        TextFormat {
            font_id: mono(11.0),
            color,
            extra_letter_spacing: 1.6,
            ..Default::default()
        },
    );
    job
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Monospace, mono(13.0)),
            (TextStyle::Heading, FontId::proportional(20.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.animation_time = 0.15;
        style.visuals = visuals();
    });
}

fn visuals() -> Visuals {
    let mut v = Visuals::dark();
    let radius = CornerRadius::same(RADIUS);

    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.window_corner_radius = radius;
    v.menu_corner_radius = radius;
    v.extreme_bg_color = SURFACE;
    v.faint_bg_color = SURFACE;
    v.code_bg_color = SURFACE;
    v.override_text_color = None;
    v.hyperlink_color = ACCENT;
    v.error_fg_color = NEGATIVE;
    v.warn_fg_color = NEGATIVE;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.25);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.text_cursor.stroke = Stroke::new(2.0, ACCENT);
    v.popup_shadow = egui::Shadow::NONE;
    v.window_shadow = egui::Shadow::NONE;

    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = radius;

    for (state, fill, stroke) in [
        (&mut w.inactive, SURFACE, BORDER),
        (&mut w.hovered, SURFACE_HOVER, BORDER_STRONG),
        (&mut w.active, SURFACE_HOVER, ACCENT),
        (&mut w.open, SURFACE_HOVER, BORDER_STRONG),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::new(1.0, stroke);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    w.active.fg_stroke = Stroke::new(1.0, TEXT);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_are_formatted_with_sign() {
        assert_eq!(format_points(5), "+5");
        assert_eq!(format_points(-10), "-10");
        assert_eq!(format_points(0), "±0");
    }
}
