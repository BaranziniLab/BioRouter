//! SPEC §14's deprecated aliases: accepted on read, normalized to v0.5, never
//! emitted.
//!
//! §11 makes accepting them a SHOULD — "This lets v0.1/v0.2/v0.3/v0.4 and
//! parallel-draft bundles validate without rewriting" — and the reason to take
//! that seriously is that an alias-blind reader does not report a v0.3 bundle as
//! old, it reports every page in it as *invalid*, which is indistinguishable
//! from the bundle being broken.
//!
//! ## What normalisation does and does not touch
//!
//! [`normalize`] returns a **copy** with the canonical spellings filled in and a
//! list of what it changed. It never writes to disk and never mutates its input,
//! because the caller has to be free to lint the document as written and *then*
//! decide whether a rewrite is wanted — a producer rewriting a foreign bundle on
//! read is how a consumer becomes a silent editor.
//!
//! Two aliases are reported and deliberately **not** applied:
//!
//! - **Inverse predicate names** (`encoded_by`, `caused_by`, `treated_by`,
//!   `produces`). §14 normalizes them to "the forward predicate authored on the
//!   *other* node" — the direction flips. Rewriting `A encoded_by B` to
//!   `A encodes B` in place would state the opposite of what the author wrote,
//!   so [`resolve_predicate`] reports the reversal and leaves the edge alone.
//!   Only a caller that can move the edge to the other page (the Stage 2 graph
//!   deriver) may act on it.
//! - **`primary_source: infores:X`**. §14 normalizes it by *synthesizing a
//!   source node* — a whole new page — which is a bundle-level operation, not a
//!   per-document one.

use super::vocabulary::{NodeType, PositivePredicate, Predicate, PredicateError};
use crate::knowledge::okf::{ConceptDoc, Edge};
use serde_yaml::Value;

/// The one non-node value SPEC §8.1 reserves for `primary_source`: "a **rare
/// escape** for claims whose origin is genuinely unknown, never a default".
pub const NOT_PROVIDED: &str = "not_provided";

/// The prefix a pre-v0.5 `primary_source` used instead of naming a source node.
pub const INFORES_PREFIX: &str = "infores:";

// ── deprecated `type` aliases (SPEC §14) ────────────────────────────────────

/// A resolved `type` alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    /// What the document said.
    pub deprecated: String,
    /// What it normalizes to, after `subtype` disambiguation.
    pub canonical: NodeType,
    /// Every value §14 lists for this alias. More than one means the alias is
    /// genuinely ambiguous and `canonical` is a *guess* made from `subtype` —
    /// worth saying in the diagnostic, because the curator is the only one who
    /// can settle it.
    pub candidates: &'static [NodeType],
}

impl TypeAlias {
    pub fn is_ambiguous(&self) -> bool {
        self.candidates.len() > 1
    }
}

/// Resolve a deprecated `type` value. `None` for a canonical name (nothing to
/// do) and for a genuinely unknown one (that is [`super::lint`]'s to report).
///
/// `subtype` is taken because three of the eight aliases split into several
/// v0.5 types "by `subtype`" / "by content"; without it the resolution would
/// have to guess blind, and a `Process` that is really a GO molecular function
/// would land in `BiologicalPathway` every time.
pub fn normalize_type(deprecated: &str, subtype: Option<&str>) -> Option<TypeAlias> {
    let sub = subtype.unwrap_or_default();
    let (canonical, candidates) = match deprecated {
        "SDOH" | "SDoH" => (NodeType::SocialFactor, &[NodeType::SocialFactor][..]),
        "ClinicalMeasure" => (
            NodeType::BiomedicalMeasure,
            &[NodeType::BiomedicalMeasure][..],
        ),
        "Procedure" | "Method" => (
            NodeType::MethodOrProcedure,
            &[NodeType::MethodOrProcedure][..],
        ),
        "GenomicFeature" => (resolve_genomic_feature(sub), GENOMIC_FEATURE_CANDIDATES),
        "Process" | "BiologicalProcess" => (resolve_process(sub), PROCESS_CANDIDATES),
        "ExposureOrFactor" => (resolve_exposure_or_factor(sub), EXPOSURE_CANDIDATES),
        _ => return None,
    };
    Some(TypeAlias {
        deprecated: deprecated.to_string(),
        canonical,
        candidates,
    })
}

