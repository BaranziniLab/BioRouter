//! The **BioOKF v0.5** profile: OKF v0.2 plus a closed biomedical vocabulary.
//!
//! Normative sources, in the order they win. `SCHEMA.md` first — SPEC §6's own
//! opening sentence says "`SCHEMA.md` (authoritative, implemented in
//! `bokf-core`) is the canonical source for this set" — then `SPEC.md` §5–§11
//! and §14 for everything SCHEMA states only in summary, chiefly the
//! domain/range tables. `bokf-core` is treated as a *reading* of those two, not
//! as a third authority; where it diverges, the divergence is recorded at the
//! site rather than copied.
//!
//! ## Stage 1 adds, it does not change
//!
//! Like [`super::okf`] before it, nothing here is wired into `graph.rs`,
//! `store.rs` or `service.rs` — those are Stages 2 and 3. The module is
//! deliberately reachable and unused so the vocabulary can be got right, and
//! reviewed, before a base on disk depends on it.
//!
//! ## It is a profile, so it owns only the *extra* rules
//!
//! Everything about the file format — the `---` delimiters, the preserved
//! unknown keys, `sources`, `generated`/`verified`, the three link grammars —
//! belongs to [`super::okf`] and is used from here, never restated. A BioOKF
//! bundle is a valid OKF bundle, and this module is exactly the difference:
//!
//! | | [`vocabulary`] | the 28 types, the 24 positive predicates, the 11 derived `not_<X>` |
//! | | [`domain_range`] | which types may sit on either end of each predicate |
//! | | [`aliases`] | SPEC §14's deprecated spellings, accepted on read |
//! | | [`lint`] | the §10/§11 rule set as diagnostics |
//! | | [`profile`] | the entry point that runs the OKF layer and then this one |
//!
//! ## Nothing here rejects either (DR-7)
//!
//! Two structural guarantees carry it. [`lint::Severity`] is
//! [`super::okf::Severity`] itself, which has no fatal variant. And a type or
//! predicate this build does not recognise never *becomes* a
//! [`vocabulary::NodeType`] or a [`vocabulary::Predicate`] — it stays the raw
//! `String` [`super::okf::ConceptDoc`] already preserves, so an unknown value is
//! reported and round-tripped rather than dropped. The one place strictness is
//! correct is a producer refusing a *write*, which is Stage 4's call to make by
//! reading [`profile::Report::errors`].

pub mod aliases;
pub mod domain_range;
pub mod lint;
pub mod profile;
pub mod vocabulary;

pub use aliases::{normalize, resolve_predicate, AliasKind, AliasNote, Normalized, NOT_PROVIDED};
pub use domain_range::{Side, Violation};
pub use lint::{check_credibility, BundleIndex, Finding, Severity};
pub use profile::{check, check_bundle, check_doc, Report};
pub use vocabulary::{
    Family, LegendFamily, NodeType, PositivePredicate, Predicate, PredicateError, PredicateGroup,
    AGENT_TYPES, KNOWLEDGE_LEVELS, NEGATION_PREFIX,
};

/// The profile revision, re-exported from [`super::okf`] rather than redeclared.
///
/// It lives there because [`super::okf::OKF_VERSION`] does, and a reader
/// comparing the two should not have to go looking. Two declarations is one more
/// than can be kept in step.
pub use super::okf::BIOOKF_VERSION;

/// The spec text the gate test reads, checked in so a spec bump fails loudly
/// instead of diverging silently.
#[cfg(test)]
pub(crate) mod fixtures {
    /// `SCHEMA.md`'s "The two rules that make this BioOKF (not just OKF)"
    /// section, **verbatim** — lines 23–54 of BioOKF v0.5's `SCHEMA.md`, copied
    /// with no edits so a `diff` against the upstream file is one command.
    ///
    /// This is the Stage 1 gate. The vocabulary is not asserted against a list
    /// somebody typed into a test — it is asserted against the spec's own
    /// prose, parsed. A hand-typed list is a second copy of the vocabulary, and
    /// a second copy that agrees with the first proves only that one person
    /// transcribed it twice the same way.
    pub const SCHEMA_VOCABULARY: &str = include_str!("fixtures/schema_vocabulary.md");
}

#[cfg(test)]
mod tests {
    use super::fixtures::SCHEMA_VOCABULARY;
    use super::*;

