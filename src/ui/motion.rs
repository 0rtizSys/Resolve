//! Motion primitives: springs, eased tweens and the reduced-motion preference.
//!
//! Everything animated in Resolve goes through here, so turning motion off (because the
//! operating system asks for it) is a single switch and never changes behavior, only timing.

use eframe::egui::{Context, Id};

/// A damped spring. `damping` below critical (`2 * sqrt(stiffness)`) gives a slight overshoot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringConfig {
    pub stiffness: f32,
    pub damping: f32,
}

/// Physical, with a small overshoot (~5%). Used for things the user drops or releases.
pub const BOUNCY: SpringConfig = SpringConfig {
    stiffness: 420.0,
    damping: 26.0,
};

/// Quick and almost without overshoot. Used for layout shifts the user didn't directly cause.
pub const SMOOTH: SpringConfig = SpringConfig {
    stiffness: 520.0,
    damping: 42.0,
};

/// Below these the spring counts as settled and stops requesting repaints.
const REST_DISTANCE: f32 = 0.05;
const REST_VELOCITY: f32 = 0.5;
/// Largest simulation step; longer frames are split so the spring stays stable.
const MAX_STEP: f32 = 1.0 / 240.0;

#[derive(Debug, Clone, Copy)]
struct SpringState {
    value: f32,
    velocity: f32,
    target: f32,
    last_time: f64,
}

impl SpringState {
    fn step(&mut self, dt: f32, config: SpringConfig) {
        let mut remaining = dt;
        while remaining > 0.0 {
            let h = remaining.min(MAX_STEP);
            let force =
                config.stiffness * (self.target - self.value) - config.damping * self.velocity;
            self.velocity += force * h;
            self.value += self.velocity * h;
            remaining -= h;
        }
        if self.is_settled() {
            self.value = self.target;
            self.velocity = 0.0;
        }
    }

    fn is_settled(&self) -> bool {
        (self.target - self.value).abs() < REST_DISTANCE && self.velocity.abs() < REST_VELOCITY
    }
}

/// Animates towards `target` with a spring and returns the current value.
///
/// The first call for an `id` starts at `target`. Repaints are requested only while moving.
pub fn spring(ctx: &Context, id: Id, target: f32, config: SpringConfig) -> f32 {
    let now = ctx.input(|i| i.time);
    let reduced = reduced_motion(ctx);
    let state = ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_insert_with(id, || SpringState {
            value: target,
            velocity: 0.0,
            target,
            last_time: now,
        });
        state.target = target;
        if reduced {
            state.value = target;
            state.velocity = 0.0;
        } else {
            let dt = (now - state.last_time).clamp(0.0, 0.05) as f32;
            state.step(dt, config);
        }
        state.last_time = now;
        *state
    });
    if !state.is_settled() {
        ctx.request_repaint();
    }
    state.value
}

/// Moves a spring to `value` without animating, e.g. to where the user released a drag.
/// The next [`spring`] call animates from there to its target.
pub fn place(ctx: &Context, id: Id, value: f32) {
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|data| {
        data.insert_temp(
            id,
            SpringState {
                value,
                velocity: 0.0,
                target: value,
                last_time: now,
            },
        );
    });
}

/// Gives a spring a push, e.g. a small "pop" when something gets completed.
pub fn kick(ctx: &Context, id: Id, velocity: f32) {
    if reduced_motion(ctx) {
        return;
    }
    let now = ctx.input(|i| i.time);
    ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_insert_with(id, || SpringState {
            value: 0.0,
            velocity: 0.0,
            target: 0.0,
            last_time: now,
        });
        state.velocity += velocity;
    });
    ctx.request_repaint();
}

/// Like [`Context::animate_bool_with_time`], but instant when motion is reduced.
pub fn animate_bool(ctx: &Context, id: Id, value: bool, seconds: f32) -> f32 {
    ctx.animate_bool_with_time(id, value, duration(ctx, seconds))
}

/// `seconds`, or zero when motion is reduced.
pub fn duration(ctx: &Context, seconds: f32) -> f32 {
    if reduced_motion(ctx) { 0.0 } else { seconds }
}

fn reduced_motion_id() -> Id {
    Id::new("resolve.reduced_motion")
}

pub fn set_reduced_motion(ctx: &Context, reduced: bool) {
    ctx.data_mut(|data| data.insert_temp(reduced_motion_id(), reduced));
    ctx.all_styles_mut(|style| {
        style.animation_time = if reduced { 0.0 } else { 0.15 };
        style.scroll_animation = if reduced {
            eframe::egui::style::ScrollAnimation::none()
        } else {
            eframe::egui::style::ScrollAnimation::default()
        };
    });
}

pub fn reduced_motion(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp(reduced_motion_id()))
        .unwrap_or(false)
}

/// Environment variable that forces the preference: `1` reduces motion, `0` enables it.
pub const REDUCED_MOTION_ENV: &str = "RESOLVE_REDUCED_MOTION";

/// Reads the operating system's "reduce motion" accessibility setting once at startup.
pub fn detect_reduced_motion() -> bool {
    if let Some(value) = std::env::var_os(REDUCED_MOTION_ENV) {
        return value != "0";
    }
    system_reduced_motion().unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn system_reduced_motion() -> Option<bool> {
    let output = command_output(
        "defaults",
        &["read", "com.apple.universalaccess", "reduceMotion"],
    )?;
    Some(output.trim() == "1")
}

#[cfg(target_os = "windows")]
fn system_reduced_motion() -> Option<bool> {
    // "Show animations in Windows" off sets MinAnimate to 0.
    let output = command_output(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\Desktop\WindowMetrics",
            "/v",
            "MinAnimate",
        ],
    )?;
    Some(output.split_whitespace().last()? == "0")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_reduced_motion() -> Option<bool> {
    // GNOME and most GTK desktops expose this; elsewhere we fall back to motion on.
    let output = command_output(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "enable-animations"],
    )?;
    Some(output.trim() == "false")
}

#[cfg(not(any(unix, target_os = "windows")))]
fn system_reduced_motion() -> Option<bool> {
    None
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simulate(config: SpringConfig) -> (f32, f32) {
        let mut state = SpringState {
            value: 0.0,
            velocity: 0.0,
            target: 100.0,
            last_time: 0.0,
        };
        let mut peak: f32 = 0.0;
        for _ in 0..120 {
            state.step(1.0 / 60.0, config);
            peak = peak.max(state.value);
        }
        (peak, state.value)
    }

    #[test]
    fn bouncy_spring_overshoots_slightly_then_settles() {
        let (peak, end) = simulate(BOUNCY);
        assert!(peak > 100.5 && peak < 112.0, "peak was {peak}");
        assert_eq!(end, 100.0);
    }

    #[test]
    fn smooth_spring_barely_overshoots() {
        let (peak, end) = simulate(SMOOTH);
        assert!(peak < 101.5, "peak was {peak}");
        assert_eq!(end, 100.0);
    }
}
