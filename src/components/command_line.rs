use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::action::{Action, Mode};
use crate::fixed_queue::FixedQueue;

use super::{AppContext, Component};

/// The neovim-style `:command` input line. Only accepts key events while `App` is in
/// `Mode::CommandLine` (App routes keys to it explicitly rather than broadcasting); always
/// renders its row so the bottom of the screen has a stable layout whether or not it's active.
#[derive(Debug, Default)]
pub struct CommandLineComponent {
    /// Command History
    history: FixedQueue<String, 500>,
    last_key_was_history: bool,
    history_index: usize,
    buffer: String,
    /// Char index into `buffer`, not byte index (so it survives multi-byte input safely).
    cursor: usize,
}

impl CommandLineComponent {
    fn reset(&mut self) {
        self.buffer.clear();
        self.history_index = 0;
        self.last_key_was_history = false;
        self.cursor = 0;
    }

    fn byte_index(&self) -> usize {
        self.buffer
            .char_indices()
            .nth(self.cursor)
            .map_or(self.buffer.len(), |(i, _)| i)
    }

    fn insert_char(&mut self, c: char) {
        let idx = self.byte_index();
        self.buffer.insert(idx, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let idx = self.byte_index();
        self.buffer.remove(idx);
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.buffer.chars().count());
    }
}

impl Component for CommandLineComponent {
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let line = if ctx.mode == Mode::CommandLine {
            Line::from(format!(":{}", self.buffer))
        } else {
            Line::from(
                "Press : for commands (resume-previous, pause, resume, break, complete, sessions, theme, quit)",
            )
            .dim()
        };
        Paragraph::new(line).render(area, buf);
    }

    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::EnterCommandMode => {
                self.reset();
                None
            }
            Action::Key(key) => {
                use crossterm::event::KeyCode;
                match key.code {
                    KeyCode::Char(c) => {
                        self.last_key_was_history = false;
                        self.insert_char(c);
                        None
                    }
                    KeyCode::Backspace => {
                        self.last_key_was_history = false;
                        self.backspace();
                        None
                    }
                    KeyCode::Left => {
                        self.move_left();
                        None
                    }
                    KeyCode::Right => {
                        self.move_right();
                        None
                    }
                    KeyCode::Enter => {
                        self.last_key_was_history = false;
                        let text = self.buffer.trim_start_matches(":").to_string();
                        self.history.push(text.clone());
                        self.reset();
                        Some(Action::SubmitCommand(text))
                    }
                    KeyCode::Up => {
                        if self.history.is_empty() {
                            return None;
                        }
                        if self.buffer.is_empty() || self.last_key_was_history {
                            self.last_key_was_history = true;
                            self.history_index =
                                (self.history_index + 1).min(self.history.len());
                            self.buffer =
                                self.history[self.history.len() - self.history_index].clone();
                            self.cursor = self.buffer.chars().count();
                        }
                        None
                    }
                    KeyCode::Down => {
                        if self.last_key_was_history {
                            self.history_index = self.history_index.saturating_sub(1);
                            if self.history_index == 0 {
                                self.buffer.clear();
                                self.last_key_was_history = false;
                            } else {
                                self.buffer = self.history
                                    [self.history.len() - self.history_index]
                                    .clone();
                            }
                            self.cursor = self.buffer.chars().count();
                        }
                        None
                    }
                    KeyCode::Esc => {
                        self.reset();
                        Some(Action::ExitCommandMode)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn cursor(&self) -> Option<(u16, u16)> {
        // Position is finalized by `App` once it knows this component's render area; see
        // `App::draw`, which offsets this by the command line row's `(x, y)`.
        Some((self.cursor as u16 + 1, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace_track_cursor() {
        let mut c = CommandLineComponent::default();
        c.insert_char('p');
        c.insert_char('a');
        c.insert_char('u');
        assert_eq!(c.buffer, "pau");
        assert_eq!(c.cursor, 3);
        c.backspace();
        assert_eq!(c.buffer, "pa");
        assert_eq!(c.cursor, 2);
    }

    #[test]
    fn insert_in_middle_respects_cursor_position() {
        let mut c = CommandLineComponent::default();
        for ch in "pase".chars() {
            c.insert_char(ch);
        }
        c.move_left();
        c.move_left();
        c.insert_char('u');
        assert_eq!(c.buffer, "pause");
    }

    #[test]
    fn enter_emits_submit_and_resets() {
        let mut c = CommandLineComponent::default();
        for ch in "pause".chars() {
            c.insert_char(ch);
        }
        let action = c.handle_action(&Action::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )));
        assert!(matches!(action, Some(Action::SubmitCommand(s)) if s == "pause"));
        assert_eq!(c.buffer, "");
        assert_eq!(c.cursor, 0);
    }
}
