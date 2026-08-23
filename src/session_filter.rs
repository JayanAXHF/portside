//! Pure matching logic behind the sessions pane's search box: turning a raw query string into a
//! yes/no match against a `Session`. Deliberately has no rendering or key-handling code —
//! `components::session_list` owns the ratatui side, mirroring the `history`/`components::history`
//! split.

use crate::db::Session;

/// Whether `session` matches `query`. `query` is split on whitespace into tokens, all of which
/// must match (AND semantics); an empty query matches everything. A token prefixed with `#`
/// requires an exact, case-insensitive match against one of `session.tags`; any other token
/// requires a case-insensitive substring match against `session.topic` or `session.description`.
pub fn matches(session: &Session, query: &str) -> bool {
    query
        .split_whitespace()
        .all(|token| match token.strip_prefix('#') {
            Some(tag) => session.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
            None => {
                let token = token.to_lowercase();
                session.topic.to_lowercase().contains(&token)
                    || session
                        .description
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&token)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(topic: &str, description: Option<&str>, tags: &[&str]) -> Session {
        let mut session = Session::new(topic.to_string());
        session.description = description.map(str::to_string);
        session.tags = tags.iter().map(|t| t.to_string()).collect();
        session
    }

    #[test]
    fn empty_query_matches_everything() {
        let session = session("writing", None, &[]);
        assert!(matches(&session, ""));
        assert!(matches(&session, "   "));
    }

    #[test]
    fn plain_token_matches_topic_substring_case_insensitively() {
        let session = session("Deep Work", None, &[]);
        assert!(matches(&session, "deep"));
        assert!(matches(&session, "WORK"));
        assert!(!matches(&session, "shallow"));
    }

    #[test]
    fn plain_token_matches_description_substring() {
        let session = session("writing", Some("working through chapter 3"), &[]);
        assert!(matches(&session, "chapter"));
        assert!(!matches(&session, "chapter 5"));
    }

    #[test]
    fn hash_token_matches_a_tag_exactly_case_insensitively() {
        let session = session("writing", None, &["Rust", "study"]);
        assert!(matches(&session, "#rust"));
        assert!(matches(&session, "#STUDY"));
        assert!(!matches(&session, "#ru"));
        assert!(!matches(&session, "#unrelated"));
    }

    #[test]
    fn multiple_tokens_are_and_ed_together() {
        let session = session("Deep Work", Some("chapter 3"), &["rust"]);
        assert!(matches(&session, "deep #rust"));
        assert!(matches(&session, "chapter #rust deep"));
        assert!(!matches(&session, "deep #study"));
        assert!(!matches(&session, "shallow #rust"));
    }

    #[test]
    fn tag_token_does_not_match_topic_or_description() {
        let session = session("rust basics", Some("about rust"), &[]);
        assert!(!matches(&session, "#rust"));
    }
}
