use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, Block, Borders, Clear, Widget};
use time::Date;

use crate::action::Action;
use crate::history::{self, HistoryView};
use crate::theme::Theme;

use super::{AppContext, Component};

/// The `:history`-triggered bottom pane charting study time. Only rendered while
/// `AppContext.mode == Mode::History`; owns the loaded daily series and the active
/// Daily/Weekly/Cumulative view, refreshed via `Action::HistoryLoaded` after `App` queries the
/// database. Switching views (`d`/`w`/`c`) is pure presentation state, handled entirely here.
#[derive(Debug, Default)]
pub struct HistoryComponent {
    daily: Vec<(Date, i64)>,
    view: HistoryView,
}

impl HistoryComponent {
    pub fn set_view(&mut self, view: HistoryView) {
        self.view = view;
    }

    /// The loaded daily series with today's bucket replaced by `ctx.today_total`, so the pane
    /// reflects the live/running session instead of the DB-only value (which is only updated on
    /// session state transitions — see `App::today_total`).
    fn adjusted_daily(&self, ctx: &AppContext) -> Vec<(Date, i64)> {
        let today = history::today_local();
        self.daily
            .iter()
            .map(|(date, secs)| {
                if *date == today {
                    (*date, ctx.today_total.as_secs() as i64)
                } else {
                    (*date, *secs)
                }
            })
            .collect()
    }
}

impl Component for HistoryComponent {
    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &AppContext) {
        Clear.render(area, buf);

        let block = Block::bordered()
            .borders(Borders::TOP)
            .title(format!(" History — {} ", self.view.label()))
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let daily = self.adjusted_daily(ctx);

        if daily.iter().all(|(_, secs)| *secs == 0) {
            Line::from(" No study time recorded yet".dim()).render(inner, buf);
            return;
        }

        match self.view {
            HistoryView::Daily => render_heatmap(&daily, inner, buf, ctx.theme),
            HistoryView::Weekly => {
                render_bar_chart(&history::weekly_totals(&daily), inner, buf, ctx.theme)
            }
            HistoryView::Cumulative => render_bar_chart(
                &history::cumulative_totals(&history::weekly_totals(&daily)),
                inner,
                buf,
                ctx.theme,
            ),
        }
    }

    fn handle_action(&mut self, action: &Action) -> Option<Action> {
        match action {
            Action::HistoryLoaded(entries) => {
                self.daily = entries.clone();
                None
            }
            Action::Key(key) => {
                match key.code {
                    KeyCode::Char('d') => self.view = HistoryView::Daily,
                    KeyCode::Char('w') => self.view = HistoryView::Weekly,
                    KeyCode::Char('c') => self.view = HistoryView::Cumulative,
                    _ => {}
                }
                None
            }
            _ => None,
        }
    }
}

/// Weekly/Cumulative rendering — unchanged bar chart, unified into one bar width/gap now that
/// Daily has moved to the heatmap below.
fn render_bar_chart(series: &[(Date, i64)], inner: Rect, buf: &mut Buffer, theme: &Theme) {
    const BAR_WIDTH: u16 = 4;
    const BAR_GAP: u16 = 1;

    let column_width = BAR_WIDTH + BAR_GAP;
    let capacity = usize::from((inner.width / column_width).max(1));
    let shown = &series[series.len().saturating_sub(capacity)..];

    let bars: Vec<Bar> = shown
        .iter()
        .map(|(date, secs)| {
            let minutes = (*secs / 60).max(0) as u64;
            Bar::default()
                .value(minutes)
                .style(Style::new().fg(theme.accent))
                .label(Line::from(format!("{:02}", date.day())))
        })
        .collect();

    let peak = shown.iter().map(|(_, s)| *s).max().unwrap_or(0);
    let total: i64 = shown.iter().map(|(_, s)| *s).sum();

    let [stats_area, chart_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    Line::from(vec![
        Span::styled(format!(" Total {}", format_minutes(total)), theme.dim()),
        Span::styled("  ·  ", theme.dim()),
        Span::styled(format!("Peak {}", format_minutes(peak)), theme.dim()),
    ])
    .render(stats_area, buf);

    BarChart::new(bars)
        .bar_width(BAR_WIDTH)
        .bar_gap(BAR_GAP)
        .render(chart_area, buf);
}

/// Left-gutter width reserved for the two-letter weekday labels ("Su", "Mo", ...) plus a
/// trailing space before the first week column.
const HEATMAP_GUTTER: u16 = 3;
const WEEKDAY_LABELS: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

fn month_abbrev(month: time::Month) -> &'static str {
    use time::Month::*;
    match month {
        January => "Jan",
        February => "Feb",
        March => "Mar",
        April => "Apr",
        May => "May",
        June => "Jun",
        July => "Jul",
        August => "Aug",
        September => "Sep",
        October => "Oct",
        November => "Nov",
        December => "Dec",
    }
}

