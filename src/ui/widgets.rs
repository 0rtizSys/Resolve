//! Small reusable widgets.

use std::f32::consts::PI;

use eframe::egui::{
    self, Color32, CornerRadius, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind,
    Ui, Vec2, emath::easing, pos2, vec2,
};

use super::theme;

/// An animated checkbox: the box fills, the check mark draws itself and a soft ring
/// expands outwards when it gets checked.
pub fn check_box(ui: &mut Ui, checked: bool, color: Color32) -> Response {
    let size = Vec2::splat(20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let progress = ui.ctx().animate_bool_with_time(response.id, checked, 0.22);
    let eased = easing::cubic_out(progress);
    let painter = ui.painter();

    // Ring that expands and fades out right after checking.
    if checked && progress < 1.0 {
        let radius = 10.0 + 9.0 * eased;
        let alpha = (1.0 - progress) * 0.45;
        painter.circle_stroke(
            rect.center(),
            radius,
            Stroke::new(1.5, color.gamma_multiply(alpha)),
        );
    }

    // A tiny "pop" while transitioning.
    let pop = (progress * PI).sin() * 1.5;
    let box_rect = rect.expand(pop);
    let border = if response.hovered() {
        lerp_color(theme::MUTED, color, eased)
    } else {
        lerp_color(theme::FAINT, color, eased)
    };
    painter.rect(
        box_rect,
        CornerRadius::same(6),
        color.gamma_multiply(eased),
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );

    if progress > 0.0 {
        let points = [
            box_rect.lerp_inside(vec2(0.26, 0.52)),
            box_rect.lerp_inside(vec2(0.43, 0.69)),
            box_rect.lerp_inside(vec2(0.75, 0.33)),
        ];
        draw_partial_path(painter, &points, eased, Stroke::new(2.2, theme::BG));
    }

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Signed points rendered as a compact monospace pill.
pub fn points_badge(ui: &mut Ui, points: i64, dimmed: f32) -> Response {
    let color = theme::points_color(points).gamma_multiply(1.0 - 0.55 * dimmed);
    let galley =
        ui.painter()
            .layout_no_wrap(theme::format_points(points), theme::mono(13.0), color);
    let padding = vec2(8.0, 3.0);
    let (rect, response) = ui.allocate_exact_size(galley.size() + padding * 2.0, Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(5), color.gamma_multiply(0.09));
    ui.painter().galley(rect.min + padding, galley, color);
    response
}

/// Glyphs used by the history. Drawn as vectors so they look identical everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Check,
    Cross,
    Undo,
}

pub fn glyph(ui: &mut Ui, glyph: Glyph, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    paint_glyph(ui.painter(), rect, glyph, color);
    response
}

/// A borderless icon button. Invisible (but still laid out) when `visible` is false.
pub fn glyph_button(ui: &mut Ui, glyph: Glyph, hover_color: Color32, visible: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
    if visible {
        let color = if response.hovered() {
            hover_color
        } else {
            theme::FAINT
        };
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(5), hover_color.gamma_multiply(0.1));
        }
        paint_glyph(ui.painter(), rect.shrink(4.0), glyph, color);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn paint_glyph(painter: &Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let r = Rect::from_center_size(rect.center(), Vec2::splat(10.0));
    let stroke = Stroke::new(1.6, color);
    match glyph {
        Glyph::Check => {
            let points = [
                r.lerp_inside(vec2(0.05, 0.55)),
                r.lerp_inside(vec2(0.38, 0.88)),
                r.lerp_inside(vec2(0.95, 0.15)),
            ];
            painter.add(Shape::line(points.to_vec(), stroke));
        }
        Glyph::Cross => {
            painter.line_segment([r.left_top(), r.right_bottom()], stroke);
            painter.line_segment([r.right_top(), r.left_bottom()], stroke);
        }
        Glyph::Undo => {
            let center = r.center();
            let radius = r.width() * 0.5;
            let arc: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let angle = -PI * 0.9 + (i as f32 / 16.0) * PI * 1.5;
                    center + radius * Vec2::angled(angle)
                })
                .collect();
            let tip = arc[0];
            painter.add(Shape::line(arc, stroke));
            painter.line_segment([tip, tip + vec2(0.0, -3.5)], stroke);
            painter.line_segment([tip, tip + vec2(3.5, 0.5)], stroke);
        }
    }
}

/// Draws the first `t` (0..=1) of a polyline, measured by length.
fn draw_partial_path(painter: &Painter, points: &[Pos2], t: f32, stroke: Stroke) {
    let lengths: Vec<f32> = points.windows(2).map(|w| w[0].distance(w[1])).collect();
    let mut remaining = lengths.iter().sum::<f32>() * t.clamp(0.0, 1.0);
    let mut path = vec![points[0]];
    for (segment, length) in points.windows(2).zip(lengths) {
        if remaining <= 0.0 {
            break;
        }
        let fraction = (remaining / length).min(1.0);
        path.push(segment[0] + (segment[1] - segment[0]) * fraction);
        remaining -= length;
    }
    if path.len() > 1 {
        painter.add(Shape::line(path, stroke));
    }
}

pub fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
        channel(from.a(), to.a()),
    )
}

/// A thin horizontal rule.
pub fn separator(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        Stroke::new(1.0, theme::BORDER),
    );
}

/// Text-only toggle used for small segmented choices (e.g. Daily / Weekly).
pub fn toggle_label(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let job = theme::caption_job(text, Color32::PLACEHOLDER);
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let padding = vec2(8.0, 4.0);
    let (rect, response) = ui.allocate_exact_size(galley.size() + padding * 2.0, Sense::click());
    let hovered = response.hovered() && !selected;
    if selected || hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(5),
            if selected {
                theme::SURFACE_HOVER
            } else {
                theme::SURFACE
            },
        );
    }
    let text_color = match (selected, hovered) {
        (true, _) => theme::TEXT,
        (false, true) => theme::MUTED,
        (false, false) => theme::FAINT,
    };
    ui.painter().galley(
        pos2(rect.min.x + padding.x, rect.min.y + padding.y),
        galley,
        text_color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
