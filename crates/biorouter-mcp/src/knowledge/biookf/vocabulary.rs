//! The BioOKF v0.5 closed vocabulary: 28 node types, 24 positive predicates, and
//! the 11 `not_<X>` negatives **derived** from them.
//!
//! Source of truth, in order: BioOKF `SCHEMA.md` ("authoritative, implemented in
//! `bokf-core`", says SPEC §6's own opening line), then SPEC §5 (nodes) and §6
//! (edges). Where the two disagree on *order*, SCHEMA wins here, because
//! [`fixtures::SCHEMA_VOCABULARY`] is a verbatim copy of SCHEMA's own two rules
//! and the gate test compares against it. (They do disagree: SPEC §6.E numbers
//! `reported_in` 23rd and `used_to_study` 24th; SCHEMA lists them the other way
//! round. Nothing depends on the order except that test, but a reader who
//! notices the swap should not have to wonder which one is a typo.)
//!
//! ## One table, four projections
//!
//! Both vocabularies are declared by a macro over a single table, and the enum,
//! the `ALL` slice, `as_str`, `parse` and the family functions are all generated
//! from it. That is not cleverness for its own sake — it closes a specific bug.
//! With four hand-written matches, adding a 29th node type compiles the moment
//! the last `match` arm is added, and an `ALL` slice that was never updated then
//! silently omits it: the type parses, but every consumer that iterates the
//! vocabulary (the legend, the facet rail, the JSON Schema `enum` DR-16 wants at
//! the provider) has never heard of it. One table makes that unrepresentable —
//! a new row updates all four at once, and the count test in
//! [`super::tests`] then fails loudly, which is exactly what a spec bump should
//! do.
//!
//! ## Why `not_<X>` is not in the table
//!
//! SPEC §6.F's negatives are 11 predicates whose names, domain, range and
//! symmetry are *entirely* a function of their base. Listing them would create
//! eleven opportunities for `not_associated_with` to stop being symmetric while
//! `associated_with` still is, or for a renamed base to leave a stale negative
//! behind — the drift SPEC §6.F guards against in prose and nothing guards
//! against in code. So [`Predicate`] carries a base plus a polarity flag, its
//! `Display` writes `not_` in front of the base's own name, and every property
//! it has is read through [`Predicate::base`]. There is no second spelling to
//! drift.

use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

// ── node types ──────────────────────────────────────────────────────────────

/// SPEC §5's two families: the science (20) and where the knowledge came from
/// (8). Structural, not cosmetic — §8.1 scopes `primary_source` to four members
/// of the second family, and the 20/8 split is what the conformance gate counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// SPEC §5.A.
    BiomedicalEntity,
    /// SPEC §5.B.
    ProvenanceAndContext,
}

impl Family {
    /// Both, in SPEC §5 order.
    pub const ALL: &'static [Family] = &[Family::BiomedicalEntity, Family::ProvenanceAndContext];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BiomedicalEntity => "Biomedical entities",
            Self::ProvenanceAndContext => "Provenance & context",
        }
    }

    /// The members of this family, derived from the one table rather than
    /// listed — so the 20/8 split is a *consequence* of the vocabulary and not a
    /// second statement of it that could disagree.
    pub fn members(self) -> Vec<NodeType> {
        NodeType::ALL
            .iter()
            .copied()
            .filter(|t| t.family() == self)
            .collect()
    }
}

/// The seven display families the BioOKF Studio palette groups the 28 types
/// into (DR-9 ports the aesthetic, and this is the grouping it needs).
///
/// Deliberately *not* the same axis as [`Family`]: this one has seven values
/// because a legend with two headings and twenty rows under one of them is
/// unreadable. [`LegendFamily::Provenance`] happens to coincide exactly with
/// [`Family::ProvenanceAndContext`], and a test pins that — if a future spec
/// bump moves a provenance type into a science family, the legend and the
/// `primary_source` rule would silently disagree about what "provenance" means.
///
/// [`Self::as_str`] returns `bokf-core`'s own category keys (`"anatomy"`,
/// `"exposome"`, …) rather than this enum's Rust spelling, so a palette keyed
/// by Studio's strings crosswalks without a second table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LegendFamily {
    Genomic,
    Molecular,
    /// Named `Anatomical`, not `Anatomy`, so the family never reads as the node
    /// type of the same name sitting inside it.
    Anatomical,
    Clinical,
    Exposome,
    Physical,
    Provenance,
}

