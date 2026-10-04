//! "Discipline Over Time" chart.
//!
//! Points are keyed by their period, so when the data changes each point glides to its new
//! place: a completed task lifts today's point, and a brand new period grows out of the
//! previous one instead of the chart being redrawn from scratch. Hovering a point lists the
//! events behind it; clicking pins that breakdown and highlights the events in the history.

use chrono::{Local, NaiveDate};
use eframe::egui::{self, Align, Layout, RichText, Ui, emath::easing};
use egui_plot::{GridInput, GridMark, HoverPosition, Line, Plot, PlotPoints, Points, VLine};

use super::{motion, theme, widgets};
use crate::core::statistics::events_in_period;
use crate::core::{DisciplineEvent, EventKind, Granularity, ScorePoint};

/// Duration of the transition between two versions of the series.
const TRANSITION_SECONDS: f32 = 0.6;
/// Events listed in a tooltip before summarizing the rest.
const TOOLTIP_EVENTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    period: NaiveDate,
    x: f64,
    y: f64,
}

#[derive(Default)]
pub struct ScoreChart {
    from: Vec<Point>,
    to: Vec<Point>,
    from_bounds: (f64, f64),
    to_bounds: (f64, f64),
    start: f64,
}

/// What the user did with the chart this frame.
#[derive(Default)]
pub struct ChartResponse {
    pub granularity: Option<Granularity>,
    /// `Some(Some(period))` selects a period, `Some(None)` clears the selection.
    pub selected: Option<Option<NaiveDate>>,
}

impl ScoreChart {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        series: &[ScorePoint],
        events: &[DisciplineEvent],
        granularity: Granularity,
        selected: Option<NaiveDate>,
        height: f32,
    ) -> ChartResponse {
        let mut response = ChartResponse::default();
        ui.horizontal(|ui| {
            theme::caption(ui, "Discipline over time");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (option, label) in [
                    (Granularity::Weekly, "Weekly"),
                    (Granularity::Daily, "Daily"),
                ] {
                    if widgets::toggle_label(ui, label, granularity == option).clicked()
                        && granularity != option
                    {
                        response.granularity = Some(option);
                    }
                }
            });
        });
        ui.add_space(4.0);

        let now = ui.input(|i| i.time);
        let duration = f64::from(motion::duration(ui.ctx(), TRANSITION_SECONDS));
        self.retarget(series, now, duration);
        let eased = self.eased(now, duration);
        if eased < 1.0 {
            ui.ctx().request_repaint();
        }
        let points = self.points_at(eased);
        let bounds = lerp_pair(self.from_bounds, self.to_bounds, eased);

        let clicked = plot(
            ui,
            series,
            &points,
            bounds,
            events,
            granularity,
            selected,
            height,
        );
        if let Some(index) = clicked {
            let period = series[index].period_start;
            response.selected = Some((selected != Some(period)).then_some(period));
        }

        if let Some(period) = selected
            && let Some(point) = series.iter().find(|p| p.period_start == period)
            && breakdown_panel(ui, point, events, granularity)
        {
            response.selected = Some(None);
        }
        response
    }

    fn retarget(&mut self, series: &[ScorePoint], now: f64, duration: f64) {
        let target: Vec<Point> = series
            .iter()
            .enumerate()
            .map(|(i, p)| Point {
                period: p.period_start,
                x: i as f64,
                y: p.score as f64,
            })
            .collect();
        if target == self.to {
            return;
        }
        let target_bounds = y_range(series);
        if self.to.is_empty() {
            // First appearance: the line rises from the bottom of the chart.
            self.from = target
                .iter()
                .map(|p| Point {
                    y: target_bounds.0,
                    ..*p
                })
                .collect();
            self.from_bounds = target_bounds;
        } else {
            // Start from wherever everything is drawn right now, even mid-transition.
            let eased = self.eased(now, duration);
            self.from = entering_points(&self.points_at(eased), &target);
            self.from_bounds = lerp_pair(self.from_bounds, self.to_bounds, eased);
        }
        self.to = target;
        self.to_bounds = target_bounds;
        self.start = now;
    }

    /// Eased progress of the current transition, 0 to 1.
    fn eased(&self, now: f64, duration: f64) -> f64 {
        if duration <= 0.0 {
            return 1.0;
        }
        let t = ((now - self.start) / duration).clamp(0.0, 1.0);
        f64::from(easing::cubic_out(t as f32))
    }

    fn points_at(&self, t: f64) -> Vec<Point> {
        self.to
            .iter()
            .zip(&self.from)
            .map(|(to, from)| Point {
                period: to.period,
                x: from.x + (to.x - from.x) * t,
                y: from.y + (to.y - from.y) * t,
            })
            .collect()
    }
}

