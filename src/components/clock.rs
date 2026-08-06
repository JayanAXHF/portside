use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Padding, Widget};
use time::format_description::FormatItem;
use time::macros::format_description;
use tui_big_text::{BigText, PixelSize};

use super::{AppContext, Component};

const TIME_FORMAT: &[FormatItem<'_>] = format_description!("[hour]:[minute]:[second]");

/// Live wall-clock readout pinned to the top-left corner, rendered with `tui-big-text`'s
/// `Octant` pixel size so it reads larger than a plain `Paragraph` but stays clearly smaller
/// than the session timer's `Full`-size digits in `TimerComponent`.
#[derive(Debug, Default)]
pub struct ClockComponent;

impl Component for ClockComponent {
    fn render(&mut self, area: Rect, buf: &mut Buffer, _ctx: &AppContext) {
        let now =
            time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        let text = now
            .format(TIME_FORMAT)
            .unwrap_or_else(|_| "--:--:--".to_string());

        BigText::builder()
            .pixel_size(PixelSize::Octant)
            .lines(vec![Line::from(text)])
            .style(Style::new().dim())
            .block(Block::new().padding(Padding::horizontal(2)))
            .alignment(Alignment::Left)
            .build()
            .render(area, buf);
    }
}
