//! The domain/range table: which node types may sit on either end of each
//! predicate.
//!
//! Transcribed from the "Typical domain → range" column of SPEC §6's five
//! tables (§6.A–§6.E), widened by SCHEMA.md's "Edges: domain/range notes"
//! section, which states several rows as `domain +=` / `range +=` extensions
//! rather than restating the whole row.
//!
//! ## Where the two disagree, the table is their **union**
//!
//! They disagree in three rows — `prevents` (SCHEMA gives it
//! `predisposes_to`'s broad factor domain and adds `BiomedicalMeasure` to the
//! range; SPEC §6.D lists five domain types and two range types),
//! `contraindicated_in` (SCHEMA adds `Device` to the domain) and `measures`
//! (SPEC adds `Molecule` to the domain SCHEMA states). Taking the union is a
//! deliberate policy, not laziness: every finding here is a **warning**, and a
//! warning that fires on a shape one of the two normative documents explicitly
//! blesses is a false positive. False positives are how a lint gets switched
//! off, and a switched-off lint catches nothing at all — whereas the union
//! still catches the case the spec actually names, `treats` pointing at a
//! `Molecule`.
//!
//! ## Deliberately not checked
//!
//! - **`is_a`'s "same type" refinement.** §6.A states the row as "any → same
//!   type", which is a *relation between* the two ends rather than a set on
//!   either. It is left unchecked because the plausible violations
//!   (`Variant is_a SequenceFeature`, `Phenotype is_a Disease`) are exactly the
//!   §5.C/§5.D boundary calls a curator is supposed to make deliberately, and
//!   flagging them would train the reader to ignore the rule.
//! - **`part_of`'s range.** §6.A says "larger whole", which is every type.
//!
//! Both are recorded here rather than left silent, because "the table says
//! nothing about this" and "nobody transcribed this row" look identical from
//! the call site.

use super::vocabulary::{NodeType, PositivePredicate, Predicate};

/// Which end of the edge a violation is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The subject — the document the `edges:` entry is authored on.
    Domain,
    /// The object — the node the edge points at.
    Range,
}

impl Side {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Range => "range",
        }
    }
}

/// One end of one edge sits outside its predicate's table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub predicate: Predicate,
    pub side: Side,
    /// The type that was found there.
    pub actual: NodeType,
    /// The types SPEC §6 permits there.
    pub allowed: &'static [NodeType],
}

impl Violation {
    /// Prose for a diagnostic. Names the predicate as written — so a
    /// `not_treats` violation says `not_treats`, not `treats`, even though the
    /// table it failed is `treats`'s.
    pub fn message(&self) -> String {
        let allowed = self
            .allowed
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join("/");
        format!(
            "`{}` takes a {} of {allowed}, but this end is a {}",
            self.predicate,
            self.side.as_str(),
            self.actual
        )
    }
}

/// The one entry point. `None` means both ends are in range, **or** that SPEC
/// §6 constrains neither end of this predicate.
///
/// `predicate` may be negative: SPEC §6.F makes a `not_<X>` inherit `<X>`'s
/// domain and range, and that inheritance is one line here
/// ([`Predicate::base`]) rather than eleven duplicated rows.
///
/// Domain is checked before range so a wholly mis-authored edge reports the
/// subject first, which is the end the author can actually fix by moving the
/// `edges:` entry to another page.
pub fn check(subject: NodeType, predicate: Predicate, object: NodeType) -> Option<Violation> {
    let base = predicate.base();
    if let Some(allowed) = domain_of(base) {
        if !allowed.contains(&subject) {
            return Some(Violation {
                predicate,
                side: Side::Domain,
                actual: subject,
                allowed,
            });
        }
    }
    let allowed = range_of(base)?;
    (!allowed.contains(&object)).then_some(Violation {
        predicate,
        side: Side::Range,
        actual: object,
        allowed,
    })
}

