use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect, Size};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Widget};
use ratatui_textarea::{DataCursor, TextArea};
use tui_scrollview::{ScrollView, ScrollViewState};

use crate::action::Action;
use crate::db::{Session, SessionStatus};
use crate::session_filter;

use super::{AppContext, Component};

/// Sub-view owned by `SessionListComponent`, toggled entirely by local keybindings — mirrors
/// `HistoryView` (`src/history.rs`) in not involving `App`-level `Mode` routing at all.
#[derive(Debug, Default)]
enum SessionListView {
    #[default]
    List,
    /// Read-only detail view of the selected row, entered via `i`.
    Detail,
    /// Editing the selected row's description, entered via `d` from `Detail`.
    EditDescription(TextArea<'static>),
    /// Editing the selected row's tags (comma-separated, single line), entered via `t` from
    /// `Detail`.
    EditTags(TextArea<'static>),
}

fn format_hhmmss(secs: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

/// The `Tab`/`:sessions`-triggered pane listing past sessions. Only rendered while
/// `AppContext.mode == Mode::SessionList`; owns its own selection cursor and cached row data,
/// refreshed whenever `App` sends `Action::SessionsLoaded` (after querying the database).
#[derive(Debug, Default)]
pub struct SessionListComponent {
    sessions: Vec<(i64, Session)>,
    state: TableState,
    scrolview_state: ScrollViewState,
    view: SessionListView,
    /// Row offsets (within the last-rendered detail area) of the description/tags content line,
    /// cached during `render` so `cursor()` can place the terminal cursor without re-deriving the
    /// detail layout. Only meaningful while `view` is an `Edit*` variant.
    description_row: u16,
    tags_row: u16,
    /// The `/`-triggered search box; always allocated, holds the current filter text whether or
    /// not it's currently focused. Single-line only — `Enter`/`Esc` unfocus it rather than being
    /// forwarded as input.
    search: TextArea<'static>,
    /// Whether the search box currently owns raw key input. Only meaningful while `view ==
    /// List` — `/` has no effect from `Detail`/`Edit*`.
    searching: bool,
    /// Indices into `sessions` matching `search`'s current text (see `session_filter::matches`),
    /// recomputed by `recompute_filtered` whenever `sessions` or the search text changes.
    /// `state.selected()` indexes into this, not into `sessions` directly.
    filtered: Vec<usize>,
}

impl SessionListComponent {
    /// The database id of the currently highlighted row, if any.
    pub fn selected_id(&self) -> Option<i64> {
        self.state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .and_then(|&idx| self.sessions.get(idx))
            .map(|(id, _)| *id)
    }

    fn selected_session(&self) -> Option<&Session> {
        self.state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .and_then(|&idx| self.sessions.get(idx))
            .map(|(_, session)| session)
    }

    fn select_next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let next = match self.state.selected() {
            Some(i) => (i + 1) % self.filtered.len(),
            None => 0,
        };
        self.state.select(Some(next));
    }

    fn select_previous(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let prev = match self.state.selected() {
            Some(0) | None => self.filtered.len() - 1,
            Some(i) => i - 1,
        };
        self.state.select(Some(prev));
    }

    /// Recomputes `filtered` from `sessions` and the search box's current text, then tries to
    /// keep the previously-selected session selected (by id) in the new filtered list, falling
    /// back to the first row, or no selection if the filtered list is empty. Called whenever
    /// `sessions` changes (`Action::SessionsLoaded`) or the search text changes (every keystroke
    /// while `searching`).
    fn recompute_filtered(&mut self) {
        let query = self.search.lines()[0].clone();
        let preferred_id = self.selected_id();

        self.filtered = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, (_, session))| session_filter::matches(session, &query))
            .map(|(i, _)| i)
            .collect();

        let new_index = preferred_id
            .and_then(|id| {
                self.filtered
                    .iter()
                    .position(|&idx| self.sessions[idx].0 == id)
            })
            .or(if self.filtered.is_empty() {
                None
            } else {
                Some(0)
            });
        self.state.select(new_index);
    }

