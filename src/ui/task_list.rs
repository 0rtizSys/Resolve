//! The task list: rows live on springs, so adding, deleting, reordering and completing tasks
//! all move with the same physical feel.

use std::collections::{HashMap, HashSet};

use eframe::egui::{
    self, Align, CornerRadius, CursorIcon, Id, Layout, Rect, RichText, ScrollArea, Sense, Stroke,
    Ui, UiBuilder, pos2, vec2,
};

use super::tasks::{TaskAction, pop_id};
use super::{motion, theme, widgets};
use crate::core::{Task, TaskId};

const ROW_HEIGHT: f32 = 42.0;
/// How long a deleted row takes to fade out.
const LEAVE_SECONDS: f64 = 0.22;
/// How long a dropped row uses the bouncy spring before going back to the smooth one.
const DROP_SETTLE_SECONDS: f64 = 0.8;

#[derive(Default)]
pub struct TaskList {
    /// Row offsets from the last frame, used to fade out deleted rows where they were.
    last_y: HashMap<TaskId, f32>,
    seen: HashSet<TaskId>,
    initialized: bool,
    leaving: Vec<Leaving>,
    drag: Option<Drag>,
    dropped: Option<(TaskId, f64)>,
}

struct Leaving {
    task: Task,
    y: f32,
    since: f64,
}

struct Drag {
    id: TaskId,
    /// Distance from the row's top edge to the pointer when the drag started.
    grab: f32,
}

impl TaskList {
    pub fn show(&mut self, ui: &mut Ui, tasks: &[Task], actions: &mut Vec<TaskAction>) {
        let ctx = ui.ctx().clone();
        let now = ui.input(|i| i.time);
        self.track_membership(&ctx, tasks, now);

        ScrollArea::vertical()
            .id_salt("tasks")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let width = ui.available_width();
                let height = ROW_HEIGHT * tasks.len() as f32;
                let (list, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());

                let pointer_y = ui.input(|i| i.pointer.interact_pos()).map(|p| p.y);
                let slot_of_drag = self.drag.as_ref().zip(pointer_y).map(|(drag, y)| {
                    let top = y - drag.grab - list.top();
                    let slot = (top / ROW_HEIGHT).round();
                    slot.clamp(0.0, tasks.len().saturating_sub(1) as f32) as usize
                });

                // Slot of every task, with the dragged one moved to where it would land.
                let mut order: Vec<TaskId> = tasks.iter().map(|t| t.id).collect();
                if let (Some(drag), Some(slot)) = (&self.drag, slot_of_drag)
                    && let Some(from) = order.iter().position(|id| *id == drag.id)
                {
                    let id = order.remove(from);
                    order.insert(slot, id);
                }

                self.paint_leaving(ui, list, now);

                let dragged_id = self.drag.as_ref().map(|d| d.id);
                // Draw the dragged row last so it floats above the others.
                let draw_order = tasks
                    .iter()
                    .filter(|t| Some(t.id) != dragged_id)
                    .chain(tasks.iter().filter(|t| Some(t.id) == dragged_id));
                for task in draw_order {
                    let slot = order.iter().position(|id| *id == task.id).unwrap_or(0);
                    let target = slot as f32 * ROW_HEIGHT;
                    let y_id = Id::new(("task_row_y", task.id));

                    let y = match (&self.drag, pointer_y) {
                        (Some(drag), Some(pointer)) if drag.id == task.id => {
                            let y = (pointer - drag.grab - list.top())
                                .clamp(-ROW_HEIGHT * 0.5, height - ROW_HEIGHT * 0.5);
                            motion::place(&ctx, y_id, y);
                            y
                        }
                        _ => {
                            let bouncy = self.dropped.is_some_and(|(id, at)| {
                                id == task.id && now - at < DROP_SETTLE_SECONDS
                            });
                            let config = if bouncy {
                                motion::BOUNCY
                            } else {
                                motion::SMOOTH
                            };
                            motion::spring(&ctx, y_id, target, config)
                        }
                    };
                    self.last_y.insert(task.id, y);

                    let rect = Rect::from_min_size(
                        pos2(list.left(), list.top() + y),
                        vec2(width, ROW_HEIGHT),
                    );
                    let lifted = dragged_id == Some(task.id);
                    let enter = motion::spring(&ctx, enter_id(task.id), 1.0, motion::BOUNCY);
                    let response = ui.interact(rect, Id::new(("task_row", task.id)), Sense::drag());

                    if response.drag_started()
                        && let Some(pointer) = pointer_y
                    {
                        self.drag = Some(Drag {
                            id: task.id,
                            grab: pointer - rect.top(),
                        });
                    }
                    if response.drag_stopped() && dragged_id == Some(task.id) {
                        self.drag = None;
                        self.dropped = Some((task.id, now));
                        motion::kick(&ctx, pop_id(task.id), 4.0);
                        actions.push(TaskAction::Move {
                            id: task.id,
                            to: slot,
                        });
                    }
                    if response.hovered() || lifted {
                        ctx.set_cursor_icon(if lifted {
                            CursorIcon::Grabbing
                        } else {
                            CursorIcon::Grab
                        });
                    }

                    row(ui, task, rect, enter, lifted, actions);
                }
            });
    }

    /// Notices rows that appeared (to animate them in) or disappeared (to fade them out).
    fn track_membership(&mut self, ctx: &egui::Context, tasks: &[Task], now: f64) {
        let current: HashSet<TaskId> = tasks.iter().map(|t| t.id).collect();
        if self.initialized {
            for task in tasks.iter().filter(|t| !self.seen.contains(&t.id)) {
                motion::place(ctx, enter_id(task.id), 0.0);
                // Land in its slot directly instead of sliding in from the top.
                motion::place(
                    ctx,
                    Id::new(("task_row_y", task.id)),
                    ROW_HEIGHT * tasks.iter().position(|t| t.id == task.id).unwrap_or(0) as f32,
                );
            }
        }
        self.leaving.retain(|l| now - l.since < LEAVE_SECONDS);
        if let Some(drag) = &self.drag
            && !current.contains(&drag.id)
        {
            self.drag = None;
        }
        self.seen = current;
        self.initialized = true;
    }

    /// Remembers a deleted task so it can fade out where it was.
    pub fn removed(&mut self, task: Task, now: f64) {
        let y = self.last_y.remove(&task.id).unwrap_or(0.0);
        self.leaving.push(Leaving {
            task,
            y,
            since: now,
        });
    }

    fn paint_leaving(&self, ui: &mut Ui, list: Rect, now: f64) {
        for leaving in &self.leaving {
            let t = ((now - leaving.since) / LEAVE_SECONDS).clamp(0.0, 1.0) as f32;
            let rect = Rect::from_min_size(
                pos2(list.left() + 12.0 * t, list.top() + leaving.y),
                vec2(list.width(), ROW_HEIGHT),
            );
            let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
            child.multiply_opacity(1.0 - t);
            let mut ignored = Vec::new();
            row(&mut child, &leaving.task, rect, 1.0, false, &mut ignored);
            ui.ctx().request_repaint();
        }
    }
}

