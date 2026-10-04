//! Task creation form and task list.

use eframe::egui::{
    self, Align, CornerRadius, Frame, Key, Layout, Margin, RichText, ScrollArea, Sense, Stroke,
    TextEdit, Ui, UiBuilder, vec2,
};

use super::{theme, widgets};
use crate::core::{Task, TaskId};

/// Something the user asked for in the tasks section.
pub enum TaskAction {
    Add { name: String, points: i64 },
    SetCompleted { id: TaskId, completed: bool },
    Delete(TaskId),
    ClearCompleted,
}

/// How long an inline error stays visible, in seconds.
const ERROR_SECONDS: f64 = 4.0;
const ROW_HEIGHT: f32 = 42.0;
const DEFAULT_POINTS: &str = "+5";

pub struct TaskPanel {
    name: String,
    points: String,
    error: Option<(String, f64)>,
    focus_name: bool,
}

impl Default for TaskPanel {
    fn default() -> Self {
        Self {
            name: String::new(),
            points: DEFAULT_POINTS.to_owned(),
            error: None,
            focus_name: true,
        }
    }
}

impl TaskPanel {
    pub fn show_error(&mut self, message: String, now: f64) {
        self.error = Some((message, now));
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
            if done > 0 {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::toggle_label(ui, "Clear done", false)
                        .on_hover_text("Remove completed tasks. Their history is kept.")
                        .clicked()
                    {
                        actions.push(TaskAction::ClearCompleted);
                    }
                });
            }
        });
        ui.add_space(4.0);

        if let Some(action) = self.add_form(ui) {
            actions.push(action);
        }
        self.inline_error(ui);
        ui.add_space(6.0);

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

        ScrollArea::vertical()
            .id_salt("tasks")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for task in tasks {
                    if let Some(action) = task_row(ui, task) {
                        actions.push(action);
                    }
                }
            });
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

fn task_row(ui: &mut Ui, task: &Task) -> Option<TaskAction> {
    let mut action = None;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let hovered = ui.rect_contains_pointer(rect);
    let id = ui.id().with(("task", task.id));
    let done = ui
        .ctx()
        .animate_bool_with_time(id.with("done"), task.is_completed(), 0.25);

    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), theme::SURFACE);
    }

    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    let color = theme::points_color(task.points);
    if widgets::check_box(&mut row, task.is_completed(), color).clicked() {
        action = Some(TaskAction::SetCompleted {
            id: task.id,
            completed: !task.is_completed(),
        });
    }
    row.add_space(10.0);

    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::glyph_button(ui, widgets::Glyph::Cross, theme::NEGATIVE, hovered)
            .on_hover_text("Delete task")
            .clicked()
        {
            action = Some(TaskAction::Delete(task.id));
        }
        ui.add_space(2.0);
        widgets::points_badge(ui, task.points, done);
        ui.add_space(8.0);

        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let text_color = widgets::lerp_color(theme::TEXT, theme::FAINT, done);
            let label = ui.add(
                egui::Label::new(RichText::new(&task.name).color(text_color))
                    .truncate()
                    .selectable(false),
            );
            if done > 0.0 {
                let r = label.rect;
                ui.painter().hline(
                    r.left()..=r.left() + r.width() * done,
                    r.center().y + 1.0,
                    Stroke::new(1.2, theme::FAINT),
                );
            }
        });
    });
    action
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