impl LegendFamily {
    /// The seven, in palette order.
    pub const ALL: &'static [LegendFamily] = &[
        LegendFamily::Genomic,
        LegendFamily::Molecular,
        LegendFamily::Anatomical,
        LegendFamily::Clinical,
        LegendFamily::Exposome,
        LegendFamily::Physical,
        LegendFamily::Provenance,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Genomic => "genomic",
            Self::Molecular => "molecular",
            Self::Anatomical => "anatomy",
            Self::Clinical => "clinical",
            Self::Exposome => "exposome",
            Self::Physical => "physical",
            Self::Provenance => "provenance",
        }
    }

    /// The members of this family, derived rather than listed for the same
    /// reason the negatives are.
    pub fn members(self) -> Vec<NodeType> {
        NodeType::ALL
            .iter()
            .copied()
            .filter(|t| t.legend_family() == self)
            .collect()
    }
}

macro_rules! node_types {
    ($($variant:ident => $name:literal, $family:ident, $legend:ident;)+) => {
        /// The 28 controlled `type` values (SPEC §5).
        ///
        /// There is no `Unknown` variant, and that is the point: an invalid
        /// `type` never becomes a [`NodeType`], it stays the raw `String` that
        /// [`crate::knowledge::okf::ConceptDoc`] already preserves, and the
        /// profile reports it. A variant that could hold arbitrary text would
        /// make "is this one of the 28?" a runtime question at every call site
        /// instead of a compile-time one at exactly this boundary.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum NodeType { $($variant),+ }

        impl NodeType {
            /// All 28, in SCHEMA order.
            pub const ALL: &'static [NodeType] = &[$(NodeType::$variant),+];

            /// The exact CamelCase string SPEC §5 spells. This is also the
            /// serde representation — see the hand-written `Serialize`, which
            /// exists so the wire form and this function cannot diverge.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }

            /// Canonical names only. Deprecated aliases (SPEC §14) are
            /// [`super::aliases::normalize_type`]'s job, kept separate so a
            /// producer cannot round-trip an alias back out as if it were
            /// canonical.
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($name => Some(Self::$variant),)+ _ => None }
            }

            pub const fn family(self) -> Family {
                match self { $(Self::$variant => Family::$family),+ }
            }

            pub const fn legend_family(self) -> LegendFamily {
                match self { $(Self::$variant => LegendFamily::$legend),+ }
            }
        }
    };
}

node_types! {
    // ── SPEC §5.A Biomedical entities (20) ──────────────────────────────────
    Gene               => "Gene",               BiomedicalEntity,     Genomic;
    Molecule           => "Molecule",           BiomedicalEntity,     Molecular;
    MolecularClass     => "MolecularClass",     BiomedicalEntity,     Molecular;
    Variant            => "Variant",            BiomedicalEntity,     Genomic;
    SequenceFeature    => "SequenceFeature",    BiomedicalEntity,     Genomic;
    Structure          => "Structure",          BiomedicalEntity,     Genomic;
    Anatomy            => "Anatomy",            BiomedicalEntity,     Anatomical;
    CellType           => "CellType",           BiomedicalEntity,     Anatomical;
    Organism           => "Organism",           BiomedicalEntity,     Anatomical;
    BiologicalPathway  => "BiologicalPathway",  BiomedicalEntity,     Molecular;
    BiologicalFunction => "BiologicalFunction", BiomedicalEntity,     Molecular;
    Disease            => "Disease",            BiomedicalEntity,     Clinical;
    Phenotype          => "Phenotype",          BiomedicalEntity,     Clinical;
    BiomedicalMeasure  => "BiomedicalMeasure",  BiomedicalEntity,     Clinical;
    MethodOrProcedure  => "MethodOrProcedure",  BiomedicalEntity,     Clinical;
    Exposure           => "Exposure",           BiomedicalEntity,     Exposome;
    SocialFactor       => "SocialFactor",       BiomedicalEntity,     Exposome;
    Food               => "Food",               BiomedicalEntity,     Exposome;
    Device             => "Device",             BiomedicalEntity,     Physical;
    MaterialSample     => "MaterialSample",     BiomedicalEntity,     Physical;
    // ── SPEC §5.B Provenance & context (8) ──────────────────────────────────
    Publication        => "Publication",        ProvenanceAndContext, Provenance;
    Study              => "Study",              ProvenanceAndContext, Provenance;
    Dataset            => "Dataset",            ProvenanceAndContext, Provenance;
    Agent              => "Agent",              ProvenanceAndContext, Provenance;
    Population         => "Population",         ProvenanceAndContext, Provenance;
    GeographicLocation => "GeographicLocation", ProvenanceAndContext, Provenance;
    Concept            => "Concept",            ProvenanceAndContext, Provenance;
    Other              => "Other",              ProvenanceAndContext, Provenance;
}

