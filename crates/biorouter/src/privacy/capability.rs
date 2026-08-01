use super::ProviderTier;
use crate::agents::types::SharedProvider;

/// The capability decision for ONE tool call, taken once and carried.
///
/// `Copy` is load-bearing: this has to thread into `async move` blocks that own
/// no `&self` (`extension_manager.rs`'s dispatch future, `agent.rs`'s wrapper).
///
/// It exists because the capability decision has FOUR readers on one call's
/// path — the Agent seam, Gate C, the built-in `_meta` bit, and the Platform
/// extensions — and every one of them used to re-read the provider mutex at a
/// different program point. `Agent::update_provider` takes that mutex, assigns
/// and drops it with no turn lock and no generation counter, and `biorouterd`
/// runs the multi-thread runtime, so the swap and a dispatch genuinely run in
/// parallel. Worse, the last of those reads happens *inside the driven future*,
/// past `tool_dispatch_limits::acquire` — an unbounded wall-clock gap, not
/// microseconds. Any read-then-read of shared mutable state across two program
/// points is a race; so there is one read, and it is carried.
///
/// The master toggle is a second axis with the identical defect, which is why
/// it is captured here too rather than re-read per gate: without that, one call
/// could pass Gate C with tiers **on** and then build an empty path policy with
/// tiers **off**.
///
/// **What this does NOT close** is AR-13: the sample is taken at *permit* time
/// and the tool runs later, so threading makes the call *consistent* — all
/// consumers agree — not *current*. Re-sampling at execution time to fix that
/// is exactly the bug, because it is what lets a Public-admitted call run
/// Private.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallCapability {
    tier: ProviderTier,
    enforced: bool,
}

impl CallCapability {
    /// The ONE read of the provider mutex and the ONE read of the master toggle
    /// on this call's path. Both, together, at one instant.
    pub async fn sample(provider: &SharedProvider) -> Self {
        // `None` — legitimately the state before the first bind — resolves to
        // Public, the safe direction for every gate that reads this.
        let tier = provider
            .lock()
            .await
            .as_ref()
            .map(|p| p.tier())
            .unwrap_or(ProviderTier::Public);
        Self {
            tier,
            enforced: crate::privacy::privacy_tiers_enabled(),
        }
    }

    /// For an entry with **no caller identity** — `POST /agent/call_tool`
    /// arrives outside the agent loop, so there is no admitted turn whose
    /// capability it could inherit. Public + enforced is the most restrictive
    /// pair this type can express.
    pub const fn public_enforced() -> Self {
        Self {
            tier: ProviderTier::Public,
            enforced: true,
        }
    }

    /// Test-only constructor.
    ///
    /// Production code obtains a capability from exactly one of [`Self::sample`]
    /// or [`Self::public_enforced`], and the whole-tree count of those two
    /// spellings under `crates/*/src/` is what pins the production entries. A
    /// test that spelled either of them would be indistinguishable from an
    /// entry nobody classified, so tests build their capability here instead.
    ///
    /// The two tests that *must* name the real constructors — because those
    /// constructors are what they exercise — live in
    /// `crates/biorouter/tests/privacy_capability.rs`, outside the window the
    /// census walks. That is why this file's own `mod tests` names neither.
    #[cfg(test)]
    pub(crate) const fn for_test(tier: ProviderTier, enforced: bool) -> Self {
        Self { tier, enforced }
    }

    /// The capability an unrelated test's dispatch or `McpMeta` carries. The
    /// most restrictive pair, so a test that quietly starts depending on extra
    /// reach fails instead of passing.
    #[cfg(test)]
    pub(crate) const fn for_test_restricted() -> Self {
        Self::for_test(ProviderTier::Public, true)
    }

    pub const fn tier(&self) -> ProviderTier {
        self.tier
    }

    pub const fn enforced(&self) -> bool {
        self.enforced
    }

    /// The predicate every barrier in this plan asks. Folded here so the tier
    /// and the toggle can never be read at two different instants.
    pub const fn restricts_private_data(&self) -> bool {
        self.enforced && !self.tier.is_private()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_predicate_is_the_conjunction_of_both_axes() {
        use ProviderTier::{Private, Public};
        // Enforced + public reach: the only combination a barrier stops.
        assert!(CallCapability::for_test(Public, true).restricts_private_data());
        // Private reach may read private data even with the feature on.
        assert!(!CallCapability::for_test(Private, true).restricts_private_data());
        // With the master toggle off nothing is refused, in either direction.
        assert!(!CallCapability::for_test(Public, false).restricts_private_data());
        assert!(!CallCapability::for_test(Private, false).restricts_private_data());
    }

    // `public_enforced` and `sample` are covered by
    // `crates/biorouter/tests/privacy_capability.rs`. They are tested from
    // there, not from here, so that Task 10's whole-tree census of those two
    // spellings under `crates/*/src/` counts production entries and nothing
    // else — see [`CallCapability::for_test`].
}
