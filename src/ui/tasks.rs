//! Tasks section: creation form plus two views of the same tasks, a list and a day calendar.

use eframe::egui::{
    self, Align, CornerRadius, Frame, Id, Key, Layout, Margin, RichText, Stroke, TextEdit, Ui,
};

use super::calendar::Calendar;
use super::task_list::TaskList;
use super::{motion, theme, widgets};
use crate::core::{Schedule, Task, TaskId};

/// Something the user asked for in the tasks section. The app turns each one into a single
/// [`crate::core::Tracker`] call; views never change state themselves.
pub enum TaskAction {
    Add {
        name: String,
        points: i64,
    },
    SetCompleted {
        id: TaskId,
        completed: bool,
    },
    Delete(TaskId),
    ClearCompleted,
    /// Move a task to another position in the list.
    Move {
        id: TaskId,
        to: usize,
    },
    /// Place a task in the calendar, move or resize it there, or (`None`) take it out.
    Schedule {
        id: TaskId,
        schedule: Option<Schedule>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    List,
    Calendar,
}

/// Spring used for the short "pop" a task gets when it is completed or dropped.
pub fn pop_id(id: TaskId) -> Id {
    Id::new(("task_pop", id))
}

/// How long an inline error stays visible, in seconds.
const ERROR_SECONDS: f64 = 4.0;
const DEFAULT_POINTS: &str = "+5";

pub struct TaskPanel {
    name: String,
    points: String,
    error: Option<(String, f64)>,
    focus_name: bool,
    view: View,
    list: TaskList,
    calendar: Calendar,
}

impl Default for TaskPanel {
    fn default() -> Self {
        Self {
            name: String::new(),
            points: DEFAULT_POINTS.to_owned(),
            error: None,
            focus_name: true,
            view: View::List,
            list: TaskList::default(),
            calendar: Calendar::default(),
        }
    }
}

impl TaskPanel {
    pub fn show_error(&mut self, message: String, now: f64) {
        self.error = Some((message, now));
    }

    /// Feedback for a completed task: the row (or calendar block) pops.
    pub fn celebrate(&self, ctx: &egui::Context, id: TaskId) {
        motion::kick(ctx, pop_id(id), 9.0);
    }

    /// Lets a deleted task fade out where it was instead of vanishing.
    pub fn removed(&mut self, task: Task, now: f64) {
        self.list.removed(task, now);
    }

    /// Clears the name after a successful add, keeping the points for quick repeated entry.
    pub fn task_added(&mut self) {
        self.name.clear();
        self.error = None;
        self.focus_name = true;
    }

    pub fn show(&mut self, ui: &mut Ui, tasks: &[Task]) -> Vec<TaskAction> {
        let mut actions = Vec::new();

        let open = tasks.iter().filter(|t| !t.is_completed()).count();
        let done = tasks.len() - open;
        ui.horizontal(|ui| {
            theme::caption(ui, "Tasks");
            if !tasks.is_empty() {
                ui.label(
                    RichText::new(format!("{open} open · {done} done"))
                        .font(theme::mono(11.0))
                        .color(theme::FAINT),
                );
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (view, label) in [(View::Calendar, "Day"), (View::List, "List")] {
                    if widgets::toggle_label(ui, label, self.view == view).clicked() {
                        self.view = view;
                    }
                }
                if done > 0 && self.view == View::List {
                    ui.add_space(10.0);
                    if widgets::toggle_label(ui, "Clear done", false)
                        .on_hover_text("Remove completed tasks. Their history is kept.")
                        .clicked()
                    {
                        actions.push(TaskAction::ClearCompleted);
                    }
                }
            });
        });
        ui.add_space(4.0);

        if let Some(action) = self.add_form(ui) {
            actions.push(action);
        }
        self.inline_error(ui);
        ui.add_space(6.0);

        if self.view == View::Calendar {
            self.calendar.show(ui, tasks, &mut actions);
            return actions;
        }
        if tasks.is_empty() {
            ui.add_space(12.0);
            ui.label(
                RichText::new(
                    "No tasks yet. Add something worth +5, or a habit to break worth -5.",
                )
                .color(theme::FAINT),
            );
            return actions;
        }

        self.list.show(ui, tasks, &mut actions);
        actions
    }

    fn add_form(&mut self, ui: &mut Ui) -> Option<TaskAction> {
        let mut submit = false;
        let points = parse_points(&self.points);

        Frame::new()
            .fill(theme::SURFACE)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(CornerRadius::same(theme::RADIUS))
            .inner_margin(Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("+")
                            .font(theme::mono(16.0))
                            .color(theme::ACCENT),
                    );

                    let trailing = 128.0;
                    let name = ui.add(
                        TextEdit::singleline(&mut self.name)
                            .hint_text(RichText::new("Add task").color(theme::FAINT))
                            .frame(Frame::NONE)
                            .desired_width((ui.available_width() - trailing).max(80.0)),
                    );
                    if self.focus_name {
                        name.request_focus();
                        self.focus_name = false;
                    }

                    let points_color = match points {
                        Some(p) => theme::points_color(p),
                        None => theme::NEGATIVE,
                    };
                    let points_edit = ui
                        .add(
                            TextEdit::singleline(&mut self.points)
                                .font(theme::mono(14.0))
                                .text_color(points_color)
                                .horizontal_align(Align::Max)
                                .frame(Frame::NONE)
                                .char_limit(6)
                                .desired_width(44.0),
                        )
                        .on_hover_text("Points: +10, -5 …");

                    let enter = ui.input(|i| i.key_pressed(Key::Enter));
                    submit |= enter && (name.lost_focus() || points_edit.lost_focus());

                    let add = ui.add_enabled(
                        !self.name.trim().is_empty(),
                        egui::Button::new(RichText::new("Add").color(theme::TEXT))
                            .fill(theme::SURFACE_HOVER)
                            .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                            .corner_radius(CornerRadius::same(6)),
                    );
                    submit |= add.clicked();
                });
            });

        if !submit || self.name.trim().is_empty() {
            return None;
        }
        self.focus_name = true;
        match points {
            Some(points) => Some(TaskAction::Add {
                name: self.name.clone(),
                points,
            }),
            None => {
                self.show_error(
                    "Points must be a whole number like +10 or -5".to_owned(),
                    ui.input(|i| i.time),
                );
                None
            }
        }
    }

    fn inline_error(&mut self, ui: &mut Ui) {
        let now = ui.input(|i| i.time);
        if let Some((message, since)) = &self.error {
            let age = now - since;
            if age > ERROR_SECONDS {
                self.error = None;
                return;
            }
            let alpha = ((ERROR_SECONDS - age) / 0.4).min(1.0) as f32;
            ui.label(
                RichText::new(message.as_str())
                    .size(12.5)
                    .color(theme::NEGATIVE.gamma_multiply(alpha)),
            );
            ui.ctx().request_repaint();
        }
    }
}

/// Parses user input such as `+10`, `10`, `-5` or `−5`.
pub fn parse_points(input: &str) -> Option<i64> {
    let input = input.trim().replace('−', "-");
    let digits = input.strip_prefix('+').unwrap_or(&input);
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_signed_points() {
        assert_eq!(parse_points("+10"), Some(10));
        assert_eq!(parse_points(" 5 "), Some(5));
        assert_eq!(parse_points("-5"), Some(-5));
        assert_eq!(parse_points("−5"), Some(-5));
        assert_eq!(parse_points("abc"), None);
        assert_eq!(parse_points(""), None);
    }
}
