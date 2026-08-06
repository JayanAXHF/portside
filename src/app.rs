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
use crate::components::command_line::CommandLineComponent;
use crate::components::session_list::SessionListComponent;
use crate::components::status_bar::StatusBar;
use crate::components::timer::TimerComponent;
use crate::components::{AppContext, Component};
use crate::db::{Database, Session, SessionStatus};
use crate::errors::{AppError, Result};
use crate::event::{Event, EventHandler};

/// How long a toast notification stays visible before `App::maybe_hide_toast` clears it. There
/// is no `tokio` feature on `ratatui-toaster` here (portside is synchronous), so this timing is
/// driven by hand off the regular tick, per the crate's documented non-async escape hatch.
const TOAST_DURATION: Duration = Duration::from_secs(3);
const TICK_RATE: Duration = Duration::from_millis(250);
/// Width of the session-list drawer that slides in from the right edge of the screen.
const DRAWER_WIDTH: u16 = 40;

pub struct App {
    db: Database,
    session: Option<Session>,
    session_id: Option<i64>,
    mode: Mode,

    timer: TimerComponent,
    session_list: SessionListComponent,
    command_line: CommandLineComponent,
    status_bar: StatusBar,

    toast_engine: ToastEngine<()>,
    toast_shown_at: Option<Instant>,

    event_handler: EventHandler,
    should_quit: bool,
}

impl App {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let db = Database::open(&db_path)?;
        Ok(Self {
            db,
            session: None,
            session_id: None,
            mode: Mode::Normal,
            timer: TimerComponent,
            session_list: SessionListComponent::default(),
            command_line: CommandLineComponent::default(),
            status_bar: StatusBar,
            toast_engine: ToastEngineBuilder::new(ratatui::layout::Rect::default())
                .default_duration(TOAST_DURATION)
                .build(),
            toast_shown_at: None,
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
                None
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
            Action::ToggleBreak => {
                let result = self.toggle_break();
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
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char(':') => Some(Action::EnterCommandMode),
            KeyCode::Char('p') => Some(Action::Pause),
            KeyCode::Char('r') => Some(Action::Resume),
            KeyCode::Char('b') => Some(Action::ToggleBreak),
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
            Command::ToggleBreak => Some(Action::ToggleBreak),
            Command::ResumePrevious(id) => Some(Action::ResumePrevious(id)),
            Command::Sessions => Some(Action::OpenSessionList),
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

    /// Takes ownership of the active session out of `self`, if any, so it can be mutated and
    /// persisted without holding a borrow of `self` across the `self.db.update_session` call.
    /// Callers must put it back via `restore` on every path, including early returns.
    fn take_active(&mut self) -> Option<(i64, Session)> {
        match (self.session_id, self.session.take()) {
            (Some(id), Some(session)) => Some((id, session)),
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
                session.freeze();
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
        session.freeze();
        session.status = SessionStatus::Paused;
        if let Err(err) = self.db.update_session(id, &session) {
            self.restore(id, session);
            return Err(err);
        }
        self.restore(id, session);
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
        Ok("Resumed".to_string())
    }

    fn toggle_break(&mut self) -> Result<String> {
        let Some((id, mut session)) = self.take_active() else {
            return Err(AppError::InvalidCommand("no active session".to_string()));
        };
        let message = match session.status {
            SessionStatus::OnBreak => {
                session.start_running();
                "Break ended, resumed"
            }
            SessionStatus::Running => {
                session.freeze();
                session.status = SessionStatus::OnBreak;
                "On break"
            }
            SessionStatus::Paused => {
                session.status = SessionStatus::OnBreak;
                "On break"
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
        self.restore(id, session);
        Ok(message.to_string())
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
            session.freeze();
        }
        session.status = SessionStatus::Completed;
        let topic = session.topic.clone();
        if let Err(err) = self.db.update_session(id, &session) {
            self.restore(id, session);
            return Err(err);
        }
        // Deliberately not restored: the session is finished, so there's no longer an active one.
        Ok(format!("Completed session: {topic}"))
    }

    fn resume_previous(&mut self, id: Option<i64>) -> Result<String> {
        if let Some((cur_id, mut cur_session)) = self.take_active() {
            if cur_session.status == SessionStatus::Running {
                cur_session.freeze();
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
            };

            let buf = frame.buffer_mut();
            self.timer.render(main_area, buf, &ctx);
            if self.mode == Mode::SessionList {
                let [drawer_area, _] = Layout::horizontal([
                    Constraint::Length(DRAWER_WIDTH.min(main_area.width)),
                    Constraint::Min(0),
                ])
                .areas(main_area);
                self.session_list.render(drawer_area, buf, &ctx);
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