/// Starting position for each target point: where the same period currently is, or, for a
/// period that wasn't shown before, the closest earlier point (so new points grow out of
/// the line). With no shared periods at all (switching daily ↔ weekly) the old line is
/// stretched over the new one.
fn entering_points(current: &[Point], target: &[Point]) -> Vec<Point> {
    let shared = target
        .iter()
        .any(|t| current.iter().any(|c| c.period == t.period));
    if !shared {
        let values: Vec<f64> = current.iter().map(|p| p.y).collect();
        return target
            .iter()
            .zip(resample(&values, target.len()))
            .map(|(p, y)| Point { y, ..*p })
            .collect();
    }
    target
        .iter()
        .map(|t| {
            if let Some(c) = current.iter().find(|c| c.period == t.period) {
                return *c;
            }
            let previous = current
                .iter()
                .filter(|c| c.period < t.period)
                .max_by_key(|c| c.period)
                .or_else(|| current.first());
            match previous {
                Some(p) => Point {
                    period: t.period,
                    x: p.x,
                    y: p.y,
                },
                None => *t,
            }
        })
        .collect()
}

/// Draws the plot and returns the index of the period the user clicked, if any.
#[allow(clippy::too_many_arguments)]
fn plot(
    ui: &mut Ui,
    series: &[ScorePoint],
    points: &[Point],
    (y_min, y_max): (f64, f64),
    events: &[DisciplineEvent],
    granularity: Granularity,
    selected: Option<NaiveDate>,
    height: f32,
) -> Option<usize> {
    let n = series.len();
    let dates: Vec<NaiveDate> = series.iter().map(|p| p.period_start).collect();
    let coords: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
    let last = coords.last().copied();
    let selected_index = selected.and_then(|s| dates.iter().position(|d| *d == s));

    let axis_dates = dates.clone();
    let result = Plot::new("discipline_over_time")
        .height(height)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .show_background(false)
        .show_grid([false, true])
        .grid_color(theme::BORDER)
        // `show_x` enables hover tooltips; the snapped guide line is drawn below instead.
        .show_x(true)
        .show_y(false)
        .show_crosshair(false)
        .y_axis_min_width(36.0)
        .x_grid_spacer(index_spacer)
        .x_axis_formatter(move |mark, _| {
            index_of(mark.value, n)
                .map(|i| date_label(axis_dates[i], granularity))
                .unwrap_or_default()
        })
        .y_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .label_formatter(|position| {
            let x = match position {
                HoverPosition::NearDataPoint { position, .. } => position.x,
                HoverPosition::Elsewhere { position } => position.x,
            };
            let i = nearest(x, n)?;
            Some(tooltip(&series[i], events, granularity))
        })
        .show(ui, |plot_ui| {
            plot_ui.set_plot_bounds_x(-0.6..=n as f64 - 0.4);
            plot_ui.set_plot_bounds_y(y_min..=y_max);

            if let Some(i) = selected_index
                && let Some(point) = coords.get(i)
            {
                plot_ui.add(
                    VLine::new("", i as f64)
                        .color(theme::ACCENT.gamma_multiply(0.35))
                        .width(1.0)
                        .allow_hover(false),
                );
                plot_ui.add(
                    Points::new("", vec![*point])
                        .radius(6.5)
                        .color(theme::ACCENT.gamma_multiply(0.35))
                        .allow_hover(false),
                );
            }

            let hovered = plot_ui.response().hovered();
            let hovered_index = plot_ui
                .pointer_coordinate()
                .filter(|_| hovered)
                .and_then(|p| nearest(p.x, n));
            if let Some(i) = hovered_index {
                plot_ui.add(
                    VLine::new("", i as f64)
                        .color(theme::BORDER_STRONG)
                        .width(1.0)
                        .allow_hover(false),
                );
                if let Some(point) = coords.get(i) {
                    plot_ui.add(
                        Points::new("", vec![*point])
                            .radius(5.5)
                            .color(theme::ACCENT)
                            .allow_hover(false),
                    );
                }
            }

            plot_ui.add(
                Line::new("score", PlotPoints::from(coords.clone()))
                    .color(theme::ACCENT)
                    .width(2.0)
                    .fill(y_min as f32)
                    .fill_alpha(0.06)
                    .allow_hover(false),
            );
            plot_ui.add(
                Points::new("score", coords.clone())
                    .radius(2.5)
                    .color(theme::ACCENT.gamma_multiply(0.7))
                    .allow_hover(false),
            );
            if let Some(last) = last {
                plot_ui.add(
                    Points::new("", vec![last])
                        .radius(4.0)
                        .color(theme::ACCENT)
                        .allow_hover(false),
                );
            }
            hovered_index
        });
    let response = result
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        result.inner
    } else {
        None
    }
}

