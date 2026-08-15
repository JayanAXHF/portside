pub mod clock;
pub mod command_line;
pub mod history;
pub mod now_playing;
pub mod session_list;
pub mod status_bar;
pub mod timer;

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::action::{Action, Mode};
use crate::db::Session;
use crate::theme::Theme;

/// Read-only snapshot of top-level app state passed to every component at render time, so
/// components don't need mutable back-references into `App` just to know what to draw.
pub struct AppContext<'a> {
    pub mode: Mode,
    pub session: Option<&'a Session>,
    pub today_total: Duration,
    pub theme: &'a Theme,
}

/// A self-contained UI panel. Presentation state (text buffers, list selection, scroll offsets)
/// lives on the component; cross-cutting state (the active session, DB writes, which mode is
/// active) lives on `App` and is read via `AppContext`. This mirrors gitv's `Component` trait,
/// trimmed of the pieces that only make sense for its async/rich-focus setup (no `HasFocus`,
/// no `async handle_event`, no animation flags) since portside has no competing focus-cycling
/// widget library and no per-frame animation layer.
pub trait Component {
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext);

    /// React to an action, optionally producing a follow-up action for `App` to dispatch next.
    #[allow(unused_variables)]
    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        None
    }

    /// Terminal cursor position to place after this frame renders, if this component wants
    /// visible cursor (e.g. the command line while it has focus).
    fn cursor(&self) -> Option<(u16, u16)> {
        None
    }
}
