//! Chronological log of every event that changed the score.

use chrono::{Local, NaiveDate};
use eframe::egui::{
    Align, CornerRadius, Layout, RichText, ScrollArea, Sense, Ui, UiBuilder, emath::easing, vec2,
};

use super::{theme, widgets};
use crate::core::{DisciplineEvent, EventId, EventKind};

const ROW_HEIGHT: f32 = 28.0;
/// How long a freshly added entry stays highlighted, in seconds.
const HIGHLIGHT_SECONDS: f64 = 1.6;

enum Row<'a> {
    Day(NaiveDate),
    Event(&'a DisciplineEvent),
}

/// `highlight` marks the newest entry with the time it was created, so it can glow briefly.
pub fn show(
    ui: &mut Ui,
    events: &[DisciplineEvent],
    highlight: Option<(EventId, f64)>,
    today: NaiveDate,
    max_height: f32,
) {
    ui.horizontal(|ui| {
        theme::caption(ui, "History");
        if !events.is_empty() {
            ui.label(
                RichText::new(format!("{} events", events.len()))
                    .font(theme::mono(11.0))
                    .color(theme::FAINT),
            );
        }
    });
    ui.add_space(4.0);

    if events.is_empty() {
        ui.add_space(8.0);
        ui.label(RichText::new("Completed tasks will show up here.").color(theme::FAINT));
        return;
    }

    let rows = rows(events);
    let now = ui.input(|i| i.time);
    ScrollArea::vertical()
        .id_salt("history")
        .auto_shrink([false, false])
        .max_height(max_height)
        .stick_to_bottom(true)
        .show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
            for row in &rows[range] {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
                let mut row_ui = ui.new_child(
                    UiBuilder::new()
                        .max_rect(rect.shrink2(vec2(8.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                match row {
                    Row::Day(date) => {
                        row_ui.add_space(-8.0);
                        theme::caption(&mut row_ui, &day_label(*date, today));
                    }
                    Row::Event(event) => {
                        if let Some((id, since)) = highlight
                            && id == event.id
                            && now - since < HIGHLIGHT_SECONDS
                        {
                            let t = ((now - since) / HIGHLIGHT_SECONDS) as f32;
                            let alpha = 0.12 * (1.0 - easing::quadratic_in(t));
                            ui.painter().rect_filled(
                                rect,
                                CornerRadius::same(5),
                                theme::points_color(event.points).gamma_multiply(alpha),
                            );
                            ui.ctx().request_repaint();
                        }
                        event_row(&mut row_ui, event);
                    }
                }
            }
        });
}

fn event_row(ui: &mut Ui, event: &DisciplineEvent) {
    let local = event.occurred_at.with_timezone(&Local);
    ui.label(
        RichText::new(local.format("%H:%M").to_string())
            .font(theme::mono(12.0))
            .color(theme::FAINT),
    );
    ui.add_space(6.0);

    // Fixed-width, right-aligned points column.
    ui.allocate_ui_with_layout(
        vec2(46.0, ROW_HEIGHT),
        Layout::right_to_left(Align::Center),
        |ui| {
            let color = match event.kind {
                EventKind::TaskReverted => theme::MUTED,
                EventKind::TaskCompleted => theme::points_color(event.points),
            };
            ui.label(
                RichText::new(theme::format_points(event.points))
                    .font(theme::mono(13.0))
                    .color(color),
            );
        },
    );
    ui.add_space(4.0);

    let (glyph, glyph_color, text, text_color) = match event.kind {
        EventKind::TaskCompleted if event.points >= 0 => (
            widgets::Glyph::Check,
            theme::ACCENT,
            event.action.clone(),
            theme::TEXT,
        ),
        EventKind::TaskCompleted => (
            widgets::Glyph::Cross,
            theme::NEGATIVE,
            event.action.clone(),
            theme::TEXT,
        ),
        EventKind::TaskReverted => (
            widgets::Glyph::Undo,
            theme::MUTED,
            format!("Undid “{}”", event.action),
            theme::MUTED,
        ),
    };
    widgets::glyph(ui, glyph, glyph_color);
    ui.add_space(2.0);
    ui.add(
        eframe::egui::Label::new(RichText::new(text).color(text_color))
            .truncate()
            .selectable(false),
    );
}

fn rows(events: &[DisciplineEvent]) -> Vec<Row<'_>> {
    let mut rows = Vec::with_capacity(events.len() + 8);
    let mut current_day = None;
    for event in events {
        let day = event.occurred_at.with_timezone(&Local).date_naive();
        if current_day != Some(day) {
            rows.push(Row::Day(day));
            current_day = Some(day);
        }
        rows.push(Row::Event(event));
    }
    rows
}

fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (today - date).num_days() {
        0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        _ => date.format("%a %d %b %Y").to_string(),
    }
}