impl NodeType {
    /// The four types SPEC §8.1 permits as a `primary_source` or `reported_in`
    /// target.
    ///
    /// **Not** the same set as [`Family::ProvenanceAndContext`], and the gap is
    /// the bug this constant exists to prevent: §8.1 is explicit that
    /// "`Population`, `GeographicLocation`, `Concept`, `Other` are **not** valid
    /// `primary_source` or `reported_in` targets". A check written as
    /// `family() == ProvenanceAndContext` would let an edge cite a
    /// `GeographicLocation` as its evidence and call the bundle conformant.
    pub const SOURCE_TYPES: &'static [NodeType] = &[
        NodeType::Publication,
        NodeType::Study,
        NodeType::Dataset,
        NodeType::Agent,
    ];

    /// See [`Self::SOURCE_TYPES`].
    pub const fn is_source(self) -> bool {
        matches!(
            self,
            NodeType::Publication | NodeType::Study | NodeType::Dataset | NodeType::Agent
        )
    }
}

impl fmt::Display for NodeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for NodeType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NodeType {
    /// Strict, unlike everything in [`crate::knowledge::okf`], and that is not a
    /// DR-7 violation: this runs over values *this build wrote* (a graph cache,
    /// a typed API payload), never over a user's frontmatter. Frontmatter is
    /// read as a `String` and turned into a `NodeType` by [`NodeType::parse`],
    /// which returns `None` instead of failing the document.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| D::Error::custom(format!("unknown BioOKF node type `{s}`")))
    }
}

// ── predicates ──────────────────────────────────────────────────────────────

/// SPEC §6's five sub-sections, which are the UMLS super-relation families.
/// Carried for display and for the §6 cross-reference in a diagnostic; nothing
/// validates against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PredicateGroup {
    /// §6.A structural & hierarchical.
    Structural,
    /// §6.B spatial & expression.
    Spatial,
    /// §6.C molecular & functional.
    Functional,
    /// §6.D clinical & causal.
    Clinical,
    /// §6.E measurement, association & provenance.
    Measurement,
}

macro_rules! predicates {
    ($($variant:ident => $name:literal, $group:ident, sym: $sym:literal, neg: $neg:literal;)+) => {
        /// The 24 **positive** predicates (SPEC §6.A–§6.E).
        ///
        /// The negatives are not here; see the module header. A value of this
        /// type is always a predicate that may be written as-is.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum PositivePredicate { $($variant),+ }

        impl PositivePredicate {
            /// All 24, in SCHEMA rule-2 order.
            pub const ALL: &'static [PositivePredicate] = &[$(PositivePredicate::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }

            /// Canonical positive spellings only — no `not_` prefix (that is
            /// [`Predicate::parse`]) and no §14 inverse aliases (that is
            /// [`super::aliases::resolve_predicate`]).
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($name => Some(Self::$variant),)+ _ => None }
            }

            pub const fn group(self) -> PredicateGroup {
                match self { $(Self::$variant => PredicateGroup::$group),+ }
            }

            /// SPEC §6: `interacts_with` and `associated_with` are the two
            /// symmetric predicates. A `not_<X>` inherits this through
            /// [`Predicate::is_symmetric`].
            pub const fn is_symmetric(self) -> bool {
                match self { $(Self::$variant => $sym),+ }
            }

            /// SPEC §6.F: only the 11 effect predicates may be negated.
            /// Negating a structural, definitional or provenance predicate is
            /// meaningless under open-world semantics — absence already covers
            /// it — and is rejected rather than silently accepted.
            pub const fn negatable(self) -> bool {
                match self { $(Self::$variant => $neg),+ }
            }
        }
    };
}

