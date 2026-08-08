//! Pure data-bucketing helpers behind the `:history` pane: turning per-day totals fetched from
//! `Database::daily_totals_since` into the daily / weekly / cumulative series the chart renders.
//! Deliberately has no rendering code — `components::history` owns the ratatui side.

use std::collections::{BTreeMap, HashMap};

use time::{Date, Duration};

/// Selects which aggregation the history chart displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryView {
    #[default]
    Daily,
    Weekly,
    Cumulative,
}

impl HistoryView {
    /// Parses the optional `:history` argument. An empty argument selects the daily view so
    /// `:history` and `:history daily` behave identically.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "day" | "daily" => Some(Self::Daily),
            "week" | "weekly" => Some(Self::Weekly),
            "cumulative" => Some(Self::Cumulative),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Cumulative => "Cumulative",
        }
    }
}

/// Today's date in the local timezone, falling back to UTC if the local offset can't be
/// determined — the same fallback used throughout the app (see `Session::new`, `App::is_today`).
pub fn today_local() -> Date {
    time::OffsetDateTime::now_local()
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc())
        .date()
}

/// Formats a `Date` as `YYYY-MM-DD`, matching the format `Database::daily_totals_since` expects
/// for its `since` bound and produces via SQLite's `date(...)` function.
pub fn format_ymd(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

/// Normalizes raw `(date string, seconds)` rows from the database into a gap-free series
/// covering every day from `since` to `today` inclusive, ordered ascending. Rows with
/// unparseable dates are skipped rather than failing the whole pane.
pub fn zero_filled_daily(rows: Vec<(String, i64)>, since: Date, today: Date) -> Vec<(Date, i64)> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let mut by_date: BTreeMap<Date, i64> = BTreeMap::new();
    for (raw_date, secs) in rows {
        if let Ok(date) = Date::parse(&raw_date, &format) {
            by_date.insert(date, secs.max(0));
        }
    }

    let mut series = Vec::new();
    let mut day = since;
    while day <= today {
        series.push((day, by_date.get(&day).copied().unwrap_or(0)));
        day += Duration::days(1);
    }
    series
}

/// Buckets a daily series into Monday-start calendar weeks, keyed by each week's Monday.
pub fn weekly_totals(daily: &[(Date, i64)]) -> Vec<(Date, i64)> {
    let mut by_week: BTreeMap<Date, i64> = BTreeMap::new();
    for (date, secs) in daily {
        let days_from_monday = i64::from(date.weekday().number_days_from_monday());
        let week_start = *date - Duration::days(days_from_monday);
        *by_week.entry(week_start).or_insert(0) += secs;
    }
    by_week.into_iter().collect()
}

/// Running total of a weekly series, ordered ascending by week start.
pub fn cumulative_totals(weekly: &[(Date, i64)]) -> Vec<(Date, i64)> {
    weekly
        .iter()
        .scan(0i64, |sum, (date, secs)| {
            *sum += secs;
            Some((*date, *sum))
        })
        .collect()
}

