//! Day calendar. Tasks become blocks you can drag to another time or stretch to change their
//! duration. Every drop is written to the model first; the block then springs into the slot
//! the model now says it occupies.

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, Id, Layout, Rect, RichText, ScrollArea,
    Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};

use super::tasks::{TaskAction, pop_id};
use super::{motion, theme, widgets};
use crate::core::schedule::{self, MIN_DURATION_MINUTES};
use crate::core::{Schedule, Task, TaskId};

const HOUR_HEIGHT: f32 = 52.0;
const GUTTER: f32 = 50.0;
const SNAP_MINUTES: i64 = 15;
const DEFAULT_DURATION: u32 = 60;
const DAY_MINUTES: i64 = 24 * 60;
const RESIZE_HANDLE: f32 = 7.0;
const DROP_SETTLE_SECONDS: f64 = 0.8;

fn per_minute() -> f32 {
    HOUR_HEIGHT / 60.0
}

#[derive(Default)]
pub struct Calendar {
    /// The day being shown; `None` follows today.
    day: Option<NaiveDate>,
    drag: Option<Drag>,
    /// Timeline content rect from the last frame, used to drop chips dragged from the strip.
    timeline: Option<Rect>,
    scroll_to: Option<f32>,
    shown_day: Option<NaiveDate>,
    dropped: Option<(TaskId, f64)>,
}

#[derive(Clone, Copy)]
struct Drag {
    id: TaskId,
    kind: DragKind,
}

#[derive(Clone, Copy)]
enum DragKind {
    /// Moving a block; `grab` is the pointer's distance from the block's top edge.
    Move { grab: f32, duration: u32 },
    /// Stretching a block from its bottom edge.
    Resize { start: i64 },
    /// Dragging an unscheduled task from the strip onto the day.
    FromStrip { grab: egui::Vec2 },
}

/// A block's place in the day, in minutes from local midnight.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Slot {
    start: i64,
    duration: i64,
}

impl Calendar {
    pub fn show(&mut self, ui: &mut Ui, tasks: &[Task], actions: &mut Vec<TaskAction>) {
        let today = Local::now().date_naive();
        let day = self.day.unwrap_or(today);
        if self.shown_day != Some(day) {
            self.shown_day = Some(day);
            self.scroll_to = Some(initial_scroll(tasks, day, today));
        }

        self.header(ui, tasks, day, today);
        ui.add_space(6.0);
        self.strip(ui, tasks, day, actions);
        ui.add_space(6.0);
        self.timeline(ui, tasks, day, today, actions);
    }

