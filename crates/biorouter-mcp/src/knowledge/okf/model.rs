//! The OKF v0.2 concept-document model.
//!
//! One struct per frontmatter family in OKF §4.1 and §5, plus the BioOKF §7
//! keys that ride alongside them, plus an `extra` mapping that catches
//! everything else. `extra` is not a convenience — it is the conformance
//! requirement: OKF §4.1 says "Consumers SHOULD preserve unknown keys when
//! round-tripping and MUST NOT reject documents with unrecognized fields", and
//! §11 repeats the second half as a MUST NOT. Without `#[serde(flatten)]` a
//! read-modify-write of a foreign bundle would silently delete every key this
//! build happens not to know about, which is the most destructive thing a
//! knowledge tool can do and the hardest to notice.
//!
//! ## Everything is lenient, because a parse failure *is* a rejection
//!
//! DR-7: "Nothing anywhere rejects a page on read." A `serde` error on one
//! field takes the whole document with it, so a struct built from strict field
//! types is a rejection mechanism wearing a different hat. Three defences,
//! layered:
//!
//! 1. **Every field is `Option` or a defaulted `Vec`/`String`**, including
//!    `type` — which OKF §4.1 makes REQUIRED. A missing `type` is reported by
//!    [`super::conformance`] as `okf.type.missing`, not by failing to parse.
//! 2. **Scalars are read leniently.** `Date`, `Timestamp`, `Actor` and `Status`
//!    accept any YAML scalar and keep its text; list-valued keys accept a bare
//!    scalar as a one-element list. A producer writing `tags: cytokine` instead
//!    of `tags: [cytokine]` loses nothing.
//! 3. **[`ConceptDoc::from_mapping`] is infallible.** If the derive path fails
//!    anyway, every key lands in `extra` and only the typed view is lost. The
//!    page still reads, still renders, and still round-trips.
//!
//! ## `type` is a `String` here on purpose
//!
//! OKF's vocabulary is open — §4.1: "Type values are **not** registered
//! centrally … consumers MUST tolerate unknown types gracefully." BioOKF closes
//! it to 28 values, and that check belongs in the profile module (Stage 1), not
//! here. An enum in this file would make the OKF profile unable to represent a
//! legal document.
//!
//! ## `title` versus `identifier`
//!
//! Both are modelled and both are read, and they are **not** the same field in
//! the two profiles. In OKF, `title` is a §4.1 Recommended display name and
//! `identifier` does not appear in the spec at all — it is a producer extension
//! under §4.1's "Producers MAY include any additional keys". In BioOKF `title`
//! is a *deprecated alias for* `identifier` (SPEC §14: "`title` → `identifier`";
//! "producers **SHOULD NOT** emit aliases"), so a BioOKF document carrying both
//! with different values has two conflicting primary keys and every `edges[]
//! .object` resolves against one of them arbitrarily. This module deliberately
//! does not resolve that: it preserves what it read. Alias normalisation and the
//! `identifier.alias_conflict` lint belong to `biookf/aliases.rs` in Stage 1,
//! where the profile is known.

use super::frontmatter::{self, FrontmatterError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_yaml::{Mapping, Value};

/// A whole concept document: the typed frontmatter plus the body it belongs to.
///
/// The pair travels together because half the checks need both — a `[^id]`
/// footnote in the body is only attributable through a `sources[].id` in the
/// frontmatter (OKF §5.1), so a validator handed only one of the two can never
/// see an unresolved citation.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub doc: ConceptDoc,
    pub body: String,
    /// False when the file carried no `---` block at all. Kept because OKF §11
    /// rule 1 is stated about the block's presence, and "empty frontmatter" and
    /// "no frontmatter" are different conformance answers.
    pub had_frontmatter: bool,
}

impl Page {
    pub fn parse(text: &str) -> Result<Self, FrontmatterError> {
        let split = frontmatter::split(text)?;
        Ok(Self {
            doc: ConceptDoc::from_mapping(split.frontmatter),
            body: split.body,
            had_frontmatter: split.had_block,
        })
    }

