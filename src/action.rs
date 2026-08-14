use std::time::Duration;

use crossterm::event::KeyEvent;
use ratatui_toaster::ToastType;
use time::Date;

use crate::commands::Command;
use crate::db::Session;
use crate::history::HistoryView;
use crate::media::NowPlayingInfo;

/// Which component currently owns raw key input. Everything not captured by the active mode
/// falls through to the global keybinds handled by `App`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    CommandLine,
    SessionList,
    History,
}

/// The single message type flowing through the app's event loop. Raw terminal/tick events are
/// translated into these by `App`, then handled by `App` itself (for session/db state) and by
/// each `Component::handle_action` (for presentation state), mirroring gitv's action-channel
/// architecture but driven synchronously instead of through a tokio mpsc channel.
#[derive(Debug, Clone)]
pub enum Action {
    Tick,
    Key(KeyEvent),
    Resize,
    Quit,

    /// Raw text submitted from the command line (leading `:` already stripped).
    SubmitCommand(String),
    /// A command line submission that parsed successfully.
    ExecuteCommand(Command),

    StartSession {
        topic: String,
    },
    Pause,
    Resume,
    ToggleBreak(Option<Duration>),
    CompleteSession,
    ResumePrevious(Option<i64>),

    OpenSessionList,
    CloseSessionList,
    SessionSelected(i64),
    SessionsLoaded(Vec<(i64, Session)>),

    OpenHistory(Option<HistoryView>),
    CloseHistory,
    HistoryLoaded(Vec<(Date, i64)>),

    Toast(ToastType, String),

    NowPlayingUpdated(Option<NowPlayingInfo>),

    EnterCommandMode,
    ExitCommandMode,

    SetDiscordEnabled(bool),
    SetTheme(String),
}
