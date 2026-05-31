pub mod csv;
pub mod docx;
pub mod html;
pub mod note;
pub mod pdf;
pub mod url_fetch;

use anyhow::Result;

#[derive(Debug, Clone)]
pub enum SourceInput {
    File { bytes: Vec<u8>, filename: String, mime: Option<String> },
    Url(String),
    Text { text: String, title: Option<String> },
}

#[derive(Debug, Clone)]
pub struct Converted {
    pub markdown: String,
    pub title: Option<String>,
    pub mime: String,
    pub needs_llm_fallback: bool,
}

pub async fn convert(input: &SourceInput) -> Result<Converted> {
    match input {
        SourceInput::Text { text, title } => Ok(Converted {
            markdown: text.clone(),
            title: title.clone(),
            mime: "text/plain".into(),
            needs_llm_fallback: false,
        }),
        SourceInput::Url(url) => {
            let fetched = url_fetch::fetch_url(url).await?;
            let file = SourceInput::File {
                bytes: fetched.bytes,
                filename: filename_from_url(&fetched.final_url),
                mime: Some(fetched.mime),
            };
            Box::pin(convert(&file)).await
        }
        SourceInput::File { bytes, filename, mime } => {
            let effective_mime = mime.clone().unwrap_or_else(|| guess_mime(filename));
            match effective_mime.as_str() {
                "text/html" | "application/xhtml+xml" => {
                    let s = std::str::from_utf8(bytes)?;
                    let c = html::html_to_markdown(s)?;
                    Ok(Converted {
                        markdown: c.markdown,
                        title: c.title,
                        mime: effective_mime,
                        needs_llm_fallback: false,
                    })
                }
                "application/pdf" => {
                    let c = pdf::pdf_to_markdown(bytes)?;
                    Ok(Converted {
                        markdown: c.markdown,
                        title: None,
                        mime: effective_mime,
                        needs_llm_fallback: c.needs_llm_fallback,
                    })
                }
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                    let md = docx::docx_to_markdown(bytes)?;
                    Ok(Converted { markdown: md, title: None, mime: effective_mime, needs_llm_fallback: false })
                }
                "text/csv" => {
                    let md = csv::csv_to_markdown(bytes)?;
                    Ok(Converted { markdown: md, title: None, mime: effective_mime, needs_llm_fallback: false })
                }
                "text/markdown" | "text/plain" => {
                    Ok(Converted {
                        markdown: String::from_utf8_lossy(bytes).into_owned(),
                        title: None,
                        mime: effective_mime,
                        needs_llm_fallback: false,
                    })
                }
                other => anyhow::bail!("unsupported mime: {other}"),
            }
        }
    }
}

fn filename_from_url(url: &str) -> String {
    url.split('/').last().unwrap_or("source").to_string()
}

fn guess_mime(filename: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.ends_with(".pdf") { "application/pdf".into() }
    else if lower.ends_with(".docx") { "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into() }
    else if lower.ends_with(".csv") { "text/csv".into() }
    else if lower.ends_with(".md") { "text/markdown".into() }
    else if lower.ends_with(".html") || lower.ends_with(".htm") { "text/html".into() }
    else { "text/plain".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatches_html() {
        let html = "<html><head><title>T</title></head><body><h1>H</h1></body></html>";
        let c = convert(&SourceInput::File {
            bytes: html.as_bytes().to_vec(),
            filename: "x.html".into(),
            mime: Some("text/html".into()),
        }).await.unwrap();
        assert!(c.markdown.contains("# H"));
        assert_eq!(c.title.as_deref(), Some("T"));
    }

    #[tokio::test]
    async fn dispatches_text_passthrough() {
        let c = convert(&SourceInput::Text {
            text: "hello".into(),
            title: Some("Note".into()),
        }).await.unwrap();
        assert_eq!(c.markdown, "hello");
        assert_eq!(c.title.as_deref(), Some("Note"));
    }
}
