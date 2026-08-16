use std::time::Duration;

use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime};

use crate::errors::{AppError, Result};
use crate::history::{self, HistoryView};

/// A parsed neovim-style `:command` line. `CommandLine` strips the leading `:` before this is
/// called; the raw text after that is whitespace-split into a verb and the rest as arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Topic(String),
    Pause,
    Resume,
    ToggleBreak(Option<Duration>),
    ResumePrevious(Option<i64>),
    Sessions,
    History(Option<HistoryView>),
    Complete,
    Quit,
    Discord(bool),
    Theme(String),
    Add {
        topic: String,
        start: OffsetDateTime,
        duration: Duration,
    },
    Remove(i64),
}

/// Parses a duration argument: `1h`/`5m`/`30s` (optionally combined, e.g. `1h30m`), or a bare
/// number (treated as minutes). Deliberately minimal — this is a personal productivity tool, not
/// a general duration parser.
fn parse_duration(s: &str) -> Result<Duration> {
    let invalid = || AppError::InvalidCommand(format!("invalid duration: {s}"));
    let s = s.trim();
    if s.is_empty() {
        return Err(invalid());
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return s
            .parse::<u64>()
            .map(|m| Duration::from_secs(m * 60))
            .map_err(|_| invalid());
    }

    let mut total_secs: u64 = 0;
    let mut num = String::new();
    let mut saw_unit = false;
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
            continue;
        }
        let n: u64 = num.parse().map_err(|_| invalid())?;
        num.clear();
        let unit_secs = match ch {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return Err(invalid()),
        };
        total_secs += n * unit_secs;
        saw_unit = true;
    }
    if !num.is_empty() || !saw_unit {
        return Err(invalid());
    }
    Ok(Duration::from_secs(total_secs))
}

/// The local UTC offset, falling back to UTC if it can't be determined — the same fallback used
/// throughout the app (see `history::today_local`).
fn local_offset() -> time::UtcOffset {
    OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .offset()
}

/// Parses an `:add` start-time argument: either `YYYY-MM-DD HH:MM`, or bare `HH:MM` meaning
/// today. No relative keywords ("yesterday") — deliberately minimal, matching `parse_duration`.
fn parse_start_time(s: &str) -> Result<OffsetDateTime> {
    let s = s.trim();
    let invalid = || AppError::InvalidCommand(format!("invalid start time: {s}"));

    let full = format_description!("[year]-[month]-[day] [hour]:[minute]");
    if let Ok(dt) = PrimitiveDateTime::parse(s, &full) {
        return Ok(dt.assume_offset(local_offset()));
    }

    let time_only = format_description!("[hour]:[minute]");
    let time = time::Time::parse(s, &time_only).map_err(|_| invalid())?;
    Ok(PrimitiveDateTime::new(history::today_local(), time).assume_offset(local_offset()))
}

/// Splits `s` into whitespace-separated tokens, treating a `"..."` span as a single token with
/// the quotes stripped. Only used by `add`, the first command needing more than one positional
/// argument where an argument can itself contain spaces.
fn tokenize(s: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&next) = chars.peek() else {
            break;
        };
        let mut token = String::new();
        if next == '"' {
            chars.next();
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '"' {
                    closed = true;
                    break;
                }
                token.push(c);
            }
            if !closed {
                return Err(AppError::InvalidCommand("unterminated quote".to_string()));
            }
        } else {
            while chars.peek().is_some_and(|c| !c.is_whitespace()) {
                token.push(chars.next().unwrap());
            }
        }
        tokens.push(token);
    }
    Ok(tokens)
}

