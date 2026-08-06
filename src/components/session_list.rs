use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Widget};

use crate::action::Action;
use crate::db::{Session, SessionStatus};

use super::{AppContext, Component};

/// The `Tab`/`:sessions`-triggered pane listing past sessions. Only rendered while
/// `AppContext.mode == Mode::SessionList`; owns its own selection cursor and cached row data,
/// refreshed whenever `App` sends `Action::SessionsLoaded` (after querying the database).
#[derive(Debug, Default)]
pub struct SessionListComponent {
    sessions: Vec<(i64, Session)>,
    state: ListState,
}

impl SessionListComponent {
    /// The database id of the currently highlighted row, if any.
    pub fn selected_id(&self) -> Option<i64> {
        self.state
            .selected()
            .and_then(|i| self.sessions.get(i))
            .map(|(id, _)| *id)
    }

    fn select_next(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let next = match self.state.selected() {
            Some(i) => (i + 1) % self.sessions.len(),
            None => 0,
        };
        self.state.select(Some(next));
    }

    fn select_previous(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let prev = match self.state.selected() {
            Some(0) | None => self.sessions.len() - 1,
            Some(i) => i - 1,
        };
        self.state.select(Some(prev));
    }
}

impl Component for SessionListComponent {
    fn render(&mut self, area: Rect, buf: &mut Buffer, _ctx: &AppContext) {
        Clear.render(area, buf);

        let items: Vec<ListItem> = self
            .sessions
            .iter()
            .map(|(_, session)| {
                let status_style = match session.status {
                    SessionStatus::Running => Style::new().fg(Color::Green),
                    SessionStatus::Paused => Style::new().fg(Color::Yellow),
                    SessionStatus::OnBreak => Style::new().fg(Color::Cyan),
                    SessionStatus::Completed => Style::new().add_modifier(Modifier::DIM),
                };
                let status_text = match session.status {
                    SessionStatus::Running => "running",
                    SessionStatus::Paused => "paused",
                    SessionStatus::OnBreak => "on break",
                    SessionStatus::Completed => "completed",
                };
                let elapsed = session.live_elapsed().as_secs();
                let line = Line::from(vec![
                    format!("{:<24}", session.topic).into(),
                    format!(
                        "{:02}:{:02}:{:02}  ",
                        elapsed / 3600,
                        (elapsed % 3600) / 60,
                        elapsed % 60
                    )
                    .into(),
                    Span::styled(status_text, status_style),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = if items.is_empty() {
            List::new(vec![ListItem::new("No previous sessions yet")])
        } else {
            List::new(items)
        }
        .block(
            Block::bordered()
                .borders(Borders::RIGHT)
                .title(" Previous Sessions ")
                .title_alignment(Alignment::Center),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

        ratatui::widgets::StatefulWidget::render(list, area, buf, &mut self.state);
    }

    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::SessionsLoaded(sessions) => {
                self.sessions = sessions.clone();
                if !self.sessions.is_empty() && self.state.selected().is_none() {
                    self.state.select(Some(0));
                }
                None
            }
            Action::Key(key) => {
                use crossterm::event::KeyCode;
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.select_next();
                        None
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.select_previous();
                        None
                    }
                    KeyCode::Enter => self.selected_id().map(Action::SessionSelected),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
