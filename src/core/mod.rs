//! Business logic. No UI and no storage details live here.

pub mod error;
pub mod event;
pub mod schedule;
pub mod score;
pub mod statistics;
pub mod store;
pub mod task;
pub mod tracker;

pub use error::{Error, Result};
pub use event::{DisciplineEvent, EventId, EventKind, NewEvent};
pub use schedule::Schedule;
pub use statistics::{Granularity, ScorePoint};
pub use store::Store;
pub use task::{NewTask, Task, TaskId};
pub use tracker::Tracker;