/// The subject types SPEC §6 admits. `None` = the table says "any".
pub fn domain_of(p: PositivePredicate) -> Option<&'static [NodeType]> {
    use NodeType::*;
    use PositivePredicate as P;
    Some(match p {
        // §6.A "any → same type"; §6.D "any agent →"; §6.E "any ↔ any" and
        // "any → Publication·Study·Dataset·Agent".
        P::IsA | P::Causes | P::AssociatedWith | P::ReportedIn => return None,
        P::PartOf => &[
            Anatomy,
            Molecule,
            Variant,
            SequenceFeature,
            BiologicalPathway,
        ],
        P::MemberOf => &[Molecule, Gene],
        P::DerivesFrom => &[
            CellType,
            Device,
            Molecule,
            Dataset,
            MaterialSample,
            Structure,
            Food,
            Population,
        ],
        P::LocatedIn => &[
            Disease,
            BiologicalPathway,
            Molecule,
            CellType,
            Variant,
            SequenceFeature,
        ],
        P::ExpressedIn => &[Gene, Molecule],
        P::Encodes => &[Gene],
        P::InteractsWith => &[Molecule, Gene, Organism],
        P::Binds => &[Molecule],
        P::Regulates => &[Molecule, Gene, Variant, SequenceFeature],
        P::Catalyzes => &[Molecule],
        P::ConvertsTo => &[Molecule],
        P::ParticipatesIn => &[Gene, Molecule, Organism],
        // SCHEMA's "broad domain: any factor as subject", which the spec then
        // enumerates — so it is a set, not "any".
        P::PredisposesTo => RISK_FACTOR_DOMAIN,
        // The union: SPEC §6.D's five, plus the risk-factor domain SCHEMA gives
        // `predisposes_to` *and* `prevents` together.
        P::Prevents => &[
            Variant,
            Gene,
            Molecule,
            MethodOrProcedure,
            Exposure,
            SocialFactor,
            Food,
            Disease,
            BiomedicalMeasure,
            Phenotype,
        ],
        // SCHEMA states `treats`/`contraindicated_in` with the same domain.
        P::Treats | P::ContraindicatedIn => &[Molecule, MethodOrProcedure, Device],
        P::AffectsResponseTo => &[Gene, Variant, BiomedicalMeasure],
        P::HasPhenotype => &[Disease, Organism, Variant],
        P::Measures => &[MethodOrProcedure, BiomedicalMeasure, Molecule],
        P::UsedToStudy => &[
            MethodOrProcedure,
            Study,
            Dataset,
            Device,
            Organism,
            CellType,
            MaterialSample,
        ],
    })
}

/// The object types SPEC §6 admits. `None` = the table says "any" (or, for
/// `part_of`, "larger whole", which is the same thing as a set).
pub fn range_of(p: PositivePredicate) -> Option<&'static [NodeType]> {
    use NodeType::*;
    use PositivePredicate as P;
    Some(match p {
        P::IsA | P::PartOf | P::AssociatedWith => return None,
        P::MemberOf => &[MolecularClass, BiologicalPathway],
        P::DerivesFrom => &[Organism, Anatomy, Study, Molecule],
        P::LocatedIn => &[Anatomy, Organism, Gene, SequenceFeature, GeographicLocation],
        P::ExpressedIn => &[Anatomy, CellType],
        P::Encodes => &[Molecule],
        P::InteractsWith => &[Molecule, Gene, Organism],
        P::Binds => &[Molecule, Gene, Variant, SequenceFeature],
        P::Regulates => &[Molecule, Gene, BiologicalPathway, BiologicalFunction],
        // §6.C says "BiologicalPathway (reaction)" and §5.D repeats it:
        // "Reactions live under BiologicalPathway (`catalyzes` points there)".
        // SCHEMA's compressed "participates_in / catalyzes: range =
        // BiologicalPathway / BiologicalFunction" pairs the two predicates with
        // the two ranges rather than giving both to both.
        P::Catalyzes => &[BiologicalPathway],
        P::ConvertsTo => &[Molecule],
        P::ParticipatesIn => &[BiologicalPathway, BiologicalFunction],
        P::Causes => &[Disease, Phenotype, BiologicalPathway],
        // The union again: SPEC gives `prevents` Disease/Phenotype, SCHEMA adds
        // BiomedicalMeasure alongside `predisposes_to`.
        P::PredisposesTo | P::Prevents => &[Disease, Phenotype, BiomedicalMeasure],
        P::Treats => &[Disease, Phenotype],
        P::ContraindicatedIn => &[Disease, Phenotype, Organism],
        P::AffectsResponseTo => &[Molecule],
        P::HasPhenotype => &[Phenotype],
        P::Measures => &[Disease, Phenotype, Molecule],
        P::UsedToStudy => &[
            Disease,
            Phenotype,
            BiologicalPathway,
            BiologicalFunction,
            Gene,
            Variant,
            Molecule,
        ],
        // Read from the vocabulary rather than re-listed, so §8.1's four source
        // types have exactly one spelling in the tree.
        P::ReportedIn => NodeType::SOURCE_TYPES,
    })
}

/// SCHEMA's "broad domain: any factor as subject" for `predisposes_to`, named
/// because `prevents` quotes the same set and a second copy would be the drift.
const RISK_FACTOR_DOMAIN: &[NodeType] = &[
    NodeType::Variant,
    NodeType::Gene,
    NodeType::Molecule,
    NodeType::Exposure,
    NodeType::SocialFactor,
    NodeType::Food,
    NodeType::Disease,
    NodeType::BiomedicalMeasure,
    NodeType::Phenotype,
];

#[cfg(test)]
mod tests {
    use super::*;
    use PositivePredicate as P;

    fn positive(p: PositivePredicate) -> Predicate {
        Predicate::positive(p)
    }

    fn negative(p: PositivePredicate) -> Predicate {
        Predicate::negated(p).expect("negatable")
    }

