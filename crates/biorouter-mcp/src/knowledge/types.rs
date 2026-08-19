use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

/// A knowledge base's privacy tier (issue #56, design §9.3 B4).
///
/// The stored form is the lowercase word in `.kb-tiers`, and it is the same
/// vocabulary [`crate::knowledge::tier`] compares against — one spelling, so the
/// enum and the store cannot drift.
///
/// ⚠ **This is a USER-FACING type, not a model-facing one.** Task 10D's metadata
/// register governs what a model may learn about a base; nothing here is added
/// to a `#[tool]` response. It travels on the HTTP surface the renderer reads
/// (`GET /knowledge/bases`, `GET|POST /knowledge/bases/{id}/tier`) and nowhere
/// else — in particular it is deliberately NOT a field on [`Manifest`], because
/// `manifest.yaml` is the on-disk record and a second copy of the tier there
/// would be a second answer to the question `.kb-tiers` already answers.
///
/// The `bool` in `tier.rs` is unchanged and stays: `biorouter-mcp` cannot depend
/// on `biorouter`, where `ProviderTier` lives, and the ratchet's argument is the
/// CALLER's capability rather than a base's tier.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KbTier {
    Public,
    Private,
}

impl KbTier {
    /// PRIVATE unless the caller's own reader said otherwise. Takes the answer
    /// `tier::is_private` already computed rather than re-deciding the polarity,
    /// so the fail-closed rules (unknown provenance, unparseable store, an
    /// unrecognised word) have exactly one implementation.
    pub fn from_is_private(is_private: bool) -> Self {
        if is_private {
            Self::Private
        } else {
            Self::Public
        }
    }

    pub fn is_private(self) -> bool {
        matches!(self, Self::Private)
    }
}

/// The schema generation a `manifest.yaml` that does not say belongs to.
///
/// 1, not 0: every manifest written before this default existed carries an
/// explicit `schema_version: 1`, so a manifest that is *missing* the key is a
/// hand-edited or externally produced one, and the right reading of it is "the
/// oldest generation we know", not "a generation that never existed".
fn default_schema_version() -> u32 {
    1
}

/// The creation date a `manifest.yaml` that does not say gets.
///
/// The epoch rather than `Utc::now()`, because a default is a statement about a
/// base that already exists: stamping it with the moment it was *read* would
/// invent a fact and make the base sort as the newest one in the list every
/// time it is loaded. The epoch reads as "unknown, and long ago", which is true.
fn default_created_at() -> DateTime<Utc> {
    DateTime::UNIX_EPOCH
}

// One knowledge base's `manifest.yaml`.
//
// A plain comment rather than a doc comment on purpose: utoipa publishes a
// struct's doc comment as its OpenAPI `description`, and none of what follows
// is addressed to an API consumer.
//
// ⚠ **Every field carries `#[serde(default)]`, and every field added later
// must too** (DR-12). `manifest::load` is a bare deserialize, so a single
// non-defaulted field fails the load for every `manifest.yaml` already on
// disk — and the cascade from there is silent and ends in *persisted* data
// loss, not in an error message: `list_bases` drops a base whose manifest will
// not parse, `installed_kb_ids_unlocked` is built from `list_bases` so the id
// leaves the installed universe, `repair_decision` sees a stored primary that
// is not installed and clears it, and `apply_selection_unlocked` writes the
// cleared pointer to disk. The user's `.active-kb` and every per-session
// pointer are gone, downgrading does not bring them back, and the trigger is
// the first thing a confused user does when their bases vanish: toggle
// something.
//
// The `#[schema(required)]` beside each default is not redundant. utoipa reads
// `serde(default)` and drops the field from the OpenAPI `required` list, which
// would loosen the published API contract — and the generated TypeScript type —
// as a side effect of a *robustness* change on the read side. The server always
// serializes all of these, so they really are required in a response; the
// default exists for what we read, not for what we send.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    #[serde(default)]
    #[cfg_attr(feature = "utoipa", schema(required))]
    pub id: String,
    #[serde(default)]
    #[cfg_attr(feature = "utoipa", schema(required))]
    pub name: String,
    #[serde(default)]
    #[cfg_attr(feature = "utoipa", schema(required))]
    pub color: String,
    #[serde(default = "default_created_at")]
    #[cfg_attr(feature = "utoipa", schema(required))]
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_schema_version")]
    #[cfg_attr(feature = "utoipa", schema(required))]
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<ModelRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegistryEntry {
    pub id: String,
    pub path: std::path::PathBuf,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: PageKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credibility_tier: Option<CredibilityTier>,
    /// True if this is a source node whose `raw/<id>/meta.yaml` marks it retracted.
    #[serde(default)]
    pub retracted: bool,
    pub path: String,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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

#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
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
