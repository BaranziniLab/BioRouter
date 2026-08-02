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

pub mod capability;
pub mod extensions;
pub mod refusal;
mod registry_private;

pub use capability::CallCapability;
pub use extensions::classify_extension;
pub use refusal::PrivacyRefusal;

/// The master opt-out (DR-15), read **inside** every gate rather than through
/// an `is_enabled()` wrapper, so a mid-session change is honoured and the
/// opt-out is one auditable line rather than an absent gate.
///
/// It is read exactly once per tool call, by [`CallCapability::sample`], and
/// carried with the tier — the two are halves of one decision and reading them
/// at two different instants is the same class of race the capability itself
/// exists to close.
///
/// A `const fn … { true }` stub until Task 30 turns it into a `pub use` of the
/// `biorouter-mcp` atomic. Task 30's own file table calls it a stub "since
/// Task 14"; it lands here instead because Task 10's `CallCapability::sample`
/// is its first caller and comes first.
pub const fn privacy_tiers_enabled() -> bool {
    true
}

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

    /// `const` so [`CallCapability::restricts_private_data`] — the predicate
    /// every barrier asks — can itself be a `const fn`.
    pub const fn is_private(self) -> bool {
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
pub(crate) fn floor(tier: ProviderTier) -> SessionClassification {
    match tier {
        ProviderTier::Private => SessionClassification::Private,
        ProviderTier::Public => SessionClassification::Public,
    }
}

/// A session may bind `incoming` only when the provider is at least as private
/// as the session's contents.
///
/// ⚠ This is Gate A's predicate written in Rust, and **Gate A does not call
/// it**: the live gate is the `WHERE` clause of
/// `SessionStorage::bind_provider_if_allowed` — `AND (privacy_tier = 'public'
/// OR ? = 1)` — because a predicate evaluated here would leave a window between
/// the read and the write that a concurrent ratchet fits inside. What this
/// function serves is `visible_to` (Gate D) and the induction below, so the two
/// spellings must stay one predicate:
/// `the_sql_predicate_and_bind_allowed_agree_on_every_combination`
/// (`agents::agent::gate_a_bind_tests`) runs the real statement over all four
/// (tier, classification) combinations and asserts the outcomes agree. Relax
/// either one and that test fails.
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

    /// Occurrences of `needle` in `code` whose preceding character is not `.`, so
    /// `session_manager.rs`'s `pos.floor()` — an `f64` method with nothing to do
    /// with this module — can never be counted as a crossing.
    ///
    /// Byte-wise on purpose: `.` is ASCII, and no byte of a multi-byte UTF-8
    /// character can equal it, so testing the preceding byte is exact and cannot
    /// split a character the way slicing the string would.
    fn count_calls_not_method(code: &str, needle: &str) -> usize {
        let bytes = code.as_bytes();
        code.match_indices(needle)
            .filter(|(at, _)| *at == 0 || bytes[*at - 1] != b'.')
            .count()
    }

    #[test]
    fn floor_is_crossed_only_where_a_capability_establishes_a_classification() {
        // The two lattices are independent by construction; `floor` is the only
        // crossing. This test is what keeps that true over the next thirty tasks.
        //
        // It asserts the exact (file, count) SET, not a total. Three reasons, each
        // learned from the first version of this test, which could not pass at any
        // point in this plan:
        //
        //  (a) A total is defeated by an UNRELATED symbol. The first version
        //      matched `line.contains("floor(")`, which already matches
        //      `session_manager.rs`'s `let lo = pos.floor() as usize;` — an f64
        //      method with nothing to do with this module. (This comment used to
        //      cite a line number for it; the number had already drifted by 63
        //      lines, so the symbol is the anchor and the number is gone.) Its
        //      "baseline 0" was really 1 before a single line of #56 existed.
        //      Matching only the QUALIFIED path `privacy::floor(` cannot see a
        //      float method, ever.
        //  (b) A total is defeated by the plan's OWN later additions, silently: a
        //      task that adds two crossings where the number says one still passes
        //      if another task removed one. A set names the file.
        //  (c) The failure message has to be actionable. `assert_eq!` on a Vec of
        //      (file, count) prints exactly which file changed.
        //
        // The qualified-path rule needs a second assertion to hold: nobody may
        // `use` the bare name, or rule (a) is evaded by an import — through
        // `crate::privacy`, and equally through the `super`/`self` paths a module
        // beside this one would use. Inside `src/privacy/` the audit does not rely
        // on the import assertion at all; see `beside_the_definition` below.
        //
        // SCOPE, written down because the plan claims more than this delivers. The
        // plan says this test catches "a refactor that adds
        // `From<ProviderTier> for SessionClassification` and sprinkles `.into()` at
        // four sites". It does not, and no wording of it could: that refactor adds
        // no `floor` call to count, and the `From` impl itself would land in this
        // file, which the audit skips. What this test pins is the set of `floor`
        // CALLERS — narrower than advertised, still the thing that keeps the two
        // lattices from merging, and not to be cited in a later review as though
        // the broader claim had been established here.
        //
        // EXPECTED grows twice, and each growth is one uncommented line in the diff
        // that causes it — Task 13 (Gate B's ratchet) and Task 23 (the spawn stamp).
        // A test written to accept `<= 2` would let a third crossing appear
        // unnoticed, which is the entire point of having this test.
        const EXPECTED: &[(&str, usize)] = &[
            // Task 13: Gate B's ratchet, in `Agent::reply`. The ONE place a
            // capability establishes a classification.
            ("crates/biorouter/src/agents/agent.rs", 1),
            // ("crates/biorouter/src/agents/subagent_tool.rs", 1),  // uncomment in Task 23
        ];

        // CARGO_MANIFEST_DIR is <workspace>/crates/biorouter; go up twice.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        // This audit is only as good as its walk, and while EXPECTED is empty a walk
        // that reads NOTHING produces the identical green result as a walk that finds
        // nothing. So every way it can silently do no work is made loud instead: a
        // wrong root, an unreadable directory, an unreadable file, and a scan that
        // comes back implausibly small each fail rather than passing vacuously.
        let crates = root.join("crates");
        assert!(
            crates.is_dir(),
            "the audit walks {} — if that path is wrong, every assertion below passes \
             for the wrong reason",
            crates.display()
        );
        let mut calls: std::collections::BTreeMap<String, usize> = Default::default();
        let mut imports: Vec<String> = vec![];
        let mut scanned = 0usize;
        for entry in walkdir::WalkDir::new(&crates) {
            let entry = entry.expect("the audit must not silently skip an unreadable directory");
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            // The definition, and the induction test below, live here. Match the
            // EXACT path: `ends_with` also exempted any future
            // `crates/<other-crate>/src/privacy/mod.rs` from the audit entirely —
            // a blind spot shaped exactly like the one this test exists to prevent.
            if rel == "crates/biorouter/src/privacy/mod.rs" {
                continue;
            }
            scanned += 1;
            let src = std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
            // A file that lives BESIDE the definition reaches `floor` through
            // `super`, not through `privacy::` — and Task 8 creates exactly such a
            // sibling (`privacy::extensions`). For those files the audit counts the
            // CALL instead of trusting an import spelling, because the form this
            // evasion would really take is not a named import anyone would notice:
            // it is the `use super::*;` that every `mod tests` in this tree already
            // writes, which makes the bare name available with nothing to grep for.
            let beside_the_definition = rel.starts_with("crates/biorouter/src/privacy/");
            for (i, line) in src.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                let crossings = if beside_the_definition {
                    // Every `floor(` that is not a `.floor()`: covers `privacy::floor(`,
                    // `super::floor(`, `self::floor(` and the bare glob-imported name.
                    count_calls_not_method(code, "floor(")
                } else {
                    // Occurrences, not lines. EXPECTED asserts exact COUNTS, so two
                    // crossings written on one line have to read as two; a
                    // `contains` scored them as one and under-counted the set it is
                    // the whole job of this test to pin.
                    code.matches("privacy::floor(").count()
                };
                if crossings > 0 {
                    *calls.entry(rel.clone()).or_default() += crossings;
                }
                if code.contains("use crate::privacy::floor")
                    || code.contains("use super::floor")
                    || code.contains("use self::floor")
                    || ((code.contains("privacy::{")
                        || code.contains("use super::{")
                        || code.contains("use self::{"))
                        && code.contains("floor"))
                {
                    imports.push(format!("{rel}:{}", i + 1));
                }
            }
        }
        assert!(
            scanned >= 400,
            "only {scanned} .rs files were scanned (612 at Task 7). The walk is no longer \
             covering the tree, and a broken walk reports the same empty caller set as a \
             clean one."
        );
        assert!(
            imports.is_empty(),
            "`floor` must be called through its qualified `privacy::floor(..)` path so this \
             audit can see it — do not import the bare name: {imports:#?}"
        );
        let found: Vec<(String, usize)> = calls.into_iter().collect();
        // `found` comes out of a BTreeMap, so it is path-sorted; `want` would
        // otherwise keep EXPECTED's declaration order. The two entries Tasks 13 and
        // 23 uncomment happen to be declared in sorted order, but relying on that
        // makes the NEXT entry fail for a reason that has nothing to do with the
        // invariant, behind a message that blames the invariant. Sort both.
        let mut want: Vec<(String, usize)> = EXPECTED
            .iter()
            .map(|(f, n)| ((*f).to_string(), *n))
            .collect();
        want.sort();
        assert_eq!(
            found, want,
            "the set of `floor` callers changed. A new crossing between the capability and \
             classification lattices is a design change, not a refactor: argue it in the PR."
        );
    }

    /// Every word of length `n` over `alphabet`, in lexicographic order by index.
    /// `alphabet.len().pow(n)` of them — the induction below uses 2^6 = 64, which
    /// is exhaustive over every bind order a session of six binds can experience.
    fn all_sequences_of_length<T: Copy>(n: usize, alphabet: &[T]) -> Vec<Vec<T>> {
        let mut out: Vec<Vec<T>> = vec![vec![]];
        for _ in 0..n {
            out = out
                .into_iter()
                .flat_map(|prefix| {
                    alphabet.iter().map(move |sym| {
                        let mut next = prefix.clone();
                        next.push(*sym);
                        next
                    })
                })
                .collect();
        }
        out
    }

    #[test]
    fn capability_never_falls_below_classification_for_any_legal_sequence() {
        use ProviderTier::{Private, Public};
        // The design's induction (§4), made executable. Every legal bind followed
        // by its ratchet must preserve capability >= classification.
        //
        // SCOPE. `visible_to` delegates to `bind_allowed`, and `bind_allowed` is
        // also this loop's admission gate — so the assertion and the gate move
        // together. This pins `floor` for consistency WITH Gate A's predicate; it
        // cannot see a relaxation OF that predicate, and must not be read as
        // covering it. What covers it is
        // `the_sql_predicate_and_bind_allowed_agree_on_every_combination`
        // (`agents::agent::gate_a_bind_tests`), which runs Gate A's live `WHERE`
        // clause over all four (tier, classification) combinations and asserts the
        // outcome equals `bind_allowed` — the debt this comment used to record as
        // owed by "whichever task first wires Gate A".
        for binds in all_sequences_of_length(6, &[Private, Public]) {
            let mut classification = SessionClassification::Public;
            let mut capability = Public;
            // The induction's BASE case. Asserting it is not decoration: without a
            // read here the initializer is dead, `unused_assignments` fires, and
            // `scripts/clippy-lint.sh`'s `-D warnings` turns the whole lint gate
            // red. Silencing that with an `#[allow]` would have thrown away the one
            // half of §4's proof the plan's loop does not cover.
            assert!(
                visible_to(capability, classification),
                "a fresh session must start with capability >= classification"
            );
            for incoming in binds {
                if !bind_allowed(incoming, classification) {
                    continue; // Gate A refuses; state unchanged
                }
                capability = incoming; // the bind succeeds
                classification = classification.max(floor(capability)); // Gate B ratchets
                assert!(
                    visible_to(capability, classification),
                    "capability {capability:?} fell below classification {classification:?}"
                );
            }
        }
    }
}
