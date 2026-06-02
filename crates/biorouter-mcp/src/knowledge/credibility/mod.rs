pub mod agentic;
pub mod allowlist;
pub mod crossref;
pub mod host_patterns;
pub mod identifiers;
pub mod openalex;

use crate::knowledge::{convert::SourceInput, subagent::loop_::Completer, types::Credibility};
use anyhow::Result;

pub async fn classify(
    input: &SourceInput,
    completer: Option<Box<dyn Completer>>,
) -> Result<Credibility> {
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

    // 4. Agentic fallback — requires a Completer from the caller.
    //    If no completer is available, return the deterministic default.
    if let Some(completer) = completer {
        agentic::classify(input, completer).await
    } else {
        deterministic_default(input)
    }
}

/// Deterministic default used when no Completer is available.
fn deterministic_default(input: &SourceInput) -> Result<Credibility> {
    use crate::knowledge::types::CredibilityTier;
    let (tier, reason) = match input {
        SourceInput::Url(_) | SourceInput::File { .. } | SourceInput::Path(_) => (
            CredibilityTier::Web,
            "No identifier found and no host-pattern matched; defaulting to web.",
        ),
        SourceInput::Text { .. } => (
            CredibilityTier::Personal,
            "Pasted text with no provenance — personal.",
        ),
    };
    Ok(Credibility {
        tier,
        confidence: 0.4,
        publisher: None,
        venue: None,
        doi: None,
        retracted: false,
        reasoning: reason.to_string(),
        classifier_version: 1,
    })
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
        SourceInput::Path(path) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::types::CredibilityTier;

    #[tokio::test]
    async fn falls_back_to_host_pattern_when_no_doi() {
        let c = classify(
            &SourceInput::Url("https://arxiv.org/abs/2403.12345".into()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(c.tier, CredibilityTier::Preprint);
    }

    #[tokio::test]
    async fn personal_text_falls_through_to_deterministic_default() {
        let c = classify(
            &SourceInput::Text {
                text: "lab notes".into(),
                title: None,
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(c.tier, CredibilityTier::Personal);
    }

    #[tokio::test]
    async fn url_with_no_doi_falls_back_to_web() {
        // A plain https URL with no DOI and no special host pattern is classified
        // as Web by the host_patterns module (generic https catch-all, confidence 0.6).
        let c = classify(
            &SourceInput::Url("https://totally-unknown-site.example.com/post/1".into()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(c.tier, CredibilityTier::Web);
    }
}
