use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, LineGauge, Padding, Paragraph, Widget};

use super::{AppContext, Component};
use crate::action::Action;
use crate::media::NowPlayingInfo;

/// Small panel pinned to the top-right corner showing the current track, if any, set apart from
/// the rest of the UI with a lighter background fill instead of a border. Unlike every other
/// component, its state isn't derived from `AppContext` — it's pushed in from outside (see
/// `current`), since playback info comes from a source external to portside's own session/db
/// state.
#[derive(Debug, Default)]
pub struct NowPlayingComponent {
    pub current: Option<NowPlayingInfo>,
}

impl Component for NowPlayingComponent {
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        let block = Block::new()
            .style(Style::new().bg(ctx.theme.panel_bg))
            .padding(Padding {
                left: 2,
                right: 2,
                top: 1,
                bottom: 1,
            });
        let inner = block.inner(area);
        block.render(area, buf);

        let Some(info) = &self.current else {
            Paragraph::new("♪ Nothing playing")
                .alignment(Alignment::Center)
                .dim()
                .render(inner, buf);
            return;
        };

        let [title_area, artist_area, gauge_area, _] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(inner);

        let width = inner.width as usize;
        let glyph = if info.playing { "▶" } else { "⏸" };
        let title_line = Line::from(
            format!("{glyph} {}", truncate(&info.title, width.saturating_sub(2))).bold(),
        );
        let artist_line = Line::from(truncate(&info.artist, width).dim());
        Paragraph::new(title_line).render(title_area, buf);
        Paragraph::new(artist_line).render(artist_area, buf);

        if info.duration > Duration::ZERO {
            // `info.elapsed` is a snapshot as of `info.fetched_at`, not a live value (the OS
            // only reports a fresh position on track/seek/play-pause events) — interpolate
            // forward using wall-clock time so the gauge ticks smoothly between polls.
            let elapsed = if info.playing {
                (info.elapsed + info.fetched_at.elapsed()).min(info.duration)
            } else {
                info.elapsed
            };
            let ratio = (elapsed.as_secs_f64() / info.duration.as_secs_f64()).clamp(0.0, 1.0);
            LineGauge::default()
                .ratio(ratio)
                .label(format!(
                    "{}/{}",
                    format_mmss(elapsed),
                    format_mmss(info.duration)
                ))
                .filled_style(Style::new().fg(ctx.theme.gauge_filled))
                .unfilled_style(Style::new().fg(ctx.theme.gauge_unfilled))
                .render(gauge_area, buf);
        }
    }

    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        if let Action::NowPlayingUpdated(info) = action {
            self.current = info.clone();
        }
        None
    }
}

fn format_mmss(d: Duration) -> String {
    let total = d.as_secs();
    format!("{}:{:02}", total / 60, total % 60)
}

/// Truncates to at most `max_chars`, appending an ellipsis when text was cut, since track/artist
/// names routinely overflow this box's narrow fixed width.
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_leaves_short_text_untouched() {
        assert_eq!(truncate("short", 20), "short");
    }

    #[test]
    fn truncate_ellipsizes_long_text() {
        assert_eq!(truncate("a very long song title indeed", 10), "a very lo…");
    }
}
