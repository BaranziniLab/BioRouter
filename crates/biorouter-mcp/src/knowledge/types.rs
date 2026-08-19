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

/// Which of the two OKF profiles a base's **producer** follows (DR-6).
///
/// Both profiles write the same on-disk shape — BioOKF only adds constraints,
/// so a BioOKF bundle is always a valid OKF bundle. The value therefore selects
/// how strictly a *write* is checked and which vocabulary the sub-agent is
/// taught; it never selects how a page is *read*, which is the property that
/// lets one reader, one graph deriver and one renderer serve both.
///
/// ⚠ **On its own this does not answer "is this base OKF?".** It carries
/// `#[serde(default)]` like every other manifest field (DR-12), so every
/// `manifest.yaml` written before Stage 3 — every base on disk — reads back as
/// `Okf` while its pages are `title`/`kind` frontmatter and `[[wiki links]]`.
/// The **generation number** is what separates them: ask [`Manifest::profile`],
/// which folds [`CURRENT_SCHEMA_VERSION`] in, and never this field alone.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KbFormat {
    /// OKF v0.2, open vocabulary. The default for a new base.
    #[default]
    Okf,
    /// OKF v0.2 plus the BioOKF v0.5 profile: 28 node types, 35 predicates, and
    /// a required per-edge provenance triplet.
    Biookf,
}

impl KbFormat {
    /// The wire/on-disk spelling, which is also what `manifest.yaml` carries.
    /// Spelled once here so a message and the serializer cannot disagree.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Okf => "okf",
            Self::Biookf => "biookf",
        }
    }

    /// The inverse of [`Self::as_str`]. `None` for anything else — see the
    /// hand-written [`Deserialize`] impl for what a caller reading a
    /// `manifest.yaml` must then do with it.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "okf" => Some(Self::Okf),
            "biookf" => Some(Self::Biookf),
            _ => None,
        }
    }

    pub const fn is_biookf(self) -> bool {
        matches!(self, Self::Biookf)
    }
}

/// Hand-written, and **lenient**, where [`crate::knowledge::biookf::NodeType`]'s
/// is hand-written and strict. The two are reading different things.
///
/// `NodeType` deserializes values *this build wrote* (a graph cache, a typed API
/// payload), so a word it does not know is a bug and failing is right. This
/// reads `manifest.yaml`, and DR-12 traces exactly what a failing manifest load
/// costs: `list_bases` drops the base, its id leaves the installed universe,
/// `repair_decision` reads the stored primary as uninstalled, and the next
/// selection edit **persists** the cleared `.active-kb`. The user loses their
/// pointers to a base that is sitting on disk intact, and downgrading does not
/// bring them back. A one-word typo must not cost that.
///
/// So an unrecognised profile resolves to [`KbFormat::Okf`] — not as a shrug,
/// but because it is the correct reading: every profile is OKF *plus*
/// constraints, so reading an unknown one as plain OKF loses constraints, never
/// content. That is OKF §11's own tolerance model applied to a value instead of
/// a key, and it is what a downgraded build must do with a profile name a later
/// build invented.
///
/// The cost is honest and worth stating: a base whose `format` is misspelled
/// `biokf` is checked as plain OKF, silently as far as the file is concerned.
/// The `tracing::warn!` is what makes it not silent.
impl<'de> Deserialize<'de> for KbFormat {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(Self::parse(&raw).unwrap_or_else(|| {
            tracing::warn!(
                "knowledge: manifest.yaml declares an unknown format `{raw}`; reading the base \
                 as plain OKF, which is every profile's common base"
            );
            Self::Okf
        }))
    }
}

/// The schema generation this build writes for a **new** base: the OKF
/// generation.
///
/// - **1** — before Plan 5 Task 2: no cross-reference rules section, so the
///   sub-agent was never told the graph is derived purely from `[[link]]`
///   patterns, and the bases of that era have nodes and no edges.
/// - **2** — with the cross-reference rules. Every base on disk carries this
///   content (see [`Self`]'s sibling [`AUTOMATIC_SCHEMA_CEILING`]).
/// - **3** — an OKF/BioOKF bundle: typed frontmatter, markdown-link edges, a
///   `type: Schema` `schema.md`, and the §8/§9 shapes for `index.md`/`log.md`.
///
/// ⚠ **3, and the number is not cosmetic** (DR-6). Stage 1.5's S-g wired
/// `schema_version` for the first time and had to give the *existing* content a
/// generation number, which took 2. Numbering OKF 2 would declare every base on
/// disk already-OKF, and every format check would then skip the bases that
/// actually need one — silently, because a skipped migration reports nothing.
///
/// Bump this together with the ladder in `service::migrated_schema`, never on
/// its own: the number is what decides that a base is behind, and the ladder is
/// what catches it up.
pub const CURRENT_SCHEMA_VERSION: u32 = 3;