predicates! {
    // ── §6.A structural & hierarchical ──────────────────────────────────────
    IsA               => "is_a",                Structural,  sym: false, neg: false;
    PartOf            => "part_of",             Structural,  sym: false, neg: false;
    MemberOf          => "member_of",           Structural,  sym: false, neg: false;
    DerivesFrom       => "derives_from",        Structural,  sym: false, neg: false;
    // ── §6.B spatial & expression ───────────────────────────────────────────
    LocatedIn         => "located_in",          Spatial,     sym: false, neg: false;
    ExpressedIn       => "expressed_in",        Spatial,     sym: false, neg: true;
    // ── §6.C molecular & functional ─────────────────────────────────────────
    Encodes           => "encodes",             Functional,  sym: false, neg: false;
    InteractsWith     => "interacts_with",      Functional,  sym: true,  neg: true;
    Binds             => "binds",               Functional,  sym: false, neg: true;
    Regulates         => "regulates",           Functional,  sym: false, neg: true;
    Catalyzes         => "catalyzes",           Functional,  sym: false, neg: false;
    ConvertsTo        => "converts_to",         Functional,  sym: false, neg: false;
    ParticipatesIn    => "participates_in",     Functional,  sym: false, neg: false;
    // ── §6.D clinical & causal ──────────────────────────────────────────────
    Causes            => "causes",              Clinical,    sym: false, neg: true;
    PredisposesTo     => "predisposes_to",      Clinical,    sym: false, neg: true;
    Treats            => "treats",              Clinical,    sym: false, neg: true;
    Prevents          => "prevents",            Clinical,    sym: false, neg: true;
    ContraindicatedIn => "contraindicated_in",  Clinical,    sym: false, neg: false;
    AffectsResponseTo => "affects_response_to", Clinical,    sym: false, neg: true;
    HasPhenotype      => "has_phenotype",       Clinical,    sym: false, neg: true;
    // ── §6.E measurement, association & provenance ──────────────────────────
    Measures          => "measures",            Measurement, sym: false, neg: false;
    AssociatedWith    => "associated_with",     Measurement, sym: true,  neg: true;
    UsedToStudy       => "used_to_study",       Measurement, sym: false, neg: false;
    ReportedIn        => "reported_in",         Measurement, sym: false, neg: false;
}

impl PositivePredicate {
    /// The 11 negatable ones, derived from the table rather than listed.
    pub fn negatables() -> Vec<PositivePredicate> {
        Self::ALL
            .iter()
            .copied()
            .filter(|p| p.negatable())
            .collect()
    }
}

impl fmt::Display for PositivePredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The prefix SPEC §6.F puts in front of a negated predicate. Spelled once, in
/// the one place both the writer ([`Predicate::fmt`]) and the reader
/// ([`Predicate::parse`]) can see it.
pub const NEGATION_PREFIX: &str = "not_";

/// A predicate as an edge actually carries it: one of the 24 positives, plus a
/// polarity.
///
/// This is the type every consumer should hold. Its name is *rendered* from the
/// base's name, its domain and range are read from the base's row, and
/// [`Self::is_symmetric`] forwards to the base — so SPEC §6.F's "a `not_<X>`
/// inherits `<X>`'s domain/range and symmetry" is a property of the
/// representation, not a rule someone has to remember to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Predicate {
    base: PositivePredicate,
    negated: bool,
}

impl Predicate {
    pub const fn positive(base: PositivePredicate) -> Self {
        Self {
            base,
            negated: false,
        }
    }

    /// The negative form of `base`, or `None` when SPEC §6.F does not permit
    /// one. The only constructor for a negative predicate — there is no way to
    /// build `not_is_a` in this module, which is what makes "rejected" a type
    /// guarantee rather than a lint that could be skipped.
    pub const fn negated(base: PositivePredicate) -> Option<Self> {
        if base.negatable() {
            Some(Self {
                base,
                negated: true,
            })
        } else {
            None
        }
    }

    pub const fn base(self) -> PositivePredicate {
        self.base
    }

    pub const fn is_negated(self) -> bool {
        self.negated
    }

    /// Inherited from the base (SPEC §6.F), so `not_associated_with` is
    /// symmetric exactly as long as `associated_with` is.
    pub const fn is_symmetric(self) -> bool {
        self.base.is_symmetric()
    }

    pub const fn group(self) -> PredicateGroup {
        self.base.group()
    }

    /// Parse a canonical predicate token, positive or `not_`-prefixed.
    ///
    /// §14's inverse aliases (`encoded_by`, …) are deliberately *not* accepted
    /// here: they change the edge's direction, so a caller that took them
    /// through this function would silently invert a claim.
    /// [`super::aliases::resolve_predicate`] handles them and reports the
    /// reversal.
    pub fn parse(s: &str) -> Result<Self, PredicateError> {
        let Some(rest) = s.strip_prefix(NEGATION_PREFIX) else {
            return PositivePredicate::parse(s)
                .map(Self::positive)
                .ok_or_else(|| PredicateError::Unknown(s.to_string()));
        };
        let base =
            PositivePredicate::parse(rest).ok_or_else(|| PredicateError::Unknown(s.to_string()))?;
        Self::negated(base).ok_or(PredicateError::NotNegatable(base))
    }

