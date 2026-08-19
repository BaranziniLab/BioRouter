//! The Open Knowledge Format (OKF) v0.2, as a format and nothing else.
//!
//! Upstream spec: `GoogleCloudPlatform/knowledge-catalog`, `okf/SPEC.md`, v0.2
//! (commit `3fcbb9f8`, 2026-07-24). Every `§` in this module tree refers to it
//! unless the sentence names BioOKF, in which case it refers to BioOKF v0.5's
//! `SPEC.md`.
//!
//! ## Scope: Stage 0 adds, it does not change
//!
//! Nothing here is wired into `graph.rs`, `store.rs` or `service.rs` yet — those
//! are Stage 2 and Stage 3. The module is deliberately reachable and unused, so
//! the format can be got right, and reviewed, before anything on disk depends on
//! it. In particular [`crate::knowledge::store::split_frontmatter`] is untouched
//! and still the parser every existing caller uses; [`frontmatter`]'s header
//! records the three ways the two differ and why.
//!
//! ## Two profiles, one on-disk shape
//!
//! BioRouter writes two profiles: **OKF** (default; open vocabulary) and
//! **BioOKF** (opt-in; 28 node types, 35 predicates, a required per-edge
//! provenance triplet). They share one on-disk shape, so one reader, one graph
//! deriver and one renderer serve both. This module is the shared half. The
//! BioOKF vocabulary lands in `knowledge/biookf/` at Stage 1 and deliberately
//! does not appear here — a type check in this module would make the OKF profile
//! report a legal document as broken.
//!
//! One correction to the design's §1 while it is fresh: "a BioOKF bundle is
//! always a valid OKF bundle" is asserted by BioOKF against OKF **v0.1**, and
//! there are two known divergences from v0.2 conformance — BioOKF reserves a
//! third file (`SCHEMA.md`), where OKF §3.1 reserves exactly `index.md` and
//! `log.md` and §11 rules 1–2 require every other `.md` to carry a `type`; and
//! BioOKF permits `biookf_version` in the bundle-root `index.md`, where OKF §8
//! and §12 permit `okf_version` alone. Neither is fatal, because §11 forbids a
//! consumer rejecting either. Both are why [`conformance::check_index`] takes an
//! `is_bundle_root` flag instead of assuming.
//!
//! ## Nothing rejects (DR-7)
//!
//! §11 gives consumers five MUST-NOT-REJECT tolerances and DR-7 goes further:
//! nothing anywhere rejects a page on read. That is enforced structurally rather
//! than by discipline — [`model::ConceptDoc::from_mapping`] is infallible,
//! [`conformance::Severity`] has no fatal variant, and every typed scalar reader
//! keeps the producer's text when it cannot parse it. The only fallible entry
//! point in the whole module is [`frontmatter::split`], and
//! [`conformance::check_source`] turns its error into a diagnostic.
//!
//! ## What a round trip does and does not preserve
//!
//! Content, not bytes. Every key — known and unknown, at every depth — survives
//! `parse → render → parse`, which is the §4.1 requirement. YAML *presentation*
//! does not: comments are dropped, quoting is normalised, and `3.0e-6` re-emits
//! as `3e-6`. The gate test therefore compares parsed mappings; a byte
//! comparison would fail on the formatting and prove nothing about the content.
//!
//! ## Not implemented, deliberately
//!
//! §10 attested computations. The contract fields are preserved like any other
//! unknown keys, and a page declaring `type: Attested Computation` raises
//! [`conformance::RULE_ATTESTATION_UNCHECKED`] rather than being silently
//! treated as verified — §10.5 asks a consumer to surface, not silently drop, an
//! attestation it cannot check.

pub mod conformance;
pub mod frontmatter;
pub mod links;
pub mod model;
pub mod trust;

pub use conformance::{check, check_index, check_log, check_source, Diagnostic, Severity};
pub use frontmatter::{FrontmatterError, Split};
pub use links::{
    extract_footnote_refs, extract_links, FootnoteKind, FootnoteRef, LinkForm, LinkRef,
};
pub use model::{
    Actor, ActorKind, ConceptDoc, Date, Edge, Generated, Page, Source, Status, Timestamp,
    UsageWindow, Verified, VerifiedField,
};
pub use trust::{
    effective_status, is_stale, latest_verified_at, normalize_verified, trust_tier, TrustTier,
};

/// The spec revision this module implements. Quoted when emitted, per OKF §12's
/// own example (`okf_version: "0.2"`): unquoted, YAML resolves it to the float
/// `0.2`, and a later `0.10` would silently become `0.1` — a version that sorts
/// *below* `0.2`.
pub const OKF_VERSION: &str = "0.2";