    pub fn render(&self) -> String {
        frontmatter::join(&self.doc.to_mapping(), &self.body)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConceptDoc {
    /// OKF §4.1 REQUIRED. Empty means absent; see the module header for why that
    /// is a diagnostic rather than a parse error. `skip_serializing_if` keeps a
    /// document that never had a `type` from gaining a `type: ""` on write.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub r#type: String,

    // ── OKF §4.1 Recommended ────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(
        default,
        deserialize_with = "de_string_list",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub tags: Vec<String>,

    // ── BioOKF §7.1 universal node attributes ───────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(
        default,
        deserialize_with = "de_string_list",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub xref: Vec<String>,
    #[serde(
        default,
        deserialize_with = "de_string_list",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub synonyms: Vec<String>,

    // ── OKF §5.4 / §5.5 lifecycle ───────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_after: Option<Date>,

    // ── OKF §5.2 trust ──────────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated: Option<Generated>,
    /// Kept in the shape the producer wrote it. §5.2 permits a bare mapping and
    /// §11 makes accepting it a MUST; normalising *here* would silently rewrite
    /// a legal document on the next write. [`super::trust::normalize_verified`]
    /// does the normalisation at the point of use instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<VerifiedField>,

    // ── OKF §5.1 provenance ─────────────────────────────────────────────────
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<Source>,
    /// Written once as a sibling of `sources` (§5.1); an entry may override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_window: Option<UsageWindow>,

    // ── BioOKF §6 typed graph layer ─────────────────────────────────────────
    /// Permitted-and-ignored in OKF mode: §4.1 lets producers add any key and
    /// §11 forbids a consumer rejecting one, so an OKF consumer that has never
    /// heard of BioOKF reads such a page without complaint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<Edge>,

    /// Every key this build does not model. See the module header.
    #[serde(flatten)]
    pub extra: Mapping,
}

impl ConceptDoc {
    /// Infallible by contract (DR-7). On the fallback path the typed view is
    /// empty and `extra` holds the entire mapping minus `type`, so nothing is
    /// lost and [`Self::to_mapping`] still reproduces the input.
    pub fn from_mapping(map: Mapping) -> Self {
        match serde_yaml::from_value::<Self>(Value::Mapping(map.clone())) {
            Ok(doc) => doc,
            Err(_) => Self::salvage(map),
        }
    }

    /// The `type` key is lifted out of `extra` so [`Self::to_mapping`] does not
    /// emit it twice — once from the struct field and once from the flattened
    /// remainder.
    fn salvage(mut map: Mapping) -> Self {
        let r#type = map
            .remove(Value::String("type".into()))
            .as_ref()
            .and_then(scalar_to_string)
            .unwrap_or_default();
        Self {
            r#type,
            extra: map,
            ..Self::default()
        }
    }

    pub fn to_mapping(&self) -> Mapping {
        match serde_yaml::to_value(self) {
            Ok(Value::Mapping(m)) => m,
            // Unreachable for this struct; falling back to the preserved
            // unknown keys loses the typed fields but never the document.
            _ => self.extra.clone(),
        }
    }

    /// The `identifier` a BioOKF edge's `object` resolves against, falling back
    /// through the aliases OKF and BioOKF each permit. This is the first two
    /// rungs of DR-3's ladder; the `br_page_id` rung and the basename rung need
    /// a path and a store, so they live in the graph deriver (Stage 2).
    pub fn primary_key(&self) -> Option<&str> {
        self.identifier
            .as_deref()
            .or(self.title.as_deref())
            .filter(|s| !s.is_empty())
    }
}

// ── §5.1 provenance ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// §5.1: "REQUIRED within an entry." Optional in this struct anyway, so a
    /// producer's omission costs one diagnostic instead of the whole page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    /// §5.1: "A stable key used to attribute individual claims." The join key a
    /// `[^id]` footnote in the body resolves through — keyed rather than
    /// positional because "a positional index misattributes silently the moment
    /// the list is reordered", and agents reorder these lists constantly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// An authority signal, in the §7 actor convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<Actor>,
    /// An adoption/liveness signal, framed by `usage_window`. §5.1 warns it is
    /// coarse: "Consumers SHOULD read it as liveness and trend, not as a score."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_count: Option<u64>,
    /// A recency signal about the *source*, deliberately distinct from
    /// `generated.at`, which is about the concept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<Date>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_window: Option<UsageWindow>,
    #[serde(flatten)]
    pub extra: Mapping,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Date>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Date>,
}

// ── §5.2 trust ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Generated {
    /// §5.2 REQUIRED within `generated`; defaulted for the same reason `type` is.
    #[serde(default, skip_serializing_if = "Actor::is_empty")]
    pub by: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Timestamp>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Verified {
    #[serde(default, skip_serializing_if = "Actor::is_empty")]
    pub by: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Timestamp>,
}

/// `verified` as written, before normalisation.
///
/// §5.2: "A single verifier MAY be written as one `{ by, at }` mapping without
/// the list dash. Consumers MUST treat a bare mapping as a one-element list."
/// Modelling the two shapes rather than coercing on read is what lets a
/// round-trip give the producer's file back unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VerifiedField {
    /// The bare `{ by, at }` mapping.
    One(Verified),
    Many(Vec<Verified>),
}

