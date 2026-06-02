use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;

static MAIN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<main\b[^>]*>(?P<body>.*?)</main>").expect("main regex"));
static ARTICLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?is)<article\b[^>]*>(?P<body>.*?)</article>").expect("article regex")
});

pub struct HtmlConversion {
    pub markdown: String,
    pub title: Option<String>,
}

pub fn html_to_markdown(html: &str) -> Result<HtmlConversion> {
    let fragment = main_content_fragment(html);
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style", "noscript", "svg"])
        .build();
    let md = converter
        .convert(fragment)
        .map_err(|e| anyhow::anyhow!("htmd: {e}"))?;
    let title = extract_title(html);
    Ok(HtmlConversion {
        markdown: md,
        title,
    })
}

fn main_content_fragment(html: &str) -> &str {
    if let Some(caps) = MAIN_RE.captures(html) {
        if let Some(body) = caps.name("body") {
            return body.as_str();
        }
    }
    if let Some(caps) = ARTICLE_RE.captures(html) {
        if let Some(body) = caps.name("body") {
            return body.as_str();
        }
    }
    html
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")? + "<title>".len();
    let end = lower.get(start..)?.find("</title>")? + start;
    Some(html.get(start..end)?.trim().to_string())
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

    #[test]
    fn prefers_main_content_when_present() {
        let html = r#"
        <html>
          <body>
            <nav><a href="/x">Nav</a></nav>
            <main><h1>Real content</h1><p>Hello world.</p></main>
            <footer>Footer links</footer>
          </body>
        </html>
        "#;
        let c = html_to_markdown(html).unwrap();
        assert!(c.markdown.contains("# Real content"));
        assert!(!c.markdown.contains("Footer links"));
    }
}
