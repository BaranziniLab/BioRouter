pub fn extract_urls(text: &str) -> Vec<String> {
    // Conservative URL detector: http(s):// followed by non-space, non-paren, non-quote chars.
    let re = regex::Regex::new(r#"https?://[^\s<>"')]+[^\s<>"').,;:!?]"#).unwrap();
    let mut out = Vec::new();
    for m in re.find_iter(text) {
        let u = m.as_str().to_string();
        if !out.contains(&u) {
            out.push(u);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_urls_in_prose() {
        let text = "See https://arxiv.org/abs/2403.12345 and also http://example.org/x.";
        let urls = extract_urls(text);
        assert_eq!(urls.len(), 2);
        assert!(urls.contains(&"https://arxiv.org/abs/2403.12345".to_string()));
    }

    #[test]
    fn dedupes() {
        let urls = extract_urls("a https://x.com b https://x.com c");
        assert_eq!(urls, vec!["https://x.com"]);
    }

    #[test]
    fn strips_trailing_punctuation() {
        let urls = extract_urls("Read https://example.org/path, please.");
        assert_eq!(urls, vec!["https://example.org/path"]);
    }

    #[test]
    fn ignores_parenthesized_correctly() {
        let urls = extract_urls("(see https://example.org/p)");
        assert_eq!(urls, vec!["https://example.org/p"]);
    }
}
