use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use tui_big_text::BigText;

use crate::db::SessionStatus;

use super::{AppContext, Component};

/// The splash-screen stopwatch: topic name, big elapsed-time digits, and a status indicator.
/// Purely a view over `AppContext.session` — it has no state of its own to track.
#[derive(Debug, Default)]
pub struct TimerComponent;

impl Component for TimerComponent {
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let [_, content_area, _] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(11),
            Constraint::Fill(1),
        ])
        .areas(area);
        let [topic_area, timer_area, status_area] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(8),
            Constraint::Length(1),
        ])
        .areas(content_area);

        let Some(session) = ctx.session else {
            Paragraph::new("No active session")
                .alignment(Alignment::Center)
                .bold()
                .render(topic_area, buf);
            BigText::builder()
                .lines(vec![Line::from("00:00:00")])
                .style(Style::new().dim())
                .alignment(Alignment::Center)
                .build()
                .render(timer_area, buf);
            Paragraph::new("Press : and run `topic <name>` to start studying")
                .alignment(Alignment::Center)
                .dim()
                .render(status_area, buf);
            return;
        };

        Paragraph::new(session.topic.as_str())
            .alignment(Alignment::Center)
            .bold()
            .render(topic_area, buf);

        let color = match session.status {
            SessionStatus::Running => Color::Green,
            SessionStatus::Paused => Color::Yellow,
            SessionStatus::OnBreak => Color::Cyan,
            SessionStatus::Completed => Color::DarkGray,
        };
        BigText::builder()
            .lines(vec![Line::from(format_hms(session.live_elapsed()))])
            .style(Style::new().fg(color))
            .alignment(Alignment::Center)
            .build()
            .render(timer_area, buf);

        let status_text = match session.status {
            SessionStatus::Running => "Running",
            SessionStatus::Paused => "Paused",
            SessionStatus::OnBreak => "On break",
            SessionStatus::Completed => "Completed",
        };
        Paragraph::new(status_text)
            .alignment(Alignment::Center)
            .fg(color)
            .render(status_area, buf);
    }
}

fn format_hms(d: Duration) -> String {
    let total = d.as_secs();
    format!("{:02}:{:02}:{:02}", total / 3600, (total % 3600) / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hours_minutes_seconds() {
        assert_eq!(format_hms(Duration::from_secs(0)), "00:00:00");
        assert_eq!(format_hms(Duration::from_secs(59)), "00:00:59");
        assert_eq!(format_hms(Duration::from_secs(60)), "00:01:00");
        assert_eq!(format_hms(Duration::from_secs(3661)), "01:01:01");
        assert_eq!(format_hms(Duration::from_secs(360_000)), "100:00:00");
    }
}