const GENOMIC_FEATURE_CANDIDATES: &[NodeType] = &[NodeType::Variant, NodeType::SequenceFeature];
const PROCESS_CANDIDATES: &[NodeType] =
    &[NodeType::BiologicalPathway, NodeType::BiologicalFunction];
const EXPOSURE_CANDIDATES: &[NodeType] = &[
    NodeType::Exposure,
    NodeType::SocialFactor,
    NodeType::Food,
    NodeType::Population,
    NodeType::GeographicLocation,
];

/// §5.D: "a *deviation from the reference* is a `Variant`; a *region of the
/// reference* is a `SequenceFeature`". The subtype vocabulary is §5.A's example
/// list for `SequenceFeature`; anything else falls to `Variant`, which is what
/// the v0.2 umbrella mostly held.
fn resolve_genomic_feature(subtype: &str) -> NodeType {
    const REGIONS: &[&str] = &[
        "enhancer",
        "promoter",
        "silencer",
        "tfbs",
        "cpg_island",
        "open_chromatin",
        "transposon",
        "utr",
    ];
    if REGIONS.contains(&subtype) {
        NodeType::SequenceFeature
    } else {
        NodeType::Variant
    }
}

/// §5.D: an *elemental molecular activity* (GO-MF) is a `BiologicalFunction`;
/// every other process is a `BiologicalPathway`.
fn resolve_process(subtype: &str) -> NodeType {
    const MOLECULAR_FUNCTIONS: &[&str] = &["catalytic", "binding", "transporter", "go_mf"];
    if MOLECULAR_FUNCTIONS.contains(&subtype) {
        NodeType::BiologicalFunction
    } else {
        NodeType::BiologicalPathway
    }
}

/// §14: "`ExposureOrFactor` → `Exposure` / `SocialFactor` / `Food` /
/// `Population` / `GeographicLocation` **by content**". The subtype examples
/// from §5.A are the only content signal available on read.
fn resolve_exposure_or_factor(subtype: &str) -> NodeType {
    match subtype {
        "economic" | "education" | "housing" | "healthcare_access" | "social_support"
        | "food_security" => NodeType::SocialFactor,
        "food_item" | "food_group" | "dietary_product" => NodeType::Food,
        "cohort" | "ancestry" | "demographic" => NodeType::Population,
        "country" | "region" | "place" => NodeType::GeographicLocation,
        _ => NodeType::Exposure,
    }
}

// ── deprecated attribute aliases (SPEC §14) ─────────────────────────────────

/// What a deprecated frontmatter key becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeAlias {
    /// `title` → `identifier`.
    Identifier,
    /// `id` → `identifier` when it is human-readable, `xref` when it is a CURIE.
    IdentifierOrXref,
    /// `<type>_kind`, `class_basis`, Structure's `method` → `subtype`.
    Subtype,
    /// `provided_by` → a `reported_in` edge (the field was removed in v0.5).
    ReportedInEdge,
}

impl AttributeAlias {
    pub const fn canonical_key(self) -> &'static str {
        match self {
            Self::Identifier | Self::IdentifierOrXref => "identifier",
            Self::Subtype => "subtype",
            Self::ReportedInEdge => "edges[].reported_in",
        }
    }
}

/// Classify a frontmatter key against §14's attribute-alias table.
///
/// The `*_kind` family is matched by suffix rather than enumerated, because §14
/// itself writes it open-ended ("e.g. `molecule_kind`, `feature_kind`, …") — a
/// closed list would silently pass a v0.3 bundle's `structure_kind` through as
/// an unknown key and lose the subtype.
pub fn attribute_alias(key: &str) -> Option<AttributeAlias> {
    match key {
        "title" => Some(AttributeAlias::Identifier),
        "id" => Some(AttributeAlias::IdentifierOrXref),
        "class_basis" | "method" => Some(AttributeAlias::Subtype),
        "provided_by" => Some(AttributeAlias::ReportedInEdge),
        k if k.ends_with("_kind") => Some(AttributeAlias::Subtype),
        _ => None,
    }
}

/// A CURIE, not a name: `prefix:local` with no whitespace and no parenthetical
/// facet. SPEC §7.1 asks an `identifier` to be human-readable and to avoid `:`
/// precisely so this test is decidable, and §14 uses the same test to decide
/// whether a legacy `id` becomes the `identifier` or an `xref`.
///
/// Conservative on purpose: `IL6 (protein)` and `Chen 2020 (IL-6 and severe
/// COVID-19)` both carry a colon-free parenthetical and must not be flagged,
/// and a real identifier with a colon in prose ("Study 3: follow-up") has
/// spaces.
pub fn looks_like_bare_curie(value: &str) -> bool {
    value.contains(':') && !value.contains(char::is_whitespace) && !value.contains('(')
}

