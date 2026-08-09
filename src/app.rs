use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::WidgetRef;
use ratatui_toaster::{ToastBuilder, ToastEngine, ToastEngineBuilder, ToastPosition, ToastType};

use crate::action::{Action, Mode};
use crate::commands::{self, Command};
use crate::components::clock::ClockComponent;
use crate::components::command_line::CommandLineComponent;
use crate::components::history::HistoryComponent;
use crate::components::session_list::SessionListComponent;
use crate::components::status_bar::StatusBar;
use crate::components::timer::TimerComponent;
use crate::components::{AppContext, Component};
use crate::db::{Database, Session, SessionStatus};
use crate::errors::{AppError, Result};
use crate::event::{Event, EventHandler};
use crate::history;

/// How long a toast notification stays visible before `App::maybe_hide_toast` clears it. There
/// is no `tokio` feature on `ratatui-toaster` here (portside is synchronous), so this timing is
/// driven by hand off the regular tick, per the crate's documented non-async escape hatch.
const TOAST_DURATION: Duration = Duration::from_secs(3);
const TICK_RATE: Duration = Duration::from_millis(250);
/// Width of the session-list drawer that slides in from the right edge of the screen.
const DRAWER_WIDTH: u16 = 40;
/// Height of the history pane that slides up from the bottom of the main area.
const HISTORY_HEIGHT: u16 = 12;
/// How many days of daily totals to fetch when opening the history pane — 53 weeks, enough to
/// fill a full 12-month Sunday-start heatmap in the daily view. Weekly/cumulative bucketing
/// just gets a deeper buffer; both already clip to whatever fits the pane's width.
const HISTORY_LOOKBACK_DAYS: i64 = 371;
/// Size of the top-left "HH:MM:SS" clock, rendered with `tui-big-text`'s `Octant` pixel size:
/// an 8x8 glyph packs into 4 columns x 2 rows of terminal cells at that size, so 8 digits/colons
/// needs `8 * 4` columns, plus a little breathing room.
const CLOCK_WIDTH: u16 = 38;
const CLOCK_HEIGHT: u16 = 2;

pub struct App {
    db: Database,
    session: Option<Session>,
    session_id: Option<i64>,
    mode: Mode,

    timer: TimerComponent,
    clock: ClockComponent,
    session_list: SessionListComponent,
    history: HistoryComponent,
    command_line: CommandLineComponent,
    status_bar: StatusBar,

    toast_engine: ToastEngine<()>,
    toast_shown_at: Option<Instant>,

    break_until: Option<Instant>,
    /// Sum of today's persisted session time, excluding whatever's currently active — recomputed
    /// on every session state transition (not every tick) since it requires a DB query.
    /// `today_total()` adds the active session's live elapsed time back on top per-frame.
    today_total_base: Duration,

    event_handler: EventHandler,
    should_quit: bool,
}

