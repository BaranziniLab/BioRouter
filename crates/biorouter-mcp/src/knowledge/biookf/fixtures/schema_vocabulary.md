## The two rules that make this BioOKF (not just OKF)

1. **Every concept document's `type` is one of these 28 values, nothing else.**
*Biomedical entities (20):* `Gene`, `Molecule`, `MolecularClass`, `Variant`,
`SequenceFeature`, `Structure`, `Anatomy`, `CellType`, `Organism`, `BiologicalPathway`,
`BiologicalFunction`, `Disease`, `Phenotype`, `BiomedicalMeasure`, `MethodOrProcedure`,
`Exposure`, `SocialFactor`, `Food`, `Device`, `MaterialSample`.
*Provenance & context (8):* `Publication`, `Study`, `Dataset`, `Agent`, `Population`,
`GeographicLocation`, `Concept`, `Other`.
If something fits none, use `Other` with a `note:`; never invent a type.

2. **Every relationship is a typed `edges:` entry whose `predicate` is one of these 24 positive predicates (a negative finding uses a `not_<X>` negative; see Negation):**
`is_a`, `part_of`, `member_of`, `derives_from`, `located_in`, `expressed_in`, `encodes`,
`interacts_with`, `binds`, `regulates`, `catalyzes`, `converts_to`, `participates_in`,
`causes`, `predisposes_to`, `treats`, `prevents`, `contraindicated_in`,
`affects_response_to`, `has_phenotype`, `measures`, `associated_with`, `used_to_study`,
`reported_in`.
Direction is always **this document → object**. The 24 are **forward-only**: there are no
inverse predicates; to express a reverse relation, author the forward edge on the other node
(a gene's `encodes`, never a protein's `encoded_by`).

**Negation (polarity).** A genuine *negative* finding stated in the source ("X does **not** treat
Y", "**no** association between X and Y", "drug A does **not** bind target B") is authored with the
canonical negative predicate **`not_<X>`**. Only the **11 effect predicates** that are actually
tested-and-refuted in source text are negatable: `binds`, `interacts_with`, `causes`,
`predisposes_to`, `prevents`, `treats`, `affects_response_to`, `associated_with`, `expressed_in`,
`regulates`, `has_phenotype`, giving 11 `not_*` predicates (**35 total**). Negating a
structural/definitional/provenance predicate (`is_a`, `part_of`, `encodes`, `measures`,
`reported_in`, `used_to_study`, …) is meaningless under open-world semantics (absence already
covers it) and is rejected. A `not_<X>` **inherits `<X>`'s domain/range and symmetry**; asserting
both `<X>` and `not_<X>` for the same subject→object is a contradiction. (A legacy `negated: true`
qualifier on a negatable predicate is accepted on read and normalized to `not_<X>`.)
