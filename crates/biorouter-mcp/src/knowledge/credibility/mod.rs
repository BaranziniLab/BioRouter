pub mod agentic;
pub mod allowlist;
pub mod crossref;
pub mod host_patterns;
pub mod identifiers;
pub mod openalex;

use crate::knowledge::{convert::SourceInput, types::Credibility};
use anyhow::Result;

pub async fn classify(input: &SourceInput) -> Result<Credibility> {
    // 1. Extract identifiers from whatever text we have.
    let probe = probe_text(input);
    let ids = identifiers::extract(&probe);

    // 2. Deterministic DOI lookup via Crossref then OpenAlex.
    if let Some(doi) = &ids.doi {
        if let Some(c) = crossref::classify(doi).await? {
            return Ok(c);
        }
        if let Some(c) = openalex::classify(doi).await? {
            return Ok(c);
        }
    }

    // 3. Host pattern.
    if let SourceInput::Url(url) = input {
        if let Some(c) = host_patterns::classify_url(url) {
            return Ok(c);
        }
    }

    // 4. Agentic fallback (stub in Plan 1).
    agentic::classify(input).await
}

fn probe_text(input: &SourceInput) -> String {
    match input {
        SourceInput::Url(u) => u.clone(),
        SourceInput::Text { text, title } => {
            format!("{}\n{}", title.clone().unwrap_or_default(), text)
        }
        SourceInput::File {
            filename, bytes, ..
        } => {
            // Sniff first 4 KB for identifiers (PDF metadata, HTML head, etc.)
            let head: String = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_string();
            format!("{filename}\n{head}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::types::CredibilityTier;

    #[tokio::test]
    async fn falls_back_to_host_pattern_when_no_doi() {
        let c = classify(&SourceInput::Url("https://arxiv.org/abs/2403.12345".into()))
            .await
            .unwrap();
        assert_eq!(c.tier, CredibilityTier::Preprint);
    }

    #[tokio::test]
    async fn personal_text_falls_through_to_agentic() {
        let c = classify(&SourceInput::Text {
            text: "lab notes".into(),
            title: None,
        })
        .await
        .unwrap();
        assert_eq!(c.tier, CredibilityTier::Personal);
    }
}
