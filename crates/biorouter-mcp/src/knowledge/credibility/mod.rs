pub mod agentic;
pub mod allowlist;
pub mod crossref;
pub mod host_patterns;
pub mod identifiers;
pub mod openalex;

use crate::knowledge::{
    convert::SourceInput,
    subagent::loop_::Completer,
    types::{Credibility, CredibilityTier},
};
use anyhow::Result;

pub async fn classify(
    input: &SourceInput,
    completer: Option<Box<dyn Completer>>,
) -> Result<Credibility> {
    classify_with_text(input, None, completer).await
}

/// Classify a source's credibility.
///
/// `extracted_text` is the converted markdown body (when available). Passing it
/// is what lets us recover the DOI / journal markers that live *inside* a PDF or
/// HTML article — the raw `input` only carries the first few KB of (often
/// binary) bytes, so a paper uploaded as `a64e171e-….pdf` used to fall straight
/// through to the `web` default even though its text clearly names a DOI and a
/// journal. Callers that have already run conversion should always pass it.
pub async fn classify_with_text(
    input: &SourceInput,
    extracted_text: Option<&str>,
    completer: Option<Box<dyn Completer>>,
) -> Result<Credibility> {
    // 1. Extract identifiers from the richest text we have: the filename / URL
    //    plus the full converted body.
    let mut probe = probe_text(input);
    if let Some(text) = extracted_text {
        probe.push('\n');
        probe.push_str(text);
    }
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
            // A bare DOI inside the body can upgrade a generic-web host verdict:
            // an article hosted on a publisher domain we don't recognise is
            // still peer-reviewed if its text carries scholarly markers.
            if c.tier == CredibilityTier::Web {
                if let Some(upgraded) = scholarly_text_signal(&probe) {
                    return Ok(upgraded);
                }
            }
            return Ok(c);
        }
    }

    // 4. Text heuristic for file/path uploads (e.g. an article PDF with no
    //    resolvable DOI but obvious journal provenance in the body).
    if matches!(input, SourceInput::File { .. } | SourceInput::Path(_)) {
        if let Some(c) = scholarly_text_signal(&probe) {
            return Ok(c);
        }
    }

    // 5. Agentic fallback — requires a Completer from the caller.
    //    If no completer is available, return the deterministic default.
    if let Some(completer) = completer {
        agentic::classify(input, completer).await
    } else {
        deterministic_default(input)
    }
}

/// Detect strong "this is a scholarly article" markers in free text and, if
/// present, return a peer-reviewed (or preprint) verdict. This is deliberately
/// conservative: it only fires on combinations that rarely appear outside
/// academic publishing, so a blog post is not mistaken for a journal article.
fn scholarly_text_signal(text: &str) -> Option<Credibility> {
    let lower = text.to_lowercase();

    // A DOI mention is the single strongest signal.
    let has_doi = lower.contains("doi.org/")
        || lower.contains("doi:")
        || lower.contains("https://doi")
        || identifiers::extract(text).doi.is_some();

    // Preprint server fingerprints.
    let preprint = [
        "biorxiv",
        "medrxiv",
        "arxiv",
        "chemrxiv",
        "preprint server",
        "ssrn",
    ]
    .iter()
    .any(|m| lower.contains(m));

    // Journal / peer-review fingerprints, requiring at least two so a single
    // stray word can't trip it.
    let journal_markers = [
        "received:",
        "accepted:",
        "published:",
        "peer-reviewed",
        "peer reviewed",
        "corresponding author",
        "this article is licensed under",
        "supplementary material",
        "©",
        "journal of",
        "et al.",
        "abstract",
        "cite this article",
    ];
    let marker_hits = journal_markers
        .iter()
        .filter(|m| lower.contains(**m))
        .count();

    if preprint && (has_doi || marker_hits >= 1) {
        return Some(Credibility {
            tier: CredibilityTier::Preprint,
            confidence: 0.7,
            publisher: None,
            venue: None,
            doi: identifiers::extract(text).doi,
            retracted: false,
            reasoning: "Text carries preprint-server fingerprints.".into(),
            classifier_version: 1,
        });
    }
    if has_doi && marker_hits >= 2 {
        return Some(Credibility {
            tier: CredibilityTier::PeerReviewed,
            confidence: 0.72,
            publisher: None,
            venue: None,
            doi: identifiers::extract(text).doi,
            retracted: false,
            reasoning: "Text carries a DOI plus multiple journal-article markers.".into(),
            classifier_version: 1,
        });
    }
    None
}

/// Deterministic default used when no Completer is available.
fn deterministic_default(input: &SourceInput) -> Result<Credibility> {
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
    async fn pdf_file_with_journal_markers_in_body_is_peer_reviewed() {
        // A paper uploaded as an opaque UUID filename: the raw bytes carry no
        // identifier, but the converted body clearly names a DOI and journal
        // markers. Classifying on the extracted text must recover peer-reviewed.
        let body =
            "Abstract\n\nReceived: 1 Jan 2023  Accepted: 1 Mar 2023  Published: 1 Apr 2023\n\
                    Corresponding author: a@b.org\n\
                    https://doi.org/10.7554/eLife.12345\n© 2023 The Authors.";
        let c = classify_with_text(
            &SourceInput::File {
                bytes: b"%PDF-1.7 binary noise".to_vec(),
                filename: "a64e171e-f161-4615-9299-839c8a066049.pdf".into(),
                mime: Some("application/pdf".into()),
            },
            Some(body),
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            c.tier,
            CredibilityTier::PeerReviewed,
            "reasoning: {}",
            c.reasoning
        );
    }

    #[tokio::test]
    async fn plain_pdf_without_markers_stays_web() {
        let c = classify_with_text(
            &SourceInput::File {
                bytes: b"%PDF-1.7".to_vec(),
                filename: "flyer.pdf".into(),
                mime: Some("application/pdf".into()),
            },
            Some("Buy one get one free at the fair this weekend!"),
            None,
        )
        .await
        .unwrap();
        assert_eq!(c.tier, CredibilityTier::Web);
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