pub fn parse(input: &str) -> Result<Command> {
    let input = input.trim();
    let (verb, rest) = match input.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest.trim()),
        None => (input, ""),
    };

    match verb {
        "topic" if !rest.is_empty() => Ok(Command::Topic(rest.to_string())),
        "topic" => Err(AppError::InvalidCommand("usage: topic <name>".to_string())),
        "pause" => Ok(Command::Pause),
        "resume" | "play" => Ok(Command::Resume),
        "break" if rest.is_empty() => Ok(Command::ToggleBreak(None)),
        "break" => parse_duration(rest).map(|d| Command::ToggleBreak(Some(d))),
        "resume-previous" if rest.is_empty() => Ok(Command::ResumePrevious(None)),
        "resume-previous" => rest
            .parse::<i64>()
            .map(|id| Command::ResumePrevious(Some(id)))
            .map_err(|_| AppError::InvalidCommand(format!("invalid session id: {rest}"))),
        "sessions" => Ok(Command::Sessions),
        "history" | "hist" if rest.is_empty() => Ok(Command::History(None)),
        "history" | "hist" => HistoryView::parse(rest)
            .map(|view| Command::History(Some(view)))
            .ok_or_else(|| AppError::InvalidCommand(format!("invalid history view: {rest}"))),
        "complete" | "end" | "done" => Ok(Command::Complete),
        "quit" | "q" => Ok(Command::Quit),
        "discord" if rest == "on" => Ok(Command::Discord(true)),
        "discord" if rest == "off" => Ok(Command::Discord(false)),
        "discord" => Err(AppError::InvalidCommand(
            "usage: discord <on|off>".to_string(),
        )),
        "theme" if rest.is_empty() => {
            Err(AppError::InvalidCommand("usage: theme <name>".to_string()))
        }
        "theme" => Ok(Command::Theme(rest.to_string())),
        "add" => {
            let tokens = tokenize(rest)?;
            match tokens.as_slice() {
                [topic, start, duration] => Ok(Command::Add {
                    topic: topic.clone(),
                    start: parse_start_time(start)?,
                    duration: parse_duration(duration)?,
                }),
                _ => Err(AppError::InvalidCommand(
                    "usage: add <topic> <start> <duration>".to_string(),
                )),
            }
        }
        "remove" | "rm" if rest.is_empty() => {
            Err(AppError::InvalidCommand("usage: remove <id>".to_string()))
        }
        "remove" | "rm" => rest
            .parse::<i64>()
            .map(Command::Remove)
            .map_err(|_| AppError::InvalidCommand(format!("invalid session id: {rest}"))),
        "" => Err(AppError::InvalidCommand("empty command".to_string())),
        other => Err(AppError::InvalidCommand(format!(
            "unknown command: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_topic_with_multi_word_name() {
        assert_eq!(
            parse("topic linear algebra").unwrap(),
            Command::Topic("linear algebra".to_string())
        );
    }

    #[test]
    fn topic_without_name_is_invalid() {
        assert!(parse("topic").is_err());
        assert!(parse("topic   ").is_err());
    }

    #[test]
    fn parses_simple_verbs() {
        assert_eq!(parse("pause").unwrap(), Command::Pause);
        assert_eq!(parse("resume").unwrap(), Command::Resume);
        assert_eq!(parse("play").unwrap(), Command::Resume);
        assert_eq!(parse("break").unwrap(), Command::ToggleBreak(None));
        assert_eq!(parse("sessions").unwrap(), Command::Sessions);
        assert_eq!(parse("complete").unwrap(), Command::Complete);
        assert_eq!(parse("end").unwrap(), Command::Complete);
        assert_eq!(parse("done").unwrap(), Command::Complete);
        assert_eq!(parse("quit").unwrap(), Command::Quit);
        assert_eq!(parse("q").unwrap(), Command::Quit);
    }

    #[test]
    fn parses_resume_previous_with_and_without_id() {
        assert_eq!(
            parse("resume-previous").unwrap(),
            Command::ResumePrevious(None)
        );
        assert_eq!(
            parse("resume-previous 42").unwrap(),
            Command::ResumePrevious(Some(42))
        );
        assert!(parse("resume-previous abc").is_err());
    }

    #[test]
    fn parses_timed_break_durations() {
        assert_eq!(
            parse("break 5m").unwrap(),
            Command::ToggleBreak(Some(Duration::from_secs(300)))
        );
        assert_eq!(
            parse("break 30s").unwrap(),
            Command::ToggleBreak(Some(Duration::from_secs(30)))
        );
        assert_eq!(
            parse("break 5").unwrap(),
            Command::ToggleBreak(Some(Duration::from_secs(300)))
        );
        assert_eq!(
            parse("break 1h").unwrap(),
            Command::ToggleBreak(Some(Duration::from_secs(3600)))
        );
        assert_eq!(
            parse("break 1h30m").unwrap(),
            Command::ToggleBreak(Some(Duration::from_secs(5400)))
        );
        assert!(parse("break abc").is_err());
    }

    #[test]
    fn parses_history_command_with_and_without_view() {
        assert_eq!(parse("history").unwrap(), Command::History(None));
        assert_eq!(parse("hist").unwrap(), Command::History(None));
        assert_eq!(
            parse("history daily").unwrap(),
            Command::History(Some(HistoryView::Daily))
        );
        assert_eq!(
            parse("history weekly").unwrap(),
            Command::History(Some(HistoryView::Weekly))
        );
        assert_eq!(
            parse("history cumulative").unwrap(),
            Command::History(Some(HistoryView::Cumulative))
        );
        assert!(parse("history nonsense").is_err());
    }

    #[test]
    fn parses_discord_toggle() {
        assert_eq!(parse("discord on").unwrap(), Command::Discord(true));
        assert_eq!(parse("discord off").unwrap(), Command::Discord(false));
        assert!(parse("discord").is_err());
        assert!(parse("discord maybe").is_err());
    }

    #[test]
    fn parses_theme_command() {
        assert_eq!(
            parse("theme solarized").unwrap(),
            Command::Theme("solarized".to_string())
        );
        assert!(parse("theme").is_err());
        assert!(parse("theme   ").is_err());
    }

    #[test]
    fn parses_add_command_with_bare_word_args() {
        let cmd = parse("add writing 09:00 45m").unwrap();
        assert_eq!(
            cmd,
            Command::Add {
                topic: "writing".to_string(),
                start: parse_start_time("09:00").unwrap(),
                duration: Duration::from_secs(45 * 60),
            }
        );
    }

    #[test]
    fn parses_add_command_with_quoted_args() {
        let cmd = parse(r#"add "deep work" "2026-08-15 09:00" 1h"#).unwrap();
        assert_eq!(
            cmd,
            Command::Add {
                topic: "deep work".to_string(),
                start: parse_start_time("2026-08-15 09:00").unwrap(),
                duration: Duration::from_secs(3600),
            }
        );
    }

    #[test]
    fn add_bare_time_defaults_to_today() {
        let cmd = parse("add writing 09:00 45m").unwrap();
        let Command::Add { start, .. } = cmd else {
            panic!("expected Command::Add");
        };
        assert_eq!(start.date(), history::today_local());
    }

    #[test]
    fn add_rejects_wrong_arg_count() {
        assert!(parse("add writing 2026-08-15 09:00").is_err());
        assert!(parse("add writing 2026-08-15 09:00 45m extra").is_err());
    }

    #[test]
    fn add_rejects_unterminated_quote() {
        assert!(parse(r#"add "unterminated writing 2026-08-15 09:00 45m"#).is_err());
    }

    #[test]
    fn add_rejects_invalid_start_or_duration() {
        assert!(parse("add writing not-a-time 45m").is_err());
        assert!(parse("add writing 2026-08-15 09:00 not-a-duration").is_err());
    }

    #[test]
    fn parses_remove_command() {
        assert_eq!(parse("remove 42").unwrap(), Command::Remove(42));
        assert_eq!(parse("rm 42").unwrap(), Command::Remove(42));
        assert!(parse("remove").is_err());
        assert!(parse("remove abc").is_err());
    }

    #[test]
    fn unknown_and_empty_commands_are_invalid() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
        assert!(parse("frobnicate").is_err());
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(parse("  pause  ").unwrap(), Command::Pause);
    }
}
