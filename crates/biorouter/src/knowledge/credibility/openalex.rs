use crate::knowledge::{
    credibility::allowlist::is_peer_reviewed_publisher,
    types::{Credibility, CredibilityTier},
};
use anyhow::Result;
use serde::Deserialize;

const API_BASE: &str = "https://api.openalex.org";

pub async fn classify(doi: &str) -> Result<Option<Credibility>> {
    let client = reqwest::Client::builder()
        .user_agent("BioRouter-Knowledge/1.0 (mailto:knowledge@biorouter.local)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    classify_with(&client, API_BASE, doi).await
}

pub async fn classify_with(client: &reqwest::Client, base: &str, doi: &str) -> Result<Option<Credibility>> {
    let url = format!("{base}/works/doi:{doi}");
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() { return Ok(None); }
    let w: Work = resp.json().await?;
    let publisher = w.host_venue.as_ref().and_then(|v| v.publisher.clone()).unwrap_or_default();
    let venue = w.host_venue.as_ref().and_then(|v| v.display_name.clone());
    let retracted = w.is_retracted.unwrap_or(false);
    let tier = match w.r#type.as_deref() {
        Some("journal-article") if is_peer_reviewed_publisher(&publisher) => CredibilityTier::PeerReviewed,
        Some("journal-article") => CredibilityTier::Web,
        Some("posted-content") | Some("preprint") => CredibilityTier::Preprint,
        Some("book") | Some("book-chapter") | Some("monograph") => CredibilityTier::Book,
        _ => return Ok(None),
    };
    let confidence = if matches!(tier, CredibilityTier::PeerReviewed | CredibilityTier::Book) { 0.93 } else { 0.83 };
    Ok(Some(Credibility {
        tier,
        confidence,
        publisher: Some(publisher),
        venue,
        doi: Some(doi.to_string()),
        retracted,
        reasoning: format!("OpenAlex returned type={:?}.", w.r#type),
        classifier_version: 1,
    }))
}

#[derive(Deserialize)]
struct Work {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    is_retracted: Option<bool>,
    #[serde(default)]
    host_venue: Option<HostVenue>,
}

#[derive(Deserialize)]
struct HostVenue {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::{method, path_regex}, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn classifies_journal_article() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/doi:.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type": "journal-article",
                "is_retracted": false,
                "host_venue": { "display_name": "Cell", "publisher": "Elsevier" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1016/x").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::PeerReviewed);
        assert_eq!(c.venue.as_deref(), Some("Cell"));
    }

    #[tokio::test]
    async fn returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let client = reqwest::Client::new();
        assert!(classify_with(&client, &server.uri(), "10.x/y").await.unwrap().is_none());
    }
}
