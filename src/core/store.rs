use chrono::{DateTime, Utc};

use super::error::Result;
use super::event::{DisciplineEvent, NewEvent};
use super::task::{NewTask, Task, TaskId};

/// Durable storage for Resolve's state.
///
/// The core only talks to this trait, so storage backends (SQLite today, maybe sync later)
/// can change without touching business logic or UI.
pub trait Store {
    /// All tasks, oldest first.
    fn load_tasks(&self) -> Result<Vec<Task>>;

    /// All events, in chronological order.
    fn load_events(&self) -> Result<Vec<DisciplineEvent>>;

    fn insert_task(&mut self, task: &NewTask, created_at: DateTime<Utc>) -> Result<Task>;

    /// Removes a task. Its events stay in the history.
    fn delete_task(&mut self, id: TaskId) -> Result<()>;

    /// Atomically updates a task's completion state and appends the event it produced.
    fn record_completion(
        &mut self,
        id: TaskId,
        completed_at: Option<DateTime<Utc>>,
        event: NewEvent,
    ) -> Result<DisciplineEvent>;
}