// ── BioOKF §6 / §7.2 edges ──────────────────────────────────────────────────

/// A typed edge. The five BioOKF-required attributes are named; the §7.3
/// quantitative bundle (`p_value`, `effect_size`, `ci_lower`, …) and every other
/// optional attribute ride in `extra`, so an edge never loses a statistic this
/// build has not heard of.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub predicate: String,
    /// The target node's `identifier` (BioOKF §7.2), never a CURIE and never a
    /// path.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_source: Option<String>,
    /// The legacy polarity form. BioOKF §7.2 normalises `negated: true` on a
    /// negatable predicate to the canonical `not_<X>`; that normalisation needs
    /// the negatable set, so it is Stage 1's, and this field only records what
    /// was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negated: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect: Option<String>,
    #[serde(
        default,
        deserialize_with = "de_string_list",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub publications: Vec<String>,
    #[serde(flatten)]
    pub extra: Mapping,
}

// ── §5.4 lifecycle ──────────────────────────────────────────────────────────

/// `draft | stable | deprecated` (§5.4), plus [`Status::Other`] for anything
/// else.
///
/// `Other` exists because §11 forbids rejecting a document over an unknown
/// value, and a three-variant enum would do exactly that — a page saying
/// `status: archived` would fail to deserialize and disappear. It also keeps the
/// unrecognised word intact for the round trip instead of quietly rewriting it
/// to `stable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Draft,
    Stable,
    Deprecated,
    Other(String),
}

impl Status {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Draft => "draft",
            Self::Stable => "stable",
            Self::Deprecated => "deprecated",
            Self::Other(s) => s,
        }
    }

    fn from_text(s: &str) -> Self {
        match s {
            "draft" => Self::Draft,
            "stable" => Self::Stable,
            "deprecated" => Self::Deprecated,
            other => Self::Other(other.to_string()),
        }
    }
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Status::from_text(&de_scalar_string(d)?))
    }
}

// ── §7 actor convention ─────────────────────────────────────────────────────

/// An identity in the §7 convention: `<producer>/<version>`, `human:<id>`, or
/// `process:<id>`.
///
/// A newtype over the raw string rather than a parsed enum, because the string
/// is what round-trips and because §7 makes exactly one prefix load-bearing:
/// "Consumers that classify trust (§5.3) key off the `human:` prefix." Anything
/// else is presentation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Actor(pub String);

/// What an actor string looks like. Advisory — only [`Actor::is_human`] feeds a
/// decision anywhere in this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Human,
    Process,
    /// `<producer>/<version>`, e.g. `reference_agent/gemini-2.5-pro`.
    Tool,
    /// Conforms to none of §7's three shapes. Not an error: §11's tolerances
    /// cover it and a bundle from a v0.1 producer is full of these.
    Unclassified,
}

impl Actor {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The one classification §5.3 actually depends on. Kept as its own method
    /// so the `human:` literal has a single spelling in the tree — a second copy
    /// is how a trust tier silently stops being reachable.
    pub fn is_human(&self) -> bool {
        self.0.starts_with("human:")
    }