/// Hover text: the period, its score and every event that moved it.
fn tooltip(point: &ScorePoint, events: &[DisciplineEvent], granularity: Granularity) -> String {
    let period_events = events_in_period(events, point.period_start, granularity, &Local);
    let net: i64 = period_events.iter().map(|e| e.points).sum();
    let mut text = format!(
        "{}\nDiscipline Score: {}\n",
        tooltip_date(point.period_start, granularity),
        point.score
    );
    if period_events.is_empty() {
        text.push_str("\nNo events");
        return text;
    }
    text.push('\n');
    for event in period_events.iter().rev().take(TOOLTIP_EVENTS) {
        text.push_str(&format!(
            "{:>5}  {}\n",
            theme::format_points(event.points),
            event_text(event)
        ));
    }
    if period_events.len() > TOOLTIP_EVENTS {
        text.push_str(&format!(
            "       … {} more (click to see all)\n",
            period_events.len() - TOOLTIP_EVENTS
        ));
    }
    text.push_str(&format!("\nNet: {}", theme::format_points(net)));
    text
}

/// The pinned breakdown under the chart. Returns `true` when the user closes it.
fn breakdown_panel(
    ui: &mut Ui,
    point: &ScorePoint,
    events: &[DisciplineEvent],
    granularity: Granularity,
) -> bool {
    let period_events = events_in_period(events, point.period_start, granularity, &Local);
    let net: i64 = period_events.iter().map(|e| e.points).sum();
    let mut closed = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tooltip_date(point.period_start, granularity))
                .font(theme::mono(12.0))
                .color(theme::TEXT),
        );
        ui.label(
            RichText::new(format!(
                "score {} · net {}",
                point.score,
                theme::format_points(net)
            ))
            .font(theme::mono(12.0))
            .color(theme::points_color(net)),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            closed = widgets::glyph_button(ui, widgets::Glyph::Cross, theme::TEXT, true)
                .on_hover_text("Close")
                .clicked();
            ui.label(
                RichText::new(format!(
                    "{} events, highlighted in history",
                    period_events.len()
                ))
                .font(theme::mono(11.0))
                .color(theme::FAINT),
            );
        });
    });
    closed
}

pub fn event_text(event: &DisciplineEvent) -> String {
    match event.kind {
        EventKind::TaskCompleted => event.action.clone(),
        EventKind::TaskReverted => format!("Undid “{}”", event.action),
    }
}

fn nearest(x: f64, len: usize) -> Option<usize> {
    (len > 0).then(|| x.round().clamp(0.0, (len - 1) as f64) as usize)
}

