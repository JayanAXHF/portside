use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::action::Mode;

use super::{AppContext, Component};

/// Bottom-most row: current mode and the always-available keybind hints on the left, today's
/// total hours worked in a small dimmed segment on the right. Otherwise static and stateless —
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

        let today_text = format_today(ctx.today_total);
        let today_width = today_text.chars().count() as u16 + 1;
        let [left_area, right_area] = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(today_width.min(area.width)),
        ])
        .areas(area);

        let line = Line::from(vec![
            format!(" {mode_label} ").reversed().bold(),
            "  ".into(),
            hints.dim(),
        ]);
        Paragraph::new(line).render(left_area, buf);
        Paragraph::new(today_text.dim()).render(right_area, buf);
    }
}

fn format_today(d: Duration) -> String {
    let total_min = d.as_secs() / 60;
    let (h, m) = (total_min / 60, total_min % 60);
    if h > 0 {
        format!("Today: {h}h {m}m")
    } else {
        format!("Today: {m}m")
    }
}