    pub fn kind(&self) -> ActorKind {
        if self.is_human() {
            ActorKind::Human
        } else if self.0.starts_with("process:") {
            ActorKind::Process
        } else if self.0.contains('/') {
            ActorKind::Tool
        } else {
            ActorKind::Unclassified
        }
    }
}

impl Serialize for Actor {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Actor {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Actor(de_scalar_string(d)?))
    }
}

// ── Dates and timestamps ────────────────────────────────────────────────────

/// A `YYYY-MM-DD` date (§5.5 `stale_after`, §5.1 `last_modified`), stored as its
/// text.
///
/// Text and not `NaiveDate` for two reasons. YAML resolves an unquoted
/// `2027-01-01` to a plain string under the core schema, but a producer may also
/// quote it, and either way the file's own spelling is what should come back out
/// of a round trip. And a *malformed* date must not take the page with it —
/// [`Date::parse`] returns `None` and [`super::trust::is_stale`] reads that as
/// "unknown", so one typo cannot flip a whole base to stale.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Date(pub String);

impl Date {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(&self) -> Option<chrono::NaiveDate> {
        chrono::NaiveDate::parse_from_str(&self.0, "%Y-%m-%d").ok()
    }
}

/// An ISO 8601 datetime (`generated.at`, `verified[].at`), stored as its text
/// for the same reasons as [`Date`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timestamp(pub String);

impl Timestamp {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::parse_from_rfc3339(&self.0)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    }
}

macro_rules! scalar_newtype_serde {
    ($t:ty) => {
        impl Serialize for $t {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }
        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Ok(Self(de_scalar_string(d)?))
            }
        }
    };
}
scalar_newtype_serde!(Date);
scalar_newtype_serde!(Timestamp);

// ── Lenient scalar readers ──────────────────────────────────────────────────

/// Render any YAML scalar as text. `None` for collections, which have no
/// sensible one-line spelling.
fn scalar_to_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Accept any scalar where a string is modelled.
///
/// The case this exists for: `stale_after: 2027` (a bare year) or
/// `identifier: 12345`. YAML types those as numbers; a `String` field would
/// reject the document, and rejecting a document over the *type of a date* is
/// exactly what DR-7 forbids.
fn de_scalar_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = Value::deserialize(d)?;
    scalar_to_string(&v).ok_or_else(|| serde::de::Error::custom("expected a scalar"))
}

