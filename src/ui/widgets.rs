//! Small reusable widgets.

use std::f32::consts::PI;

use eframe::egui::{
    self, Color32, CornerRadius, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind,
    Ui, Vec2, emath::easing, pos2, vec2,
};

use super::{motion, theme};

/// An animated checkbox: the box fills, the check mark draws itself, the box springs a
/// little and a soft ring expands outwards when it gets checked.
///
/// `id` must be stable for the thing being checked (not its position), so the animation
/// state follows it when rows move.
pub fn check_box(ui: &mut Ui, id: egui::Id, checked: bool, color: Color32) -> Response {
    let size = Vec2::splat(20.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let ctx = ui.ctx();
    let progress = motion::animate_bool(ctx, response.id, checked, 0.22);
    let eased = easing::cubic_out(progress);

    // Kick a spring whenever the state flips, so the box "pops" and settles.
    let state_id = response.id.with("state");
    let previous = ctx.data_mut(|d| {
        let previous = d.get_temp::<bool>(state_id);
        d.insert_temp(state_id, checked);
        previous
    });
    if previous.is_some_and(|was| was != checked) {
        motion::kick(
            ctx,
            response.id.with("pop"),
            if checked { 60.0 } else { -25.0 },
        );
    }
    let pop = motion::spring(ctx, response.id.with("pop"), 0.0, motion::BOUNCY);
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

    let box_rect = rect.expand(pop.clamp(-3.0, 4.0));
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
///
/// `scale` grows the pill around its center without changing the layout, for "pop" effects.
pub fn points_badge(ui: &mut Ui, points: i64, dimmed: f32, scale: f32) -> Response {
    let color = theme::points_color(points).gamma_multiply(1.0 - 0.55 * dimmed);
    let text = theme::format_points(points);
    let padding = vec2(8.0, 3.0);
    let size = ui
        .painter()
        .layout_no_wrap(text.clone(), theme::mono(13.0), color)
        .size()
        + padding * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

    let scale = scale.clamp(0.8, 1.3);
    let galley = ui
        .painter()
        .layout_no_wrap(text, theme::mono(13.0 * scale), color);
    let shown = Rect::from_center_size(rect.center(), size * scale);
    ui.painter()
        .rect_filled(shown, CornerRadius::same(5), color.gamma_multiply(0.09));
    ui.painter()
        .galley(shown.center() - galley.size() / 2.0, galley, color);
    response
}

/// Glyphs used by the history. Drawn as vectors so they look identical everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Check,
    Cross,
    Undo,
    ChevronLeft,
    ChevronRight,
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
        Glyph::ChevronLeft | Glyph::ChevronRight => {
            let (tip, back) = if glyph == Glyph::ChevronLeft {
                (0.3, 0.65)
            } else {
                (0.7, 0.35)
            };
            let points = vec![
                r.lerp_inside(vec2(back, 0.1)),
                r.lerp_inside(vec2(tip, 0.5)),
                r.lerp_inside(vec2(back, 0.9)),
            ];
            painter.add(Shape::line(points, stroke));
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
