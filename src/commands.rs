use crate::errors::{AppError, Result};

/// A parsed neovim-style `:command` line. `CommandLine` strips the leading `:` before this is
/// called; the raw text after that is whitespace-split into a verb and the rest as arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Topic(String),
    Pause,
    Resume,
    ToggleBreak,
    ResumePrevious(Option<i64>),
    Sessions,
    Complete,
    Quit,
}

pub fn parse(input: &str) -> Result<Command> {
    let input = input.trim();
    let (verb, rest) = match input.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest.trim()),
        None => (input, ""),
    };

    match verb {
        "topic" if !rest.is_empty() => Ok(Command::Topic(rest.to_string())),
        "topic" => Err(AppError::InvalidCommand(
            "usage: topic <name>".to_string(),
        )),
        "pause" => Ok(Command::Pause),
        "resume" | "play" => Ok(Command::Resume),
        "break" => Ok(Command::ToggleBreak),
        "resume-previous" if rest.is_empty() => Ok(Command::ResumePrevious(None)),
        "resume-previous" => rest
            .parse::<i64>()
            .map(|id| Command::ResumePrevious(Some(id)))
            .map_err(|_| AppError::InvalidCommand(format!("invalid session id: {rest}"))),
        "sessions" => Ok(Command::Sessions),
        "complete" | "end" | "done" => Ok(Command::Complete),
        "quit" | "q" => Ok(Command::Quit),
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
        assert_eq!(parse("break").unwrap(), Command::ToggleBreak);
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
