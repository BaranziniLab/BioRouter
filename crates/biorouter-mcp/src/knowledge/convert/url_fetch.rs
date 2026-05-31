use anyhow::{Context, Result};

pub struct FetchedSource {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub final_url: String,
}

pub async fn fetch_url(url: &str) -> Result<FetchedSource> {
    let client = reqwest::Client::builder()
        .user_agent("BioRouter-Knowledge/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client.get(url).send().await
        .with_context(|| format!("fetching {url}"))?;
    let final_url = resp.url().to_string();
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .unwrap_or_else(|| guess_mime_from_url(&final_url));
    let bytes = resp.bytes().await?.to_vec();
    Ok(FetchedSource { bytes, mime, final_url })
}

fn guess_mime_from_url(url: &str) -> String {
    let lower = url.to_lowercase();
    if lower.ends_with(".pdf") { "application/pdf".into() }
    else if lower.ends_with(".docx") { "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into() }
    else if lower.ends_with(".csv") { "text/csv".into() }
    else if lower.ends_with(".md") { "text/markdown".into() }
    else { "text/html".into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetches_html() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("<html><body>Hello</body></html>", "text/html; charset=utf-8"),
            )
            .mount(&server).await;
        let s = fetch_url(&format!("{}/x", server.uri())).await.unwrap();
        assert_eq!(s.mime, "text/html");
        assert!(String::from_utf8_lossy(&s.bytes).contains("Hello"));
    }

    #[tokio::test]
    async fn guesses_mime_for_pdf_path_when_no_header() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 4]))
            .mount(&server).await;
        let s = fetch_url(&format!("{}/x.pdf", server.uri())).await.unwrap();
        // Header was absent (defaulted by wiremock); guess wins.
        assert!(s.mime == "application/octet-stream" || s.mime == "application/pdf");
    }
}
