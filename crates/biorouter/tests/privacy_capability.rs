//! The two capability constructors, tested from OUTSIDE `crates/*/src/` on
//! purpose (issue #56).
//!
//! Task 10's census greps `crates/*/src/` for `CallCapability::sample(` and
//! `CallCapability::public_enforced(` and asserts the exact set of production
//! entries — the check that catches a fifth entry nobody classified. A unit test
//! beside the definition spells both constructors, and no filter grep can
//! express separates a test hit from a production one: `grep -v "mod tests"`
//! drops only lines that literally contain that string, which a call to either
//! constructor does not. Excluding the whole definition file instead would
//! blind the census in exactly the file where a fifth sampler is most plausible.
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
}

#[tokio::test]
async fn an_unbound_provider_samples_public() {
    // `None` is the legitimate state before the first bind, not an error —
    // and it must resolve to the tier that grants the least reach.
    let provider: SharedProvider = std::sync::Arc::new(tokio::sync::Mutex::new(None));
    assert_eq!(
        CallCapability::sample(&provider).await.tier(),
        ProviderTier::Public
    );
}
