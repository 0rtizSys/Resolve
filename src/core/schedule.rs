//! When a task is planned to happen.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};

use super::error::ValidationError;
use super::task::Task;

/// Shortest block a task can occupy in the calendar.
pub const MIN_DURATION_MINUTES: u32 = 15;
/// Longest block a task can occupy in the calendar.
pub const MAX_DURATION_MINUTES: u32 = 24 * 60;

/// A start time and a duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    start: DateTime<Utc>,
    duration_minutes: u32,
}

impl Schedule {
    pub fn new(start: DateTime<Utc>, duration_minutes: u32) -> Result<Self, ValidationError> {
        if !(MIN_DURATION_MINUTES..=MAX_DURATION_MINUTES).contains(&duration_minutes) {
            return Err(ValidationError::DurationOutOfRange);
        }
        Ok(Self {
            start,
            duration_minutes,
        })
    }

    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    pub fn duration_minutes(&self) -> u32 {
        self.duration_minutes
    }

    /// Whether the block starts on `date` in the given time zone.
    pub fn starts_on<Tz: TimeZone>(&self, date: NaiveDate, tz: &Tz) -> bool {
        self.start.with_timezone(tz).date_naive() == date
    }
}

/// What a day of the calendar looks like, derived from the tasks scheduled on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DaySummary {
    pub planned: usize,
    pub completed: usize,
    pub planned_minutes: u32,
    /// Net points of the planned tasks if all of them get completed.
    pub planned_points: i64,
}

pub fn day_summary<Tz: TimeZone>(tasks: &[Task], date: NaiveDate, tz: &Tz) -> DaySummary {
    tasks
        .iter()
        .filter_map(|task| Some((task, task.schedule?)))
        .filter(|(_, schedule)| schedule.starts_on(date, tz))
        .fold(DaySummary::default(), |mut summary, (task, schedule)| {
            summary.planned += 1;
            summary.completed += usize::from(task.is_completed());
            summary.planned_minutes += schedule.duration_minutes();
            summary.planned_points += task.points;
            summary
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u32) -> DateTime<Utc> {
        NaiveDate::from_ymd_opt(2026, 10, 5)
            .unwrap()
            .and_hms_opt(hour, 0, 0)
            .unwrap()
            .and_utc()
    }

    #[test]
    fn rejects_durations_out_of_range() {
        assert!(Schedule::new(at(15), 60).is_ok());
        assert_eq!(
            Schedule::new(at(15), 5),
            Err(ValidationError::DurationOutOfRange)
        );
        assert_eq!(
            Schedule::new(at(15), MAX_DURATION_MINUTES + 1),
            Err(ValidationError::DurationOutOfRange)
        );
    }

    #[test]
    fn summarizes_a_day() {
        let task = |id, points, hour: Option<u32>, done: bool| Task {
            id,
            name: String::new(),
            points,
            created_at: at(0),
            completed_at: done.then(|| at(1)),
            schedule: hour.map(|h| Schedule::new(at(h), 90).unwrap()),
        };
        let tasks = [
            task(1, 10, Some(15), true),
            task(2, -5, Some(17), false),
            task(3, 5, None, false),
        ];
        let summary = day_summary(&tasks, at(0).date_naive(), &Utc);
        assert_eq!(
            summary,
            DaySummary {
                planned: 2,
                completed: 1,
                planned_minutes: 180,
                planned_points: 5,
            }
        );
    }
}
