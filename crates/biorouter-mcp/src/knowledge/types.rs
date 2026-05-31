use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredibilityTier {
    PeerReviewed,
    Preprint,
    Book,
    GrayLit,
    Web,
    Personal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Credibility {
    pub tier: CredibilityTier,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(default)]
    pub retracted: bool,
    pub reasoning: String,
    pub classifier_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceMeta {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub ingested_at: DateTime<Utc>,
    pub sha256: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
    pub credibility: Credibility,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: DateTime<Utc>,
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegistryEntry {
    pub id: String,
    pub path: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    Source,
    Entity,
    Concept,
    Hub,
    Note,
    Flag,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: PageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credibility_tier: Option<CredibilityTier>,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Ingest,
    Link,
    Flag,
    Query,
    Lint,
    Restore,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryEntry {
    pub commit_sha: String,
    pub kind: ChangeKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credibility_tier_serde_roundtrip() {
        for tier in [
            CredibilityTier::PeerReviewed,
            CredibilityTier::Preprint,
            CredibilityTier::Book,
            CredibilityTier::GrayLit,
            CredibilityTier::Web,
            CredibilityTier::Personal,
        ] {
            let s = serde_yaml::to_string(&tier).unwrap();
            let back: CredibilityTier = serde_yaml::from_str(&s).unwrap();
            assert_eq!(tier, back);
        }
    }

    #[test]
    fn credibility_tier_yaml_form_is_snake_case() {
        let s = serde_yaml::to_string(&CredibilityTier::PeerReviewed).unwrap();
        assert!(s.contains("peer_reviewed"), "got: {s}");
    }

    #[test]
    fn source_meta_yaml_roundtrip() {
        let meta = SourceMeta {
            id: "abc-123".into(),
            title: "Title".into(),
            url: Some("https://arxiv.org/abs/2403.12345".into()),
            ingested_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            sha256: "deadbeef".into(),
            mime: "application/pdf".into(),
            original_filename: Some("paper.pdf".into()),
            credibility: Credibility {
                tier: CredibilityTier::Preprint,
                confidence: 0.97,
                publisher: Some("arXiv".into()),
                venue: Some("arXiv:2403.12345".into()),
                doi: None,
                retracted: false,
                reasoning: "URL host arxiv.org → preprint server.".into(),
                classifier_version: 1,
            },
        };
        let s = serde_yaml::to_string(&meta).unwrap();
        let back: SourceMeta = serde_yaml::from_str(&s).unwrap();
        assert_eq!(meta, back);
    }

    #[test]
    fn manifest_yaml_roundtrip() {
        let m = Manifest {
            id: "ms".into(),
            name: "MS Patient Analysis".into(),
            color: "#5a6394".into(),
            created_at: chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            schema_version: 1,
            default_model: None,
        };
        let s = serde_yaml::to_string(&m).unwrap();
        let back: Manifest = serde_yaml::from_str(&s).unwrap();
        assert_eq!(m, back);
    }
}