/// The BioOKF revision the BioOKF profile targets. Here rather than in the
/// profile module because [`OKF_VERSION`] is here and a reader comparing the two
/// should not have to go looking.
pub const BIOOKF_VERSION: &str = "0.5";

/// Real documents, used by the tests in every module in this tree.
///
/// Files rather than string literals, and `include_str!` rather than a runtime
/// read: a fixture that is a real `.md` file can be opened in an editor,
/// diffed, and pasted into a bundle, and one that is compiled in cannot go
/// missing at test time depending on the working directory.
#[cfg(test)]
pub(crate) mod fixtures {
    /// OKF §4.1's "a concept carrying just `type`".
    pub const MINIMAL: &str = include_str!("fixtures/minimal.md");
    /// Every v0.2 frontmatter family at once: §4.1 recommended, §5.1 sources
    /// plus `usage_window`, §5.2 `generated`/`verified`, §5.4 `status`, §5.5
    /// `stale_after`, and a §5.1 footnote joined to a `sources[].id`.
    pub const FULL_V0_2: &str = include_str!("fixtures/full_v0_2.md");
    /// §5.2's bare `verified` mapping — the shape §11 makes a consumer MUST
    /// accept, and the one a naive consumer silently reads as unverified.
    pub const BARE_VERIFIED: &str = include_str!("fixtures/bare_verified.md");
    /// §4.1 producer extensions, including a nested mapping, to pin that
    /// preservation is not shallow.
    pub const UNKNOWN_KEYS: &str = include_str!("fixtures/unknown_keys.md");
    /// Non-conformant with §11 rules 1 and 2, and read anyway.
    pub const NO_FRONTMATTER: &str = include_str!("fixtures/no_frontmatter.md");
    /// The one shape that fails to split. Deliberately not in
    /// [`ROUND_TRIPPABLE`]: there is nothing to round-trip.
    pub const UNTERMINATED: &str = include_str!("fixtures/unterminated.md");
    /// BioOKF v0.5 SPEC §12's worked example, copied verbatim.
    pub const TOCILIZUMAB: &str = include_str!("fixtures/tocilizumab.md");
    /// BioOKF §4.1 inline edge sugar, including the wrapped-across-lines form
    /// the spec itself prints.
    pub const INLINE_SUGAR: &str = include_str!("fixtures/inline_sugar.md");
    /// The grammar every base on disk is written in today (DR-2).
    pub const LEGACY_WIKI: &str = include_str!("fixtures/legacy_wiki.md");

    /// Every fixture that parses, for the Stage 0 gate. Named pairs so a
    /// failure says which document lost a key rather than which array index.
    pub const ROUND_TRIPPABLE: &[(&str, &str)] = &[
        ("minimal", MINIMAL),
        ("full_v0_2", FULL_V0_2),
        ("bare_verified", BARE_VERIFIED),
        ("unknown_keys", UNKNOWN_KEYS),
        ("no_frontmatter", NO_FRONTMATTER),
        ("tocilizumab", TOCILIZUMAB),
        ("inline_sugar", INLINE_SUGAR),
        ("legacy_wiki", LEGACY_WIKI),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn okf_version_is_a_string_so_a_later_0_10_does_not_become_0_1() {
        // The trap §12's own example sidesteps by quoting. A YAML float `0.10`
        // is `0.1`, which compares BELOW `0.2` — a bundle declaring a newer
        // revision would read as an older one.
        assert_eq!(OKF_VERSION, "0.2");
        let quoted = serde_yaml::to_string(&OKF_VERSION).unwrap();
        assert!(quoted.contains('\''), "must serialize quoted, got {quoted}");
    }

    #[test]
    fn every_round_trippable_fixture_parses() {
        for (name, text) in fixtures::ROUND_TRIPPABLE {
            Page::parse(text).unwrap_or_else(|e| panic!("{name} failed to parse: {e}"));
        }
    }

    #[test]
    fn the_unterminated_fixture_is_the_only_one_that_does_not() {
        assert!(Page::parse(fixtures::UNTERMINATED).is_err());
    }

    #[test]
    fn the_public_surface_is_reachable_without_naming_a_submodule() {
        // The re-exports exist so Stage 2 and Stage 3 depend on `okf::…` and not
        // on the internal file layout, which will move as the profile module
        // lands beside it.
        let page = Page::parse(fixtures::FULL_V0_2).unwrap();
        assert_eq!(trust_tier(&page.doc), TrustTier::HumanReviewed);
        assert!(check(&page).is_empty());
        assert!(!extract_links(&page.body).is_empty());
        assert!(!extract_footnote_refs(&page.body).is_empty());
        assert_eq!(normalize_verified(page.doc.verified.as_ref()).len(), 2);
    }
}
