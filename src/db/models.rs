use std::time::{Duration, Instant};

use time::OffsetDateTime;

use crate::errors::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    Paused,
    OnBreak,
    Completed,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Paused => "paused",
            Self::OnBreak => "on_break",
            Self::Completed => "completed",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "on_break" => Ok(Self::OnBreak),
            "completed" => Ok(Self::Completed),
            other => Err(AppError::InvalidCommand(format!(
                "unknown session status stored in database: {other}"
            ))),
        }
    }
}

/// A study session in memory. `running_since` is a wall-clock anchor used to compute the live
/// elapsed time while `status == Running`; it is never persisted (only `elapsed` is), since an
/// `Instant` has no meaning across process restarts.
#[derive(Debug, Clone)]
pub struct Session {
    pub topic: String,
    pub started_at: OffsetDateTime,
    pub elapsed: Duration,
    pub status: SessionStatus,
    pub running_since: Option<Instant>,
}

impl Session {
    pub fn new(topic: String) -> Self {
        let now = Instant::now();
        Self {
            topic,
            started_at: OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc()),
            elapsed: Duration::ZERO,
            status: SessionStatus::Running,
            running_since: Some(now),
        }
    }

    /// Elapsed running time, including time accrued since `running_since` if currently running.
    /// Time spent paused or on break is excluded, since `elapsed` is only advanced by
    /// `App::pause`/`App::toggle_break` at the moment the session stops running.
    pub fn live_elapsed(&self) -> Duration {
        match self.running_since {
            Some(since) if self.status == SessionStatus::Running => {
                self.elapsed + since.elapsed()
            }
            _ => self.elapsed,
        }
    }

    /// Freezes `elapsed` at the current live value and clears the running anchor. Called before
    /// any transition away from `Running` (pause, break, complete).
    pub fn freeze(&mut self) {
        self.elapsed = self.live_elapsed();
        self.running_since = None;
    }

    pub fn start_running(&mut self) {
        self.running_since = Some(Instant::now());
        self.status = SessionStatus::Running;
    }
}