fn lerp_pair(from: (f64, f64), to: (f64, f64), t: f64) -> (f64, f64) {
    (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
}

/// Vertical range with some breathing room, never collapsing to a flat line.
fn y_range(series: &[ScorePoint]) -> (f64, f64) {
    let (min, max) = series.iter().fold((i64::MAX, i64::MIN), |(lo, hi), p| {
        (lo.min(p.score), hi.max(p.score))
    });
    if min > max {
        return (0.0, 10.0);
    }
    let (min, max) = (min as f64, max as f64);
    let span = (max - min).max(10.0);
    let lower = min - span * 0.12;
    // Don't invent negative territory for a score that never went below zero.
    let lower = if min >= 0.0 { lower.max(0.0) } else { lower };
    (lower, max + span * 0.18)
}

/// One grid mark per period, thinned out when the plot is too narrow for every label.
fn index_spacer(input: GridInput) -> Vec<GridMark> {
    // `base_step_size` is the smallest readable step; labels need a bit more room.
    let step = (input.base_step_size * 12.0).ceil().max(1.0);
    let (lo, hi) = input.bounds;
    let mut marks = Vec::new();
    // Anchor on the latest period so the current one always has a label.
    let mut x = hi.floor();
    while x >= lo.ceil() {
        marks.push(GridMark {
            value: x,
            step_size: step,
        });
        x -= step;
    }
    marks
}

fn index_of(x: f64, len: usize) -> Option<usize> {
    let i = x.round();
    ((x - i).abs() < 1e-6 && i >= 0.0 && (i as usize) < len).then_some(i as usize)
}

fn date_label(date: NaiveDate, granularity: Granularity) -> String {
    match granularity {
        Granularity::Daily => date.format("%a %d").to_string(),
        Granularity::Weekly => date.format("%d %b").to_string(),
    }
}

fn tooltip_date(date: NaiveDate, granularity: Granularity) -> String {
    match granularity {
        Granularity::Daily => date.format("%A, %-d %B").to_string(),
        Granularity::Weekly => format!("Week of {}", date.format("%-d %B")),
    }
}

/// Linearly resamples `values` to `len` points so series of different lengths can morph.
fn resample(values: &[f64], len: usize) -> Vec<f64> {
    match (values.len(), len) {
        (_, 0) => Vec::new(),
        (0, _) => vec![0.0; len],
        (1, _) => vec![values[0]; len],
        (_, 1) => vec![*values.last().unwrap_or(&0.0)],
        (from_len, _) => (0..len)
            .map(|i| {
                let x = i as f64 * (from_len - 1) as f64 / (len - 1) as f64;
                let lo = x.floor() as usize;
                let hi = (lo + 1).min(from_len - 1);
                values[lo] + (values[hi] - values[lo]) * (x - lo as f64)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    fn point(d: u32, x: f64, y: f64) -> Point {
        Point {
            period: day(d),
            x,
            y,
        }
    }

    #[test]
    fn resample_keeps_endpoints() {
        let values = [0.0, 10.0, 20.0];
        assert_eq!(resample(&values, 5), vec![0.0, 5.0, 10.0, 15.0, 20.0]);
        assert_eq!(resample(&values, 2), vec![0.0, 20.0]);
        assert_eq!(resample(&[], 2), vec![0.0, 0.0]);
    }

    #[test]
    fn existing_points_move_and_new_points_grow_from_the_previous_one() {
        // 100 → 105 → 110, then a new day arrives with 120 and the window slides by one.
        let current = [
            point(1, 0.0, 100.0),
            point(2, 1.0, 105.0),
            point(3, 2.0, 110.0),
        ];
        let target = [
            point(2, 0.0, 105.0),
            point(3, 1.0, 110.0),
            point(4, 2.0, 120.0),
        ];
        let from = entering_points(&current, &target);
        assert_eq!(from[0], point(2, 1.0, 105.0));
        assert_eq!(from[1], point(3, 2.0, 110.0));
        // The new point starts on top of the previous last point.
        assert_eq!(from[2], point(4, 2.0, 110.0));
    }

    #[test]
    fn y_range_never_collapses() {
        let flat = [ScorePoint {
            period_start: NaiveDate::MIN,
            score: 50,
        }];
        let (lo, hi) = y_range(&flat);
        assert!(lo < 50.0 && hi > 50.0);

        let zero = [ScorePoint {
            period_start: NaiveDate::MIN,
            score: 0,
        }];
        assert_eq!(y_range(&zero).0, 0.0);
    }
}