impl App {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let db = Database::open(&db_path)?;
        let today_total_base = Duration::from_secs(db.today_elapsed_secs()?.max(0) as u64);
        Ok(Self {
            db,
            session: None,
            session_id: None,
            mode: Mode::Normal,
            timer: TimerComponent,
            clock: ClockComponent,
            session_list: SessionListComponent::default(),
            history: HistoryComponent::default(),
            command_line: CommandLineComponent::default(),
            status_bar: StatusBar,
            toast_engine: ToastEngineBuilder::new(ratatui::layout::Rect::default())
                .default_duration(TOAST_DURATION)
                .build(),
            toast_shown_at: None,
            break_until: None,
            today_total_base,
            event_handler: EventHandler::new(TICK_RATE),
            should_quit: false,
        })
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.draw(terminal)?;
        while let Some(event) = self.event_handler.next() {
            let action = match event {
                Event::Tick => Action::Tick,
                Event::Resize => Action::Resize,
                Event::Key(key) => Action::Key(key),
            };
            self.process(action);
            if self.should_quit {
                break;
            }
            self.draw(terminal)?;
        }
        Ok(())
    }

    /// Drains a small in-loop queue of actions: dispatching one can enqueue a follow-up (e.g. a
    /// submitted command becomes a parsed `ExecuteCommand`, which becomes a `Toast`), which is
    /// the synchronous version of gitv re-sending actions on its `action_tx` channel.
    fn process(&mut self, action: Action) {
        let mut queue = VecDeque::from([action]);
        while let Some(action) = queue.pop_front() {
            if let Some(next) = self.dispatch(&action) {
                queue.push_back(next);
            }
        }
    }

    fn dispatch(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::Tick => {
                self.maybe_hide_toast();
                self.maybe_notify_break_expired()
            }
            Action::Resize => None,
            Action::Key(key) => self.handle_key(*key),
            Action::Quit => {
                self.should_quit = true;
                None
            }
            Action::SubmitCommand(text) => match commands::parse(text) {
                Ok(cmd) => Some(Action::ExecuteCommand(cmd)),
                Err(err) => Some(Action::Toast(ToastType::Error, err.to_string())),
            },
            Action::ExecuteCommand(cmd) => self.execute_command(cmd.clone()),
            Action::StartSession { topic } => {
                let result = self.start_session(topic.clone());
                Some(self.toast_result(result))
            }
            Action::Pause => {
                let result = self.pause();
                Some(self.toast_result(result))
            }
            Action::Resume => {
                let result = self.resume();
                Some(self.toast_result(result))
            }
            Action::ToggleBreak(duration) => {
                let result = self.toggle_break(*duration);
                Some(self.toast_result(result))
            }
            Action::CompleteSession => {
                let result = self.complete_session();
                Some(self.toast_result(result))
            }
            Action::ResumePrevious(id) => {
                let result = self.resume_previous(*id);
                Some(self.toast_result(result))
            }
            Action::OpenSessionList => {
                self.mode = Mode::SessionList;
                match self.db.list_recent_sessions(50) {
                    Ok(sessions) => Some(Action::SessionsLoaded(sessions)),
                    Err(err) => Some(Action::Toast(ToastType::Error, err.to_string())),
                }
            }
            Action::CloseSessionList => {
                self.mode = Mode::Normal;
                None
            }
            Action::SessionsLoaded(_) => {
                self.session_list.handle_action(action);
                None
            }
            Action::OpenHistory(view) => {
                self.mode = Mode::History;
                if let Some(view) = view {
                    self.history.set_view(*view);
                }
                let today = history::today_local();
                let since = today - time::Duration::days(HISTORY_LOOKBACK_DAYS);
                match self.db.daily_totals_since(&history::format_ymd(since)) {
                    Ok(rows) => {
                        let series = history::zero_filled_daily(rows, since, today);
                        Some(Action::HistoryLoaded(series))
                    }
                    Err(err) => Some(Action::Toast(ToastType::Error, err.to_string())),
                }
            }
            Action::CloseHistory => {
                self.mode = Mode::Normal;
                None
            }
            Action::HistoryLoaded(_) => {
                self.history.handle_action(action);
                None
            }
            Action::SessionSelected(id) => {
                self.mode = Mode::Normal;
                let result = self.resume_previous(Some(*id));
                Some(self.toast_result(result))
            }
            Action::Toast(toast_type, message) => {
                self.show_toast(*toast_type, message.clone());
                None
            }
            Action::EnterCommandMode => {
                self.mode = Mode::CommandLine;
                self.command_line.handle_action(action);
                None
            }
            Action::ExitCommandMode => {
                self.mode = Mode::Normal;
                None
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }
        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::CommandLine => self.command_line.handle_action(&Action::Key(key)),
            Mode::SessionList => {
                if key.code == KeyCode::Esc || key.code == KeyCode::Tab {
                    Some(Action::CloseSessionList)
                } else {
                    self.session_list.handle_action(&Action::Key(key))
                }
            }
            Mode::History => {
                if key.code == KeyCode::Esc || key.code == KeyCode::Tab {
                    Some(Action::CloseHistory)
                } else {
                    self.history.handle_action(&Action::Key(key))
                }
            }
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char(':') => Some(Action::EnterCommandMode),
            KeyCode::Char('p') => Some(Action::Pause),
            KeyCode::Char('r') => Some(Action::Resume),
            KeyCode::Char(' ') => match self.session {
                Some(Session {
                    status: SessionStatus::Running,
                    ..
                }) => Some(Action::Pause),
                _ => Some(Action::Resume),
            },
            KeyCode::Char('b') => Some(Action::ToggleBreak(None)),
            KeyCode::Char('c') => Some(Action::CompleteSession),
            KeyCode::Tab | KeyCode::Char('s') => Some(Action::OpenSessionList),
            KeyCode::Char('q') => Some(Action::Quit),
            _ => None,
        }
    }

    fn execute_command(&mut self, cmd: Command) -> Option<Action> {
        match cmd {
            Command::Topic(topic) => Some(Action::StartSession { topic }),
            Command::Pause => Some(Action::Pause),
            Command::Resume => Some(Action::Resume),
            Command::ToggleBreak(duration) => Some(Action::ToggleBreak(duration)),
            Command::ResumePrevious(id) => Some(Action::ResumePrevious(id)),
            Command::Sessions => Some(Action::OpenSessionList),
            Command::History(view) => Some(Action::OpenHistory(view)),
            Command::Complete => Some(Action::CompleteSession),
            Command::Quit => Some(Action::Quit),
        }
    }

    fn toast_result(&self, result: Result<String>) -> Action {
        match result {
            Ok(message) => Action::Toast(ToastType::Success, message),
            Err(err) => Action::Toast(ToastType::Error, err.to_string()),
        }
    }

    fn show_toast(&mut self, toast_type: ToastType, message: String) {
        self.toast_engine.show_toast(
            ToastBuilder::new(message.into())
                .toast_type(toast_type)
                .position(ToastPosition::TopRight),
        );
        self.toast_shown_at = Some(Instant::now());
    }

    fn maybe_hide_toast(&mut self) {
        if let Some(shown_at) = self.toast_shown_at
            && shown_at.elapsed() >= TOAST_DURATION
        {
            self.toast_engine.hide_toast();
            self.toast_shown_at = None;
        }
    }

    /// Recomputes `today_total_base` from the DB's persisted `session_time_entries`. Called after
    /// every session state transition that persists a change, rather than every tick, to avoid a
    /// SQLite query 4x/sec. The active session's live segment isn't persisted yet at this point
    /// (it's only recorded on freeze), so no exclusion/double-counting concern here.
    fn refresh_today_total_base(&mut self) {
        if let Ok(secs) = self.db.today_elapsed_secs() {
            self.today_total_base = Duration::from_secs(secs.max(0) as u64);
        }
    }

    /// Total time worked today: today's persisted time entries plus whatever portion of the
    /// active session's current running segment falls on today, even if that session was created
    /// on an earlier day (e.g. resumed via `resume-previous`). See `refresh_today_total_base` for
    /// the persisted portion and `Session::live_today_secs` for the live portion.
    fn today_total(&self) -> Duration {
        let live = self
            .session
            .as_ref()
            .map(Session::live_today_secs)
            .unwrap_or(Duration::ZERO);
        self.today_total_base + live
    }

    /// Persists the per-day splits returned by `Session::freeze` to `session_time_entries`. Must
    /// be called at every `freeze()` call site or that segment's time silently stops counting
    /// toward today's total and the history pane.
    fn record_freeze(&self, id: i64, splits: &[(time::Date, Duration)]) -> Result<()> {
        for (date, duration) in splits {
            self.db
                .add_time_entry(id, &history::format_ymd(*date), duration.as_secs() as i64)?;
        }
        Ok(())
    }

    /// Checks whether an active timed break has expired and, if so, fires the terminal bell +
    /// best-effort OS notification and clears the deadline. The session is deliberately left
    /// `OnBreak` — this is a prompt for the user to manually resume, not an auto-resume.
    fn maybe_notify_break_expired(&mut self) -> Option<Action> {
        let until = self.break_until?;
        if Instant::now() < until {
            return None;
        }
        self.break_until = None;
        let topic = self
            .session
            .as_ref()
            .map(|s| s.topic.as_str())
            .unwrap_or("your session");
        crate::notify::fire_break_alert(topic);
        Some(Action::Toast(
            ToastType::Info,
            format!("Break over — resume {topic} when ready"),
        ))
    }

    /// Takes ownership of the active session out of `self`, if any, so it can be mutated and
    /// persisted without holding a borrow of `self` across the `self.db.update_session` call.
    /// Callers must put it back via `restore` on every path, including early returns.
    fn take_active(&mut self) -> Option<(i64, Session)> {
        match (self.session_id, self.session.take()) {
            (Some(id), Some(session)) => {
                self.break_until = None;
                Some((id, session))
            }
            (id, session) => {
                self.session_id = id;
                self.session = session;
                None
            }
        }
    }

    fn restore(&mut self, id: i64, session: Session) {
        self.session_id = Some(id);
        self.session = Some(session);
    }

    fn start_session(&mut self, topic: String) -> Result<String> {
        if let Some((id, mut session)) = self.take_active() {
            if session.status == SessionStatus::Running {
                let splits = session.freeze();
                if let Err(err) = self.record_freeze(id, &splits) {
                    self.restore(id, session);
                    return Err(err);
                }
            }
            if session.status != SessionStatus::Completed {
                session.status = SessionStatus::Paused;
            }
            if let Err(err) = self.db.update_session(id, &session) {
                self.restore(id, session);
                return Err(err);
            }
            // Deliberately not restored: we're switching away from it to a new session.
        }

        let session = Session::new(topic.clone());
        let id = self.db.insert_session(&session)?;
        self.session_id = Some(id);
        self.session = Some(session);
        self.refresh_today_total_base();
        Ok(format!("Started session: {topic}"))
    }

    fn pause(&mut self) -> Result<String> {
        let Some((id, mut session)) = self.take_active() else {
            return Err(AppError::InvalidCommand("no active session".to_string()));
        };
        if session.status != SessionStatus::Running {
            self.restore(id, session);
            return Err(AppError::InvalidCommand(
                "session is not running".to_string(),
            ));
        }
        let splits = session.freeze();
        if let Err(err) = self.record_freeze(id, &splits) {
            self.restore(id, session);
            return Err(err);
        }
        session.status = SessionStatus::Paused;
        if let Err(err) = self.db.update_session(id, &session) {
            self.restore(id, session);
            return Err(err);
        }
        self.restore(id, session);
        self.refresh_today_total_base();
        Ok("Paused".to_string())
    }

    fn resume(&mut self) -> Result<String> {
        let Some((id, mut session)) = self.take_active() else {
            return Err(AppError::InvalidCommand("no active session".to_string()));
        };
        if session.status == SessionStatus::Running {
            self.restore(id, session);
            return Err(AppError::InvalidCommand(
                "session is already running".to_string(),
            ));
        }
        if session.status == SessionStatus::Completed {
            self.restore(id, session);
            return Err(AppError::InvalidCommand(
                "session is already completed".to_string(),
            ));
        }
        session.start_running();
        if let Err(err) = self.db.update_session(id, &session) {
            self.restore(id, session);
            return Err(err);
        }
        self.restore(id, session);
        self.refresh_today_total_base();
        Ok("Resumed".to_string())
    }

    fn toggle_break(&mut self, duration: Option<Duration>) -> Result<String> {
        let Some((id, mut session)) = self.take_active() else {
            return Err(AppError::InvalidCommand("no active session".to_string()));
        };
        let message = match session.status {
            SessionStatus::OnBreak => {
                session.start_running();
                "Break ended, resumed".to_string()
            }
            SessionStatus::Running => {
                let splits = session.freeze();
                if let Err(err) = self.record_freeze(id, &splits) {
                    self.restore(id, session);
                    return Err(err);
                }
                session.status = SessionStatus::OnBreak;
                break_message(duration)
            }
            SessionStatus::Paused => {
                session.status = SessionStatus::OnBreak;
                break_message(duration)
            }
            SessionStatus::Completed => {
                self.restore(id, session);
                return Err(AppError::InvalidCommand(
                    "session is already completed".to_string(),
                ));
            }
        };
        if let Err(err) = self.db.update_session(id, &session) {
            self.restore(id, session);
            return Err(err);
        }
        if session.status == SessionStatus::OnBreak {
            self.break_until = duration.map(|d| Instant::now() + d);
        }
        self.restore(id, session);
        self.refresh_today_total_base();
        Ok(message)
    }

    fn complete_session(&mut self) -> Result<String> {
        let Some((id, mut session)) = self.take_active() else {
            return Err(AppError::InvalidCommand("no active session".to_string()));
        };
        if session.status == SessionStatus::Completed {
            self.restore(id, session);
            return Err(AppError::InvalidCommand(
                "session is already completed".to_string(),
            ));
        }
        if session.status == SessionStatus::Running {
            let splits = session.freeze();
            if let Err(err) = self.record_freeze(id, &splits) {
                self.restore(id, session);
                return Err(err);
            }
        }
        session.status = SessionStatus::Completed;
        let topic = session.topic.clone();
        if let Err(err) = self.db.update_session(id, &session) {
            self.restore(id, session);
            return Err(err);
        }
        // Deliberately not restored: the session is finished, so there's no longer an active one.
        self.refresh_today_total_base();
        Ok(format!("Completed session: {topic}"))
    }

    fn resume_previous(&mut self, id: Option<i64>) -> Result<String> {
        if let Some((cur_id, mut cur_session)) = self.take_active() {
            if cur_session.status == SessionStatus::Running {
                let splits = cur_session.freeze();
                if let Err(err) = self.record_freeze(cur_id, &splits) {
                    self.restore(cur_id, cur_session);
                    return Err(err);
                }
                cur_session.status = SessionStatus::Paused;
            }
            if let Err(err) = self.db.update_session(cur_id, &cur_session) {
                self.restore(cur_id, cur_session);
                return Err(err);
            }
        }

        let found = match id {
            Some(id) => self.db.get_session(id)?,
            None => self.db.latest_resumable_session()?,
        };
        let (row_id, mut session) = found
            .ok_or_else(|| AppError::InvalidCommand("no resumable session found".to_string()))?;
        session.start_running();
        self.db.update_session(row_id, &session)?;
        let topic = session.topic.clone();
        self.session_id = Some(row_id);
        self.session = Some(session);
        self.refresh_today_total_base();
        Ok(format!("Resumed session: {topic}"))
    }

    fn draw(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        terminal.draw(|frame| {
            let area = frame.area();
            let [main_area, command_area, status_area] = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .areas(area);

            let ctx = AppContext {
                mode: self.mode,
                session: self.session.as_ref(),
                today_total: self.today_total(),
            };

            let buf = frame.buffer_mut();
            let (content_area, history_area) = if self.mode == Mode::History {
                let [content_area, history_area] = Layout::vertical([
                    Constraint::Min(0),
                    Constraint::Length(HISTORY_HEIGHT.min(main_area.height)),
                ])
                .areas(main_area);
                (content_area, Some(history_area))
            } else {
                (main_area, None)
            };

            let [clock_area, _] = Layout::horizontal([
                Constraint::Length(CLOCK_WIDTH.min(content_area.width)),
                Constraint::Min(0),
            ])
            .areas(content_area);
            let [clock_area, _] = Layout::vertical([
                Constraint::Length(CLOCK_HEIGHT.min(content_area.height)),
                Constraint::Min(0),
            ])
            .areas(clock_area);
            self.clock.render(clock_area, buf, &ctx);

            self.timer.render(content_area, buf, &ctx);
            if self.mode == Mode::SessionList {
                let [drawer_area, _] = Layout::horizontal([
                    Constraint::Length(DRAWER_WIDTH.min(content_area.width)),
                    Constraint::Min(0),
                ])
                .areas(content_area);
                self.session_list.render(drawer_area, buf, &ctx);
            }
            if let Some(history_area) = history_area {
                self.history.render(history_area, buf, &ctx);
            }
            self.command_line.render(command_area, buf, &ctx);
            self.status_bar.render(status_area, buf, &ctx);

            self.toast_engine.set_area(area);
            self.toast_engine.render_ref(area, buf);

            if self.mode == Mode::CommandLine
                && let Some((dx, dy)) = self.command_line.cursor()
            {
                frame.set_cursor_position((command_area.x + dx, command_area.y + dy));
            }
        })?;
        Ok(())
    }
}