    fn header(&mut self, ui: &mut Ui, tasks: &[Task], day: NaiveDate, today: NaiveDate) {
        ui.horizontal(|ui| {
            if widgets::glyph_button(ui, widgets::Glyph::ChevronLeft, theme::TEXT, true)
                .on_hover_text("Previous day")
                .clicked()
            {
                self.day = day.pred_opt();
            }
            ui.label(
                RichText::new(day.format("%A, %-d %B").to_string())
                    .font(theme::mono(13.0))
                    .color(theme::TEXT),
            );
            if widgets::glyph_button(ui, widgets::Glyph::ChevronRight, theme::TEXT, true)
                .on_hover_text("Next day")
                .clicked()
            {
                self.day = day.succ_opt();
            }
            if day != today && widgets::toggle_label(ui, "Today", false).clicked() {
                self.day = None;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let summary = schedule::day_summary(tasks, day, &Local);
                if summary.planned > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{}/{} done · {} · {} pts",
                            summary.completed,
                            summary.planned,
                            format_minutes(summary.planned_minutes),
                            theme::format_points(summary.planned_points),
                        ))
                        .font(theme::mono(11.5))
                        .color(theme::FAINT),
                    )
                    .on_hover_text("Completed / planned · planned time · points at stake");
                }
            });
        });
    }

    /// Unscheduled, open tasks, ready to be dragged onto the day.
    fn strip(
        &mut self,
        ui: &mut Ui,
        tasks: &[Task],
        day: NaiveDate,
        actions: &mut Vec<TaskAction>,
    ) {
        let unscheduled: Vec<&Task> = tasks
            .iter()
            .filter(|t| t.schedule.is_none() && !t.is_completed())
            .collect();
        if unscheduled.is_empty() {
            return;
        }
        let ctx = ui.ctx().clone();
        let pointer = ui.input(|i| i.pointer.interact_pos());

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for task in unscheduled {
                let text = format!("{}  {}", task.name, theme::format_points(task.points));
                let galley = ui.painter().layout_no_wrap(
                    text.clone(),
                    egui::FontId::proportional(13.0),
                    theme::TEXT,
                );
                let size = galley.size() + vec2(20.0, 10.0);
                let (rect, response) = ui.allocate_exact_size(size, Sense::drag());
                let dragging = matches!(self.drag, Some(Drag { id, kind: DragKind::FromStrip { .. } }) if id == task.id);

                if response.drag_started()
                    && let Some(p) = pointer
                {
                    self.drag = Some(Drag {
                        id: task.id,
                        kind: DragKind::FromStrip { grab: p - rect.min },
                    });
                }

                // The chip follows the pointer while dragged and springs home if dropped
                // outside the day.
                let offset_id = Id::new(("chip_offset", task.id));
                let follow = match (dragging, pointer, self.drag) {
                    (true, Some(p), Some(Drag { kind: DragKind::FromStrip { grab }, .. })) => {
                        let offset = p - grab - rect.min;
                        motion::place(&ctx, offset_id.with("x"), offset.x);
                        motion::place(&ctx, offset_id.with("y"), offset.y);
                        offset
                    }
                    _ => vec2(
                        motion::spring(&ctx, offset_id.with("x"), 0.0, motion::BOUNCY),
                        motion::spring(&ctx, offset_id.with("y"), 0.0, motion::BOUNCY),
                    ),
                };

                if response.drag_stopped() && dragging {
                    self.drag = None;
                    if let (Some(p), Some(timeline)) = (pointer, self.timeline)
                        && timeline.contains(p)
                        && let Some(slot) = drop_slot(p.y, timeline, DEFAULT_DURATION.into())
                        && let Some(schedule) = to_schedule(day, slot)
                    {
                        motion::place(&ctx, Id::new(("cal_block_y", task.id)), slot.start as f32 * per_minute());
                        self.dropped = Some((task.id, ui.input(|i| i.time)));
                        actions.push(TaskAction::Schedule {
                            id: task.id,
                            schedule: Some(schedule),
                        });
                    }
                }

                let shown = rect.translate(follow);
                let layer = if dragging {
                    egui::LayerId::new(egui::Order::Tooltip, Id::new("calendar_drag"))
                } else {
                    ui.layer_id()
                };
                let painter = ctx.layer_painter(layer);
                if dragging {
                    painter.rect_filled(
                        shown.translate(vec2(0.0, 4.0)),
                        CornerRadius::same(7),
                        Color32::from_black_alpha(90),
                    );
                }
                painter.rect(
                    shown,
                    CornerRadius::same(7),
                    if response.hovered() || dragging { theme::SURFACE_HOVER } else { theme::SURFACE },
                    Stroke::new(1.0, if dragging { theme::BORDER_STRONG } else { theme::BORDER }),
                    StrokeKind::Inside,
                );
                painter.text(
                    shown.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    &task.name,
                    egui::FontId::proportional(13.0),
                    theme::TEXT,
                );
                painter.text(
                    shown.right_center() - vec2(10.0, 0.0),
                    Align2::RIGHT_CENTER,
                    theme::format_points(task.points),
                    theme::mono(12.0),
                    theme::points_color(task.points),
                );
                if response.hovered() || dragging {
                    ctx.set_cursor_icon(if dragging { CursorIcon::Grabbing } else { CursorIcon::Grab });
                }
                response.on_hover_text("Drag onto the day to schedule it");
            }
        });
    }

    fn timeline(
        &mut self,
        ui: &mut Ui,
        tasks: &[Task],
        day: NaiveDate,
        today: NaiveDate,
        actions: &mut Vec<TaskAction>,
    ) {
        let ctx = ui.ctx().clone();
        let now = ui.input(|i| i.time);
        let pointer = ui.input(|i| i.pointer.interact_pos());

        let mut scroll = ScrollArea::vertical()
            .id_salt("calendar")
            .auto_shrink([false, false]);
        if let Some(offset) = self.scroll_to.take() {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        scroll.show(ui, |ui| {
            let width = ui.available_width();
            let (area, _) = ui.allocate_exact_size(vec2(width, HOUR_HEIGHT * 24.0), Sense::hover());
            self.timeline = Some(area);
            paint_grid(ui, area, day == today);

            let blocks: Vec<(&Task, Slot)> = tasks
                .iter()
                .filter_map(|task| {
                    let schedule = task.schedule?;
                    schedule
                        .starts_on(day, &Local)
                        .then(|| (task, slot_of(&schedule)))
                })
                .collect();
            let columns = columns(&blocks.iter().map(|(_, s)| *s).collect::<Vec<_>>());
            let lane = area.with_min_x(area.left() + GUTTER + 4.0);

            // The dragged block is drawn last, on top of everything else.
            let dragged = self.drag.map(|d| d.id);
            let mut order: Vec<usize> = (0..blocks.len()).collect();
            order.sort_by_key(|i| Some(blocks[*i].0.id) == dragged);

            for i in order {
                let (task, slot) = blocks[i];
                let (column, column_count) = columns[i];
                let y_id = Id::new(("cal_block_y", task.id));
                let h_id = Id::new(("cal_block_h", task.id));
                let target_y = slot.start as f32 * per_minute();
                let target_h = slot.duration as f32 * per_minute();
                let bouncy = self
                    .dropped
                    .is_some_and(|(id, at)| id == task.id && now - at < DROP_SETTLE_SECONDS);
                let config = if bouncy {
                    motion::BOUNCY
                } else {
                    motion::SMOOTH
                };

                let drag = self.drag.filter(|d| d.id == task.id);
                let (y, h) = match (drag.map(|d| d.kind), pointer) {
                    (Some(DragKind::Move { grab, .. }), Some(p)) => {
                        let y = (p.y - area.top() - grab).clamp(0.0, area.height() - target_h);
                        motion::place(&ctx, y_id, y);
                        (y, motion::spring(&ctx, h_id, target_h, config))
                    }
                    (Some(DragKind::Resize { start }), Some(p)) => {
                        let top = start as f32 * per_minute();
                        let min = MIN_DURATION_MINUTES as f32 * per_minute();
                        let h = (p.y - area.top() - top).clamp(min, area.height() - top);
                        motion::place(&ctx, h_id, h);
                        (motion::spring(&ctx, y_id, target_y, config), h)
                    }
                    _ => (
                        motion::spring(&ctx, y_id, target_y, config),
                        motion::spring(&ctx, h_id, target_h, config),
                    ),
                };

                let column_width = lane.width() / column_count as f32;
                let rect = Rect::from_min_size(
                    pos2(lane.left() + column as f32 * column_width, area.top() + y),
                    vec2(column_width - 4.0, h.max(14.0)),
                );

                // Snapped preview of where the block will land.
                let preview = drag.zip(pointer).and_then(|(drag, p)| match drag.kind {
                    DragKind::Move { grab, duration } => {
                        drop_slot(p.y - grab + 1.0, area, duration.into())
                    }
                    DragKind::Resize { start } => resize_slot(start, p.y, area),
                    DragKind::FromStrip { .. } => None,
                });
                if let Some(preview) = preview {
                    paint_preview(ui, lane, area, preview, column, column_width);
                }

                let body = ui.interact(
                    rect,
                    Id::new(("cal_block", task.id)),
                    Sense::click_and_drag(),
                );
                let handle_rect = Rect::from_min_max(
                    pos2(rect.left(), rect.bottom() - RESIZE_HANDLE),
                    rect.right_bottom(),
                );
                let handle =
                    ui.interact(handle_rect, Id::new(("cal_resize", task.id)), Sense::drag());

                if let Some(p) = pointer {
                    if handle.drag_started() {
                        self.drag = Some(Drag {
                            id: task.id,
                            kind: DragKind::Resize { start: slot.start },
                        });
                    } else if body.drag_started() {
                        self.drag = Some(Drag {
                            id: task.id,
                            kind: DragKind::Move {
                                grab: p.y - rect.top(),
                                duration: slot.duration as u32,
                            },
                        });
                    }
                }

                if (body.drag_stopped() || handle.drag_stopped())
                    && let Some(drag) = self.drag.filter(|d| d.id == task.id)
                {
                    self.drag = None;
                    let landed = pointer.and_then(|p| match drag.kind {
                        DragKind::Move { grab, duration } => {
                            drop_slot(p.y - grab + 1.0, area, duration.into())
                        }
                        DragKind::Resize { start } => resize_slot(start, p.y, area),
                        DragKind::FromStrip { .. } => None,
                    });
                    if let Some(schedule) = landed.and_then(|s| to_schedule(day, s)) {
                        self.dropped = Some((task.id, now));
                        motion::kick(&ctx, pop_id(task.id), 3.0);
                        actions.push(TaskAction::Schedule {
                            id: task.id,
                            schedule: Some(schedule),
                        });
                    }
                }

                if handle.hovered() || matches!(drag.map(|d| d.kind), Some(DragKind::Resize { .. }))
                {
                    ctx.set_cursor_icon(CursorIcon::ResizeVertical);
                } else if body.hovered() || drag.is_some() {
                    ctx.set_cursor_icon(if drag.is_some() {
                        CursorIcon::Grabbing
                    } else {
                        CursorIcon::Grab
                    });
                }

                body.context_menu(|ui| {
                    if ui.button("Remove from calendar").clicked() {
                        actions.push(TaskAction::Schedule {
                            id: task.id,
                            schedule: None,
                        });
                        ui.close();
                    }
                });

                block(ui, task, rect, slot, preview, actions);
            }

            // Preview for a chip being dragged in from the strip.
            if let (
                Some(Drag {
                    kind: DragKind::FromStrip { .. },
                    ..
                }),
                Some(p),
            ) = (self.drag, pointer)
                && area.contains(p)
                && let Some(slot) = drop_slot(p.y, area, DEFAULT_DURATION.into())
            {
                paint_preview(ui, lane, area, slot, 0, lane.width());
            }

            if day == today {
                let minutes = local_minutes(Utc::now());
                let y = area.top() + minutes * per_minute();
                ui.painter().hline(
                    area.left() + GUTTER - 4.0..=area.right(),
                    y,
                    Stroke::new(1.0, theme::NEGATIVE.gamma_multiply(0.7)),
                );
                ui.painter().circle_filled(
                    pos2(area.left() + GUTTER - 4.0, y),
                    3.0,
                    theme::NEGATIVE,
                );
            }
        });
    }
}