/// Current and best "at least some study time recorded" streaks, in days, over an ascending
/// zero-filled daily series. Current streak counts backward from the end of `daily` (assumed
/// to end on today) until the first zero day; best streak is the longest such run anywhere in
/// the series. Feeds the heatmap pane's `Streak {cur}d (best {best}d)` stat.
pub fn current_and_best_streak(daily: &[(Date, i64)]) -> (u32, u32) {
    let mut current = 0u32;
    for (_, secs) in daily.iter().rev() {
        if *secs > 0 {
            current += 1;
        } else {
            break;
        }
    }

    let mut best = 0u32;
    let mut run = 0u32;
    for (_, secs) in daily {
        if *secs > 0 {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }

    (current, best)
}

/// Buckets a day's seconds into a 5-step "Less -> More" intensity level (0..=4) relative to
/// `peak`, the busiest day's total in the visible window. `0` is reserved for no recorded
/// time; any positive value gets at least level `1`, with the peak day(s) at level `4`.
pub fn heatmap_level(value: i64, peak: i64) -> u8 {
    if value <= 0 || peak <= 0 {
        return 0;
    }
    let frac = value as f64 / peak as f64;
    ((frac * 4.0).ceil() as u8).clamp(1, 4)
}

/// Buckets an ascending, zero-filled daily series into Sunday-start week columns for the
/// contribution-style heatmap, returning at most the last `max_weeks` columns (oldest first).
/// Each column holds 7 slots for Sunday..Saturday; a slot is `None` when its date falls outside
/// `daily`'s range (i.e. before the fetched window, or after the series' last day).
pub fn heatmap_weeks(daily: &[(Date, i64)], max_weeks: usize) -> Vec<[Option<(Date, i64)>; 7]> {
    let Some(&(today, _)) = daily.last() else {
        return Vec::new();
    };
    let by_date: HashMap<Date, i64> = daily.iter().copied().collect();

    let days_from_sunday = i64::from(today.weekday().number_days_from_sunday());
    let week_end = today + Duration::days(6 - days_from_sunday);

    let max_weeks = max_weeks.max(1);
    let mut weeks = Vec::with_capacity(max_weeks);
    for w in 0..max_weeks {
        let sunday = week_end - Duration::days(7 * w as i64 + 6);
        let mut column: [Option<(Date, i64)>; 7] = [None; 7];
        for (i, slot) in column.iter_mut().enumerate() {
            let date = sunday + Duration::days(i as i64);
            *slot = by_date.get(&date).map(|secs| (date, *secs));
        }
        weeks.push(column);
    }
    weeks.reverse();
    weeks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, time::Month::try_from(month).unwrap(), day).unwrap()
    }

    #[test]
    fn parses_history_view_arguments() {
        assert_eq!(HistoryView::parse(""), Some(HistoryView::Daily));
        assert_eq!(HistoryView::parse("day"), Some(HistoryView::Daily));
        assert_eq!(HistoryView::parse("daily"), Some(HistoryView::Daily));
        assert_eq!(HistoryView::parse("week"), Some(HistoryView::Weekly));
        assert_eq!(HistoryView::parse("weekly"), Some(HistoryView::Weekly));
        assert_eq!(
            HistoryView::parse("cumulative"),
            Some(HistoryView::Cumulative)
        );
        assert_eq!(HistoryView::parse("nonsense"), None);
    }

    #[test]
    fn zero_fills_gaps_between_since_and_today() {
        let rows = vec![("2026-08-06".to_string(), 120)];
        let series = zero_filled_daily(rows, date(2026, 8, 5), date(2026, 8, 8));
        assert_eq!(
            series,
            vec![
                (date(2026, 8, 5), 0),
                (date(2026, 8, 6), 120),
                (date(2026, 8, 7), 0),
                (date(2026, 8, 8), 0),
            ]
        );
    }

    #[test]
    fn zero_filled_daily_skips_unparseable_rows() {
        let rows = vec![
            ("not-a-date".to_string(), 999),
            ("2026-08-05".to_string(), 60),
        ];
        let series = zero_filled_daily(rows, date(2026, 8, 5), date(2026, 8, 5));
        assert_eq!(series, vec![(date(2026, 8, 5), 60)]);
    }

    #[test]
    fn weekly_totals_buckets_by_monday_start_week() {
        // 2026-08-03 is a Monday; 2026-08-09 is the following Sunday.
        let daily = vec![
            (date(2026, 8, 3), 60),
            (date(2026, 8, 9), 30),
            (date(2026, 8, 10), 90), // next week's Monday
        ];
        let weekly = weekly_totals(&daily);
        assert_eq!(
            weekly,
            vec![(date(2026, 8, 3), 90), (date(2026, 8, 10), 90)]
        );
    }

    #[test]
    fn cumulative_totals_runs_a_sum_in_order() {
        let weekly = vec![
            (date(2026, 8, 3), 60),
            (date(2026, 8, 10), 40),
            (date(2026, 8, 17), 0),
        ];
        assert_eq!(
            cumulative_totals(&weekly),
            vec![
                (date(2026, 8, 3), 60),
                (date(2026, 8, 10), 100),
                (date(2026, 8, 17), 100),
            ]
        );
    }

    #[test]
    fn streaks_count_trailing_and_longest_nonzero_runs() {
        let daily = vec![
            (date(2026, 8, 1), 60),
            (date(2026, 8, 2), 60),
            (date(2026, 8, 3), 0),
            (date(2026, 8, 4), 60),
            (date(2026, 8, 5), 60),
            (date(2026, 8, 6), 60),
            (date(2026, 8, 7), 0),
        ];
        assert_eq!(current_and_best_streak(&daily), (0, 3));
    }

    #[test]
    fn current_streak_is_zero_when_today_has_no_time() {
        let daily = vec![(date(2026, 8, 7), 60), (date(2026, 8, 8), 0)];
        assert_eq!(current_and_best_streak(&daily), (0, 1));
    }

    #[test]
    fn current_streak_runs_to_the_end_of_the_series() {
        let daily = vec![
            (date(2026, 8, 6), 0),
            (date(2026, 8, 7), 60),
            (date(2026, 8, 8), 90),
        ];
        assert_eq!(current_and_best_streak(&daily), (2, 2));
    }

    #[test]
    fn heatmap_level_zero_for_non_positive_value_or_peak() {
        assert_eq!(heatmap_level(0, 100), 0);
        assert_eq!(heatmap_level(50, 0), 0);
    }

    #[test]
    fn heatmap_level_scales_between_one_and_four() {
        assert_eq!(heatmap_level(1, 100), 1);
        assert_eq!(heatmap_level(25, 100), 1);
        assert_eq!(heatmap_level(26, 100), 2);
        assert_eq!(heatmap_level(75, 100), 3);
        assert_eq!(heatmap_level(100, 100), 4);
    }

    #[test]
    fn heatmap_weeks_aligns_sunday_start_columns() {
        // 2026-08-08 is a Saturday.
        let daily = vec![(date(2026, 8, 8), 60)];
        let weeks = heatmap_weeks(&daily, 1);
        assert_eq!(weeks.len(), 1);
        assert_eq!(weeks[0][0], None); // Sunday 08-02: outside the fetched window
        assert_eq!(weeks[0][6], Some((date(2026, 8, 8), 60))); // Saturday
    }

    #[test]
    fn heatmap_weeks_blanks_cells_outside_the_fetched_window() {
        let daily = vec![(date(2026, 8, 3), 60), (date(2026, 8, 4), 0)];
        let weeks = heatmap_weeks(&daily, 2);
        assert_eq!(weeks.len(), 2);
        let flat: Vec<Option<(Date, i64)>> = weeks.into_iter().flatten().collect();
        assert!(flat.contains(&Some((date(2026, 8, 3), 60))));
        assert!(flat.contains(&Some((date(2026, 8, 4), 0))));
        assert!(flat.iter().filter(|c| c.is_none()).count() >= 12);
    }

    #[test]
    fn heatmap_weeks_empty_series_yields_no_columns() {
        assert!(heatmap_weeks(&[], 5).is_empty());
    }
}