fn break_message(duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let total_secs = d.as_secs();
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;
            let mut string = String::new();
            if hours > 0 {
                string.push_str(&format!("{hours}h "));
            }
            if minutes > 0 {
                string.push_str(&format!("{minutes}m "));
            }
            if seconds > 0 {
                string.push_str(&format!("{seconds}s"));
            }
            format!("On break for {string}")
        }
        None => "On break".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Unique scratch DB path per test, since `App::new` opens a real SQLite file rather than the
    /// in-memory DB used by `db::tests` (there's no in-memory constructor on `App`).
    fn temp_db_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("portside-app-test-{name}-{nanos}.sqlite3"))
    }

    struct TempDb(PathBuf);
    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// End-to-end reproduction of the bug this module's `record_freeze`/`Session::freeze` split
    /// logic fixes: resuming a session created days ago must add the newly-accrued time to
    /// *today's* total, not silently leave it attributed to the session's original day.
    #[test]
    fn resuming_a_session_from_days_ago_adds_to_todays_total() {
        let path = temp_db_path("resume-old");
        let _cleanup = TempDb(path.clone());
        let mut app = App::new(path).unwrap();

        app.start_session("old-topic".to_string()).unwrap();
        app.pause().unwrap();
        let old_id = app.session_id.unwrap();

        // Back-date the session as if it had been created (and left paused) 5 days ago.
        let (_, mut session) = app.db.get_session(old_id).unwrap().unwrap();
        session.started_at -= time::Duration::days(5);
        app.db.update_session(old_id, &session).unwrap();
        app.refresh_today_total_base();
        let before = app.today_total();

        app.resume_previous(Some(old_id)).unwrap();
        // Time entries are persisted in whole seconds (`add_time_entry` takes `secs: i64`), so
        // the running segment needs to clear a full second before `pause` will record anything.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        app.pause().unwrap();

        let after = app.today_total();
        assert!(
            after > before,
            "resuming an old session should add newly-accrued time to today's total \
             (before = {before:?}, after = {after:?})"
        );
    }
}