    /// All 35: the 24 positives followed by the 11 derived negatives.
    pub fn all() -> Vec<Predicate> {
        let positives = PositivePredicate::ALL.iter().copied().map(Self::positive);
        let negatives = PositivePredicate::ALL
            .iter()
            .copied()
            .filter_map(Self::negated);
        positives.chain(negatives).collect()
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_str(NEGATION_PREFIX)?;
        }
        f.write_str(self.base.as_str())
    }
}

impl Serialize for Predicate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Predicate {
    /// Strict for the same reason [`NodeType`]'s is; see that impl.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(|e| D::Error::custom(e.to_string()))
    }
}

/// Why a predicate token is not one of the 35.
///
/// Two variants and not one, because the two want different messages and
/// different severities: an unknown token is a typo or a foreign vocabulary,
/// while `not_is_a` is a *modelling* mistake SPEC §6.F names and explains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredicateError {
    /// Not one of the 24, with or without the prefix.
    Unknown(String),
    /// `not_<X>` where `<X>` is one of the 24 but §6.F does not permit negating
    /// it.
    NotNegatable(PositivePredicate),
}

impl fmt::Display for PredicateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(s) => write!(
                f,
                "`{s}` is not one of the 35 BioOKF predicates (24 positive + 11 `not_<X>`)"
            ),
            Self::NotNegatable(p) => write!(
                f,
                "`{NEGATION_PREFIX}{p}` is not a legal predicate: SPEC §6.F permits a negation \
                 only on the 11 effect predicates, and `{p}` is a \
                 structural/definitional/provenance predicate whose absence already carries the \
                 negative claim"
            ),
        }
    }
}

impl std::error::Error for PredicateError {}

// ── the two controlled provenance enums (SPEC §7.2) ─────────────────────────

/// Biolink's `KnowledgeLevelEnum`, required on every edge.
///
/// A `&[&str]` rather than an enum because nothing in the profile branches on
/// the value — §8 describes it as a *consumer filter* ("admit only
/// `knowledge_assertion` for clinical decisions"), which is a decision the
/// consumer makes, not one this module makes for it. The list exists so lint
/// can say "that is not one of the five".
pub const KNOWLEDGE_LEVELS: &[&str] = &[
    "knowledge_assertion",
    "statistical_association",
    "prediction",
    "observation",
    "not_provided",
];

