pub mod models;

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::errors::Result;
pub use models::{Session, SessionStatus, Topic};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    fn migrate(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS topics (
                id   INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS sessions (
                id           INTEGER PRIMARY KEY,
                topic_id     INTEGER NOT NULL REFERENCES topics(id),
                started_at   TEXT NOT NULL,
                ended_at     TEXT,
                elapsed_secs INTEGER NOT NULL,
                status       TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn get_or_create_topic(&self, name: &str) -> Result<Topic> {
        self.conn.execute(
            "INSERT INTO topics (name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            params![name],
        )?;
        let id = self.conn.query_row(
            "SELECT id FROM topics WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        Ok(Topic {
            id,
            name: name.to_string(),
        })
    }

    pub fn insert_session(&self, session: &Session) -> Result<i64> {
        let topic = self.get_or_create_topic(&session.topic)?;
        let started_at = session.started_at.format(&Rfc3339)?;
        self.conn.execute(
            "INSERT INTO sessions (topic_id, started_at, ended_at, elapsed_secs, status)
             VALUES (?1, ?2, NULL, ?3, ?4)",
            params![
                topic.id,
                started_at,
                session.elapsed.as_secs() as i64,
                session.status.as_str(),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_session(&self, id: i64, session: &Session) -> Result<()> {
        let ended_at = if session.status == SessionStatus::Completed {
            Some(OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()).format(&Rfc3339)?)
        } else {
            None
        };
        self.conn.execute(
            "UPDATE sessions SET elapsed_secs = ?1, status = ?2, ended_at = ?3 WHERE id = ?4",
            params![
                session.elapsed.as_secs() as i64,
                session.status.as_str(),
                ended_at,
                id,
            ],
        )?;
        Ok(())
    }

    pub fn list_recent_sessions(&self, limit: u32) -> Result<Vec<(i64, Session)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, t.name, s.started_at, s.elapsed_secs, s.status
             FROM sessions s JOIN topics t ON t.id = s.topic_id
             ORDER BY s.started_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], row_to_session)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Most recently started session that is not yet `Completed`, for `resume-previous`.
    pub fn latest_resumable_session(&self) -> Result<Option<(i64, Session)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, t.name, s.started_at, s.elapsed_secs, s.status
             FROM sessions s JOIN topics t ON t.id = s.topic_id
             WHERE s.status != 'completed'
             ORDER BY s.started_at DESC
             LIMIT 1",
        )?;
        let session = stmt
            .query_row([], row_to_session)
            .optional()?;
        Ok(session)
    }

    pub fn get_session(&self, id: i64) -> Result<Option<(i64, Session)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, t.name, s.started_at, s.elapsed_secs, s.status
             FROM sessions s JOIN topics t ON t.id = s.topic_id
             WHERE s.id = ?1",
        )?;
        let session = stmt.query_row(params![id], row_to_session).optional()?;
        Ok(session)
    }
}

fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<(i64, Session)> {
    let id: i64 = row.get(0)?;
    let topic: String = row.get(1)?;
    let started_at_raw: String = row.get(2)?;
    let elapsed_secs: i64 = row.get(3)?;
    let status_raw: String = row.get(4)?;

    let started_at = OffsetDateTime::parse(&started_at_raw, &Rfc3339).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let status = SessionStatus::parse(&status_raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok((
        id,
        Session {
            topic,
            started_at,
            elapsed: Duration::from_secs(elapsed_secs.max(0) as u64),
            status,
            running_since: None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_fetch_topic_is_idempotent() {
        let db = Database::open_in_memory().unwrap();
        let a = db.get_or_create_topic("rust").unwrap();
        let b = db.get_or_create_topic("rust").unwrap();
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn insert_update_and_list_sessions_round_trip() {
        let db = Database::open_in_memory().unwrap();
        let mut session = Session::new("algorithms".to_string());
        session.elapsed = Duration::from_secs(60);
        let id = db.insert_session(&session).unwrap();

        session.status = SessionStatus::Paused;
        session.elapsed = Duration::from_secs(120);
        db.update_session(id, &session).unwrap();

        let (fetched_id, fetched) = db.get_session(id).unwrap().unwrap();
        assert_eq!(fetched_id, id);
        assert_eq!(fetched.topic, "algorithms");
        assert_eq!(fetched.elapsed, Duration::from_secs(120));
        assert_eq!(fetched.status, SessionStatus::Paused);

        let recent = db.list_recent_sessions(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].0, id);
    }

    #[test]
    fn latest_resumable_session_skips_completed() {
        let db = Database::open_in_memory().unwrap();

        let mut done = Session::new("done-topic".to_string());
        done.status = SessionStatus::Completed;
        db.insert_session(&done).unwrap();

        std::thread::sleep(Duration::from_millis(5));

        let mut paused = Session::new("paused-topic".to_string());
        paused.status = SessionStatus::Paused;
        let paused_id = db.insert_session(&paused).unwrap();

        let (id, session) = db.latest_resumable_session().unwrap().unwrap();
        assert_eq!(id, paused_id);
        assert_eq!(session.topic, "paused-topic");
    }

    #[test]
    fn latest_resumable_session_none_when_all_completed() {
        let db = Database::open_in_memory().unwrap();
        let mut done = Session::new("done-topic".to_string());
        done.status = SessionStatus::Completed;
        db.insert_session(&done).unwrap();

        assert!(db.latest_resumable_session().unwrap().is_none());
    }
}