/// Daily view: a GitHub-style contribution heatmap. `daily` is the adjusted, zero-filled
/// ascending series for the full fetched lookback window (see `HISTORY_LOOKBACK_DAYS`);
/// `history::heatmap_weeks` buckets it into Sunday-start week columns, clipped to whatever fits
/// `inner`'s width, and each cell is shaded via `heatmap_color`/`history::heatmap_level` scaled
/// against the peak day across the *whole* fetched window (not just the visible columns), so
/// the legend stays consistent with the reported Peak stat even when narrower terminals clip
/// off older weeks.
fn render_heatmap(daily: &[(Date, i64)], area: Rect, buf: &mut Buffer, theme: &Theme) {
    let peak = daily.iter().map(|(_, s)| *s).max().unwrap_or(0);
    let total: i64 = daily.iter().map(|(_, s)| *s).sum();
    let (current, best) = history::current_and_best_streak(daily);

    let [stats_area, month_area, grid_area, legend_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(7),
        Constraint::Length(1),
    ])
    .areas(area);

    Line::from(vec![
        Span::styled(format!(" Total {}", format_minutes(total)), theme.dim()),
        Span::styled("  ·  ", theme.dim()),
        Span::styled(format!("Peak {}", format_minutes(peak)), theme.dim()),
        Span::styled("  ·  ", theme.dim()),
        Span::styled(format!("Streak {current}d (best {best}d)"), theme.dim()),
    ])
    .render(stats_area, buf);

    if grid_area.width <= HEATMAP_GUTTER || grid_area.height == 0 {
        return;
    }

    // Cap columns to what the fetched window can actually fill (+1 for the partial boundary
    // weeks at each end) so a terminal wider than ~12 months of weeks doesn't request columns
    // that would only ever render blank.
    let capacity = usize::from((grid_area.width - HEATMAP_GUTTER) / 2).max(1);
    let data_weeks = daily.len().div_ceil(7) + 1;
    let max_weeks = capacity.min(data_weeks);
    let weeks = history::heatmap_weeks(daily, max_weeks);

    // Center the weekday-label + week-column block horizontally in the pane, rather than
    // hugging the left edge, since it's almost always narrower than the full pane width.
    let grid_width = HEATMAP_GUTTER + (weeks.len() as u16) * 2;
    let base_x = grid_area.x + (grid_area.width.saturating_sub(grid_width)) / 2;

    for (row, label) in WEEKDAY_LABELS.iter().enumerate() {
        buf.set_string(base_x, grid_area.y + row as u16, label, theme.dim());
    }

    let mut last_month = None;
    for (col, week) in weeks.iter().enumerate() {
        let x = base_x + HEATMAP_GUTTER + (col as u16) * 2;
        if x >= grid_area.x + grid_area.width {
            break;
        }

        if let Some(first_of_month) = week.iter().flatten().find(|(date, _)| date.day() == 1) {
            let month = first_of_month.0.month();
            if last_month != Some(month) {
                last_month = Some(month);
                buf.set_string(x, month_area.y, month_abbrev(month), theme.dim());
            }
        }

        for (row, cell) in week.iter().enumerate() {
            let y = grid_area.y + row as u16;
            match cell {
                Some((_, secs)) => {
                    let level = history::heatmap_level(*secs, peak);
                    buf.set_string(x, y, "■", Style::new().fg(theme.heatmap_color(level)));
                }
                None => buf.set_string(x, y, " ", Style::default()),
            }
        }
    }

    let mut legend = vec![Span::styled(" Less ", theme.dim())];
    for level in 0..=4u8 {
        legend.push(Span::styled(
            "■",
            Style::new().fg(theme.heatmap_color(level)),
        ));
    }
    legend.push(Span::styled(" More", theme.dim()));
    Line::from(legend)
        .alignment(Alignment::Center)
        .render(legend_area, buf);
}

fn format_minutes(total_secs: i64) -> String {
    let total_min = total_secs.max(0) / 60;
    let (h, m) = (total_min / 60, total_min % 60);
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}