/// `preview` is where a dragged block would land; its time label follows it live.
fn block(
    ui: &mut Ui,
    task: &Task,
    rect: Rect,
    slot: Slot,
    preview: Option<Slot>,
    actions: &mut Vec<TaskAction>,
) {
    let lifted = preview.is_some();
    let ctx = ui.ctx().clone();
    let color = theme::points_color(task.points);
    let done = motion::animate_bool(
        &ctx,
        Id::new(("cal_done", task.id)),
        task.is_completed(),
        0.25,
    );
    let pop = motion::spring(&ctx, pop_id(task.id), 0.0, motion::BOUNCY);
    let rect = rect.expand(pop.clamp(-1.0, 3.0));

    let painter = ui.painter();
    if lifted {
        painter.rect_filled(
            rect.translate(vec2(0.0, 5.0)),
            CornerRadius::same(7),
            Color32::from_black_alpha(100),
        );
    }
    let fill = widgets::lerp_color(theme::SURFACE_HOVER, color, 0.12 * (1.0 - done * 0.6));
    painter.rect(
        rect,
        CornerRadius::same(7),
        fill,
        Stroke::new(
            1.0,
            if lifted {
                color.gamma_multiply(0.6)
            } else {
                theme::BORDER
            },
        ),
        StrokeKind::Inside,
    );
    painter.rect_filled(
        Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
        CornerRadius {
            nw: 7,
            sw: 7,
            ne: 0,
            se: 0,
        },
        color.gamma_multiply(1.0 - done * 0.5),
    );
    // Grip lines on the resize handle.
    if rect.height() > 30.0 && ui.rect_contains_pointer(rect) {
        let y = rect.bottom() - 4.0;
        painter.hline(
            rect.center().x - 8.0..=rect.center().x + 8.0,
            y,
            Stroke::new(1.0, theme::FAINT),
        );
    }

    let compact = rect.height() < 40.0;
    let mut content = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, if compact { 0.0 } else { 6.0 })))
            .layout(Layout::left_to_right(if compact {
                Align::Center
            } else {
                Align::Min
            })),
    );
    content.set_clip_rect(rect.intersect(ui.clip_rect()));
    if widgets::check_box(
        &mut content,
        Id::new(("calendar_check", task.id)),
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
    content.add_space(6.0);
    content.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let text_color = widgets::lerp_color(theme::TEXT, theme::FAINT, done);
        let mut name = RichText::new(&task.name).color(text_color);
        if task.is_completed() {
            name = name.strikethrough();
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::Label::new(name)
                    .truncate()
                    .selectable(false)
                    .sense(Sense::hover()),
            );
            ui.label(
                RichText::new(theme::format_points(task.points))
                    .font(theme::mono(12.0))
                    .color(color.gamma_multiply(1.0 - done * 0.5)),
            );
        });
        if !compact {
            let shown = preview.unwrap_or(slot);
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{} – {} · {}",
                        clock(shown.start),
                        clock(shown.start + shown.duration),
                        format_minutes(shown.duration as u32)
                    ))
                    .font(theme::mono(11.0))
                    .color(if lifted { theme::ACCENT } else { theme::FAINT }),
                )
                .truncate()
                .selectable(false),
            );
        }
    });
}

