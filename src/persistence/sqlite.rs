use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, Row, params};

use crate::core::{
    DisciplineEvent, Error, EventKind, NewEvent, NewTask, Result, Schedule, Store, Task, TaskId,
};

/// Schema migrations, applied in order. Append new ones; never edit an existing entry.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    "CREATE TABLE tasks (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        name         TEXT    NOT NULL,
        points       INTEGER NOT NULL,
        created_at   INTEGER NOT NULL,
        completed_at INTEGER
    );
    CREATE TABLE events (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        kind        TEXT    NOT NULL,
        action      TEXT    NOT NULL,
        points      INTEGER NOT NULL,
        task_id     INTEGER,
        occurred_at INTEGER NOT NULL
    );
    CREATE INDEX events_occurred_at ON events (occurred_at);",
    // 2: manual ordering and calendar scheduling
    "ALTER TABLE tasks ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
    UPDATE tasks SET position = id;
    ALTER TABLE tasks ADD COLUMN scheduled_start INTEGER;
    ALTER TABLE tasks ADD COLUMN duration_minutes INTEGER;",
];

/// [`Store`] backed by a local SQLite database.
pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    /// Opens (or creates) the database at `path` and brings its schema up to date.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(storage)?;
        }
        Self::init(Connection::open(path).map_err(storage)?)
    }

    /// A throwaway database, used by tests.
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(storage)?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)
            .map_err(storage)?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(storage)?;
    for (index, sql) in (0u32..).zip(MIGRATIONS).skip(version as usize) {
        let tx = conn.transaction().map_err(storage)?;
        tx.execute_batch(sql).map_err(storage)?;
        tx.pragma_update(None, "user_version", index + 1)
            .map_err(storage)?;
        tx.commit().map_err(storage)?;
    }
    Ok(())
}

impl Store for SqliteStore {
    fn load_tasks(&self) -> Result<Vec<Task>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, points, created_at, completed_at, scheduled_start, duration_minutes
                 FROM tasks ORDER BY position, id",
            )
            .map_err(storage)?;
        let tasks = stmt
            .query_map([], task_from_row)
            .map_err(storage)?
            .collect::<rusqlite::Result<_>>()
            .map_err(storage)?;
        Ok(tasks)
    }

    fn load_events(&self) -> Result<Vec<DisciplineEvent>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, action, points, task_id, occurred_at
                 FROM events ORDER BY occurred_at, id",
            )
            .map_err(storage)?;
        let events = stmt
            .query_map([], event_from_row)
            .map_err(storage)?
            .collect::<rusqlite::Result<_>>()
            .map_err(storage)?;
        Ok(events)
    }

    fn insert_task(&mut self, task: &NewTask, created_at: DateTime<Utc>) -> Result<Task> {
        self.conn
            .execute(
                "INSERT INTO tasks (name, points, created_at, position)
                 VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), 0) + 1 FROM tasks))",
                params![task.name(), task.points(), to_millis(created_at)],
            )
            .map_err(storage)?;
        Ok(Task {
            id: self.conn.last_insert_rowid(),
            name: task.name().to_owned(),
            points: task.points(),
            created_at: truncate(created_at),
            completed_at: None,
            schedule: None,
        })
    }

    fn update_schedule(&mut self, id: TaskId, schedule: Option<Schedule>) -> Result<()> {
        let updated = self
            .conn
            .execute(
                "UPDATE tasks SET scheduled_start = ?1, duration_minutes = ?2 WHERE id = ?3",
                params![
                    schedule.map(|s| to_millis(s.start())),
                    schedule.map(|s| s.duration_minutes()),
                    id
                ],
            )
            .map_err(storage)?;
        if updated == 0 {
            return Err(Error::TaskNotFound(id));
        }
        Ok(())
    }

    fn update_order(&mut self, ordered: &[TaskId]) -> Result<()> {
        let tx = self.conn.transaction().map_err(storage)?;
        {
            let mut stmt = tx
                .prepare("UPDATE tasks SET position = ?1 WHERE id = ?2")
                .map_err(storage)?;
            for (position, id) in (1i64..).zip(ordered) {
                stmt.execute(params![position, id]).map_err(storage)?;
            }
        }
        tx.commit().map_err(storage)
    }

    fn delete_task(&mut self, id: TaskId) -> Result<()> {
        let deleted = self
            .conn
            .execute("DELETE FROM tasks WHERE id = ?1", [id])
            .map_err(storage)?;
        if deleted == 0 {
            return Err(Error::TaskNotFound(id));
        }
        Ok(())
    }

    fn record_completion(
        &mut self,
        id: TaskId,
        completed_at: Option<DateTime<Utc>>,
        event: NewEvent,
    ) -> Result<DisciplineEvent> {
        let tx = self.conn.transaction().map_err(storage)?;
        let updated = tx
            .execute(
                "UPDATE tasks SET completed_at = ?1 WHERE id = ?2",
                params![completed_at.map(to_millis), id],
            )
            .map_err(storage)?;
        if updated == 0 {
            return Err(Error::TaskNotFound(id));
        }
        tx.execute(
            "INSERT INTO events (kind, action, points, task_id, occurred_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.kind.as_str(),
                event.action,
                event.points,
                event.task_id,
                to_millis(event.occurred_at)
            ],
        )
        .map_err(storage)?;
        let event_id = tx.last_insert_rowid();
        tx.commit().map_err(storage)?;

        let mut event = event.into_event(event_id);
        event.occurred_at = truncate(event.occurred_at);
        Ok(event)
    }
}

