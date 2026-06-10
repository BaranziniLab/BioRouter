//! Matcher semantics: `None`/`""`/`"*"` match everything; otherwise exact
//! string equality, falling back to an anchored regex (which provides `a|b`
//! alternation and full regex patterns in one rule, mirroring Claude Code).

use tracing::warn;

/// Returns true when `matcher` matches `key`.
///
/// Invalid regex patterns are logged and treated as non-matching.
pub fn matcher_matches(matcher: Option<&str>, key: &str) -> bool {
    let Some(pattern) = matcher else {
        return true;
    };
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern == "*" {
        return true;
    }
    if pattern == key {
        return true;
    }
    match regex::Regex::new(&format!("^(?:{pattern})$")) {
        Ok(re) => re.is_match(key),
        Err(e) => {
            warn!("hooks: invalid matcher regex '{}': {}", pattern, e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_star_and_none_match_all() {
        assert!(matcher_matches(None, "anything"));
        assert!(matcher_matches(Some(""), "anything"));
        assert!(matcher_matches(Some("*"), "anything"));
    }

    #[test]
    fn exact_match() {
        assert!(matcher_matches(
            Some("developer__shell"),
            "developer__shell"
        ));
        assert!(!matcher_matches(
            Some("developer__shell"),
            "developer__text_editor"
        ));
    }

    #[test]
    fn alternation() {
        let m = Some("developer__shell|developer__text_editor");
        assert!(matcher_matches(m, "developer__shell"));
        assert!(matcher_matches(m, "developer__text_editor"));
        assert!(!matcher_matches(m, "memory__store"));
    }

    #[test]
    fn regex_is_anchored() {
        assert!(matcher_matches(Some("developer__.*"), "developer__shell"));
        assert!(!matcher_matches(Some("developer"), "developer__shell"));
        assert!(!matcher_matches(Some("__shell"), "developer__shell"));
    }

    #[test]
    fn invalid_regex_is_non_match() {
        assert!(!matcher_matches(Some("foo[("), "foo"));
    }
}