    /// Everything between `start` and `end`, panicking with the anchor that
    /// went missing — because the likely reason a bump breaks this test is that
    /// the paragraph was reworded, and "no such anchor" says that much more
    /// usefully than an off-by-one token count.
    fn between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
        let (_, rest) = text
            .split_once(start)
            .unwrap_or_else(|| panic!("the fixture no longer contains `{start}`"));
        let (segment, _) = rest
            .split_once(end)
            .unwrap_or_else(|| panic!("the fixture no longer contains `{end}` after `{start}`"));
        segment
    }

    /// Every backticked token in a segment. Markdown alternates outside/inside
    /// on each backtick, so the odd-indexed pieces are the code spans.
    fn backticked(segment: &str) -> Vec<&str> {
        segment.split('`').skip(1).step_by(2).collect()
    }

    fn sorted(mut values: Vec<String>) -> Vec<String> {
        values.sort();
        values
    }

    fn names(types: &[NodeType]) -> Vec<&'static str> {
        types.iter().map(|t| t.as_str()).collect()
    }

    #[test]
    fn the_28_node_types_are_exactly_schema_mds_own_two_lists() {
        let biomedical = backticked(between(
            SCHEMA_VOCABULARY,
            "*Biomedical entities (20):*",
            "*Provenance & context (8):*",
        ));
        let provenance = backticked(between(
            SCHEMA_VOCABULARY,
            "*Provenance & context (8):*",
            "If something fits none",
        ));
        assert_eq!(biomedical.len(), 20, "SCHEMA lists {biomedical:?}");
        assert_eq!(provenance.len(), 8, "SCHEMA lists {provenance:?}");
        assert_eq!(NodeType::ALL.len(), 28);

        assert_eq!(names(&Family::BiomedicalEntity.members()), biomedical);
        assert_eq!(names(&Family::ProvenanceAndContext.members()), provenance);
        let split: usize = Family::ALL.iter().map(|f| f.members().len()).sum();
        assert_eq!(
            split,
            NodeType::ALL.len(),
            "the two families partition the 28"
        );
    }

    #[test]
    fn the_24_positive_predicates_are_exactly_schema_mds_own_list() {
        let listed = backticked(between(
            SCHEMA_VOCABULARY,
            "see Negation):**",
            "Direction is always",
        ));
        assert_eq!(listed.len(), 24, "SCHEMA lists {listed:?}");
        let ours: Vec<&str> = PositivePredicate::ALL.iter().map(|p| p.as_str()).collect();
        assert_eq!(ours, listed);
    }

    #[test]
    fn the_11_negatable_predicates_are_exactly_schema_mds_own_list() {
        let listed = backticked(between(SCHEMA_VOCABULARY, "are negatable:", "giving 11"));
        assert_eq!(listed.len(), 11, "SCHEMA lists {listed:?}");
        let ours = PositivePredicate::negatables();
        assert_eq!(ours.len(), 11);
        assert_eq!(
            sorted(ours.iter().map(|p| p.to_string()).collect()),
            sorted(listed.iter().map(|s| s.to_string()).collect()),
        );
    }

    /// 24 + 11 = 35, and the 11 are *rendered* from the positives rather than
    /// listed anywhere. This is the drift the module exists to make
    /// impossible: rename a base and its negative follows, because there is no
    /// second spelling to forget.
    #[test]
    fn the_35_predicates_are_24_positives_plus_11_derived_negatives() {
        let all = Predicate::all();
        assert_eq!(all.len(), 35);
        assert_eq!(all.iter().filter(|p| !p.is_negated()).count(), 24);
        assert_eq!(all.iter().filter(|p| p.is_negated()).count(), 11);
        for base in PositivePredicate::negatables() {
            let negative = Predicate::negated(base).expect("negatable");
            assert_eq!(negative.to_string(), format!("not_{base}"));
            assert_eq!(Predicate::parse(&negative.to_string()).unwrap(), negative);
        }
    }

    /// The exact spelling of every positive predicate, written out once. The
    /// parsed-from-SCHEMA tests above would still pass if the fixture and the
    /// code were wrong in the same way (a bad copy of `SCHEMA.md`); this one
    /// would not.
    #[test]
    fn every_positive_predicate_string_is_spelled_exactly() {
        let expected = [
            "is_a",
            "part_of",
            "member_of",
            "derives_from",
            "located_in",
            "expressed_in",
            "encodes",
            "interacts_with",
            "binds",
            "regulates",
            "catalyzes",
            "converts_to",
            "participates_in",
            "causes",
            "predisposes_to",
            "treats",
            "prevents",
            "contraindicated_in",
            "affects_response_to",
            "has_phenotype",
            "measures",
            "associated_with",
            "used_to_study",
            "reported_in",
        ];
        assert_eq!(expected.len(), 24);
        let ours: Vec<&str> = PositivePredicate::ALL.iter().map(|p| p.as_str()).collect();
        assert_eq!(ours, expected);
        for name in expected {
            assert!(
                PositivePredicate::parse(name).is_some(),
                "`{name}` no longer parses"
            );
        }
    }

    /// Same argument as the predicate test above, for the 28 types.
    #[test]
    fn every_node_type_string_is_spelled_exactly() {
        let expected = [
            "Gene",
            "Molecule",
            "MolecularClass",
            "Variant",
            "SequenceFeature",
            "Structure",
            "Anatomy",
            "CellType",
            "Organism",
            "BiologicalPathway",
            "BiologicalFunction",
            "Disease",
            "Phenotype",
            "BiomedicalMeasure",
            "MethodOrProcedure",
            "Exposure",
            "SocialFactor",
            "Food",
            "Device",
            "MaterialSample",
            "Publication",
            "Study",
            "Dataset",
            "Agent",
            "Population",
            "GeographicLocation",
            "Concept",
            "Other",
        ];
        assert_eq!(expected.len(), 28);
        assert_eq!(names(NodeType::ALL), expected);
        for name in expected {
            assert_eq!(NodeType::parse(name).map(|t| t.as_str()), Some(name));
        }
    }

    #[test]
    fn the_public_surface_is_reachable_without_naming_a_submodule() {
        // The re-exports exist so Stages 2–7 depend on `biookf::…` and not on
        // this file layout, which will move as the graph deriver lands.
        let page = crate::knowledge::okf::Page::parse(
            "---\ntype: Molecule\nidentifier: Aspirin\n---\n\n# Aspirin\n",
        )
        .unwrap();
        let report = check(
            Some("knowledge/aspirin.md"),
            &page,
            &BundleIndex::unindexed(),
        );
        assert!(
            report.is_empty(),
            "unexpected findings: {:?}",
            report.findings
        );
        assert_eq!(BIOOKF_VERSION, "0.5");
        assert_eq!(NEGATION_PREFIX, "not_");
        assert_eq!(NOT_PROVIDED, "not_provided");
    }
}