impl SqliteStore {
    /// Whether a task row exists.
    #[cfg(test)]
    fn task_exists(&self, id: TaskId) -> bool {
        self.conn
            .query_row("SELECT COUNT(*) FROM tasks WHERE id = ?1", [id], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
            > 0
    }
}

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        name: row.get(1)?,
        points: row.get(2)?,
        created_at: from_millis(row.get(3)?),
        completed_at: row.get::<_, Option<i64>>(4)?.map(from_millis),
        schedule: schedule_from_row(row)?,
    })
}

fn schedule_from_row(row: &Row<'_>) -> rusqlite::Result<Option<Schedule>> {
    let start: Option<i64> = row.get(5)?;
    let minutes: Option<u32> = row.get(6)?;
    let (Some(start), Some(minutes)) = (start, minutes) else {
        return Ok(None);
    };
    Schedule::new(from_millis(start), minutes)
        .map(Some)
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Integer, err.into())
        })
}

fn event_from_row(row: &Row<'_>) -> rusqlite::Result<DisciplineEvent> {
    let kind: String = row.get(1)?;
    let kind = EventKind::parse(&kind).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            format!("unknown event kind {kind:?}").into(),
        )
    })?;
    Ok(DisciplineEvent {
        id: row.get(0)?,
        kind,
        action: row.get(2)?,
        points: row.get(3)?,
        task_id: row.get(4)?,
        occurred_at: from_millis(row.get(5)?),
    })
}

fn to_millis(time: DateTime<Utc>) -> i64 {
    time.timestamp_millis()
}

fn from_millis(millis: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(millis).unwrap_or_default()
}

/// Drops sub-millisecond precision so in-memory values match what a reload returns.
fn truncate(time: DateTime<Utc>) -> DateTime<Utc> {
    from_millis(to_millis(time))
}

fn storage(err: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::Storage(Box::new(err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Tracker;

    #[test]
    fn state_survives_a_reopen() {
        let dir = std::env::temp_dir().join(format!("resolve-test-{}", std::process::id()));
        let path = dir.join("resolve.db");
        let _ = std::fs::remove_file(&path);
        let now = Utc::now();

        let (done, pending) = {
            let mut tracker = Tracker::load(SqliteStore::open(&path).unwrap()).unwrap();
            let done = tracker.add_task("Estudiar física", 10, now).unwrap().id;
            let pending = tracker.add_task("No hacer X", -5, now).unwrap().id;
            tracker.set_completed(done, true, now).unwrap();
            (done, pending)
        };

        let tracker = Tracker::load(SqliteStore::open(&path).unwrap()).unwrap();
        assert_eq!(tracker.score().total(), 10);
        assert_eq!(tracker.tasks().len(), 2);
        assert!(
            tracker
                .tasks()
                .iter()
                .any(|t| t.id == done && t.is_completed())
        );
        assert!(
            tracker
                .tasks()
                .iter()
                .any(|t| t.id == pending && !t.is_completed())
        );
        assert_eq!(tracker.events().len(), 1);
        assert_eq!(tracker.events()[0].occurred_at, truncate(now));
        drop(tracker);

        // Order and schedules survive too.
        let schedule = Schedule::new(truncate(now), 45).unwrap();
        {
            let mut tracker = Tracker::load(SqliteStore::open(&path).unwrap()).unwrap();
            tracker.move_task(pending, 0).unwrap();
            tracker.schedule_task(done, Some(schedule)).unwrap();
            tracker.add_task("Leer", 5, now).unwrap();
        }
        let tracker = Tracker::load(SqliteStore::open(&path).unwrap()).unwrap();
        let names: Vec<_> = tracker.tasks().iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["No hacer X", "Estudiar física", "Leer"]);
        assert_eq!(tracker.tasks()[1].schedule, Some(schedule));

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn completion_and_event_are_written_together() {
        let mut store = SqliteStore::in_memory().unwrap();
        let task = store
            .insert_task(&NewTask::new("Programar", 10).unwrap(), Utc::now())
            .unwrap();
        store.delete_task(task.id).unwrap();
        assert!(!store.task_exists(task.id));

        let event = NewEvent {
            kind: EventKind::TaskCompleted,
            action: "Programar".into(),
            points: 10,
            task_id: Some(task.id),
            occurred_at: Utc::now(),
        };
        assert!(matches!(
            store.record_completion(task.id, Some(Utc::now()), event),
            Err(Error::TaskNotFound(_))
        ));
        assert!(store.load_events().unwrap().is_empty());
    }

    #[test]
    fn upgrades_a_version_1_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(
            "INSERT INTO tasks (name, points, created_at) VALUES ('A', 5, 0), ('B', -5, 0);",
        )
        .unwrap();

        let store = SqliteStore::init(conn).unwrap();
        let tasks = store.load_tasks().unwrap();
        let names: Vec<_> = tasks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["A", "B"]);
        assert!(tasks.iter().all(|t| t.schedule.is_none()));
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut store = SqliteStore::in_memory().unwrap();
        migrate(&mut store.conn).unwrap();
        let version: u32 = store
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
    }
}
