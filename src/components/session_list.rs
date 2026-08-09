use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Rect, Size};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Clear, Row, Table, TableState, Widget};
use tui_scrollview::{ScrollView, ScrollViewState};

use crate::action::Action;
use crate::db::{Session, SessionStatus};

use super::{AppContext, Component};

/// The `Tab`/`:sessions`-triggered pane listing past sessions. Only rendered while
/// `AppContext.mode == Mode::SessionList`; owns its own selection cursor and cached row data,
/// refreshed whenever `App` sends `Action::SessionsLoaded` (after querying the database).
#[derive(Debug, Default)]
pub struct SessionListComponent {
    sessions: Vec<(i64, Session)>,
    state: TableState,
    scrolview_state: ScrollViewState,
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

        let mut max_width = 0;

        let rows: Vec<_> = self
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
                let t_str = format!(
                    "{:02}:{:02}:{:02}  ",
                    elapsed / 3600,
                    (elapsed % 3600) / 60,
                    elapsed % 60
                );
                let length = session.topic.len() + t_str.len() + status_text.len();
                if length > max_width {
                    max_width = length;
                }
                Row::new(vec![
                    Cell::new(session.topic.clone()),
                    Cell::new(t_str),
                    Cell::new(Span::styled(status_text, status_style)),
                ])
            })
            .collect();

        let widths = [
            Constraint::Fill(1),
            Constraint::Percentage(15),
            Constraint::Length(10),
        ];

        let table = if rows.is_empty() {
            Table::new(
                vec![Row::new(["No previous sessions yet"])],
                [Constraint::Fill(1)],
            )
        } else {
            Table::new(rows, widths)
        }
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

        let mut scrollview = ScrollView::new(Size::new((max_width + 2) as u16, area.height))
            .vertical_scrollbar_visibility(tui_scrollview::ScrollbarVisibility::Never); // 2 for padding
        scrollview.render_stateful_widget(table, scrollview.area(), &mut self.state);

        let block = Block::bordered()
            .borders(Borders::RIGHT)
            .title(" Previous Sessions ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);
        ratatui::widgets::StatefulWidget::render(scrollview, inner, buf, &mut self.scrolview_state);
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
                    KeyCode::Left | KeyCode::Char('h') => {
                        self.scrolview_state.scroll_left();
                        None
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        self.scrolview_state.scroll_right();
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