/// The highest generation an **automatic** `schema.md` migration may carry an
/// existing base to.
///
/// Deliberately one below [`CURRENT_SCHEMA_VERSION`], and the gap is the whole
/// of requirement F. The 2 → 3 step is not a schema edit, it is a **format
/// migration**: it would have to rewrite the base's pages out of `title`/`kind`
/// frontmatter and `[[wiki]]` links into typed OKF frontmatter. DR-17 is
/// explicit that such a migration is a fifth privacy write choke point that
/// bypasses all four that exist — an eager one has no caller identity at all —
/// and DR-22 defers it outright. So the ladder stops here, a legacy base keeps
/// working untouched at generation 2, and nothing automatic ever stamps a base
/// as OKF that is not.
pub const AUTOMATIC_SCHEMA_CEILING: u32 = 2;

/// The gap above, asserted at **compile time** rather than in a test.
///
/// A ceiling that reached [`CURRENT_SCHEMA_VERSION`] would make the automatic
/// ladder into the format migration DR-17 refuses and DR-22 defers, and that is
/// too quiet a way to get there — bumping one number and not the other is a
/// one-character edit whose consequence is a privacy write choke point being
/// bypassed. This fails the build instead.
const _: () = assert!(
    AUTOMATIC_SCHEMA_CEILING < CURRENT_SCHEMA_VERSION,
    "an automatic schema migration must never be able to reach the OKF generation"
);

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

    // ── OKF profile (Stage 3, DR-6) ─────────────────────────────────────────
    /// Which profile the producer follows. Read it through
    /// [`Manifest::profile`], never on its own — see [`KbFormat`].
    #[serde(default)]
    #[cfg_attr(feature = "utoipa", schema(required))]
    pub format: KbFormat,
    /// The OKF revision this bundle declares, mirroring the bundle-root
    /// `index.md` frontmatter (OKF §8/§12). `None` on a base written before the
    /// OKF generation, which is the honest answer: it declares no revision.
    ///
    /// Unlike the six fields above these two take no `#[schema(required)]`.
    /// That pairing exists there because the server always serializes those, so
    /// the `serde(default)` describes only the read side; here the `None`
    /// describes genuinely absent data, and `required` would be a false
    /// statement about the response that the generated TypeScript would then
    /// believe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub okf_version: Option<String>,
    /// The BioOKF revision, when [`Self::format`] is `biookf`.
    ///
    /// It lives **here and not in `index.md`** (DR-23's corollary): OKF §8
    /// permits `okf_version` in a bundle-root index file and nothing else, so a
    /// `biookf_version` there is a conformance failure — which is exactly the
    /// deviation BioOKF's own spec makes and BioRouter does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biookf_version: Option<String>,
}

impl Manifest {
    /// The profile this base's pages are **actually** written in, or `None` for
    /// a base written before the OKF generation.
    ///
    /// This is the accessor every reader wants, and reading [`Self::format`]
    /// directly is the DR-6 trap: `format` defaults to `Okf` on the millions of
    /// bytes of `manifest.yaml` that predate it, so a check written against the
    /// field alone treats every legacy base as already-migrated. A legacy base
    /// gets `None` here and "is read through its own generation's path,
    /// unchanged, until the user migrates it".
    pub fn profile(&self) -> Option<KbFormat> {
        (self.schema_version >= CURRENT_SCHEMA_VERSION).then_some(self.format)
    }