    fn render_search_bar(&self, area: Rect, buf: &mut Buffer) {
        if self.searching || !self.search.lines()[0].is_empty() {
            self.search.render(area, buf);
        } else {
            Paragraph::new(Line::from("/ to search").dim()).render(area, buf);
        }
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let mut max_width = 0;
        let mut max_tstr_len = 0;

        let rows: Vec<_> = self
            .filtered
            .iter()
            .map(|&i| &self.sessions[i].1)
            .map(|session| {
                let status_style = ctx.theme.status_style(session.status);
                let status_text = match session.status {
                    SessionStatus::Running => "running",
                    SessionStatus::Paused => "paused",
                    SessionStatus::OnBreak => "on break",
                    SessionStatus::Completed => "completed",
                };
                let t_str = format!("{}  ", format_hhmmss(session.live_elapsed().as_secs()));
                if t_str.len() > max_tstr_len {
                    max_tstr_len = t_str.len();
                }
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
            Constraint::Length(max_tstr_len as u16),
            Constraint::Length(10),
        ];

        let table = if rows.is_empty() {
            let message = if self.sessions.is_empty() {
                "No previous sessions yet"
            } else {
                "No sessions match your search"
            };
            Table::new(vec![Row::new([message])], [Constraint::Fill(1)])
        } else {
            Table::new(rows, widths)
        }
        .row_highlight_style(ctx.theme.highlight())
        .highlight_symbol("> ");

        let mut scrollview = ScrollView::new(Size::new((max_width + 2) as u16, area.height))
            .vertical_scrollbar_visibility(tui_scrollview::ScrollbarVisibility::Never); // 2 for padding
        scrollview.render_stateful_widget(table, scrollview.area(), &mut self.state);
        ratatui::widgets::StatefulWidget::render(scrollview, area, buf, &mut self.scrolview_state);
    }

    /// Renders the read-only detail view, or the same layout with the description/tags line
    /// replaced by a live `TextArea` while editing. Caches `description_row`/`tags_row` so
    /// `cursor()` can position the terminal cursor over whichever field is being edited.
    fn render_detail(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let Some(session) = self.selected_session() else {
            Paragraph::new("No session selected").render(area, buf);
            return;
        };

        let editing_description = matches!(self.view, SessionListView::EditDescription(_));
        let editing_tags = matches!(self.view, SessionListView::EditTags(_));

        let status_text = match session.status {
            SessionStatus::Running => "running",
            SessionStatus::Paused => "paused",
            SessionStatus::OnBreak => "on break",
            SessionStatus::Completed => "completed",
        };
        let elapsed = format_hhmmss(session.live_elapsed().as_secs());

        let mut lines = vec![
            Line::from(format!("Topic: {}", session.topic)),
            Line::from(vec![
                Span::raw("Status: "),
                Span::styled(status_text, ctx.theme.status_style(session.status)),
                Span::raw(format!("   Elapsed: {elapsed}")),
            ]),
            Line::from(format!("Started: {}", session.started_at)),
            Line::from(""),
            Line::from("Description:"),
        ];

        let description_row = lines.len() as u16;
        if editing_description {
            lines.push(Line::from(""));
        } else {
            match &session.description {
                Some(d) => lines.push(Line::from(d.clone())),
                None => lines.push(Line::from("No description").dim()),
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from("Tags:"));
        let tags_row = lines.len() as u16;
        if editing_tags {
            lines.push(Line::from(""));
        } else if session.tags.is_empty() {
            lines.push(Line::from("No tags").dim());
        } else {
            lines.push(Line::from(session.tags.join(", ")));
        }

        lines.push(Line::from(""));
        let hint = if editing_description || editing_tags {
            "Enter: save   Esc: cancel"
        } else {
            "d: edit description   t: edit tags   Esc: back"
        };
        lines.push(Line::from(hint).dim());

        Paragraph::new(lines).render(area, buf);
        self.description_row = description_row;
        self.tags_row = tags_row;

        match &self.view {
            SessionListView::EditDescription(ta) => {
                let row_area = Rect {
                    y: area.y + description_row,
                    height: 1,
                    ..area
                };
                ta.render(row_area, buf);
            }
            SessionListView::EditTags(ta) => {
                let row_area = Rect {
                    y: area.y + tags_row,
                    height: 1,
                    ..area
                };
                ta.render(row_area, buf);
            }
            _ => {}
        }
    }
}

impl Component for SessionListComponent {
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        Clear.render(area, buf);

        let title = if matches!(self.view, SessionListView::List) {
            " Previous Sessions "
        } else {
            " Session Detail "
        };
        let block = Block::bordered()
            .borders(Borders::RIGHT)
            .title(title)
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        if matches!(self.view, SessionListView::List) {
            let [search_area, list_area] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
            self.render_search_bar(search_area, buf);
            self.render_list(list_area, buf, ctx);
        } else {
            self.render_detail(inner, buf, ctx);
        }
    }

    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::SessionsLoaded(sessions) => {
                self.sessions = sessions.clone();
                self.recompute_filtered();
                None
            }
            Action::Key(key) => {
                use crossterm::event::KeyCode;

                if matches!(self.view, SessionListView::EditDescription(_)) {
                    let SessionListView::EditDescription(mut ta) = std::mem::take(&mut self.view)
                    else {
                        unreachable!()
                    };
                    let id = self.selected_id();
                    return match key.code {
                        KeyCode::Enter => {
                            let text = ta.lines().join("\n");
                            self.view = SessionListView::Detail;
                            Some(Action::SetSessionDescription { id, text })
                        }
                        KeyCode::Esc => {
                            self.view = SessionListView::Detail;
                            None
                        }
                        _ => {
                            ta.input(*key);
                            self.view = SessionListView::EditDescription(ta);
                            None
                        }
                    };
                }

                if matches!(self.view, SessionListView::EditTags(_)) {
                    let SessionListView::EditTags(mut ta) = std::mem::take(&mut self.view) else {
                        unreachable!()
                    };
                    let id = self.selected_id();
                    return match key.code {
                        KeyCode::Enter => {
                            let tags = ta.lines()[0]
                                .split(',')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_string)
                                .collect();
                            self.view = SessionListView::Detail;
                            Some(Action::SetSessionTags { id, tags })
                        }
                        KeyCode::Esc => {
                            self.view = SessionListView::Detail;
                            None
                        }
                        _ => {
                            ta.input(*key);
                            self.view = SessionListView::EditTags(ta);
                            None
                        }
                    };
                }

                if matches!(self.view, SessionListView::Detail) {
                    return match key.code {
                        KeyCode::Esc => {
                            self.view = SessionListView::List;
                            None
                        }
                        KeyCode::Char('d') => {
                            let text = self
                                .selected_session()
                                .and_then(|s| s.description.clone())
                                .unwrap_or_default();
                            self.view = SessionListView::EditDescription(TextArea::new(vec![text]));
                            None
                        }
                        KeyCode::Char('t') => {
                            let text = self
                                .selected_session()
                                .map(|s| s.tags.join(", "))
                                .unwrap_or_default();
                            self.view = SessionListView::EditTags(TextArea::new(vec![text]));
                            None
                        }
                        _ => None,
                    };
                }

                // SessionListView::List
                if self.searching {
                    return match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            self.searching = false;
                            None
                        }
                        _ => {
                            self.search.input(*key);
                            self.recompute_filtered();
                            None
                        }
                    };
                }

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
                    KeyCode::Char('i') => {
                        if self.selected_id().is_some() {
                            self.view = SessionListView::Detail;
                        }
                        None
                    }
                    KeyCode::Char('/') => {
                        self.searching = true;
                        None
                    }
                    KeyCode::Esc => Some(Action::CloseSessionList),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn cursor(&self) -> Option<(u16, u16)> {
        if self.searching {
            let DataCursor(_, col) = self.search.cursor();
            return Some((col as u16, 0));
        }
        match &self.view {
            SessionListView::EditDescription(ta) => {
                let DataCursor(_, col) = ta.cursor();
                Some((col as u16, self.description_row))
            }
            SessionListView::EditTags(ta) => {
                let DataCursor(_, col) = ta.cursor();
                Some((col as u16, self.tags_row))
            }
            _ => None,
        }
    }
}