fn paint_grid(ui: &Ui, area: Rect, today: bool) {
    let painter = ui.painter();
    let current_hour = today.then(|| Local::now().hour());
    for hour in 0..24 {
        let y = area.top() + hour as f32 * HOUR_HEIGHT;
        painter.hline(
            area.left() + GUTTER..=area.right(),
            y,
            Stroke::new(1.0, theme::BORDER),
        );
        let half = y + HOUR_HEIGHT / 2.0;
        painter.hline(
            area.left() + GUTTER..=area.right(),
            half,
            Stroke::new(1.0, theme::BORDER.gamma_multiply(0.45)),
        );
        let color = if current_hour == Some(hour) {
            theme::MUTED
        } else {
            theme::FAINT
        };
        painter.text(
            pos2(area.left() + GUTTER - 10.0, y + 2.0),
            Align2::RIGHT_TOP,
            format!("{hour:02}:00"),
            theme::mono(11.0),
            color,
        );
    }
}

fn paint_preview(ui: &Ui, lane: Rect, area: Rect, slot: Slot, column: usize, column_width: f32) {
    let rect = Rect::from_min_size(
        pos2(
            lane.left() + column as f32 * column_width,
            area.top() + slot.start as f32 * per_minute(),
        ),
        vec2(column_width - 4.0, slot.duration as f32 * per_minute()),
    );
    ui.painter().rect(
        rect,
        CornerRadius::same(7),
        theme::ACCENT.gamma_multiply(0.05),
        Stroke::new(1.0, theme::ACCENT.gamma_multiply(0.45)),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.right_top() + vec2(-8.0, 6.0),
        Align2::RIGHT_TOP,
        format!(
            "{} – {}",
            clock(slot.start),
            clock(slot.start + slot.duration)
        ),
        theme::mono(11.0),
        theme::ACCENT,
    );
}