/// Accept a list, or a bare scalar as a one-element list.
///
/// `tags: cytokine` instead of `tags: [cytokine]` is the single commonest thing
/// a language model writes into a list-valued key, and a strict `Vec<String>`
/// turns it into a whole unreadable page.
fn de_string_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(match v {
        Value::Null => Vec::new(),
        Value::Sequence(items) => items.iter().filter_map(scalar_to_string).collect(),
        other => scalar_to_string(&other).into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf::fixtures;

    fn parse(text: &str) -> Page {
        Page::parse(text).expect("fixture parses")
    }

    #[test]
    fn minimal_document_needs_only_a_type() {
        let p = parse(fixtures::MINIMAL);
        assert_eq!(p.doc.r#type, "Reference");
        assert!(p.doc.title.is_none());
        assert!(p.doc.sources.is_empty());
    }

    #[test]
    fn the_full_v0_2_document_reads_every_family() {
        let d = parse(fixtures::FULL_V0_2).doc;
        assert_eq!(d.r#type, "BigQuery Table");
        assert_eq!(d.title.as_deref(), Some("Customer Orders"));
        assert_eq!(d.tags, vec!["sales", "orders"]);
        assert_eq!(d.status, Some(Status::Stable));
        assert_eq!(d.stale_after.as_ref().unwrap().as_str(), "2026-09-23");
        assert!(d.generated.as_ref().unwrap().by.kind() == ActorKind::Tool);
        assert_eq!(d.sources.len(), 2);
        assert_eq!(d.sources[0].id.as_deref(), Some("ga4-schema"));
        assert_eq!(d.sources[0].usage_count, Some(5000));
        assert_eq!(
            d.usage_window.as_ref().unwrap().from.as_ref().unwrap().0,
            "2026-06-01"
        );
    }

    #[test]
    fn unknown_producer_keys_survive_a_round_trip() {
        // OKF §4.1's "preserve unknown keys when round-tripping". The failure
        // this pins is silent: without `extra`, reading and re-writing a foreign
        // bundle deletes every key this build does not model.
        let page = parse(fixtures::UNKNOWN_KEYS);
        let extra = &page.doc.extra;
        assert!(extra.contains_key(Value::String("br_page_id".into())));
        assert!(extra.contains_key(Value::String("acme_cost_center".into())));

        let again = parse(&page.render());
        assert_eq!(again.doc, page.doc, "typed view drifted");
        assert_eq!(again.doc.extra, page.doc.extra, "unknown keys drifted");
        assert_eq!(again.body, page.body, "body drifted");
    }

    #[test]
    fn every_parseable_fixture_round_trips_without_losing_a_key() {
        // The Stage 0 gate, expressed against parsed mappings rather than bytes:
        // serde_yaml re-emits `3.0e-6` as `3e-6`, so a byte comparison would
        // fail while proving nothing about content. Mapping equality is
        // order-insensitive and value-exact, which is the property we want.
        for (name, text) in fixtures::ROUND_TRIPPABLE {
            let page = parse(text);
            let rendered = page.render();
            let again = parse(&rendered);
            assert_eq!(again.doc.to_mapping(), page.doc.to_mapping(), "{name}");
            assert_eq!(again.body, page.body, "{name} body");
        }
    }

    #[test]
    fn a_bare_verified_mapping_keeps_its_bare_shape_on_disk() {
        // Normalising on read would rewrite a legal §5.2 document into a list
        // the next time anything touched it. `trust::normalize_verified` is
        // where the MUST is honoured; the model only preserves.
        let d = parse(fixtures::BARE_VERIFIED).doc;
        assert!(matches!(d.verified, Some(VerifiedField::One(_))));
        let re = ConceptDoc::from_mapping(d.to_mapping());
        assert!(matches!(re.verified, Some(VerifiedField::One(_))));
    }

    #[test]
    fn a_verified_list_stays_a_list() {
        let d = parse(fixtures::FULL_V0_2).doc;
        match d.verified {
            Some(VerifiedField::Many(ref v)) => assert_eq!(v.len(), 2),
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn the_biookf_worked_example_reads_as_typed_edges() {
        let d = parse(fixtures::TOCILIZUMAB).doc;
        assert_eq!(d.r#type, "Molecule");
        assert_eq!(d.identifier.as_deref(), Some("Tocilizumab"));
        assert_eq!(d.edges.len(), 7, "SPEC §12 declares seven typed edges");
        let covid = d
            .edges
            .iter()
            .find(|e| e.object == "COVID-19")
            .expect("the COVID-19 treats edge");
        assert_eq!(covid.predicate, "treats");
        assert_eq!(covid.primary_source.as_deref(), Some("RECOVERY trial"));
        assert_eq!(covid.publications, vec!["PMID:33933206"]);
        // The §7.3 quantitative bundle is unmodelled by name and must still be
        // there — an edge that loses its effect size loses the finding.
        assert!(covid
            .extra
            .contains_key(Value::String("effect_size".into())));
        assert!(covid
            .extra
            .contains_key(Value::String("sample_size".into())));
    }

    #[test]
    fn a_document_with_no_frontmatter_parses_with_an_empty_type() {
        // Non-conformant per §11 rule 2, reported by `conformance`, never a
        // parse failure.
        let p = parse(fixtures::NO_FRONTMATTER);
        assert!(!p.had_frontmatter);
        assert!(p.doc.r#type.is_empty());
        assert!(p.body.starts_with("# A plain note"));
    }

    #[test]
    fn an_absent_type_does_not_gain_an_empty_string_on_write() {
        let page = parse(fixtures::NO_FRONTMATTER);
        assert!(
            !page.render().contains("type:"),
            "an empty type must not be materialised: {}",
            page.render()
        );
    }

    #[test]
    fn an_unterminated_block_is_the_one_shape_that_fails_to_split() {
        assert_eq!(
            Page::parse(fixtures::UNTERMINATED).unwrap_err(),
            FrontmatterError::Unterminated
        );
    }

    #[test]
    fn an_unknown_status_word_survives_instead_of_becoming_stable() {
        let d = ConceptDoc::from_mapping(
            frontmatter::split("---\ntype: X\nstatus: archived\n---\n")
                .unwrap()
                .frontmatter,
        );
        assert_eq!(d.status, Some(Status::Other("archived".into())));
        assert!(d.to_mapping().contains_key(Value::String("status".into())));
    }

    #[test]
    fn a_scalar_where_a_list_belongs_is_read_as_a_one_element_list() {
        let d = ConceptDoc::from_mapping(
            frontmatter::split("---\ntype: X\ntags: cytokine\nxref: HGNC:6018\n---\n")
                .unwrap()
                .frontmatter,
        );
        assert_eq!(d.tags, vec!["cytokine"]);
        assert_eq!(d.xref, vec!["HGNC:6018"]);
    }

    #[test]
    fn a_numeric_scalar_where_a_string_belongs_does_not_reject_the_page() {
        let d = ConceptDoc::from_mapping(
            frontmatter::split("---\ntype: X\nstale_after: 2027\n---\n")
                .unwrap()
                .frontmatter,
        );
        assert_eq!(d.stale_after.as_ref().unwrap().as_str(), "2027");
        assert!(d.stale_after.unwrap().parse().is_none(), "not a full date");
    }

    #[test]
    fn from_mapping_never_fails_even_on_a_wholly_wrong_shape() {
        // The DR-7 backstop. `sources` as a scalar defeats every lenient reader
        // above, and the page must still come back with all of its content.
        let map = frontmatter::split("---\ntype: X\nsources: nonsense\nkeep: me\n---\n")
            .unwrap()
            .frontmatter;
        let d = ConceptDoc::from_mapping(map);
        assert_eq!(d.r#type, "X", "type is lifted out on the salvage path");
        assert!(d.extra.contains_key(Value::String("sources".into())));
        assert!(d.extra.contains_key(Value::String("keep".into())));
        assert_eq!(
            d.to_mapping()
                .get(Value::String("type".into()))
                .and_then(|v| v.as_str()),
            Some("X"),
            "type must appear exactly once, not twice"
        );
    }

    #[test]
    fn primary_key_prefers_identifier_and_falls_back_to_title() {
        let with_both = ConceptDoc {
            identifier: Some("IL6 (protein)".into()),
            title: Some("Interleukin-6".into()),
            ..ConceptDoc::default()
        };
        assert_eq!(with_both.primary_key(), Some("IL6 (protein)"));
        let title_only = ConceptDoc {
            title: Some("Interleukin-6".into()),
            ..ConceptDoc::default()
        };
        assert_eq!(title_only.primary_key(), Some("Interleukin-6"));
        assert_eq!(ConceptDoc::default().primary_key(), None);
    }

    #[test]
    fn actor_kinds_follow_the_section_7_convention() {
        assert_eq!(Actor("human:ahormati".into()).kind(), ActorKind::Human);
        assert_eq!(
            Actor("process:finance-nightly".into()).kind(),
            ActorKind::Process
        );
        assert_eq!(
            Actor("reference_agent/gemini-2.5-pro".into()).kind(),
            ActorKind::Tool
        );
        assert_eq!(Actor("Chen et al.".into()).kind(), ActorKind::Unclassified);
        // Only the prefix is load-bearing, and it is a prefix, not a substring:
        // an actor merely *mentioning* a human is not one.
        assert!(!Actor("team:human-factors".into()).is_human());
    }

    #[test]
    fn timestamps_and_dates_parse_when_well_formed_and_yield_none_otherwise() {
        assert!(Timestamp("2026-06-25T09:00:00Z".into()).parse().is_some());
        assert!(Timestamp("last tuesday".into()).parse().is_none());
        assert!(Date("2026-09-23".into()).parse().is_some());
        assert!(Date("2026-09".into()).parse().is_none());
    }
}
