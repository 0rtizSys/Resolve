//! Desktop UI. Renders state from [`Tracker`] and turns user input into tracker calls;
//! no business rules live here.
//!
//! Every action follows the same path: a view emits a [`TaskAction`], [`ResolveApp::apply`]
//! hands it to the tracker (which persists it), and the next frame renders the new state.
//! Animations only ever move things towards what the model says.

mod calendar;
mod dashboard;
mod history;
mod motion;
mod statistics;
mod task_list;
mod tasks;
mod theme;
mod widgets;

use chrono::{Days, Local, NaiveDate, Utc};
use eframe::egui::{self, Align, Frame, Layout, Margin, RichText, Ui, vec2};

use crate::core::{self, EventId, Granularity, ScorePoint, Store, Tracker};
use dashboard::ScoreDisplay;
use statistics::ScoreChart;
use tasks::{TaskAction, TaskPanel};

/// Below this width the two columns stack vertically.
const WIDE_LAYOUT_MIN_WIDTH: f32 = 860.0;

pub struct ResolveApp<S> {
    tracker: Tracker<S>,
    task_panel: TaskPanel,
    score: ScoreDisplay,
    chart: ScoreChart,
    granularity: Granularity,
    series: SeriesCache,
    /// Newest event and when it happened, for the history highlight.
    highlight: Option<(EventId, f64)>,
    /// Chart period whose events are pinned and highlighted in the history.
    selected_period: Option<NaiveDate>,
    scroll_history: bool,
    error: Option<String>,
}

impl<S: Store> ResolveApp<S> {
    pub fn new(ctx: &egui::Context, tracker: Tracker<S>) -> Self {
        theme::apply(ctx);
        motion::set_reduced_motion(ctx, motion::detect_reduced_motion());
        let granularity = Granularity::recommended(tracker.events(), today(), &Local);
        Self {
            tracker,
            task_panel: TaskPanel::default(),
            score: ScoreDisplay::default(),
            chart: ScoreChart::default(),
            granularity,
            series: SeriesCache::default(),
            highlight: None,
            selected_period: None,
            scroll_history: false,
            error: None,
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: TaskAction, now: f64) {
        let result = match action {
            TaskAction::Add { name, points } => self
                .tracker
                .add_task(&name, points, Utc::now())
                .map(|_| self.task_panel.task_added()),
            TaskAction::SetCompleted { id, completed } => self
                .tracker
                .set_completed(id, completed, Utc::now())
                .map(|event| {
                    if let Some(event) = event {
                        if completed {
                            self.task_panel.celebrate(ctx, id);
                        }
                        self.score.celebrate(ctx, event.points, now);
                        self.highlight = Some((event.id, now));
                    }
                }),
            TaskAction::Delete(id) => self.delete(id, now),
            TaskAction::ClearCompleted => {
                let done: Vec<_> = self
                    .tracker
                    .tasks()
                    .iter()
                    .filter(|task| task.is_completed())
                    .map(|task| task.id)
                    .collect();
                done.into_iter().try_for_each(|id| self.delete(id, now))
            }
            TaskAction::Move { id, to } => self.tracker.move_task(id, to),
            TaskAction::Schedule { id, schedule } => self.tracker.schedule_task(id, schedule),
        };

        match result {
            Ok(()) => {}
            Err(core::Error::Validation(err)) => self.task_panel.show_error(err.to_string(), now),
            Err(err) => self.error = Some(err.to_string()),
        }
    }

    fn delete(&mut self, id: core::TaskId, now: f64) -> core::Result<()> {
        let task = self.tracker.tasks().iter().find(|t| t.id == id).cloned();
        self.tracker.delete_task(id)?;
        if let Some(task) = task {
            self.task_panel.removed(task, now);
        }
        Ok(())
    }

    fn header(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 3.5, theme::ACCENT);
            ui.label(theme::caption_job("Resolve", theme::TEXT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(Local::now().format("%A, %-d %B").to_string())
                        .font(theme::mono(12.0))
                        .color(theme::FAINT),
                );
            });
        });
    }

    fn left_column(&mut self, ui: &mut Ui, now: f64) {
        let this_week = core::statistics::points_this_week(self.tracker.events(), today(), &Local);
        self.score.show(ui, self.tracker.score().total(), this_week);
        ui.add_space(28.0);

        let actions = self.task_panel.show(ui, self.tracker.tasks());
        let ctx = ui.ctx().clone();
        for action in actions {
            self.apply(&ctx, action, now);
        }
    }

    /// `history_height` is `None` to fill the remaining space.
    fn right_column(&mut self, ui: &mut Ui, chart_height: f32, history_height: Option<f32>) {
        let today = today();
        let series = self
            .series
            .get(self.tracker.events(), self.granularity, today);
        let response = self.chart.show(
            ui,
            series,
            self.tracker.events(),
            self.granularity,
            self.selected_period,
            chart_height,
        );
        if let Some(picked) = response.granularity {
            self.granularity = picked;
            self.selected_period = None;
        }
        if let Some(selected) = response.selected {
            self.selected_period = selected;
            self.scroll_history = selected.is_some();
        }
        let selection = self.selected_period.map(|start| history::PeriodSelection {
            start,
            end: start + Days::new(self.granularity.days()),
            scroll: std::mem::take(&mut self.scroll_history),
        });
        ui.add_space(20.0);
        widgets::separator(ui);
        ui.add_space(14.0);
        let history_height = history_height.unwrap_or_else(|| ui.available_height());
        history::show(
            ui,
            self.tracker.events(),
            self.highlight,
            today,
            history_height,
            selection,
        );
    }

    fn error_banner(&mut self, ui: &mut Ui) {
        let Some(message) = &self.error else { return };
        let mut dismissed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(message).color(theme::NEGATIVE));
            if widgets::glyph_button(ui, widgets::Glyph::Cross, theme::NEGATIVE, true).clicked() {
                dismissed = true;
            }
        });
        if dismissed {
            self.error = None;
        }
    }
}

