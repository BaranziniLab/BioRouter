//! The two capability constructors, tested from OUTSIDE `crates/*/src/` on
//! purpose (issue #56).
//!
//! ⚠ **The census this file was placed here for is NOT WRITTEN.** The sentences
//! below describe the design it anticipates, and they were phrased in the
//! present tense as though it existed; review looked for it and found nothing.
//! `grep -rn '"CallCapability' crates/` returns no audit, and the only whole-tree
//! censuses in the tree today are
//! `privacy::tests::floor_is_crossed_only_where_a_capability_establishes_a_classification`
//! and `declassify::tests::the_proof_of_user_is_constructed_in_exactly_two_places`.
//! Task 51 is where it lands. Until then nothing stops a fifth `sample()` from
//! joining the four in `crates/*/src/`, and no comment elsewhere should cite it
//! as a constraint that already binds.
//!
//! The placement still earns its keep, because the reasoning is what makes the
//! census writable at all: it will grep `crates/*/src/` for
//! `CallCapability::sample(` and `CallCapability::public_enforced(` and assert
//! the exact set of production entries — the check that catches an entry nobody
//! classified. A unit test beside the definition spells both constructors, and
//! no filter grep can express separates a test hit from a production one:
//! `grep -v "mod tests"` drops only lines that literally contain that string,
//! which a call to either constructor does not. Excluding the whole definition
//! file instead would blind the census in exactly the file where a fifth sampler
//! is most plausible.
//!
//! So the two tests that must name the real constructors live here, where the
//! census does not walk, and the census needs no exception list at all. Every
//! other test builds its capability with `CallCapability::for_test*` and stays
//! beside the code it exercises.

use biorouter::agents::types::SharedProvider;
use biorouter::privacy::{CallCapability, ProviderTier};

#[test]
fn an_entry_with_no_caller_identity_is_the_most_restrictive_pair() {
    let cap = CallCapability::public_enforced();
    assert_eq!(cap.tier(), ProviderTier::Public);
    assert!(cap.enforced());
    assert!(cap.restricts_private_data());
    // Issue #56 Task 48 (DR-26). A public model's affiliation is the absence of
    // one, and this constructor is the entry with no caller identity at all.
    assert_eq!(cap.affiliation(), None);
}

#[tokio::test]
async fn an_unbound_provider_samples_public_and_states_no_affiliation() {
    // `None` is the legitimate state before the first bind, not an error —
    // and it must resolve to the tier that grants the least reach.
    let provider: SharedProvider = std::sync::Arc::new(tokio::sync::Mutex::new(None));
    let cap = CallCapability::sample(&provider).await;
    assert_eq!(cap.tier(), ProviderTier::Public);
    // Issue #56 Task 48 (DR-26), the other half of `sample`'s `unwrap_or`. It
    // had no assertion at all, so the affiliation of an unbound provider could
    // have been anything — including the `Some(Local)` that pairs with Private
    // in the test constructors, which would hand a chat with no model bound the
    // most permissive affiliation in the vocabulary.
    //
    // Public + `None` is also the only pair that is self-consistent here:
    // `gate_cross_affiliation_warning`'s guards ask affiliation only of two
    // Private endpoints, so a Public tier makes this field unreachable rather
    // than merely unused.
    //
    // The BOUND path — that `sample` reads `p.affiliation()` off the same
    // `as_ref()` as `p.tier()` — is driven end to end by
    // `agents::agent::gate_c_dispatch_tests::
    // binding_a_foreign_institutions_model_warns_and_still_binds`, which binds
    // a real provider covered by another institution and reads the warning back
    // out through `Agent::cross_affiliation_warnings`.
    assert_eq!(cap.affiliation(), None);
}
