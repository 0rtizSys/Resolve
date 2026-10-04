use chrono::{DateTime, Utc};

use super::task::TaskId;

/// Identifier of a persisted event.
pub type EventId = i64;

/// What produced a change in the Discipline Score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// A task was checked; its points were applied.
    TaskCompleted,
    /// A completed task was unchecked; its points were reverted.
    TaskReverted,
}

impl EventKind {
    /// Stable identifier used by persistence. Never change existing values.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TaskCompleted => "task_completed",
            Self::TaskReverted => "task_reverted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "task_completed" => Some(Self::TaskCompleted),
            "task_reverted" => Some(Self::TaskReverted),
            _ => None,
        }
    }
}

/// An immutable entry in the history. The score is the sum of all event points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisciplineEvent {
    pub id: EventId,
    pub kind: EventKind,
    /// Human readable description, usually the task name at the time of the event.
    pub action: String,
    /// Signed change applied to the score.
    pub points: i64,
    /// The task that produced this event, if any. Kept even after the task is deleted.
    pub task_id: Option<TaskId>,
    pub occurred_at: DateTime<Utc>,
}

/// An event that has not been stored yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    pub kind: EventKind,
    pub action: String,
    pub points: i64,
    pub task_id: Option<TaskId>,
    pub occurred_at: DateTime<Utc>,
}

impl NewEvent {
    pub fn into_event(self, id: EventId) -> DisciplineEvent {
        DisciplineEvent {
            id,
            kind: self.kind,
            action: self.action,
            points: self.points,
            task_id: self.task_id,
            occurred_at: self.occurred_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_round_trips_through_its_storage_name() {
        for kind in [EventKind::TaskCompleted, EventKind::TaskReverted] {
            assert_eq!(EventKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(EventKind::parse("unknown"), None);
    }
}