    /// True for a base below the OKF generation: `title`/`kind` frontmatter and
    /// `[[wiki]]` links, still fully readable and never rewritten by this build.
    pub fn is_legacy_format(&self) -> bool {
        self.profile().is_none()
    }
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

/// `skip_serializing_if` for a `bool` whose interesting value is `true`.
///
/// Not cosmetic: a graph runs to thousands of nodes and edges, is written to
/// disk as `graph-cache.json` and is shipped whole over HTTP on every Knowledge
/// view mount. `"stale": false` on every node and `"negated": false` on every
/// edge is pure weight, and — the reason it matters more than size — it would
/// also make a *legacy* base's cache stop matching the one this build wrote
/// before Stage 2, for fields that base has no data for.
fn is_false(b: &bool) -> bool {
    !*b
}

// A node in the derived knowledge graph.
//
// A plain comment rather than a doc comment: utoipa publishes a struct's doc
// comment as its OpenAPI `description`, and the paragraphs below are addressed
// to whoever next edits the deriver.
//
// ⚠ **Every field added by the OKF work is `Option` or a defaulted collection,
// and every field added later must be too.** The five pre-OKF fields are the
// ones a base on disk has always had; everything after them describes a *typed*
// page, and the overwhelming majority of pages on disk today are untyped. A
// non-defaulted field here would fail to deserialize every `graph-cache.json`
// that exists — which `graph::read_cache` correctly treats as "re-derive", so
// the damage is a silent full re-walk of every base rather than an error, which
// is the kind of regression nothing reports.
//
// Unlike [`Manifest`], these do **not** take a paired `#[schema(required)]`.
// That pairing exists there because the server always serializes those fields,
// so the `serde(default)` describes only what is *read* and dropping them from
// the OpenAPI `required` list would loosen the published contract for no
// reason. Here the defaults describe genuinely absent data — a legacy page has
// no `type`, no `identifier` and no `stale_after` — so `required` would be a
// false statement about the response, and the generated TypeScript would be
// wrong in the direction that breaks at runtime rather than at compile time.
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
    /// The page's logical path, or the empty string for an `external` node,
    /// which has no page to open.
    pub path: String,

    // ── OKF typed layer (Stage 2) ───────────────────────────────────────────
    /// The concept document's own `type` (OKF §4.1), exactly as written.
    ///
    /// A raw `String` and not an enum, in both profiles. OKF leaves `type` open,
    /// and DR-7 forbids rejecting a page over an unrecognised value — an enum
    /// here would be a rejection mechanism wearing a different hat, silently
    /// replacing the producer's word with a fallback on the way past. `None` for
    /// every page in a legacy base, which carries `kind` and no `type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    /// BioOKF §7.1 `subtype`: agent-coined, never validated against anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    /// The display identity — `identifier`, or `title` as SPEC §14's deprecated
    /// alias for it. Distinct from `label`, which stays whatever the page list
    /// has always shown, so a renderer can change what it shows without the
    /// deriver changing what an edge resolves against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    /// OKF §5.4 `draft | stable | deprecated`, or whatever else the producer
    /// wrote. Emitted only when the page states one: §5.4 says an absent
    /// `status` *reads* as `stable`, and writing `stable` onto every legacy node
    /// would turn a consumer's assumption into the producer's assertion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// OKF §5.5: `stale_after` has passed. Computed here, at derivation time,
    /// rather than in the renderer — a renderer that compares dates would give a
    /// different answer per client clock and per cache age.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stale: bool,
    /// A node an edge points at that has no page in this bundle yet.
    ///
    /// OKF §11 makes a broken link something a consumer MUST tolerate, so this
    /// is not an error state — it is the curation queue, surfaced. See
    /// `graph::derive` for why a dangling *legacy* `[[…]]` link does not produce
    /// one of these.
    #[serde(default, skip_serializing_if = "is_false")]
    pub external: bool,
}

