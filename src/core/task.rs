use chrono::{DateTime, Utc};

use super::error::ValidationError;

/// Identifier of a persisted task.
pub type TaskId = i64;

/// Longest task name accepted, in characters.
pub const MAX_NAME_CHARS: usize = 120;

/// Largest absolute amount of Discipline Points a single task can be worth.
pub const MAX_POINTS: i64 = 1000;

/// Something the user intends to do (or avoid), worth a fixed amount of points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: TaskId,
    pub name: String,
    /// Points applied to the score when the task is checked. May be negative.
    pub points: i64,
    pub created_at: DateTime<Utc>,
    /// `Some` once the task has been checked.
    pub completed_at: Option<DateTime<Utc>>,
}

impl Task {
    pub fn is_completed(&self) -> bool {
        self.completed_at.is_some()
    }
}

/// A validated request to create a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    name: String,
    points: i64,
}

impl NewTask {
    pub fn new(name: &str, points: i64) -> Result<Self, ValidationError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_CHARS {
            return Err(ValidationError::NameTooLong);
        }
        if points == 0 {
            return Err(ValidationError::ZeroPoints);
        }
        if points.abs() > MAX_POINTS {
            return Err(ValidationError::PointsOutOfRange);
        }
        Ok(Self {
            name: name.to_owned(),
            points,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn points(&self) -> i64 {
        self.points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_accepts_valid_tasks() {
        let task = NewTask::new("  Estudiar física ", 10).unwrap();
        assert_eq!(task.name(), "Estudiar física");
        assert_eq!(task.points(), 10);
        assert!(NewTask::new("No hacer X", -5).is_ok());
    }

    #[test]
    fn rejects_invalid_tasks() {
        assert_eq!(NewTask::new("   ", 5), Err(ValidationError::EmptyName));
        assert_eq!(NewTask::new("Task", 0), Err(ValidationError::ZeroPoints));
        assert_eq!(
            NewTask::new("Task", MAX_POINTS + 1),
            Err(ValidationError::PointsOutOfRange)
        );
        let long = "x".repeat(MAX_NAME_CHARS + 1);
        assert_eq!(NewTask::new(&long, 5), Err(ValidationError::NameTooLong));
    }
}
