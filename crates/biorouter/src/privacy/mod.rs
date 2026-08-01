//! Privacy tiers (issue #56). Two lattices over two different domains.
//!
//! * [`ProviderTier`] is CAPABILITY — what a session may *do*. It reduces with
//!   [`ProviderTier::least`] over the components of the provider bound right
//!   now. It is a pure function of live state and is never stored.
//! * [`SessionClassification`] is CLASSIFICATION — how sensitive a session's
//!   *contents* are. It reduces with `max` over events in time and is stored in
//!   `sessions.privacy_tier`, where it is a permanent ratchet.
//!
//! They do not interconvert. There is exactly one crossing, [`floor`], and a
//! repo-grep test in `Task 7` asserts its caller count.
//!
//! Invariant, proven by induction in the design (§4): for any sequence of legal
//! binds, `capability(S) >= classification(S)`. The bind admits `P` only when
//! `tier(P) >= classification(S)`; the ratchet then sets
//! `classification := max(old, floor(tier(P))) <= floor(tier(P))`.

use serde::{Deserialize, Serialize};

/// CAPABILITY — the least-privileged model currently bound to a session.
///
/// Deliberately **not** `Ord`: `max` over this type is always a bug. A mixed
/// lead/worker composite is `least(lead, worker)`, so a private lead with a
/// public worker has **public** reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderTier {
    Public,
    Private,
}

impl ProviderTier {
    /// The capability reduction. Public is less privileged, so it wins.
    pub fn least(a: Self, b: Self) -> Self {
        match (a, b) {
            (Self::Private, Self::Private) => Self::Private,
            _ => Self::Public,
        }
    }

    pub fn is_private(self) -> bool {
        matches!(self, Self::Private)
    }
}

impl Default for ProviderTier {
    /// Fail-**safe**, not fail-open: Public is the *less* privileged tier, so a
    /// provider module that forgets `tier()` gets less reach, never more.
    fn default() -> Self {
        Self::Public
    }
}

/// CLASSIFICATION — the most sensitive thing a session has ever touched.
///
/// `Ord` is derived and `Public < Private`, so `max` is the accumulation and is
/// spellable. Monotone in time; the storage layer refuses to lower it (see
/// `SessionUpdateBuilder`'s `CASE WHEN` emission).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum SessionClassification {
    Public,
    Private,
}

impl SessionClassification {
    pub const PUBLIC_SQL: &'static str = "public";
    pub const PRIVATE_SQL: &'static str = "private";

    /// Named constructor for `#[serde(default = "…")]`, which takes a **path to
    /// a function**, not a variant. Task 6's `Session::privacy_tier` field uses
    /// `#[serde(default = "SessionClassification::public")]`; without this the
    /// struct does not compile (`expected function, found variant`).
    ///
    /// Serde's default is the *deserialization* fallback for a JSON document
    /// with no `privacy_tier` — an exported/imported session file, not a
    /// database row. Public is right here and Private is right for the DB read,
    /// and they differ on purpose: Task 22's `import_session` never trusts the
    /// deserialized value as authority to be public (it raises to Private and
    /// only ever raises), while `from_stored` below fails closed because a
    /// missing *column* is a projection bug rather than an absent field.
    pub fn public() -> Self {
        Self::Public
    }

    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Public => Self::PUBLIC_SQL,
            Self::Private => Self::PRIVATE_SQL,
        }
    }

    /// Parse a stored value. **Fails closed**, deliberately breaking the
    /// tree's `try_get(..).ok().flatten()` convention for optional columns
    /// (`session_manager.rs:1971-1977`): an unrecognised or absent value is a
    /// bug in a projection, and `branch_point_msg_uid`'s absence from
    /// `list_sessions_by_types` is the live proof that a projection does get
    /// missed. Private paints every row with a badge the user will report on
    /// day one, and is safe until they do.
    pub fn from_stored(raw: &str) -> Self {
        match raw {
            Self::PUBLIC_SQL => Self::Public,
            Self::PRIVATE_SQL => Self::Private,
            other => {
                tracing::error!(
                    value = other,
                    "unrecognised sessions.privacy_tier; reading Private (fail-closed)"
                );
                Self::Private
            }
        }
    }

    pub fn is_private(self) -> bool {
        matches!(self, Self::Private)
    }
}

