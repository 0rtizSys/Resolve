use chrono::{DateTime, Utc};

use super::error::{Error, Result};
use super::event::{DisciplineEvent, EventKind, NewEvent};
use super::schedule::Schedule;
use super::score::DisciplineScore;
use super::store::Store;
use super::task::{NewTask, Task, TaskId};

/// The application's single source of truth: tasks, history and score, kept in sync with a
/// [`Store`]. Every mutation is persisted before the in-memory state changes.
///
/// This is the only place that turns a task completion into a [`DisciplineEvent`] and applies
/// it to the score. Dashboard, history and statistics all read from the same event log here.
pub struct Tracker<S> {
    store: S,
    tasks: Vec<Task>,
    events: Vec<DisciplineEvent>,
    score: DisciplineScore,
}

impl<S: Store> Tracker<S> {
    pub fn load(store: S) -> Result<Self> {
        let tasks = store.load_tasks()?;
        let events = store.load_events()?;
        let score = DisciplineScore::from_events(&events);
        Ok(Self {
            store,
            tasks,
            events,
            score,
        })
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// The full history, oldest first.
    pub fn events(&self) -> &[DisciplineEvent] {
        &self.events
    }

    pub fn score(&self) -> DisciplineScore {
        self.score
    }

    pub fn add_task(&mut self, name: &str, points: i64, now: DateTime<Utc>) -> Result<&Task> {
        let new_task = NewTask::new(name, points)?;
        let task = self.store.insert_task(&new_task, now)?;
        self.tasks.push(task);
        Ok(self.tasks.last().expect("task was just pushed"))
    }

    pub fn delete_task(&mut self, id: TaskId) -> Result<()> {
        let index = self.index_of(id)?;
        self.store.delete_task(id)?;
        self.tasks.remove(index);
        Ok(())
    }

    /// Checks or unchecks a task.
    ///
    /// Checking applies the task's points; unchecking reverts them. Both are recorded in the
    /// history. Returns `None` when the task already was in the requested state.
    pub fn set_completed(
        &mut self,
        id: TaskId,
        completed: bool,
        now: DateTime<Utc>,
    ) -> Result<Option<DisciplineEvent>> {
        let index = self.index_of(id)?;
        let task = &self.tasks[index];
        if task.is_completed() == completed {
            return Ok(None);
        }

        let (kind, points, completed_at) = if completed {
            (EventKind::TaskCompleted, task.points, Some(now))
        } else {
            (EventKind::TaskReverted, -task.points, None)
        };
        let new_event = NewEvent {
            kind,
            action: task.name.clone(),
            points,
            task_id: Some(id),
            occurred_at: now,
        };

        let event = self.store.record_completion(id, completed_at, new_event)?;
        self.tasks[index].completed_at = completed_at;
        self.score.apply(&event);
        self.events.push(event.clone());
        Ok(Some(event))
    }

    /// Places a task in the calendar, moves it, resizes it or (with `None`) unschedules it.
    pub fn schedule_task(&mut self, id: TaskId, schedule: Option<Schedule>) -> Result<()> {
        let index = self.index_of(id)?;
        if self.tasks[index].schedule == schedule {
            return Ok(());
        }
        self.store.update_schedule(id, schedule)?;
        self.tasks[index].schedule = schedule;
        Ok(())
    }

    /// Moves a task to `to_index` in the list, shifting the others.
    pub fn move_task(&mut self, id: TaskId, to_index: usize) -> Result<()> {
        let from = self.index_of(id)?;
        let to = to_index.min(self.tasks.len() - 1);
        if from == to {
            return Ok(());
        }
        let mut reordered = self.tasks.clone();
        let task = reordered.remove(from);
        reordered.insert(to, task);
        let ids: Vec<TaskId> = reordered.iter().map(|task| task.id).collect();
        self.store.update_order(&ids)?;
        self.tasks = reordered;
        Ok(())
    }

    fn index_of(&self, id: TaskId) -> Result<usize> {
        self.tasks
            .iter()
            .position(|task| task.id == id)
            .ok_or(Error::TaskNotFound(id))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::core::error::ValidationError;

    /// Minimal in-memory [`Store`] for exercising core logic without SQLite.
    #[derive(Default)]
    pub struct MemoryStore {
        tasks: Vec<Task>,
        events: Vec<DisciplineEvent>,
        next_id: i64,
    }

    impl Store for MemoryStore {
        fn load_tasks(&self) -> Result<Vec<Task>> {
            Ok(self.tasks.clone())
        }

        fn load_events(&self) -> Result<Vec<DisciplineEvent>> {
            Ok(self.events.clone())
        }

        fn insert_task(&mut self, task: &NewTask, created_at: DateTime<Utc>) -> Result<Task> {
            self.next_id += 1;
            let task = Task {
                id: self.next_id,
                name: task.name().to_owned(),
                points: task.points(),
                created_at,
                completed_at: None,
                schedule: None,
            };
            self.tasks.push(task.clone());
            Ok(task)
        }

        fn update_schedule(&mut self, id: TaskId, schedule: Option<Schedule>) -> Result<()> {
            let task = self
                .tasks
                .iter_mut()
                .find(|task| task.id == id)
                .ok_or(Error::TaskNotFound(id))?;
            task.schedule = schedule;
            Ok(())
        }

        fn update_order(&mut self, ordered: &[TaskId]) -> Result<()> {
            self.tasks
                .sort_by_key(|task| ordered.iter().position(|id| *id == task.id));
            Ok(())
        }

        fn delete_task(&mut self, id: TaskId) -> Result<()> {
            self.tasks.retain(|task| task.id != id);
            Ok(())
        }

        fn record_completion(
            &mut self,
            id: TaskId,
            completed_at: Option<DateTime<Utc>>,
            event: NewEvent,
        ) -> Result<DisciplineEvent> {
            let task = self
                .tasks
                .iter_mut()
                .find(|task| task.id == id)
                .ok_or(Error::TaskNotFound(id))?;
            task.completed_at = completed_at;
            self.next_id += 1;
            let event = event.into_event(self.next_id);
            self.events.push(event.clone());
            Ok(event)
        }
    }

    fn tracker() -> Tracker<MemoryStore> {
        Tracker::load(MemoryStore::default()).unwrap()
    }

    #[test]
    fn completing_tasks_changes_score_and_history() {
        let mut tracker = tracker();
        let now = Utc::now();
        let study = tracker.add_task("Estudiar física", 10, now).unwrap().id;
        let vice = tracker.add_task("No hacer X", -5, now).unwrap().id;

        let event = tracker.set_completed(study, true, now).unwrap().unwrap();
        assert_eq!(event.kind, EventKind::TaskCompleted);
        assert_eq!(event.points, 10);
        assert_eq!(event.action, "Estudiar física");

        tracker.set_completed(vice, true, now).unwrap();
        assert_eq!(tracker.score().total(), 5);
        assert_eq!(tracker.events().len(), 2);
        assert!(tracker.tasks().iter().all(Task::is_completed));
    }

    #[test]
    fn unchecking_reverts_points_and_is_recorded() {
        let mut tracker = tracker();
        let now = Utc::now();
        let id = tracker.add_task("Programar 1 hora", 10, now).unwrap().id;
        tracker.set_completed(id, true, now).unwrap();

        let event = tracker.set_completed(id, false, now).unwrap().unwrap();
        assert_eq!(event.kind, EventKind::TaskReverted);
        assert_eq!(event.points, -10);
        assert_eq!(tracker.score().total(), 0);
        assert_eq!(tracker.events().len(), 2);
        assert!(!tracker.tasks()[0].is_completed());
    }

    #[test]
    fn repeating_the_same_state_is_a_no_op() {
        let mut tracker = tracker();
        let now = Utc::now();
        let id = tracker.add_task("Ordenar mi cuarto", 5, now).unwrap().id;
        assert!(tracker.set_completed(id, false, now).unwrap().is_none());
        tracker.set_completed(id, true, now).unwrap();
        assert!(tracker.set_completed(id, true, now).unwrap().is_none());
        assert_eq!(tracker.score().total(), 5);
    }

    #[test]
    fn deleting_a_task_keeps_its_history() {
        let mut tracker = tracker();
        let now = Utc::now();
        let id = tracker.add_task("Ordenar mi cuarto", 5, now).unwrap().id;
        tracker.set_completed(id, true, now).unwrap();
        tracker.delete_task(id).unwrap();

        assert!(tracker.tasks().is_empty());
        assert_eq!(tracker.events().len(), 1);
        assert_eq!(tracker.score().total(), 5);
        assert!(matches!(
            tracker.set_completed(id, false, now),
            Err(Error::TaskNotFound(_))
        ));
    }

    #[test]
    fn scheduling_and_reordering_are_persisted() {
        let mut tracker = tracker();
        let now = Utc::now();
        let a = tracker.add_task("A", 5, now).unwrap().id;
        let b = tracker.add_task("B", 5, now).unwrap().id;
        let c = tracker.add_task("C", 5, now).unwrap().id;

        let schedule = Schedule::new(now, 60).unwrap();
        tracker.schedule_task(b, Some(schedule)).unwrap();
        tracker.move_task(c, 0).unwrap();

        let order: Vec<_> = tracker.tasks().iter().map(|t| t.id).collect();
        assert_eq!(order, vec![c, a, b]);
        assert_eq!(tracker.tasks()[2].schedule, Some(schedule));

        // A fresh tracker over the same store sees the same state.
        let reloaded = Tracker::load(tracker.store).unwrap();
        let order: Vec<_> = reloaded.tasks().iter().map(|t| t.id).collect();
        assert_eq!(order, vec![c, a, b]);
        assert_eq!(reloaded.tasks()[2].schedule, Some(schedule));
    }

    #[test]
    fn score_chart_and_history_agree() {
        use crate::core::statistics::{Granularity, score_over_time};

        let mut tracker = tracker();
        let now = Utc::now();
        for (name, points) in [("Física", 10), ("Cuarto", 5), ("X", -5)] {
            let id = tracker.add_task(name, points, now).unwrap().id;
            tracker.set_completed(id, true, now).unwrap();
        }
        let first = tracker.tasks()[0].id;
        tracker.set_completed(first, false, now).unwrap();

        let history_total: i64 = tracker.events().iter().map(|e| e.points).sum();
        let today = now.date_naive();
        for granularity in [Granularity::Daily, Granularity::Weekly] {
            let series = score_over_time(tracker.events(), granularity, today, &Utc);
            assert_eq!(series.last().unwrap().score, tracker.score().total());
        }
        assert_eq!(history_total, tracker.score().total());
        assert_eq!(tracker.score().total(), 0);
    }

    #[test]
    fn invalid_tasks_are_rejected() {
        let mut tracker = tracker();
        let result = tracker.add_task(" ", 5, Utc::now());
        assert!(matches!(
            result,
            Err(Error::Validation(ValidationError::EmptyName))
        ));
        assert!(tracker.tasks().is_empty());
    }
}
