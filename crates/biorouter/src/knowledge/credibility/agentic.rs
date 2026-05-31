use crate::knowledge::{
    convert::SourceInput,
    types::{Credibility, CredibilityTier},
};
use anyhow::Result;

pub async fn classify(input: &SourceInput) -> Result<Credibility> {
    let (tier, reason) = match input {
        SourceInput::Url(_) | SourceInput::File { .. } => (
            CredibilityTier::Web,
            "Agentic fallback (stub): defaulting to web — no identifier and no host pattern matched.",
        ),
        SourceInput::Text { .. } => (
            CredibilityTier::Personal,
            "Agentic fallback (stub): pasted text with no provenance — personal.",
        ),
    };
    Ok(Credibility {
        tier,
        confidence: 0.4,
        publisher: None, venue: None, doi: None, retracted: false,
        reasoning: reason.to_string(),
        classifier_version: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn url_defaults_to_web() {
        let c = classify(&SourceInput::Url("https://x.com/y".into())).await.unwrap();
        assert_eq!(c.tier, CredibilityTier::Web);
    }

    #[tokio::test]
    async fn text_defaults_to_personal() {
        let c = classify(&SourceInput::Text { text: "note".into(), title: None }).await.unwrap();
        assert_eq!(c.tier, CredibilityTier::Personal);
    }
}