impl<S: Store> eframe::App for ResolveApp<S> {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let now = ui.input(|i| i.time);
        let frame = Frame::new()
            .fill(theme::BG)
            .inner_margin(Margin::symmetric(32, 24));

        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            self.header(ui);
            self.error_banner(ui);
            ui.add_space(18.0);

            let available = ui.available_size();
            if available.x >= WIDE_LAYOUT_MIN_WIDTH {
                let left_width = ((available.x - theme::COLUMN_GAP) * 0.42).max(340.0);
                let chart_height = (available.y * 0.42).clamp(200.0, 380.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        vec2(left_width, available.y),
                        Layout::top_down(Align::Min),
                        |ui| self.left_column(ui, now),
                    );
                    ui.add_space(theme::COLUMN_GAP);
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), available.y),
                        Layout::top_down(Align::Min),
                        |ui| self.right_column(ui, chart_height, None),
                    );
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("narrow")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.left_column(ui, now);
                        ui.add_space(28.0);
                        self.right_column(ui, 220.0, Some(360.0));
                    });
            }
        });
    }
}

/// Recomputes the chart series only when the history, view or day changes.
#[derive(Default)]
struct SeriesCache {
    key: Option<(usize, Option<EventId>, Granularity, NaiveDate)>,
    series: Vec<ScorePoint>,
}

impl SeriesCache {
    fn get(
        &mut self,
        events: &[core::DisciplineEvent],
        granularity: Granularity,
        today: NaiveDate,
    ) -> &[ScorePoint] {
        let key = Some((
            events.len(),
            events.last().map(|e| e.id),
            granularity,
            today,
        ));
        if self.key != key {
            self.series = core::statistics::score_over_time(events, granularity, today, &Local);
            self.key = key;
        }
        &self.series
    }
}

/// The window icon: an accent ring on a dark rounded square, drawn procedurally so the
/// repository needs no binary assets.
pub fn app_icon() -> egui::IconData {
    const SIZE: u32 = 64;
    let center = (SIZE as f32 - 1.0) / 2.0;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 - center, y as f32 - center);
            // Rounded square mask (superellipse) and ring distance.
            let square = (dx.abs().powi(4) + dy.abs().powi(4)).powf(0.25) <= center;
            let ring = ((dx * dx + dy * dy).sqrt() - 17.0).abs() <= 3.5;
            let color = match (square, ring) {
                (false, _) => egui::Color32::TRANSPARENT,
                (true, true) => theme::ACCENT,
                (true, false) => theme::SURFACE_HOVER,
            };
            rgba.extend_from_slice(&color.to_array());
        }
    }
    egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// Shown instead of the main window when the database cannot be opened.
pub struct StartupError(pub String);

impl eframe::App for StartupError {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        theme::apply(ui.ctx());
        let frame = Frame::new().fill(theme::BG).inner_margin(Margin::same(32));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            theme::caption(ui, "Resolve could not start");
            ui.add_space(8.0);
            ui.label(RichText::new(&self.0).color(theme::NEGATIVE));
        });
    }
}