fn enter_id(id: TaskId) -> Id {
    Id::new(("task_row_enter", id))
}

fn row(
    ui: &mut Ui,
    task: &Task,
    rect: Rect,
    enter: f32,
    lifted: bool,
    actions: &mut Vec<TaskAction>,
) {
    let ctx = ui.ctx().clone();
    let hovered = ui.rect_contains_pointer(rect);
    let done = motion::animate_bool(
        &ctx,
        Id::new(("task_done", task.id)),
        task.is_completed(),
        0.25,
    );
    let pop = motion::spring(&ctx, pop_id(task.id), 0.0, motion::BOUNCY);

    // New rows drop in from slightly above and fade in, with a touch of overshoot.
    let rect = rect.translate(vec2(0.0, (1.0 - enter) * -10.0));
    let painter = ui.painter();
    if lifted {
        painter.rect_filled(
            rect.translate(vec2(0.0, 4.0)).expand(1.0),
            CornerRadius::same(8),
            egui::Color32::from_black_alpha(90),
        );
        painter.rect(
            rect,
            CornerRadius::same(8),
            theme::SURFACE_HOVER,
            Stroke::new(1.0, theme::BORDER_STRONG),
            egui::StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_filled(rect, CornerRadius::same(6), theme::SURFACE);
    }
    if pop > 0.01 {
        let color = theme::points_color(task.points);
        painter.rect_filled(
            rect,
            CornerRadius::same(6),
            color.gamma_multiply((pop * 0.25).clamp(0.0, 0.12)),
        );
    }

    let mut row = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.multiply_opacity(enter.clamp(0.0, 1.0));

    let color = theme::points_color(task.points);
    if widgets::check_box(
        &mut row,
        Id::new(("list_check", task.id)),
        task.is_completed(),
        color,
    )
    .clicked()
    {
        actions.push(TaskAction::SetCompleted {
            id: task.id,
            completed: !task.is_completed(),
        });
    }
    row.add_space(10.0);

    row.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::glyph_button(
            ui,
            widgets::Glyph::Cross,
            theme::NEGATIVE,
            hovered && !lifted,
        )
        .on_hover_text("Delete task")
        .clicked()
        {
            actions.push(TaskAction::Delete(task.id));
        }
        ui.add_space(2.0);
        widgets::points_badge(ui, task.points, done, 1.0 + pop * 0.6);
        ui.add_space(8.0);
        if let Some(schedule) = task.schedule {
            let start = schedule.start().with_timezone(&chrono::Local);
            ui.label(
                RichText::new(start.format("%a %H:%M").to_string())
                    .font(theme::mono(11.5))
                    .color(theme::FAINT),
            );
        }

        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let text_color = widgets::lerp_color(theme::TEXT, theme::FAINT, done);
            let label = ui.add(
                egui::Label::new(RichText::new(&task.name).color(text_color))
                    .truncate()
                    .selectable(false)
                    .sense(Sense::hover()),
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
}