// An edge in the derived knowledge graph. See [`GraphNode`] for why every field
// past the first two is defaulted and why none of them takes `#[schema(required)]`.
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    /// The deprecated alias of [`Self::predicate`], carrying the identical
    /// value. Kept for one release because it is the only relation field the
    /// generated TypeScript client has ever had; new readers use `predicate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,

    // ── BioOKF typed layer (Stage 2) ────────────────────────────────────────
    /// The BioOKF §6 predicate. `None` for an untyped link, which is what makes
    /// "this edge has no type" answerable rather than inferred from an empty
    /// string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    /// SPEC §6.F polarity, emitted rather than left for the renderer to infer
    /// from a `not_` prefix — the prefix is one of two spellings (the other is
    /// the legacy `negated: true` attribute) and a renderer that knows only the
    /// first draws a negative claim as a positive one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub negated: bool,
    /// The BioOKF §8.1 provenance triplet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// The source node this claim came from: a node id when the identifier
    /// resolves to a page in this bundle, and otherwise the identifier exactly
    /// as written, so an unresolved one is visible instead of vanishing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub publications: Vec<String>,

    // ── BioOKF §7.3 quantitative bundle ─────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_metric: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_lower: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci_upper: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_size: Option<u64>,

    /// Every other edge attribute the producer wrote — `direction`, `aspect`,
    /// `clinical_phase`, `frequency`, and anything BioOKF adds after this build
    /// shipped — as text, in key order.
    ///
    /// An open map rather than a field per attribute, so a vocabulary addition
    /// costs no change here and no change in the renderer. The alternative loses
    /// data: the attribute exists on disk, and a deriver that models only what
    /// it recognises drops the rest on the floor where nothing can report it.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub qualifiers: std::collections::BTreeMap<String, String>,
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
            format: KbFormat::Okf,
            okf_version: None,
            biookf_version: None,
        };
        let s = serde_yaml::to_string(&m).unwrap();
        let back: Manifest = serde_yaml::from_str(&s).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn the_format_enum_writes_the_two_words_dr_6_names() {
        // `format: okf | biookf` is the on-disk grammar DR-6 specifies, and a
        // `rename_all` that drifted would be invisible until someone opened a
        // `manifest.yaml`.
        assert_eq!(serde_yaml::to_string(&KbFormat::Okf).unwrap().trim(), "okf");
        assert_eq!(
            serde_yaml::to_string(&KbFormat::Biookf).unwrap().trim(),
            "biookf"
        );
        // …and `as_str` is the same spelling, so a message and the serializer
        // cannot disagree.
        for f in [KbFormat::Okf, KbFormat::Biookf] {
            assert_eq!(
                serde_yaml::to_string(&f).unwrap().trim(),
                f.as_str(),
                "as_str drifted from the serializer for {f:?}"
            );
            assert_eq!(KbFormat::parse(f.as_str()), Some(f));
            assert_eq!(
                serde_yaml::from_str::<KbFormat>(f.as_str()).unwrap(),
                f,
                "the hand-written reader must still read what the writer wrote"
            );
        }
    }

    /// The lenient half of the hand-written reader, in both formats it is read
    /// from. See its doc comment for why leniency is the correct reading and
    /// not a shrug.
    #[test]
    fn an_unknown_profile_reads_as_plain_okf_in_both_yaml_and_json() {
        assert_eq!(KbFormat::parse("biokf"), None);
        assert_eq!(
            serde_yaml::from_str::<KbFormat>("okf-lite-2027").unwrap(),
            KbFormat::Okf
        );
        assert_eq!(
            serde_json::from_str::<KbFormat>("\"whatever\"").unwrap(),
            KbFormat::Okf
        );
        // Case is not folded: `Biookf` is a different word, and guessing at
        // capitalisation would be a second, undocumented rule.
        assert_eq!(
            serde_yaml::from_str::<KbFormat>("BioOKF").unwrap(),
            KbFormat::Okf
        );
    }

    /// DR-6's trap, as a property of the type: `format` alone says `Okf` for
    /// every base on disk, and only [`Manifest::profile`] says otherwise.
    #[test]
    fn a_legacy_generation_has_no_profile_however_the_format_field_reads() {
        let legacy = Manifest {
            id: "old".into(),
            name: "Old".into(),
            color: "#5a6394".into(),
            created_at: chrono::DateTime::UNIX_EPOCH,
            schema_version: AUTOMATIC_SCHEMA_CEILING,
            default_model: None,
            format: KbFormat::Okf,
            okf_version: None,
            biookf_version: None,
        };
        assert_eq!(legacy.format, KbFormat::Okf, "the field defaults to Okf");
        assert_eq!(legacy.profile(), None, "and it means nothing at gen 2");
        assert!(legacy.is_legacy_format());

        let current = Manifest {
            schema_version: CURRENT_SCHEMA_VERSION,
            format: KbFormat::Biookf,
            ..legacy.clone()
        };
        assert_eq!(current.profile(), Some(KbFormat::Biookf));
        assert!(!current.is_legacy_format());
    }

    /// The two numbers are the whole of requirement F, so they are pinned
    /// rather than left to be re-derived by a reader in a hurry.
    #[test]
    fn the_okf_generation_is_three_and_the_automatic_ladder_stops_below_it() {
        assert_eq!(CURRENT_SCHEMA_VERSION, 3, "DR-6: not 2, which is taken");
        assert_eq!(AUTOMATIC_SCHEMA_CEILING, 2);
        // The ordering between them is asserted at compile time beside the
        // constants themselves — a test can be skipped, a `const` block cannot.
    }
}
