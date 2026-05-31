use anyhow::Result;

pub struct HtmlConversion {
    pub markdown: String,
    pub title: Option<String>,
}

pub fn html_to_markdown(html: &str) -> Result<HtmlConversion> {
    let md = htmd::convert(html).map_err(|e| anyhow::anyhow!("htmd: {e}"))?;
    let title = extract_title(html);
    Ok(HtmlConversion { markdown: md, title })
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")? + "<title>".len();
    let end = lower[start..].find("</title>")? + start;
    Some(html[start..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("fixtures/article.html");

    #[test]
    fn converts_headings_and_links() {
        let c = html_to_markdown(FIXTURE).unwrap();
        assert!(c.markdown.contains("# Example article"));
        assert!(c.markdown.contains("[a link](https://example.org)"));
        assert!(c.markdown.contains("## Section two"));
    }

    #[test]
    fn extracts_title() {
        let c = html_to_markdown(FIXTURE).unwrap();
        assert_eq!(c.title.as_deref(), Some("Example article"));
    }

    #[test]
    fn handles_empty_html() {
        let c = html_to_markdown("").unwrap();
        assert!(c.markdown.is_empty() || c.markdown.trim().is_empty());
        assert_eq!(c.title, None);
    }
}
