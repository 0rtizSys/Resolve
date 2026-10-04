use super::event::DisciplineEvent;

/// The current Discipline Score: the deterministic sum of every event's points.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisciplineScore {
    total: i64,
}

impl DisciplineScore {
    pub fn from_events<'a>(events: impl IntoIterator<Item = &'a DisciplineEvent>) -> Self {
        let mut score = Self::default();
        for event in events {
            score.apply(event);
        }
        score
    }

    pub fn apply(&mut self, event: &DisciplineEvent) {
        self.total = self.total.saturating_add(event.points);
    }

    pub fn total(self) -> i64 {
        self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::event::EventKind;
    use chrono::Utc;

    fn event(points: i64) -> DisciplineEvent {
        DisciplineEvent {
            id: 0,
            kind: EventKind::TaskCompleted,
            action: String::new(),
            points,
            task_id: None,
            occurred_at: Utc::now(),
        }
    }

    #[test]
    fn score_is_the_sum_of_event_points() {
        let events = [event(5), event(10), event(-5), event(25)];
        assert_eq!(DisciplineScore::from_events(&events).total(), 35);
        assert_eq!(DisciplineScore::from_events(&[]).total(), 0);
    }
}