    #[test]
    fn the_specs_own_example_violation_is_caught() {
        // §10: "domain/range violations (e.g. a `treats` edge pointing at a
        // `Molecule` instead of a `Disease`/`Phenotype`)".
        let v = check(NodeType::Molecule, positive(P::Treats), NodeType::Molecule)
            .expect("a treats edge pointing at a Molecule is a range violation");
        assert_eq!(v.side, Side::Range);
        assert_eq!(v.actual, NodeType::Molecule);
        assert!(v.message().contains("Disease/Phenotype"), "{}", v.message());
        // …and the shape the spec blesses is silent.
        assert!(check(NodeType::Molecule, positive(P::Treats), NodeType::Disease).is_none());
    }

    /// §6.F: "a `not_<X>` inherits `<X>`'s domain/range". Asserted over **every**
    /// negatable predicate and not just one, because the inheritance is the
    /// property the whole `Predicate` representation exists to guarantee — a
    /// regression here would mean the 11 negatives had grown a second table.
    #[test]
    fn every_negative_predicate_uses_its_bases_table() {
        for base in PositivePredicate::negatables() {
            assert_eq!(domain_of(negative(base).base()), domain_of(base), "{base}");
            assert_eq!(range_of(negative(base).base()), range_of(base), "{base}");
        }
        let v = check(NodeType::Molecule, negative(P::Treats), NodeType::Gene)
            .expect("not_treats fails treats' range check");
        assert_eq!(v.side, Side::Range);
        // The message names the predicate as written, so a curator reading the
        // report can find the line in the file.
        assert!(v.message().contains("not_treats"), "{}", v.message());
    }

    #[test]
    fn the_domain_is_reported_before_the_range() {
        // Both ends wrong: subject should be Molecule/MethodOrProcedure/Device,
        // object should be Disease/Phenotype. The domain is the end the author
        // fixes by moving the edge to another page, so it is reported first.
        let v = check(NodeType::Anatomy, positive(P::Treats), NodeType::Gene).unwrap();
        assert_eq!(v.side, Side::Domain);
        assert_eq!(v.actual, NodeType::Anatomy);
    }

    /// `reported_in` reads its range straight off the vocabulary, so §8.1's four
    /// source types have one spelling in the tree.
    #[test]
    fn reported_in_admits_exactly_the_four_source_types() {
        assert_eq!(range_of(P::ReportedIn), Some(NodeType::SOURCE_TYPES));
        for t in NodeType::ALL.iter().copied() {
            let violated = check(NodeType::Gene, positive(P::ReportedIn), t).is_some();
            assert_eq!(violated, !t.is_source(), "reported_in -> {t}");
        }
    }

    #[test]
    fn the_unconstrained_rows_never_fire() {
        // §6.A "any → same type", §6.E "any ↔ any", plus `part_of`'s "larger
        // whole" range. Recorded as tests so "unconstrained" stays a decision
        // rather than becoming an accident nobody notices.
        for p in [P::IsA, P::AssociatedWith] {
            assert_eq!(domain_of(p), None);
            assert_eq!(range_of(p), None);
            for subject in NodeType::ALL.iter().copied() {
                assert!(check(subject, positive(p), NodeType::Other).is_none());
            }
        }
        assert_eq!(range_of(P::PartOf), None);
        assert_eq!(domain_of(P::Causes), None);
    }

    /// The three rows where SPEC §6 and SCHEMA.md disagree. Each assertion is
    /// the *other* document's shape, which the union has to accept — a
    /// regression to either single source would fire a warning on a
    /// spec-blessed edge.
    #[test]
    fn the_union_accepts_both_documents_shapes() {
        // SCHEMA gives `prevents` the broad risk-factor domain and a
        // BiomedicalMeasure range; SPEC §6.D gives it neither.
        assert!(check(NodeType::Variant, positive(P::Prevents), NodeType::Disease).is_none());
        assert!(check(
            NodeType::Food,
            positive(P::Prevents),
            NodeType::BiomedicalMeasure
        )
        .is_none());
        // SCHEMA adds Device to `contraindicated_in`'s domain.
        assert!(check(
            NodeType::Device,
            positive(P::ContraindicatedIn),
            NodeType::Disease
        )
        .is_none());
        // SPEC adds Molecule to `measures`' domain.
        assert!(check(NodeType::Molecule, positive(P::Measures), NodeType::Disease).is_none());
        // The union is still not "anything": a `prevents` edge pointing at a
        // Gene is as wrong under both documents as it ever was.
        assert!(check(NodeType::Molecule, positive(P::Prevents), NodeType::Gene).is_some());
    }

    #[test]
    fn used_to_study_runs_from_the_instrument_to_what_it_studies() {
        // §6.E's newest predicate, and the one bokf-core's own test exercises.
        assert!(check(NodeType::Study, positive(P::UsedToStudy), NodeType::Disease).is_none());
        let v = check(
            NodeType::Study,
            positive(P::UsedToStudy),
            NodeType::Publication,
        )
        .expect("a Publication is not a studied entity");
        assert_eq!(v.side, Side::Range);
    }
}