/// Where a block whose top edge is at `top_y` would land: snapped and kept inside the day.
fn drop_slot(top_y: f32, area: Rect, duration: i64) -> Option<Slot> {
    let minutes = ((top_y - area.top()) / per_minute()).round() as i64;
    let start = snap(minutes).clamp(0, DAY_MINUTES - duration);
    (start >= 0).then_some(Slot { start, duration })
}

/// The slot after stretching a block that starts at `start` down to `bottom_y`.
fn resize_slot(start: i64, bottom_y: f32, area: Rect) -> Option<Slot> {
    let end = snap(((bottom_y - area.top()) / per_minute()).round() as i64);
    let duration = (end - start).clamp(MIN_DURATION_MINUTES.into(), DAY_MINUTES - start);
    Some(Slot { start, duration })
}

fn snap(minutes: i64) -> i64 {
    ((minutes as f64 / SNAP_MINUTES as f64).round() as i64) * SNAP_MINUTES
}

fn slot_of(schedule: &Schedule) -> Slot {
    Slot {
        start: local_minutes(schedule.start()).round() as i64,
        duration: schedule.duration_minutes().into(),
    }
}

/// Minutes since local midnight.
fn local_minutes(time: DateTime<Utc>) -> f32 {
    let local = time.with_timezone(&Local);
    local.hour() as f32 * 60.0 + local.minute() as f32 + local.second() as f32 / 60.0
}

fn to_schedule(day: NaiveDate, slot: Slot) -> Option<Schedule> {
    let midnight = day.and_hms_opt(0, 0, 0)?;
    let local = Local
        .from_local_datetime(&(midnight + Duration::minutes(slot.start)))
        .earliest()?;
    Schedule::new(
        local.with_timezone(&Utc),
        u32::try_from(slot.duration).ok()?,
    )
    .ok()
}

