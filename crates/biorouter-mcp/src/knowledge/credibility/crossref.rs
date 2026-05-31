use crate::knowledge::{
    credibility::allowlist::is_peer_reviewed_publisher,
    types::{Credibility, CredibilityTier},
};
use anyhow::Result;
use serde::Deserialize;

const API_BASE: &str = "https://api.crossref.org";

pub async fn classify(doi: &str) -> Result<Option<Credibility>> {
    let client = reqwest::Client::builder()
        .user_agent("BioRouter-Knowledge/1.0 (mailto:knowledge@biorouter.local)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    classify_with(&client, API_BASE, doi).await
}

pub async fn classify_with(client: &reqwest::Client, base: &str, doi: &str) -> Result<Option<Credibility>> {
    let url = format!("{base}/works/{doi}");
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() { return Ok(None); }
    let body: CrossrefResponse = resp.json().await?;
    let w = body.message;
    let publisher = w.publisher.unwrap_or_default();
    let venue = w.container_title.unwrap_or_default().into_iter().next();
    let retracted = w.update_to
        .map(|u| u.iter().any(|i| i.r#type == "retraction"))
        .unwrap_or(false);
    let tier = match w.r#type.as_str() {
        "journal-article" if is_peer_reviewed_publisher(&publisher) => CredibilityTier::PeerReviewed,
        "journal-article" => CredibilityTier::Web, // recognized type, but unknown publisher
        "posted-content" => CredibilityTier::Preprint,
        "book" | "book-chapter" | "monograph" | "edited-book" => CredibilityTier::Book,
        _ => return Ok(None),
    };
    let confidence = if matches!(tier, CredibilityTier::PeerReviewed | CredibilityTier::Book) { 0.95 } else { 0.85 };
    let reasoning = format!(
        "Crossref returned type={:?}, publisher='{}'.",
        w.r#type, publisher
    );
    Ok(Some(Credibility {
        tier,
        confidence,
        publisher: Some(publisher),
        venue,
        doi: Some(doi.to_string()),
        retracted,
        reasoning,
        classifier_version: 1,
    }))
}

#[derive(Deserialize)]
struct CrossrefResponse { message: CrossrefWork }

#[derive(Deserialize)]
struct CrossrefWork {
    r#type: String,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(rename = "container-title", default)]
    container_title: Option<Vec<String>>,
    #[serde(rename = "update-to", default)]
    update_to: Option<Vec<UpdateTo>>,
}

#[derive(Deserialize)]
struct UpdateTo { #[serde(default)] r#type: String }

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::{method, path_regex}, Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn classifies_peer_reviewed_journal_article() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "journal-article", "publisher": "Springer Nature",
                              "container-title": ["Nature Genetics"] }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1038/s41588-024-01893-6").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::PeerReviewed);
        assert_eq!(c.venue.as_deref(), Some("Nature Genetics"));
        assert_eq!(c.publisher.as_deref(), Some("Springer Nature"));
    }

    #[tokio::test]
    async fn classifies_preprint_via_posted_content() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "posted-content", "publisher": "Cold Spring Harbor Laboratory" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1101/2024.01.01").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::Preprint);
    }

    #[tokio::test]
    async fn classifies_book() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "book", "publisher": "MIT Press" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.7551/mitpress/0").await.unwrap().unwrap();
        assert_eq!(c.tier, CredibilityTier::Book);
    }

    #[tokio::test]
    async fn detects_retraction() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "journal-article", "publisher": "Wiley",
                              "update-to": [{ "type": "retraction" }] }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        let c = classify_with(&client, &server.uri(), "10.1002/x").await.unwrap().unwrap();
        assert!(c.retracted);
    }

    #[tokio::test]
    async fn returns_none_on_unknown_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path_regex(r"^/works/.*"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "type": "dataset", "publisher": "X" }
            })))
            .mount(&server).await;
        let client = reqwest::Client::new();
        assert!(classify_with(&client, &server.uri(), "10.x/y").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn returns_none_on_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(404)).mount(&server).await;
        let client = reqwest::Client::new();
        assert!(classify_with(&client, &server.uri(), "10.x/y").await.unwrap().is_none());
    }
}
