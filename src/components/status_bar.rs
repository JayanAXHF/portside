use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::action::Mode;

use super::{AppContext, Component};

/// Bottom-most row: current mode and the always-available keybind hints. Static and stateless —
/// unlike gitv's dynamic per-panel help overlay, portside has few enough keybinds that a single
/// fixed hint line covers all of them.
#[derive(Debug, Default)]
pub struct StatusBar;

impl Component for StatusBar {
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let mode_label = match ctx.mode {
            Mode::Normal => "NORMAL",
            Mode::CommandLine => "COMMAND",
            Mode::SessionList => "SESSIONS",
        };

        let hints = match ctx.mode {
            Mode::Normal => {
                "Space pause/resume  p pause  r resume  b break  c complete  Tab sessions  : command  q quit"
            }
            Mode::CommandLine => "Enter submit  Esc cancel",
            Mode::SessionList => "j/k select  Enter resume  Esc close",
        };

        let line = Line::from(vec![
            format!(" {mode_label} ").reversed().bold(),
            "  ".into(),
            hints.dim(),
        ]);
        Paragraph::new(line).render(area, buf);
    }
}
