//! "Discipline Over Time" chart.

use chrono::NaiveDate;
use eframe::egui::{self, Align, Layout, Ui, emath::easing};
use egui_plot::{GridInput, GridMark, HoverPosition, Line, Plot, PlotPoints, Points, VLine};

use super::{theme, widgets};
use crate::core::{Granularity, ScorePoint};

/// Duration of the transition between two versions of the series.
const TRANSITION_SECONDS: f64 = 0.7;

/// Animates the plotted values so the line morphs whenever the data changes.
#[derive(Default)]
pub struct ScoreChart {
    from: Vec<f64>,
    to: Vec<f64>,
    start: f64,
}

impl ScoreChart {
    /// Draws the header with the Daily/Weekly switch, then the chart.
    /// Returns the granularity the user picked, if they changed it.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        series: &[ScorePoint],
        granularity: Granularity,
        height: f32,
    ) -> Option<Granularity> {
        let mut picked = None;
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
                        picked = Some(option);
                    }
                }
            });
        });
        ui.add_space(4.0);

        let now = ui.input(|i| i.time);
        let values = self.animated_values(series, now);
        if now - self.start < TRANSITION_SECONDS {
            ui.ctx().request_repaint();
        }
        plot(ui, series, &values, granularity, height);
        picked
    }

    fn animated_values(&mut self, series: &[ScorePoint], now: f64) -> Vec<f64> {
        let target: Vec<f64> = series.iter().map(|p| p.score as f64).collect();
        if target != self.to {
            let current = self.values_at(now);
            self.from = if current.is_empty() {
                // First appearance: rise from the lowest value.
                let floor = target.iter().copied().fold(f64::INFINITY, f64::min);
                vec![floor; target.len()]
            } else {
                resample(&current, target.len())
            };
            self.to = target;
            self.start = now;
        }
        self.values_at(now)
    }

    fn values_at(&self, now: f64) -> Vec<f64> {
        let t = ((now - self.start) / TRANSITION_SECONDS).clamp(0.0, 1.0) as f32;
        let t = f64::from(easing::cubic_out(t));
        self.to
            .iter()
            .zip(&self.from)
            .map(|(to, from)| from + (to - from) * t)
            .collect()
    }
}

fn plot(ui: &mut Ui, series: &[ScorePoint], values: &[f64], granularity: Granularity, height: f32) {
    let n = series.len();
    let (y_min, y_max) = y_range(series);
    let dates: Vec<NaiveDate> = series.iter().map(|p| p.period_start).collect();
    let scores: Vec<i64> = series.iter().map(|p| p.score).collect();

    let points: Vec<[f64; 2]> = values
        .iter()
        .enumerate()
        .map(|(i, v)| [i as f64, *v])
        .collect();
    let last = points.last().copied();

    let label_dates = dates.clone();
    let response = Plot::new("discipline_over_time")
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
                .map(|i| date_label(dates[i], granularity))
                .unwrap_or_default()
        })
        .y_axis_formatter(|mark, _| format!("{:.0}", mark.value))
        .label_formatter(move |position| {
            let x = match position {
                HoverPosition::NearDataPoint { position, .. } => position.x,
                HoverPosition::Elsewhere { position } => position.x,
            };
            let i = x.round().clamp(0.0, n.saturating_sub(1) as f64) as usize;
            let delta = if i > 0 { scores[i] - scores[i - 1] } else { 0 };
            Some(format!(
                "{}\n{} pts   {}",
                tooltip_date(label_dates[i], granularity),
                scores[i],
                theme::format_points(delta)
            ))
        })
        .show(ui, |plot_ui| {
            plot_ui.set_plot_bounds_x(-0.6..=n as f64 - 0.4);
            plot_ui.set_plot_bounds_y(y_min..=y_max);

            let hovered = plot_ui.response().hovered();
            if let Some(pointer) = plot_ui.pointer_coordinate().filter(|_| hovered) {
                let i = pointer.x.round().clamp(0.0, n.saturating_sub(1) as f64);
                plot_ui.add(
                    VLine::new("", i)
                        .color(theme::BORDER_STRONG)
                        .width(1.0)
                        .allow_hover(false),
                );
                if let Some(value) = values.get(i as usize) {
                    plot_ui.add(
                        Points::new("", vec![[i, *value]])
                            .radius(5.5)
                            .color(theme::ACCENT)
                            .allow_hover(false),
                    );
                }
            }

            plot_ui.add(
                Line::new("score", PlotPoints::from(points.clone()))
                    .color(theme::ACCENT)
                    .width(2.0)
                    .fill(y_min as f32)
                    .fill_alpha(0.06)
                    .allow_hover(false),
            );
            plot_ui.add(
                Points::new("score", points)
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
        });
    response
        .response
        .on_hover_cursor(egui::CursorIcon::Crosshair);
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

    #[test]
    fn resample_keeps_endpoints() {
        let values = [0.0, 10.0, 20.0];
        assert_eq!(resample(&values, 5), vec![0.0, 5.0, 10.0, 15.0, 20.0]);
        assert_eq!(resample(&values, 2), vec![0.0, 20.0]);
        assert_eq!(resample(&[], 2), vec![0.0, 0.0]);
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