/// Column index and column count for each block, so overlapping blocks sit side by side.
fn columns(slots: &[Slot]) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by_key(|i| (slots[*i].start, slots[*i].duration));

    let mut result = vec![(0, 1); slots.len()];
    let mut cluster: Vec<usize> = Vec::new();
    let mut column_ends: Vec<i64> = Vec::new();
    let mut cluster_end = i64::MIN;

    let close = |cluster: &mut Vec<usize>, columns: usize, result: &mut Vec<(usize, usize)>| {
        for &i in cluster.iter() {
            result[i].1 = columns;
        }
        cluster.clear();
    };

    for i in order {
        let slot = slots[i];
        if slot.start >= cluster_end && !cluster.is_empty() {
            close(&mut cluster, column_ends.len(), &mut result);
            column_ends.clear();
        }
        let column = match column_ends.iter().position(|end| *end <= slot.start) {
            Some(column) => column,
            None => {
                column_ends.push(0);
                column_ends.len() - 1
            }
        };
        column_ends[column] = slot.start + slot.duration;
        result[i].0 = column;
        cluster.push(i);
        cluster_end = if cluster.len() == 1 {
            slot.start + slot.duration
        } else {
            cluster_end.max(slot.start + slot.duration)
        };
    }
    close(&mut cluster, column_ends.len(), &mut result);
    result
}

/// Scroll so the first block of the day (or the current hour, or 8:00) is near the top.
fn initial_scroll(tasks: &[Task], day: NaiveDate, today: NaiveDate) -> f32 {
    let first = tasks
        .iter()
        .filter_map(|t| t.schedule)
        .filter(|s| s.starts_on(day, &Local))
        .map(|s| local_minutes(s.start()))
        .fold(f32::INFINITY, f32::min);
    let minutes = if first.is_finite() {
        first
    } else if day == today {
        local_minutes(Utc::now())
    } else {
        8.0 * 60.0
    };
    ((minutes - 60.0) * per_minute()).max(0.0)
}

fn clock(minutes: i64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn format_minutes(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m:02}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Pos2;

    fn slot(start: i64, duration: i64) -> Slot {
        Slot { start, duration }
    }

    #[test]
    fn snaps_to_quarter_hours_and_stays_inside_the_day() {
        assert_eq!(snap(907), 900);
        assert_eq!(snap(908), 915);
        let area = Rect::from_min_size(Pos2::ZERO, vec2(100.0, HOUR_HEIGHT * 24.0));
        let late = drop_slot(23.5 * HOUR_HEIGHT, area, 60).unwrap();
        assert_eq!(late, slot(23 * 60, 60));
        let early = drop_slot(-50.0, area, 60).unwrap();
        assert_eq!(early, slot(0, 60));
    }

    #[test]
    fn resizing_respects_the_minimum_duration() {
        let area = Rect::from_min_size(Pos2::ZERO, vec2(100.0, HOUR_HEIGHT * 24.0));
        let start = 15 * 60;
        let shrunk = resize_slot(start, start as f32 * per_minute(), area).unwrap();
        assert_eq!(shrunk.duration, MIN_DURATION_MINUTES as i64);
        let stretched = resize_slot(start, 17.0 * HOUR_HEIGHT, area).unwrap();
        assert_eq!(stretched, slot(start, 120));
    }

    #[test]
    fn overlapping_blocks_share_the_width() {
        let slots = [slot(600, 60), slot(630, 60), slot(720, 30), slot(900, 60)];
        let result = columns(&slots);
        assert_eq!(result[0], (0, 2));
        assert_eq!(result[1], (1, 2));
        assert_eq!(result[2], (0, 1)); // starts after both earlier blocks end
        assert_eq!(result[3], (0, 1));
    }

    #[test]
    fn schedule_round_trips_through_local_time() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let schedule = to_schedule(day, slot(17 * 60, 90)).unwrap();
        assert!(schedule.starts_on(day, &Local));
        assert_eq!(slot_of(&schedule), slot(17 * 60, 90));
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_minutes(45), "45m");
        assert_eq!(format_minutes(120), "2h");
        assert_eq!(format_minutes(150), "2h 30m");
    }
}
