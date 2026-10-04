//! The Discipline Score header: big number, weekly delta and the "+5" pulse animation.

use eframe::egui::{self, Align2, RichText, Ui, emath::easing, vec2};

use super::{theme, widgets};

/// How long a "+5" pulse fades in.
const PULSE_APPEAR: f64 = 0.18;
/// How long it stays next to the score before merging into it.
const PULSE_HOLD: f64 = 0.7;
/// How long the merge (slide into the number + fade out) takes.
const PULSE_MERGE: f64 = 0.45;
/// How long the number takes to count to its new value.
const COUNT_DURATION: f64 = 0.5;

#[derive(Default)]
pub struct ScoreDisplay {
    pulses: Vec<Pulse>,
    count: Option<Tween>,
}

struct Pulse {
    delta: i64,
    born: f64,
}

impl ScoreDisplay {
    /// Shows `delta` next to the score for a moment before it is added to the number.
    pub fn celebrate(&mut self, delta: i64, now: f64) {
        self.pulses.push(Pulse { delta, born: now });
    }

    pub fn show(&mut self, ui: &mut Ui, total: i64, this_week: i64) {
        let now = ui.input(|i| i.time);
        self.pulses
            .retain(|p| now - p.born < PULSE_HOLD + PULSE_MERGE);

        // Pulses still "floating" are not part of the displayed number yet.
        let floating: i64 = self
            .pulses
            .iter()
            .filter(|p| now - p.born < PULSE_HOLD)
            .map(|p| p.delta)
            .sum();
        let target = (total - floating) as f64;
        let count = self.count.get_or_insert(Tween::settled(target));
        count.retarget(target, now);
        let shown = count.value(now).round() as i64;
        let counting = !count.is_settled(now);

        // While a pulse merges, the number briefly takes its color.
        let (flash, flash_color) = self
            .pulses
            .iter()
            .filter_map(|p| {
                let t = ((now - p.born - PULSE_HOLD) / PULSE_MERGE) as f32;
                (0.0..=1.0)
                    .contains(&t)
                    .then(|| (1.0 - t, theme::points_color(p.delta)))
            })
            .fold(
                (0.0, theme::TEXT),
                |acc, item| {
                    if item.0 > acc.0 { item } else { acc }
                },
            );
        let number_color = widgets::lerp_color(theme::TEXT, flash_color, flash * 0.75);

        let number = ui.label(
            RichText::new(shown.to_string())
                .font(theme::mono(84.0))
                .color(number_color),
        );
        self.paint_pulses(ui, number.rect, now);

        ui.add_space(-6.0);
        theme::caption(ui, "Discipline points");
        ui.add_space(2.0);
        let week_color = if this_week == 0 {
            theme::MUTED
        } else {
            theme::points_color(this_week)
        };
        ui.label(
            RichText::new(format!("{} this week", theme::format_points(this_week)))
                .font(theme::mono(13.5))
                .color(week_color),
        );

        if !self.pulses.is_empty() || counting {
            ui.ctx().request_repaint();
        }
    }

    fn paint_pulses(&self, ui: &Ui, number: egui::Rect, now: f64) {
        let painter = ui.painter();
        for (index, pulse) in self.pulses.iter().rev().enumerate() {
            let age = now - pulse.born;
            let appear = easing::cubic_out((age / PULSE_APPEAR).min(1.0) as f32);
            let merge =
                easing::cubic_in_out(((age - PULSE_HOLD) / PULSE_MERGE).clamp(0.0, 1.0) as f32);
            let alpha = appear * (1.0 - merge);
            let offset = vec2(
                16.0 - 30.0 * merge,
                -14.0 + 8.0 * (1.0 - appear) - 26.0 * index as f32,
            );
            let color = theme::points_color(pulse.delta).gamma_multiply(alpha);
            painter.text(
                number.right_center() + offset,
                Align2::LEFT_CENTER,
                theme::format_points(pulse.delta),
                theme::mono(26.0),
                color,
            );
        }
    }
}

/// An eased transition between two numbers.
struct Tween {
    from: f64,
    to: f64,
    start: f64,
}

impl Tween {
    fn settled(value: f64) -> Self {
        Self {
            from: value,
            to: value,
            start: f64::NEG_INFINITY,
        }
    }

    fn retarget(&mut self, target: f64, now: f64) {
        if target != self.to {
            self.from = self.value(now);
            self.to = target;
            self.start = now;
        }
    }

    fn value(&self, now: f64) -> f64 {
        let t = ((now - self.start) / COUNT_DURATION).clamp(0.0, 1.0) as f32;
        self.from + (self.to - self.from) * f64::from(easing::cubic_out(t))
    }

    fn is_settled(&self, now: f64) -> bool {
        now - self.start >= COUNT_DURATION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_eases_towards_its_target() {
        let mut tween = Tween::settled(100.0);
        assert_eq!(tween.value(0.0), 100.0);
        tween.retarget(110.0, 1.0);
        let halfway = tween.value(1.0 + COUNT_DURATION / 2.0);
        assert!(halfway > 100.0 && halfway < 110.0);
        assert_eq!(tween.value(1.0 + COUNT_DURATION), 110.0);
        assert!(tween.is_settled(1.0 + COUNT_DURATION));
    }
}
