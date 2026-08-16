use std::time::{Duration, Instant};

use time::{Date, OffsetDateTime};

use crate::errors::{AppError, Result};

/// Local `now`, falling back to UTC if the local offset can't be determined — the same fallback
/// used throughout the app (see `history::today_local`).
fn now_local() -> OffsetDateTime {
    OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
}

/// Splits the half-open interval `[from, to)` into per-local-calendar-day chunks. Almost always
/// a single entry; produces more than one only if the interval spans a midnight boundary (e.g. a
/// session left running across the day change). Returns an empty vec if `to <= from`.
pub(crate) fn split_by_day(from: OffsetDateTime, to: OffsetDateTime) -> Vec<(Date, Duration)> {
    if to <= from {
        return Vec::new();
    }
    let mut splits = Vec::new();
    let mut cursor = from;
    while cursor.date() < to.date() {
        let next_midnight = cursor
            .date()
            .next_day()
            .expect("date arithmetic within a session's lifetime never overflows")
            .midnight()
            .assume_offset(cursor.offset());
        splits.push((cursor.date(), (next_midnight - cursor).unsigned_abs()));
        cursor = next_midnight;
    }
    splits.push((cursor.date(), (to - cursor).unsigned_abs()));
    splits
}

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

/// A study session in memory. `running_since` is a monotonic anchor used to compute the live
/// elapsed time while `status == Running`; `running_since_wall` is its wall-clock counterpart,
/// needed to attribute that live time to the calendar day(s) it's actually worked on (an
/// `Instant` can't be mapped back to a date). Neither is persisted (only `elapsed` is), since
/// they have no meaning across process restarts.
#[derive(Debug, Clone)]
pub struct Session {
    pub topic: String,
    pub started_at: OffsetDateTime,
    pub elapsed: Duration,
    pub status: SessionStatus,
    pub running_since: Option<Instant>,
    pub running_since_wall: Option<OffsetDateTime>,
}

impl Session {
    pub fn new(topic: String) -> Self {
        Self {
            topic,
            started_at: now_local(),
            elapsed: Duration::ZERO,
            status: SessionStatus::Running,
            running_since: Some(Instant::now()),
            running_since_wall: Some(now_local()),
        }
    }

    /// Elapsed running time, including time accrued since `running_since` if currently running.
    /// Time spent paused or on break is excluded, since `elapsed` is only advanced by
    /// `App::pause`/`App::toggle_break` at the moment the session stops running.
    pub fn live_elapsed(&self) -> Duration {
        match self.running_since {
            Some(since) if self.status == SessionStatus::Running => self.elapsed + since.elapsed(),
            _ => self.elapsed,
        }
    }

    /// The portion of the *current* running segment that falls on today's local calendar date,
    /// without freezing or mutating anything. Used to add the active session's not-yet-persisted
    /// time to the live "today's total" display.
    pub fn live_today_secs(&self) -> Duration {
        let Some(since_wall) = self.running_since_wall else {
            return Duration::ZERO;
        };
        if self.status != SessionStatus::Running {
            return Duration::ZERO;
        }
        let now = now_local();
        split_by_day(since_wall, now)
            .into_iter()
            .find(|(date, _)| *date == now.date())
            .map(|(_, secs)| secs)
            .unwrap_or(Duration::ZERO)
    }

    /// Freezes `elapsed` at the current live value and clears the running anchors. Called before
    /// any transition away from `Running` (pause, break, complete, switching sessions). Returns
    /// the per-calendar-day split of the just-finished running segment so the caller can persist
    /// it to `session_time_entries` — this is the only place that segment's time is recorded, so
    /// callers must not drop the result.
    pub fn freeze(&mut self) -> Vec<(Date, Duration)> {
        let splits = match self.running_since_wall {
            Some(since_wall) if self.status == SessionStatus::Running => {
                split_by_day(since_wall, now_local())
            }
            _ => Vec::new(),
        };
        self.elapsed = self.live_elapsed();
        self.running_since = None;
        self.running_since_wall = None;
        splits
    }

    pub fn start_running(&mut self) {
        self.running_since = Some(Instant::now());
        self.running_since_wall = Some(now_local());
        self.status = SessionStatus::Running;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn split_by_day_returns_empty_for_non_positive_interval() {
        let t = datetime!(2026-08-09 10:00:00 +00:00);
        assert!(split_by_day(t, t).is_empty());
        assert!(split_by_day(t, t - time::Duration::seconds(1)).is_empty());
    }

    #[test]
    fn split_by_day_within_a_single_day_is_one_entry() {
        let from = datetime!(2026-08-09 10:00:00 +00:00);
        let to = datetime!(2026-08-09 10:05:00 +00:00);
        assert_eq!(
            split_by_day(from, to),
            vec![(from.date(), Duration::from_secs(300))]
        );
    }

    #[test]
    fn split_by_day_spans_a_single_midnight() {
        let from = datetime!(2026-08-09 23:55:00 +00:00);
        let to = datetime!(2026-08-10 00:10:00 +00:00);
        assert_eq!(
            split_by_day(from, to),
            vec![
                (from.date(), Duration::from_secs(5 * 60)),
                (to.date(), Duration::from_secs(10 * 60)),
            ]
        );
    }

    #[test]
    fn split_by_day_spans_multiple_midnights() {
        let from = datetime!(2026-08-09 23:00:00 +00:00);
        let to = datetime!(2026-08-11 01:00:00 +00:00);
        assert_eq!(
            split_by_day(from, to),
            vec![
                (from.date(), Duration::from_secs(3600)),
                (
                    datetime!(2026-08-10 00:00:00 +00:00).date(),
                    Duration::from_secs(86_400)
                ),
                (to.date(), Duration::from_secs(3600)),
            ]
        );
    }

    /// The scenario this whole mechanism exists for: `resume-previous` on a session created
    /// days ago must attribute newly-accrued time to the day it's actually worked on (today, via
    /// the fresh `running_since_wall` set by `start_running`), not the session's original
    /// creation day.
    #[test]
    fn freeze_attributes_time_to_the_day_worked_not_the_session_creation_day() {
        let mut session = Session::new("resumed-topic".to_string());
        session.started_at -= time::Duration::days(5);
        session.freeze(); // simulate the session having been paused since creation
        session.start_running(); // `resume-previous`: a fresh wall-clock anchor set to "now"
        std::thread::sleep(std::time::Duration::from_millis(20));

        let splits = session.freeze();

        assert_eq!(splits.len(), 1);
        let (day, secs) = splits[0];
        assert_eq!(day, now_local().date());
        assert_ne!(day, session.started_at.date());
        assert!(secs > Duration::ZERO);
    }

    #[test]
    fn live_today_secs_is_zero_when_not_running() {
        let mut session = Session::new("topic".to_string());
        session.freeze();
        assert_eq!(session.live_today_secs(), Duration::ZERO);
    }

    #[test]
    fn live_today_secs_reflects_the_current_running_segment() {
        let session = Session::new("topic".to_string());
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(session.live_today_secs() > Duration::ZERO);
    }
}