// ── predicate resolution ────────────────────────────────────────────────────

/// A predicate token read off an edge, with everything §14 and §6.F say about
/// how it got there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPredicate {
    pub predicate: Predicate,
    /// The author used a §14 inverse alias, so the relationship really runs
    /// object → subject. The edge is **not** rewritten; see the module header.
    pub reversed: bool,
    /// Polarity came from the legacy `negated: true` qualifier (§7.2) rather
    /// than from a `not_<X>` predicate name.
    pub from_legacy_negated: bool,
}

/// §14's four inverse aliases, each mapping to the forward predicate that must
/// be authored on the *other* node.
const INVERSE_ALIASES: &[(&str, PositivePredicate)] = &[
    ("encoded_by", PositivePredicate::Encodes),
    ("caused_by", PositivePredicate::Causes),
    ("treated_by", PositivePredicate::Treats),
    ("produces", PositivePredicate::Catalyzes),
];

/// Resolve an edge's `predicate` plus its legacy `negated` qualifier into one
/// canonical [`Predicate`].
///
/// The two polarity mechanisms are combined here rather than at each call site
/// because they can contradict: `predicate: not_treats` with `negated: false`
/// is a document saying both things at once. The rule taken is that the
/// *canonical* form wins — a `not_<X>` predicate is negative whatever the legacy
/// flag says — because §6.F makes `not_<X>` the canonical mechanism and §7.2
/// demotes `negated` to "Legacy".
///
/// `negated: true` on a non-negatable predicate is an error, not a silent
/// downgrade to the positive: §6.F says it is "rejected (`edge.not_negatable`)"
/// and dropping the flag would turn a stated negative finding into its opposite.
pub fn resolve_predicate(
    raw: &str,
    legacy_negated: Option<bool>,
) -> Result<ResolvedPredicate, PredicateError> {
    let legacy = legacy_negated.unwrap_or(false);
    if let Some(&(alias, forward)) = INVERSE_ALIASES.iter().find(|(a, _)| *a == raw) {
        debug_assert_eq!(alias, raw);
        let predicate = apply_legacy_negation(forward, legacy)?;
        return Ok(ResolvedPredicate {
            predicate,
            reversed: true,
            from_legacy_negated: legacy,
        });
    }
    let predicate = Predicate::parse(raw)?;
    if predicate.is_negated() || !legacy {
        return Ok(ResolvedPredicate {
            predicate,
            reversed: false,
            from_legacy_negated: false,
        });
    }
    Ok(ResolvedPredicate {
        predicate: apply_legacy_negation(predicate.base(), true)?,
        reversed: false,
        from_legacy_negated: true,
    })
}

fn apply_legacy_negation(
    base: PositivePredicate,
    negated: bool,
) -> Result<Predicate, PredicateError> {
    if !negated {
        return Ok(Predicate::positive(base));
    }
    Predicate::negated(base).ok_or(PredicateError::NotNegatable(base))
}

/// True for the pre-v0.5 `primary_source` form §14 normalizes by synthesizing a
/// source node.
pub fn is_legacy_primary_source(value: &str) -> bool {
    value.starts_with(INFORES_PREFIX)
}

// ── whole-document normalisation ────────────────────────────────────────────

/// Which §14 alias a note is about. [`super::lint`] maps these to rule ids; the
/// mapping lives there so every rule id in the profile has one home.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasKind {
    Type,
    Attribute,
    InversePredicate,
    NegatedQualifier,
    PrimarySourceCurie,
}

/// One thing [`normalize`] changed, or declined to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasNote {
    pub kind: AliasKind,
    pub message: String,
}

