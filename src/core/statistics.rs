//! Derived metrics. Everything here is a pure function of the event history.

use chrono::{Datelike, Days, NaiveDate, TimeZone};

use super::event::DisciplineEvent;

/// Number of days shown by the daily view.
pub const DAILY_PERIODS: u64 = 14;
/// Number of weeks shown by the weekly view.
pub const WEEKLY_PERIODS: u64 = 12;
/// Shortest series returned, so a brand new history still draws a readable chart.
pub const MIN_PERIODS: usize = 7;

/// Resolution of the "Discipline Over Time" series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Granularity {
    Daily,
    Weekly,
}

impl Granularity {
    /// Weekly once the daily view would hide more than half of the history.
    pub fn recommended<Tz: TimeZone>(
        events: &[DisciplineEvent],
        today: NaiveDate,
        tz: &Tz,
    ) -> Self {
        let first_day = events
            .iter()
            .map(|event| local_date(event, tz))
            .min()
            .unwrap_or(today);
        if (today - first_day).num_days() >= 2 * DAILY_PERIODS as i64 {
            Self::Weekly
        } else {
            Self::Daily
        }
    }
}

/// Score at the end of one period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScorePoint {
    /// First day of the period (the day itself, or the Monday of the week).
    pub period_start: NaiveDate,
    pub score: i64,
}

/// The score at the end of each of the most recent periods, oldest first, ending with the
/// period that contains `today`.
///
/// Empty periods before the first event are dropped (keeping one zero baseline and at
/// least [`MIN_PERIODS`] points), so a young history is not squeezed into a corner.
pub fn score_over_time<Tz: TimeZone>(
    events: &[DisciplineEvent],
    granularity: Granularity,
    today: NaiveDate,
    tz: &Tz,
) -> Vec<ScorePoint> {
    let (count, step, last_start) = match granularity {
        Granularity::Daily => (DAILY_PERIODS, 1, today),
        Granularity::Weekly => (WEEKLY_PERIODS, 7, week_start(today)),
    };
    let first_start = last_start - Days::new(step * (count - 1));

    let mut dated: Vec<(NaiveDate, i64)> = events
        .iter()
        .map(|event| (local_date(event, tz), event.points))
        .collect();
    dated.sort_by_key(|(date, _)| *date);

    let first_event = dated.first().map(|(date, _)| *date);
    let mut pending = dated.into_iter().peekable();
    let mut score = 0i64;
    let mut series = Vec::with_capacity(count as usize);
    let mut period_start = first_start;
    for _ in 0..count {
        let next_start = period_start + Days::new(step);
        while let Some((_, points)) = pending.next_if(|(date, _)| *date < next_start) {
            score = score.saturating_add(points);
        }
        series.push(ScorePoint {
            period_start,
            score,
        });
        period_start = next_start;
    }

    let empty_prefix = first_event.map_or(series.len(), |first| {
        series
            .iter()
            .take_while(|p| p.period_start + Days::new(step) <= first)
            .count()
    });
    let drop = empty_prefix
        .saturating_sub(1)
        .min(series.len().saturating_sub(MIN_PERIODS));
    series.drain(..drop);
    series
}

/// Net points earned from Monday of the current week until `today` (inclusive).
pub fn points_this_week<Tz: TimeZone>(
    events: &[DisciplineEvent],
    today: NaiveDate,
    tz: &Tz,
) -> i64 {
    let monday = week_start(today);
    events
        .iter()
        .filter(|event| local_date(event, tz) >= monday)
        .map(|event| event.points)
        .sum()
}

/// Monday of the ISO week that contains `date`.
pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Days::new(u64::from(date.weekday().num_days_from_monday()))
}

fn local_date<Tz: TimeZone>(event: &DisciplineEvent, tz: &Tz) -> NaiveDate {
    event.occurred_at.with_timezone(tz).date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::event::EventKind;
    use chrono::{DateTime, Utc};

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn event_on(date: NaiveDate, points: i64) -> DisciplineEvent {
        let at: DateTime<Utc> = date.and_hms_opt(12, 0, 0).unwrap().and_utc();
        DisciplineEvent {
            id: 0,
            kind: EventKind::TaskCompleted,
            action: String::new(),
            points,
            task_id: None,
            occurred_at: at,
        }
    }

    #[test]
    fn daily_series_accumulates_score_per_day() {
        // Thursday 2026-10-01.
        let today = day(2026, 10, 1);
        let events = [
            event_on(day(2026, 9, 1), 100), // before the window: part of the baseline
            event_on(day(2026, 9, 29), 5),
            event_on(day(2026, 9, 30), 10),
            event_on(day(2026, 9, 30), -5),
            event_on(day(2026, 10, 1), 15),
        ];
        let series = score_over_time(&events, Granularity::Daily, today, &Utc);

        assert_eq!(series.len(), DAILY_PERIODS as usize);
        assert_eq!(series.first().unwrap().period_start, day(2026, 9, 18));
        assert_eq!(series.first().unwrap().score, 100);
        let tail: Vec<i64> = series.iter().rev().take(4).map(|p| p.score).collect();
        assert_eq!(tail, vec![125, 110, 105, 100]);
        assert_eq!(series.last().unwrap().period_start, today);
    }

    #[test]
    fn weekly_series_starts_on_mondays() {
        let today = day(2026, 10, 1);
        let events = [
            event_on(day(2026, 9, 21), 10),
            event_on(day(2026, 9, 30), 5),
        ];
        let series = score_over_time(&events, Granularity::Weekly, today, &Utc);

        assert_eq!(series.len(), MIN_PERIODS);
        assert!(
            series
                .iter()
                .all(|p| p.period_start.weekday() == chrono::Weekday::Mon)
        );
        assert_eq!(series.last().unwrap().period_start, day(2026, 9, 28));
        assert_eq!(series.last().unwrap().score, 15);
        assert_eq!(series[series.len() - 2].score, 10);
    }

    #[test]
    fn leading_empty_periods_are_trimmed() {
        let today = day(2026, 10, 1);
        let events = [event_on(day(2026, 9, 20), 5), event_on(day(2026, 9, 25), 5)];
        let series = score_over_time(&events, Granularity::Daily, today, &Utc);
        assert_eq!(series.first().unwrap().period_start, day(2026, 9, 19));
        assert_eq!(series.first().unwrap().score, 0);
        assert_eq!(series.len(), 13);

        let empty = score_over_time(&[], Granularity::Daily, today, &Utc);
        assert_eq!(empty.len(), MIN_PERIODS);
        assert!(empty.iter().all(|p| p.score == 0));
    }

    #[test]
    fn week_delta_only_counts_the_current_week() {
        let today = day(2026, 10, 1);
        let events = [
            event_on(day(2026, 9, 27), 50), // Sunday of the previous week
            event_on(day(2026, 9, 28), 10), // Monday
            event_on(day(2026, 10, 1), -5),
        ];
        assert_eq!(points_this_week(&events, today, &Utc), 5);
    }

    #[test]
    fn recommends_weekly_view_for_long_histories() {
        let today = day(2026, 10, 1);
        let short = [event_on(day(2026, 9, 25), 5)];
        let long = [event_on(day(2026, 8, 20), 5)];
        assert_eq!(
            Granularity::recommended(&short, today, &Utc),
            Granularity::Daily
        );
        assert_eq!(
            Granularity::recommended(&long, today, &Utc),
            Granularity::Weekly
        );
        assert_eq!(
            Granularity::recommended(&[], today, &Utc),
            Granularity::Daily
        );
    }
}