/// Biolink's `AgentTypeEnum`, required on every edge. See [`KNOWLEDGE_LEVELS`]
/// for why it is a list and not an enum.
pub const AGENT_TYPES: &[&str] = &[
    "manual_agent",
    "automated_agent",
    "text_mining_agent",
    "data_analysis_pipeline",
    "computational_model",
    "not_provided",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seven_legend_families_partition_the_28_types() {
        let total: usize = LegendFamily::ALL.iter().map(|f| f.members().len()).sum();
        assert_eq!(LegendFamily::ALL.len(), 7);
        assert_eq!(total, NodeType::ALL.len());
        // A partition, not just a cover: no type may appear under two headings.
        for t in NodeType::ALL {
            let homes = LegendFamily::ALL
                .iter()
                .filter(|f| f.members().contains(t))
                .count();
            assert_eq!(homes, 1, "{t} is in {homes} legend families");
        }
    }

    /// The two axes agree on what "provenance" means. They are separate enums
    /// and nothing forces this — if a spec bump moved, say, `Population` into a
    /// science family, the legend and §8.1's source rule would start disagreeing
    /// about a word they both use, and the graph would show a type under one
    /// heading while lint talked about the other.
    #[test]
    fn the_provenance_legend_family_is_exactly_the_provenance_and_context_family() {
        let by_legend = LegendFamily::Provenance.members();
        let by_family: Vec<NodeType> = NodeType::ALL
            .iter()
            .copied()
            .filter(|t| t.family() == Family::ProvenanceAndContext)
            .collect();
        assert_eq!(by_legend, by_family);
        assert_eq!(by_legend.len(), 8);
    }

    /// §8.1 admits four of those eight and excludes the other four by name.
    /// The test exists because `family() == ProvenanceAndContext` is the
    /// plausible wrong implementation, and it would let an edge cite a
    /// `GeographicLocation` as its evidence.
    #[test]
    fn only_four_of_the_eight_provenance_types_may_bear_a_source() {
        assert_eq!(NodeType::SOURCE_TYPES.len(), 4);
        for t in NodeType::SOURCE_TYPES {
            assert!(t.is_source(), "{t}");
            assert_eq!(t.family(), Family::ProvenanceAndContext);
        }
        for t in [
            NodeType::Population,
            NodeType::GeographicLocation,
            NodeType::Concept,
            NodeType::Other,
        ] {
            assert_eq!(t.family(), Family::ProvenanceAndContext);
            assert!(!t.is_source(), "§8.1 excludes {t} as a source");
        }
    }

    #[test]
    fn a_negative_inherits_its_bases_symmetry() {
        for base in PositivePredicate::negatables() {
            let negative = Predicate::negated(base).expect("negatable");
            assert_eq!(negative.is_symmetric(), base.is_symmetric());
            assert_eq!(negative.group(), base.group());
            assert_eq!(negative.base(), base);
        }
        let symmetric: Vec<&str> = PositivePredicate::ALL
            .iter()
            .filter(|p| p.is_symmetric())
            .map(|p| p.as_str())
            .collect();
        assert_eq!(symmetric, ["interacts_with", "associated_with"]);
        assert!(Predicate::negated(PositivePredicate::AssociatedWith)
            .unwrap()
            .is_symmetric());
    }

    #[test]
    fn a_non_negatable_predicate_has_no_negative_form_at_all() {
        for base in PositivePredicate::ALL.iter().copied() {
            assert_eq!(
                Predicate::negated(base).is_some(),
                base.negatable(),
                "{base}"
            );
        }
        // The four §6.F names, each rejected with the modelling reason rather
        // than as an unknown token.
        for name in ["is_a", "part_of", "encodes", "measures", "reported_in"] {
            let base = PositivePredicate::parse(name).unwrap();
            assert_eq!(Predicate::negated(base), None);
            assert_eq!(
                Predicate::parse(&format!("not_{name}")),
                Err(PredicateError::NotNegatable(base)),
            );
        }
    }

    #[test]
    fn an_unknown_token_is_unknown_with_or_without_the_prefix() {
        assert_eq!(
            Predicate::parse("frobnicates"),
            Err(PredicateError::Unknown("frobnicates".into()))
        );
        assert_eq!(
            Predicate::parse("not_frobnicates"),
            Err(PredicateError::Unknown("not_frobnicates".into()))
        );
        // §14's inverse aliases are *not* accepted here: taking them would
        // silently invert the claim. `aliases::resolve_predicate` handles them.
        assert!(Predicate::parse("encoded_by").is_err());
        assert!(NodeType::parse("SDOH").is_none());
    }

    /// The wire form is the spec's own string, and there is only one table
    /// producing it — so a serde payload and a rendered label can never
    /// disagree about how a type is spelled.
    #[test]
    fn serde_emits_the_specs_exact_strings() {
        assert_eq!(
            serde_json::to_string(&NodeType::BiologicalPathway).unwrap(),
            "\"BiologicalPathway\""
        );
        assert_eq!(
            serde_json::from_str::<NodeType>("\"MaterialSample\"").unwrap(),
            NodeType::MaterialSample
        );
        assert!(serde_json::from_str::<NodeType>("\"Sandwich\"").is_err());
        let negative = Predicate::negated(PositivePredicate::Treats).unwrap();
        assert_eq!(serde_json::to_string(&negative).unwrap(), "\"not_treats\"");
        assert_eq!(
            serde_json::from_str::<Predicate>("\"not_treats\"").unwrap(),
            negative
        );
        assert!(serde_json::from_str::<Predicate>("\"not_is_a\"").is_err());
    }

    #[test]
    fn the_two_provenance_enums_are_the_biolink_sets_spec_7_2_names() {
        assert_eq!(KNOWLEDGE_LEVELS.len(), 5);
        assert_eq!(AGENT_TYPES.len(), 6);
        // `not_provided` is the one value both share, and §8.1 reserves it as
        // an escape rather than a default.
        assert!(KNOWLEDGE_LEVELS.contains(&"not_provided"));
        assert!(AGENT_TYPES.contains(&"not_provided"));
    }
}