impl AliasNote {
    fn new(kind: AliasKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// The result of reading a possibly-legacy document.
#[derive(Debug, Clone)]
pub struct Normalized {
    pub doc: ConceptDoc,
    pub notes: Vec<AliasNote>,
}

/// Accept a v0.1–v0.4 document and hand back its v0.5 reading.
pub fn normalize(doc: &ConceptDoc) -> Normalized {
    let mut out = doc.clone();
    let mut notes = Vec::new();
    // `subtype` first, and the order is load-bearing: three of §14's eight type
    // aliases split "by `subtype`", and a v0.3 page spells its subtype
    // `feature_kind`. Resolving the type before lifting that key would resolve
    // `GenomicFeature` blind on every legacy page — precisely the pages the
    // alias exists for.
    normalize_subtype(&mut out, &mut notes);
    normalize_type_field(&mut out, &mut notes);
    normalize_identifier(&mut out, &mut notes);
    normalize_provided_by(&mut out, &mut notes);
    normalize_edges(&mut out, &mut notes);
    Normalized { doc: out, notes }
}

fn normalize_type_field(doc: &mut ConceptDoc, notes: &mut Vec<AliasNote>) {
    if NodeType::parse(&doc.r#type).is_some() {
        return;
    }
    let Some(alias) = normalize_type(&doc.r#type, doc.subtype.as_deref()) else {
        return;
    };
    let hedge = if alias.is_ambiguous() {
        format!(
            " (ambiguous: §14 splits it across {}; resolved from `subtype`)",
            alias
                .candidates
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join("/")
        )
    } else {
        String::new()
    };
    notes.push(AliasNote::new(
        AliasKind::Type,
        format!(
            "§14: deprecated type `{}` reads as `{}`{hedge}",
            alias.deprecated, alias.canonical
        ),
    ));
    doc.r#type = alias.canonical.as_str().to_string();
}

/// `title` → `identifier`, then a legacy `id` → `identifier` or `xref`.
///
/// Order matters and is the §14 order: `title` is a plain rename, while `id`
/// splits in two. A document carrying both `title` and a human-readable `id`
/// keeps the `title` as its key, because that is the field a v0.4 reader would
/// have shown the user.
fn normalize_identifier(doc: &mut ConceptDoc, notes: &mut Vec<AliasNote>) {
    if doc.identifier.as_deref().is_none_or(str::is_empty) {
        if let Some(title) = doc.title.clone().filter(|t| !t.is_empty()) {
            notes.push(AliasNote::new(
                AliasKind::Attribute,
                format!("§14: `title` is a deprecated alias for `identifier` (`{title}`)"),
            ));
            doc.identifier = Some(title);
        }
    }
    let Some(id) = take_string(doc, "id") else {
        return;
    };
    if looks_like_bare_curie(&id) {
        notes.push(AliasNote::new(
            AliasKind::Attribute,
            format!("§14: legacy CURIE key `id: {id}` moves to `xref`"),
        ));
        if !doc.xref.iter().any(|x| x == &id) {
            doc.xref.push(id);
        }
    } else if doc.identifier.as_deref().is_none_or(str::is_empty) {
        notes.push(AliasNote::new(
            AliasKind::Attribute,
            format!("§14: legacy key `id: {id}` reads as `identifier`"),
        ));
        doc.identifier = Some(id);
    }
}

fn normalize_subtype(doc: &mut ConceptDoc, notes: &mut Vec<AliasNote>) {
    if doc.subtype.is_some() {
        return;
    }
    let key = doc
        .extra
        .iter()
        .filter_map(|(k, _)| k.as_str())
        .find(|k| attribute_alias(k) == Some(AttributeAlias::Subtype))
        .map(str::to_string);
    let Some(key) = key else { return };
    let Some(value) = take_string(doc, &key) else {
        return;
    };
    notes.push(AliasNote::new(
        AliasKind::Attribute,
        format!("§14: deprecated key `{key}` reads as `subtype` (`{value}`)"),
    ));
    doc.subtype = Some(value);
}

/// §14: "`provided_by` (node field, removed in v0.5) → a `reported_in` edge to
/// that source node."
///
/// The synthesized edge's provenance triplet uses §8.1's terminating base case
/// — "a `reported_in` edge's own `primary_source` is, by convention, its own
/// `object`" — and `not_provided` for the two enums, because a v0.4 document
/// genuinely did not record them and inventing `manual_agent` would assert
/// something about the claim's origin that nobody wrote down.
fn normalize_provided_by(doc: &mut ConceptDoc, notes: &mut Vec<AliasNote>) {
    let Some(source) = take_string(doc, "provided_by") else {
        return;
    };
    notes.push(AliasNote::new(
        AliasKind::Attribute,
        format!("§14: `provided_by: {source}` reads as a `reported_in` edge to that source node"),
    ));
    doc.edges.push(Edge {
        predicate: PositivePredicate::ReportedIn.as_str().to_string(),
        object: source.clone(),
        knowledge_level: Some(NOT_PROVIDED.to_string()),
        agent_type: Some(NOT_PROVIDED.to_string()),
        primary_source: Some(source),
        ..Edge::default()
    });
}

fn normalize_edges(doc: &mut ConceptDoc, notes: &mut Vec<AliasNote>) {
    for edge in &mut doc.edges {
        note_legacy_primary_source(edge, notes);
        let Ok(resolved) = resolve_predicate(&edge.predicate, edge.negated) else {
            continue;
        };
        if resolved.reversed {
            notes.push(AliasNote::new(
                AliasKind::InversePredicate,
                format!(
                    "§14: `{}` is a deprecated inverse alias; author the forward `{}` on `{}` \
                     instead. The edge is left as written — rewriting it here would reverse the \
                     claim.",
                    edge.predicate, resolved.predicate, edge.object
                ),
            ));
            continue;
        }
        if resolved.from_legacy_negated {
            notes.push(AliasNote::new(
                AliasKind::NegatedQualifier,
                format!(
                    "§7.2: legacy `negated: true` on `{}` normalizes to `{}`",
                    edge.predicate, resolved.predicate
                ),
            ));
        }
        edge.predicate = resolved.predicate.to_string();
        edge.negated = None;
    }
}

fn note_legacy_primary_source(edge: &Edge, notes: &mut Vec<AliasNote>) {
    let Some(source) = edge.primary_source.as_deref() else {
        return;
    };
    if !is_legacy_primary_source(source) {
        return;
    }
    notes.push(AliasNote::new(
        AliasKind::PrimarySourceCurie,
        format!(
            "§14: `primary_source: {source}` is a pre-v0.5 CURIE; it should name a source node \
             carrying `xref: [{source}]`. Synthesizing that node is a bundle-level edit, so the \
             value is left as written."
        ),
    ));
}

/// Remove `key` from the preserved-unknown-keys mapping and return it as a
/// string.
///
/// Removing rather than copying is what makes normalisation idempotent: leaving
/// the deprecated key behind would re-emit it on the next write, which §11 asks
/// producers not to do.
fn take_string(doc: &mut ConceptDoc, key: &str) -> Option<String> {
    let value = doc.extra.remove(Value::String(key.to_string()))?;
    match value {
        Value::String(s) if !s.is_empty() => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::okf::Page;

    fn doc(frontmatter: &str) -> ConceptDoc {
        Page::parse(&format!("---\n{frontmatter}---\n\n# body\n"))
            .expect("fixture parses")
            .doc
    }

    fn kinds(notes: &[AliasNote], kind: AliasKind) -> Vec<&str> {
        notes
            .iter()
            .filter(|n| n.kind == kind)
            .map(|n| n.message.as_str())
            .collect()
    }

    #[test]
    fn the_unambiguous_type_aliases_normalize() {
        for (deprecated, expected) in [
            ("SDOH", NodeType::SocialFactor),
            ("SDoH", NodeType::SocialFactor),
            ("ClinicalMeasure", NodeType::BiomedicalMeasure),
            ("Procedure", NodeType::MethodOrProcedure),
            ("Method", NodeType::MethodOrProcedure),
        ] {
            let alias = normalize_type(deprecated, None).expect(deprecated);
            assert_eq!(alias.canonical, expected);
            assert!(!alias.is_ambiguous(), "{deprecated}");
        }
        // A canonical name is not an alias, and neither is a typo.
        assert!(normalize_type("SocialFactor", None).is_none());
        assert!(normalize_type("Sandwich", None).is_none());
    }

    #[test]
    fn the_ambiguous_type_aliases_are_resolved_by_subtype_and_say_so() {
        let region = normalize_type("GenomicFeature", Some("enhancer")).unwrap();
        assert_eq!(region.canonical, NodeType::SequenceFeature);
        assert!(region.is_ambiguous());
        let deviation = normalize_type("GenomicFeature", Some("snv")).unwrap();
        assert_eq!(deviation.canonical, NodeType::Variant);
        // Without a subtype there is nothing to resolve from, so the alias
        // still resolves — to the value the v0.2 umbrella mostly held — and
        // stays flagged as a guess.
        let blind = normalize_type("GenomicFeature", None).unwrap();
        assert_eq!(blind.canonical, NodeType::Variant);
        assert!(blind.is_ambiguous());

        assert_eq!(
            normalize_type("Process", Some("catalytic"))
                .unwrap()
                .canonical,
            NodeType::BiologicalFunction
        );
        // …and through `normalize`, where the subtype is still spelled the v0.3
        // way. The type is resolved *after* `feature_kind` is lifted, so the
        // ambiguity is settled from the page's own content rather than blind.
        let lifted = normalize(&doc(
            "type: GenomicFeature\nidentifier: HS2 enhancer\nfeature_kind: enhancer\n",
        ));
        assert_eq!(lifted.doc.r#type, "SequenceFeature");
        assert_eq!(lifted.doc.subtype.as_deref(), Some("enhancer"));
        assert_eq!(
            normalize_type("BiologicalProcess", Some("signaling"))
                .unwrap()
                .canonical,
            NodeType::BiologicalPathway
        );
        assert_eq!(
            normalize_type("ExposureOrFactor", Some("housing"))
                .unwrap()
                .canonical,
            NodeType::SocialFactor
        );
        assert_eq!(
            normalize_type("ExposureOrFactor", Some("ancestry"))
                .unwrap()
                .canonical,
            NodeType::Population
        );
    }

    #[test]
    fn title_reads_as_identifier_and_the_deprecated_key_is_not_re_emitted() {
        let normalized = normalize(&doc("type: Molecule\ntitle: Aspirin\n"));
        assert_eq!(normalized.doc.primary_key(), Some("Aspirin"));
        assert_eq!(kinds(&normalized.notes, AliasKind::Attribute).len(), 1);
        // An explicit `identifier` wins: `title` is then a display name, not a
        // key, and overwriting it would change what every edge resolves against.
        let both = normalize(&doc("type: Molecule\ntitle: Old\nidentifier: New\n"));
        assert_eq!(both.doc.primary_key(), Some("New"));
    }

    #[test]
    fn a_legacy_id_splits_by_whether_it_is_a_curie() {
        let curie = normalize(&doc("type: Gene\nidentifier: IL6\nid: HGNC:6018\n"));
        assert_eq!(curie.doc.xref, ["HGNC:6018"]);
        assert_eq!(curie.doc.primary_key(), Some("IL6"));
        assert!(!curie.doc.extra.contains_key("id"));

        let readable = normalize(&doc("type: Gene\nid: Interleukin-6\n"));
        assert_eq!(readable.doc.primary_key(), Some("Interleukin-6"));
        assert!(readable.doc.xref.is_empty());
    }

    #[test]
    fn the_open_ended_kind_family_reads_as_subtype() {
        for key in ["molecule_kind", "feature_kind", "structure_kind"] {
            let normalized = normalize(&doc(&format!("type: Molecule\n{key}: protein\n")));
            assert_eq!(
                normalized.doc.subtype.as_deref(),
                Some("protein"),
                "{key} did not normalize"
            );
        }
        let class_basis = normalize(&doc("type: MolecularClass\nclass_basis: pharmacologic\n"));
        assert_eq!(class_basis.doc.subtype.as_deref(), Some("pharmacologic"));
        let method = normalize(&doc("type: Structure\nmethod: cryo_em\n"));
        assert_eq!(method.doc.subtype.as_deref(), Some("cryo_em"));
    }

    #[test]
    fn provided_by_becomes_a_reported_in_edge_that_does_not_invent_provenance() {
        let normalized = normalize(&doc("type: Gene\nidentifier: IL6\nprovided_by: HGNC\n"));
        assert!(!normalized.doc.extra.contains_key("provided_by"));
        let edge = normalized.doc.edges.first().expect("one synthesized edge");
        assert_eq!(edge.predicate, "reported_in");
        assert_eq!(edge.object, "HGNC");
        // §8.1's terminating base case: the source attests its own contents.
        assert_eq!(edge.primary_source.as_deref(), Some("HGNC"));
        // A v0.4 document did not record these, so they say so rather than
        // claiming a human curated the claim.
        assert_eq!(edge.knowledge_level.as_deref(), Some(NOT_PROVIDED));
        assert_eq!(edge.agent_type.as_deref(), Some(NOT_PROVIDED));
    }

    #[test]
    fn the_legacy_negated_qualifier_normalizes_to_the_canonical_predicate() {
        let resolved = resolve_predicate("treats", Some(true)).unwrap();
        assert_eq!(resolved.predicate.to_string(), "not_treats");
        assert!(resolved.from_legacy_negated);
        assert!(!resolved.reversed);
        // A `not_<X>` predicate is already canonical; the legacy flag adds
        // nothing and must not double-negate it back to the positive.
        let canonical = resolve_predicate("not_treats", Some(true)).unwrap();
        assert_eq!(canonical.predicate.to_string(), "not_treats");
        assert!(!canonical.from_legacy_negated);
    }

    /// §6.F: rejected, not silently downgraded. Dropping the flag would turn a
    /// stated negative finding into its exact opposite — the worst outcome
    /// available, and the reason this is an error rather than a warning.
    #[test]
    fn a_negated_qualifier_on_a_non_negatable_predicate_is_rejected() {
        assert_eq!(
            resolve_predicate("is_a", Some(true)),
            Err(PredicateError::NotNegatable(PositivePredicate::IsA))
        );
        assert_eq!(
            resolve_predicate("reported_in", Some(true)),
            Err(PredicateError::NotNegatable(PositivePredicate::ReportedIn))
        );
        // The same predicate without the flag is perfectly legal.
        assert!(resolve_predicate("is_a", Some(false)).is_ok());
        assert!(resolve_predicate("is_a", None).is_ok());
    }

    #[test]
    fn an_inverse_alias_is_reported_and_never_rewritten_in_place() {
        let resolved = resolve_predicate("encoded_by", None).unwrap();
        assert!(resolved.reversed);
        assert_eq!(resolved.predicate.to_string(), "encodes");

        let normalized = normalize(&doc(
            "type: Molecule\nidentifier: IL-6 protein\nedges:\n  - predicate: encoded_by\n    object: IL6\n",
        ));
        // Reported…
        assert_eq!(
            kinds(&normalized.notes, AliasKind::InversePredicate).len(),
            1
        );
        // …and left exactly as written, because `A encoded_by B` and
        // `A encodes B` are opposite claims.
        assert_eq!(normalized.doc.edges[0].predicate, "encoded_by");
    }

    #[test]
    fn a_pre_v0_5_infores_primary_source_is_reported_and_left_alone() {
        assert!(is_legacy_primary_source("infores:hgnc"));
        assert!(!is_legacy_primary_source("HGNC"));
        let normalized = normalize(&doc(
            "type: Gene\nidentifier: IL6\nedges:\n  - predicate: is_a\n    object: cytokine gene\n    primary_source: infores:hgnc\n",
        ));
        assert_eq!(
            kinds(&normalized.notes, AliasKind::PrimarySourceCurie).len(),
            1
        );
        assert_eq!(
            normalized.doc.edges[0].primary_source.as_deref(),
            Some("infores:hgnc")
        );
    }

    /// Normalisation has to be idempotent, or a read-modify-write cycle would
    /// keep synthesizing another `reported_in` edge on every pass until the
    /// page is nothing but provenance.
    #[test]
    fn normalizing_twice_changes_nothing_the_second_time() {
        let original = doc(
            "type: SDOH\ntitle: Housing instability\nfactor_kind: housing\nprovided_by: HGNC\n\
             edges:\n  - predicate: predisposes_to\n    object: asthma\n    negated: true\n",
        );
        let once = normalize(&original);
        let twice = normalize(&once.doc);
        assert_eq!(once.doc, twice.doc);
        assert!(twice.notes.is_empty(), "second pass: {:?}", twice.notes);
        assert_eq!(once.doc.r#type, "SocialFactor");
        assert_eq!(once.doc.subtype.as_deref(), Some("housing"));
        assert_eq!(once.doc.edges[0].predicate, "not_predisposes_to");
        assert_eq!(once.doc.edges[0].negated, None);
    }

    #[test]
    fn a_bare_curie_is_distinguished_from_a_name_that_merely_contains_a_colon() {
        assert!(looks_like_bare_curie("HGNC:6018"));
        assert!(looks_like_bare_curie("infores:hgnc"));
        assert!(!looks_like_bare_curie("IL6 (protein)"));
        assert!(!looks_like_bare_curie(
            "Chen 2020 (IL-6 and severe COVID-19)"
        ));
        assert!(!looks_like_bare_curie("Study 3: follow-up"));
        assert!(!looks_like_bare_curie("Interleukin-6"));
    }
}