/// The ONE crossing between the two lattices: the classification floor a turn
/// run under `tier` establishes. `pub(crate)` on purpose — a repo-grep test
/// asserts the caller count, so a third crossing cannot appear unnoticed.
// No production caller yet: the classification ratchet that crosses the lattices
// lands later in this series, and until it does the plain (non-`cfg(test)`) lib
// build warns `never used` — which `scripts/clippy-lint.sh` promotes to an error
// with `-D warnings`. Remove this line once the ratchet is wired.
#[allow(dead_code)]
pub(crate) fn floor(tier: ProviderTier) -> SessionClassification {
    match tier {
        ProviderTier::Private => SessionClassification::Private,
        ProviderTier::Public => SessionClassification::Public,
    }
}

/// A session may bind `incoming` only when the provider is at least as private
/// as the session's contents. This is Gate A's predicate, extracted so it can
/// be unit-tested without a database.
pub fn bind_allowed(incoming: ProviderTier, target: SessionClassification) -> bool {
    match target {
        SessionClassification::Public => true,
        SessionClassification::Private => incoming.is_private(),
    }
}

/// Gate D / the §7 matrix's VIS rule: a caller sees a target only when the
/// target's classification does not exceed the caller's capability.
pub fn visible_to(caller: ProviderTier, target: SessionClassification) -> bool {
    bind_allowed(caller, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_is_a_least_and_classification_is_a_max() {
        use ProviderTier::{Private, Public};
        // CAPABILITY: least privileged wins. A private lead with a public worker
        // has public reach, because the transcript already goes to the worker.
        assert_eq!(ProviderTier::least(Private, Public), Public);
        assert_eq!(ProviderTier::least(Private, Private), Private);
        assert_eq!(ProviderTier::least(Public, Public), Public);

        // CLASSIFICATION: most sensitive wins, and it is Ord so `max` is spellable.
        assert!(SessionClassification::Private > SessionClassification::Public);
        assert_eq!(
            SessionClassification::Public.max(SessionClassification::Private),
            SessionClassification::Private
        );

        // The ONE crossing.
        assert_eq!(floor(Private), SessionClassification::Private);
        assert_eq!(floor(Public), SessionClassification::Public);
    }

    #[test]
    fn an_unparseable_or_absent_classification_reads_private() {
        assert_eq!(
            SessionClassification::from_stored("private"),
            SessionClassification::Private
        );
        assert_eq!(
            SessionClassification::from_stored("public"),
            SessionClassification::Public
        );
        // Fail closed, loudly: a bug in a projection paints every session Private —
        // immediately visible, immediately fixed, safe meanwhile. The match is
        // case-SENSITIVE on purpose: `as_sql` is the only writer and it only ever
        // emits lowercase, so `"PUBLIC"` in the column is an anomaly, and an
        // anomaly reads Private like any other unrecognised value. Do not "fix"
        // this by lowercasing the input — that is a leniency, and leniency here
        // fails open.
        assert_eq!(
            SessionClassification::from_stored("PUBLIC"),
            SessionClassification::Private
        );
        assert_eq!(
            SessionClassification::from_stored("nonsense"),
            SessionClassification::Private
        );
        assert_eq!(
            SessionClassification::from_stored(""),
            SessionClassification::Private
        );
    }

    #[test]
    fn deriving_ord_on_provider_tier_would_be_caught_here() {
        // If someone makes ProviderTier orderable, `least` becomes
        // interchangeable with `min` and a reviewer stops noticing which is meant.
        // This is the semantic assertion: the two lattices disagree on the same
        // pair, which is the entire point of having two of them.
        //
        // The companion static check is the derive list above — exactly one type
        // in this file carries an ordering derive, and it is not this one. A grep
        // counts that token, so do not spell it here.
        let cap = ProviderTier::least(ProviderTier::Private, ProviderTier::Public);
        let cls = SessionClassification::Private.max(SessionClassification::Public);
        assert_eq!(cap, ProviderTier::Public);
        assert_eq!(cls, SessionClassification::Private);
    }
}
