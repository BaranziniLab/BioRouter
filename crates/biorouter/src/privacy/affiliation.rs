//! Affiliation — the third axis (DR-26). *Under whose agreements?*
//!
//! [`super::ProviderTier`] asks how sensitive a thing is. Affiliation asks a
//! different question, and the two do not compose: a HIPAA-compliant LLM
//! approved at one institution has **no** blanket permission over another
//! institution's PHI. Compliance is established per data flow — by BAAs,
//! subcontractor chains, IRB approvals and DUAs — and it does not transfer
//! because both endpoints happen to be called "private". UCSF's Versa reaching
//! the UCSF OMOP agent is the arrangement everyone approved; the same Versa
//! model reaching another institution's connector is a cross-institutional
//! linkage nobody papered. Both pass every tier gate this campaign built, which
//! is why this cannot be expressed by subdividing the tier lattice.
//!
//! This module is the vocabulary, the single comparison, and the words a
//! mismatch is stated in. It decides nothing on its own: a mismatch **warns and
//! asks**, it does not block (DR-19 applied to the third axis), and the grant
//! and the gates that consult them are the tasks that follow.
//!
//! [`cross_affiliation_warning`] lives here rather than beside a gate because
//! DR-26 makes the warning the product: it must name both institutions
//! specifically enough for the user to act on, and a copy composed at each gate
//! would drift into the shrug ("this may be a compliance risk") the ruling
//! rejects.
//!
//! ⚠ **The inversion to get right.** [`ModelAffiliation::Local`] is the *most*
//! permissive value, not a peer of the institutions — see [`compatible`].

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::sync::{LazyLock, Mutex, PoisonError};

/// The institution an agreement is with, as a normalised slug.
///
/// `"UCSF"`, `"ucsf"` and `" ucsf "` are the same institution. A
/// case-sensitive comparison here would make a mismatch appear between an
/// institution and *itself*, which fails **open** in the worst way: it does not
/// leak anything directly, it trains users to click through the warning until
/// the one that mattered goes by too.
///
/// # Why this is interned rather than a `String`
///
/// [`ModelAffiliation`] must be [`Copy`] — Task 48 puts it on
/// [`super::capability::CallCapability`], whose `Copy` derive is load-bearing
/// because that value threads into `async move` blocks owning no `&self`. An
/// owned `String` or `BTreeSet` on the model side would break every gate
/// signature downstream. So the slug is leaked once, on first sight, and the id
/// is a `Copy` pointer to it.
///
/// ⚠ **The leak is bounded by the caller, not by this type.** Every distinct
/// normalised slug reaching [`Self::new`] is leaked for the life of the
/// process, and nothing here caps their number or their length. That is safe
/// *today* by construction, not by promise: the only institution strings in the
/// tree are a provider's own gateway-host decision and the compiled snapshot
/// beside `PRIVATE_EXTENSIONS`, whose header records that there is no network
/// path to the registry from Rust at all — the only fetch is Electron's
/// `registry:fetch`. Task 47 is where a registry-sourced value first arrives,
/// and the cap belongs **there**, at the parse that admits it, where refusing
/// is meaningful: an institution the registry does not publish is already
/// specified to be a *mismatch*, which is the fail-closed answer.
///
/// It cannot be enforced in this constructor. Truncating an over-long slug or
/// stripping a non-ASCII one rewrites two distinct institutions into one id,
/// which grants a cross-institutional flow that should have warned — strictly
/// worse than leaking the string. See
/// `distinct_institution_names_never_collide`, which fails if anyone adds
/// either.
///
/// ⚠ **Equality compares the string CONTENTS, not the pointer.** That is
/// deliberate belt-and-braces: were the interner ever to hand out two pointers
/// for one slug, pointer equality would make an institution mismatch itself —
/// the precise failure above. Content equality cannot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstitutionId(&'static str);

impl InstitutionId {
    /// Normalise and intern. Total: every input yields an id, including an
    /// empty one, which matches only other empties and therefore only ever
    /// costs a mismatch warning — the safe direction.
    ///
    /// ⚠ **The normaliser is [`name_to_key`](crate::config::extensions::name_to_key),
    /// reused rather than re-derived.** That function is what makes today's
    /// extension classification work at all; two different normalisers for two
    /// axes is how `cdwagent` ends up classified Private under one and
    /// mismatched under the other.
    pub fn new(name: &str) -> Self {
        Self(intern(crate::config::extensions::name_to_key(name)))
    }

    /// The normalised slug. `'static` because it is interned, so this composes
    /// into a warning message without borrowing the id.
    ///
    /// ⚠ **Normalised is not sanitised, and this is the identity, not display
    /// text.** [`name_to_key`](crate::config::extensions::name_to_key)
    /// lowercases and strips whitespace; it does not remove control characters
    /// or bidi overrides, and it must not — a lossy rewrite here would collide
    /// two institutions (above). Task 47 specifies that an institution the
    /// registry does not publish is surfaced **raw** in the warning, so the
    /// composer of that warning is what escapes for its medium. DR-26 requires
    /// the warning name both institutions specifically enough to act on, and a
    /// slug that reorders the sentence around it fails that requirement.
    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for InstitutionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Serialize for InstitutionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for InstitutionId {
    /// Deserialising is a third door a raw institution string comes through,
    /// beside a provider decider and the registry snapshot, and it normalises
    /// like the other two. A `"UCSF"` in `registry.json` that landed as an id
    /// distinct from `ucsf` would be the self-mismatch this type exists to
    /// prevent, arriving by a different route.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::new(&raw))
    }
}

/// Interned slugs, so a repeated institution is leaked once rather than per
/// construction. Poisoning is recovered from rather than propagated: this table
/// is an allocation cache, a panic elsewhere tells us nothing about its
/// contents, and a gate that panicked here would fail a call that should merely
/// have warned.
static INTERNER: LazyLock<Mutex<HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn intern(slug: String) -> &'static str {
    let mut table = INTERNER.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = table.get(slug.as_str()) {
        return existing;
    }
    let leaked: &'static str = Box::leak(slug.into_boxed_str());
    table.insert(leaked);
    leaked
}

/// Whose compliance regime covers the model bound right now. Orthogonal to
/// [`super::ProviderTier`] — see DR-26.
///
/// There is no `None` variant: a public model's affiliation is not a value in
/// this type, it is the absence of one. The tier gates already keep a public
/// model away from private data, so affiliation never applies to it, and giving
/// "public" a seat here would invite a gate to compare it with something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelAffiliation {
    /// Runs on this machine; the data never leaves it.
    Local,
    /// Covered by that institution's agreements — `ucsf` for both Versa
    /// providers, derived from the same gateway-host check that decides their
    /// tier, so a Versa module repointed elsewhere loses Private *and* `ucsf`
    /// together.
    Institution(InstitutionId),
}

impl ModelAffiliation {
    /// The bound institution, or `None` for [`Self::Local`].
    pub fn institution(&self) -> Option<InstitutionId> {
        match self {
            Self::Local => None,
            Self::Institution(id) => Some(*id),
        }
    }
}

/// Whose data an extension holds, and therefore which private models may reach
/// it.
///
/// `Institution(x)` from DR-26's table is spelled `Institutions({x})` here —
/// **one shape, not two**, because a second spelling is a second thing every
/// comparison, serialiser and warning would have to handle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ExtensionAffiliation {
    /// Safe for **any** private model. The default for a private extension with
    /// no institutional constraint.
    #[default]
    Any,
    /// An explicit allowlist. A one-element set is the single-institution case;
    /// an **empty** set permits nothing, and is emphatically not [`Self::Any`]
    /// — conflating them would turn a typo in the registry into a granted flow.
    Institutions(BTreeSet<InstitutionId>),
}

impl ExtensionAffiliation {
    /// The single-institution case — `Institutions({id})`.
    pub fn institution(id: InstitutionId) -> Self {
        Self::Institutions(BTreeSet::from([id]))
    }

    /// An allowlist from anything iterable.
    pub fn institutions(ids: impl IntoIterator<Item = InstitutionId>) -> Self {
        Self::Institutions(ids.into_iter().collect())
    }
}

/// **The** affiliation comparison. Is extension `ext` reachable from a session
/// bound to model affiliation `model`, without a cross-affiliation grant?
///
/// Both endpoints are assumed already Private; affiliation is asked only after
/// tier has been. `false` does not mean "blocked" — it means *mismatch*, which
/// warns and offers the user an explicit, once-per-triple approval (DR-19,
/// DR-26). A blocked-outright design is one researchers route around by turning
/// the feature off, and legitimate cross-institutional work under a real DUA
/// exists.
///
/// There must be exactly **one** of these. A gate that hand-compares
/// affiliations is a second implementation of the table below, and the two will
/// disagree on the row nobody thought about.
///
/// | model | ext | |
/// |---|---|---|
/// | `Local` | anything | compatible |
/// | `Institution(X)` | `Any` | compatible |
/// | `Institution(X)` | `Institutions([… X …])` | compatible |
/// | `Institution(X)` | `Institutions([…])` without X | **mismatch** |
pub fn compatible(model: &ModelAffiliation, ext: &ExtensionAffiliation) -> bool {
    let ModelAffiliation::Institution(bound) = model else {
        // `Local` is the MOST permissive affiliation, not a peer of the
        // institutions, and it returns here BEFORE any comparison happens.
        //
        // The reason is not a rule about which sets contain what: a local model
        // may reach everything private because **no transfer occurs at all**.
        // There is no disclosure, so there is nothing for an agreement to
        // govern. Expressing this as equality — or as membership in a set that
        // happens to contain `Local` — makes the local model match only itself
        // and breaks the single most important case, while every other row of
        // the table still passes. See DR-26.
        return true;
    };

    match ext {
        ExtensionAffiliation::Any => true,
        ExtensionAffiliation::Institutions(allowed) => allowed.contains(bound),
    }
}

/// The published display name for an institution id, or `None` if the registry
/// does not publish one.
///
/// The map is generated into the compiled snapshot beside `PRIVATE_EXTENSIONS`
/// (`registry_private::INSTITUTIONS`) from `registry.json`'s `institutions`
/// field, so there is no second hand-maintained list to drift.
///
/// ⚠ **`None` is not an error and must never be treated as one.** Task 47: an
/// affiliation naming an institution the registry does not publish is a
/// *mismatch* whose raw id is surfaced — failing open on a typo is how a real
/// constraint disappears. The absence of a display name changes only how the
/// institution is rendered, never whether it counts.
pub fn institution_display_name(id: InstitutionId) -> Option<&'static str> {
    super::registry_private::INSTITUTIONS
        .iter()
        .find(|(published, _)| *published == id.as_str())
        .map(|(_, name)| *name)
}

/// How an institution is written in a warning: `UCSF (ucsf)` when the registry
/// publishes a name for it, and the bare id when it does not.
///
/// The id is included even when a display name exists, because the id is what
/// appears in `registry.json`, in `baam.html`'s `data-affiliation` and in a
/// support conversation — a user asked to accept a cross-institutional risk
/// should be able to match the warning to the record.
fn label(id: InstitutionId) -> String {
    match institution_display_name(id) {
        Some(name) => format!("{name} ({id})"),
        None => id.as_str().to_string(),
    }
}

/// The copy shown to a user before a cross-affiliation flow proceeds — `None`
/// when there is nothing to warn about.
///
/// ⚠ **It returns `None` for every compatible pair, and that is the point.** A
/// composer callers had to guard themselves would eventually put a compliance
/// warning on UCSF's Versa reaching the UCSF OMOP agent — the arrangement
/// everyone approved — which is precisely the prompt fatigue DR-19 warns about.
/// The mismatch test and the copy are one call, so they cannot disagree.
///
/// DR-26 requires the warning be specific enough to act on: it names the
/// extension, the institution(s) whose data it holds, and the institution whose
/// agreements cover the bound model. A user can only accept a risk that was
/// stated to them.
///
/// This composes the sentence; it does not decide anything, does not record a
/// grant and does not block. Where it is surfaced — a refusal at dispatch, a
/// mark in discovery, a bind-time prompt — is the tasks that follow.
pub fn cross_affiliation_warning(
    model: ModelAffiliation,
    extension: &str,
    ext: &ExtensionAffiliation,
) -> Option<String> {
    if compatible(&model, ext) {
        return None;
    }
    // Both `let … else` arms are unreachable: `compatible` already returned
    // true for every `Local` model and every `Any` extension, so a mismatch is
    // always a bound institution against an allowlist. They are written as
    // early returns rather than `unreachable!()` because a warning composer is
    // reached from a refusal path, and a panic there converts a control into an
    // outage.
    let ModelAffiliation::Institution(bound) = model else {
        return None;
    };
    let ExtensionAffiliation::Institutions(owners) = ext else {
        return None;
    };

    let held_by = if owners.is_empty() {
        // Not a sentence with a blank in it: an empty allowlist is a real state
        // (a hand-edited snapshot), it permits nothing, and saying so is more
        // useful than naming zero institutions.
        "no institution at all — its allowlist names none".to_string()
    } else {
        owners
            .iter()
            .map(|id| label(*id))
            .collect::<Vec<_>>()
            .join(", ")
    };

    Some(format!(
        "Cross-institutional data flow. The extension `{extension}` holds data belonging to \
         {held_by}, but this chat is bound to a model covered by {}'s agreements. Using it \
         would send `{extension}`'s inputs and results across that boundary. Compliance does \
         not transfer between institutions: a model approved at one has no permission over \
         another's data unless a BAA, DUA or IRB approval covers this specific flow. Proceed \
         only if you know one does.",
        label(bound)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn inst(s: &str) -> InstitutionId {
        InstitutionId::new(s)
    }

    fn bound(s: &str) -> ModelAffiliation {
        ModelAffiliation::Institution(inst(s))
    }

    fn allowlist(names: &[&str]) -> ExtensionAffiliation {
        ExtensionAffiliation::institutions(names.iter().map(|n| inst(n)))
    }

    /// One fixed-key hasher per call, so two ids hash comparably. A fresh
    /// `RandomState` per id would differ regardless of the values.
    fn hash_of(id: &InstitutionId) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut hasher);
        hasher.finish()
    }

    // ---------------------------------------------------------------- DR-26's
    // compatibility table, one named test per row. The rows are the whole
    // specification of this module; anything else in this file exists to catch
    // an implementation that satisfies the table and is still wrong.

    /// Row 1a: `Local` × `Any`.
    #[test]
    fn local_reaches_an_unconstrained_extension() {
        assert!(compatible(
            &ModelAffiliation::Local,
            &ExtensionAffiliation::Any
        ));
    }

    /// Row 1b, and the mistake this task exists to prevent. An equality test —
    /// or set membership that happens to contain `Local` — passes every other
    /// row in this table and fails exactly here.
    #[test]
    fn local_reaches_every_institutions_extension() {
        assert!(compatible(
            &ModelAffiliation::Local,
            &allowlist(&["stanford"])
        ));
        assert!(compatible(
            &ModelAffiliation::Local,
            &ExtensionAffiliation::institution(inst("ucsf"))
        ));
        assert!(compatible(
            &ModelAffiliation::Local,
            &allowlist(&["stanford", "broad", "ucsf"])
        ));
    }

    /// Row 2: `Institution(X)` × `Any`.
    #[test]
    fn an_institution_model_reaches_an_unconstrained_extension() {
        assert!(compatible(&bound("ucsf"), &ExtensionAffiliation::Any));
    }

    /// Row 3: `Institution(X)` × `Institution(X)`.
    #[test]
    fn an_institution_model_reaches_its_own_institutions_extension() {
        assert!(compatible(
            &bound("ucsf"),
            &ExtensionAffiliation::institution(inst("ucsf"))
        ));
    }

    /// Row 4: `Institution(X)` × `Institutions([… X …])`.
    #[test]
    fn an_institution_model_reaches_an_allowlist_that_names_it() {
        assert!(compatible(
            &bound("ucsf"),
            &allowlist(&["stanford", "ucsf", "broad"])
        ));
    }

    /// Row 5: `Institution(X)` × `Institution(Y)`, X ≠ Y. The operator's case —
    /// UCSF's Versa reaching another institution's private connector.
    #[test]
    fn a_different_institution_is_a_mismatch() {
        assert!(!compatible(
            &bound("ucsf"),
            &ExtensionAffiliation::institution(inst("stanford"))
        ));
        assert!(!compatible(
            &bound("stanford"),
            &ExtensionAffiliation::institution(inst("ucsf"))
        ));
    }

    /// Row 6: `Institution(X)` × `Institutions([…])` without X.
    #[test]
    fn an_allowlist_that_omits_the_bound_institution_is_a_mismatch() {
        assert!(!compatible(
            &bound("ucsf"),
            &allowlist(&["stanford", "broad"])
        ));
    }

    // ------------------------------------------------- The four that catch the
    // real mistakes (Task 45, Step 3).

    /// A case-sensitive comparison would make a mismatch appear between an
    /// institution and itself — which fails OPEN in the worst way, by training
    /// users to click through the warning.
    #[test]
    fn institution_ids_compare_case_insensitively() {
        assert!(compatible(&bound("UCSF"), &allowlist(&["ucsf"])));
        assert!(compatible(&bound("ucsf"), &allowlist(&["UCSF"])));
        assert!(compatible(&bound("UcSf"), &allowlist(&["uCsF"])));
        assert_eq!(inst("UCSF"), inst("ucsf"));
    }

    /// The `" ucsf "` half of the same requirement. `name_to_key` strips **all**
    /// whitespace, not just the ends, which is the discipline this axis reuses
    /// rather than inventing a second normaliser beside it.
    #[test]
    fn institution_ids_ignore_whitespace() {
        assert_eq!(inst(" ucsf "), inst("ucsf"));
        assert_eq!(inst("u c s f"), inst("ucsf"));
        assert_eq!(inst("\tUCSF\n"), inst("ucsf"));
        assert!(compatible(&bound(" UCSF "), &allowlist(&["ucsf"])));
    }

    /// `Any` is the default for a private extension with no institutional
    /// constraint, so it must accept every institution — including ones this
    /// build has never heard of.
    #[test]
    fn any_accepts_every_institution() {
        for name in ["ucsf", "stanford", "broad", "somewhere-new", ""] {
            assert!(
                compatible(&bound(name), &ExtensionAffiliation::Any),
                "Any must accept Institution({name:?})"
            );
        }
    }

    /// The mirror, and it is not symmetric with `Any`. An extension that names
    /// an EMPTY allowlist permits nothing; conflating the two would turn a typo
    /// in the registry into a granted flow.
    #[test]
    fn an_empty_allowlist_is_not_any() {
        let empty = ExtensionAffiliation::Institutions(BTreeSet::new());
        assert_ne!(empty, ExtensionAffiliation::Any);
        for name in ["ucsf", "stanford", "broad"] {
            assert!(
                !compatible(&bound(name), &empty),
                "an empty allowlist must not admit Institution({name:?})"
            );
        }
        // ...and `Local` still reaches it, because `Local` never compares.
        assert!(compatible(&ModelAffiliation::Local, &empty));
    }

    /// `compatible` is total: it answers for every pair, and never panics. A
    /// deterministic generator, so a failure reproduces exactly.
    #[test]
    fn compatible_is_total_over_generated_pairs() {
        let corpus = fuzz_corpus();
        let mut rng = Xorshift(0x5EED_1234_ABCD_0001);
        for _ in 0..4000 {
            let model = if rng.next().is_multiple_of(5) {
                ModelAffiliation::Local
            } else {
                ModelAffiliation::Institution(inst(rng.pick(&corpus)))
            };
            let ext = if rng.next().is_multiple_of(4) {
                ExtensionAffiliation::Any
            } else {
                let n = (rng.next() % 4) as usize;
                ExtensionAffiliation::institutions(
                    (0..n).map(|_| inst(rng.pick(&corpus))).collect::<Vec<_>>(),
                )
            };

            // Totality: it returns. (A panic aborts the test.)
            let answer = compatible(&model, &ext);

            // Determinism: a control that answered differently on a second look
            // would be unauditable.
            assert_eq!(answer, compatible(&model, &ext), "{model:?} vs {ext:?}");

            // ...and the answer is DR-26's table, restated independently.
            let expected = match (&model, &ext) {
                (ModelAffiliation::Local, _) => true,
                (_, ExtensionAffiliation::Any) => true,
                (ModelAffiliation::Institution(id), ExtensionAffiliation::Institutions(set)) => {
                    set.contains(id)
                }
            };
            assert_eq!(answer, expected, "{model:?} vs {ext:?}");
        }
    }

    // --------------------------------------------------------------- Shape and
    // type-level invariants the rest of Phase 6 depends on.

    /// Task 48 puts `ModelAffiliation` on `CallCapability`, whose `Copy` derive
    /// is load-bearing (`capability.rs`: it threads into `async move` blocks
    /// owning no `&self`). An owned `String`/`BTreeSet` inside it would break
    /// every gate signature downstream, so pin it here where the cause is
    /// visible rather than at the far end of the blast radius.
    #[test]
    fn the_model_side_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<InstitutionId>();
        assert_copy::<ModelAffiliation>();
    }

    /// `InstitutionId`'s own doc comment claims equality compares the string
    /// **contents**, not the pointer, and rests the safety of the interned
    /// design on it: were the table ever to hand out two pointers for one slug,
    /// content equality still says "same institution" where pointer identity
    /// would make an institution mismatch *itself* — the fail-open that trains
    /// users to click through the warning.
    ///
    /// Nothing defended that claim. Replacing the derives with pointer
    /// identity — the obvious "optimisation" for an interner, and one that
    /// looks correct because the table normally does return one pointer per
    /// slug — passed every other test in this file (measured, 16/16). So the
    /// stated safety net could have been removed silently. This is the test
    /// that fails instead.
    #[test]
    fn ids_with_equal_contents_but_distinct_pointers_are_equal() {
        let interned = InstitutionId::new("ucsf");
        // Bypass the interner. The interned copy is leaked and therefore still
        // live, so a second allocation of the same bytes cannot share its
        // address — the two pointers differ deterministically, not by luck.
        let bypassed = InstitutionId(Box::leak(String::from("ucsf").into_boxed_str()));
        assert!(
            !std::ptr::eq(interned.0.as_ptr(), bypassed.0.as_ptr()),
            "fixture is only meaningful if the two pointers really differ"
        );

        // `Eq`, which every hand-written comparison would reach for...
        assert_eq!(interned, bypassed);

        // ...`Ord`, which is what `BTreeSet::contains` uses inside
        // `compatible`, so this is the property an actual gate depends on...
        assert!(compatible(
            &ModelAffiliation::Institution(interned),
            &ExtensionAffiliation::institution(bypassed)
        ));

        // ...and `Hash`, which must agree with `Eq` or any downstream
        // `HashSet<InstitutionId>` silently holds one institution twice.
        assert_eq!(hash_of(&interned), hash_of(&bypassed));
    }

    /// Two institutions that normalise **differently** must never collapse into
    /// one id. This is the direction that fails open: a collision silently
    /// grants a cross-institutional flow that should have warned, whereas a
    /// spurious distinction only costs a warning the user can clear.
    ///
    /// It is pinned because the tempting answers to an unbounded interner —
    /// truncate an over-long slug, strip a non-ASCII one — are precisely the
    /// lossy transforms that collide, and review proposed them. Neither may be
    /// added to `InstitutionId::new`: a malformed or hostile institution id has
    /// to be **rejected at the parse that admits it** (Task 47, where an
    /// institution the registry does not publish is already a mismatch), never
    /// quietly rewritten into a well-formed one that means something else.
    #[test]
    fn distinct_institution_names_never_collide() {
        let names = [
            "ucsf",
            "ucsfhealth",
            "ucsf-health",
            "ucsf_health",
            "ücsf",
            "stanford",
            "",
            "i",
            "İ",
        ];
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                assert_ne!(inst(a), inst(b), "{a:?} and {b:?} became one institution");
            }
        }

        // ...including a pair that differs only after a long common prefix,
        // which is exactly what a length cap in the constructor would erase.
        let long_a = format!("{}a", "u".repeat(4096));
        let long_b = format!("{}b", "u".repeat(4096));
        assert_ne!(inst(&long_a), inst(&long_b));
    }

    /// "Represent `Institution(x)` on the extension side as `Institutions({x})`;
    /// one shape, not two." A second spelling is a second thing to compare.
    #[test]
    fn a_single_institution_extension_is_just_a_one_element_allowlist() {
        assert_eq!(
            ExtensionAffiliation::institution(inst("ucsf")),
            allowlist(&["ucsf"])
        );
    }

    /// A private extension with no institutional constraint is safe for any
    /// private model — so the default may not be an allowlist.
    #[test]
    fn the_default_extension_affiliation_is_any() {
        assert_eq!(ExtensionAffiliation::default(), ExtensionAffiliation::Any);
    }

    /// Deserialising is the third place a raw institution string enters the
    /// process (after a provider decider and the registry), and it must
    /// normalise like the other two. A `"UCSF"` in `registry.json` that landed
    /// as a distinct institution from `ucsf` is precisely the self-mismatch
    /// above, arriving by a different door.
    #[test]
    fn deserialising_an_institution_id_normalises_it() {
        let parsed: InstitutionId = serde_json::from_str("\" UCSF \"").unwrap();
        assert_eq!(parsed, inst("ucsf"));
        assert_eq!(serde_json::to_string(&parsed).unwrap(), "\"ucsf\"");
    }

    // ------------------------------------------------ The warning (Task 47).
    //
    // DR-26: a mismatch WARNS, and the warning is the product. "This may be a
    // compliance risk" is a shrug — it must name the institution that owns the
    // extension, the institution whose model is bound, and what will be sent
    // where, because a user can only accept a risk that was stated to them.

    /// A warning is offered for a mismatch and for nothing else. A composer that
    /// produced copy for a compatible pair would put a compliance warning on the
    /// arrangement everyone approved — UCSF's Versa reaching the UCSF OMOP agent
    /// — which is the prompt fatigue DR-19 exists to avoid.
    #[test]
    fn a_compatible_pair_has_no_warning() {
        for (model, ext) in [
            (ModelAffiliation::Local, allowlist(&["stanford"])),
            (ModelAffiliation::Local, ExtensionAffiliation::Any),
            (bound("ucsf"), ExtensionAffiliation::Any),
            (bound("ucsf"), allowlist(&["ucsf"])),
            (bound("ucsf"), allowlist(&["stanford", "ucsf"])),
        ] {
            assert_eq!(
                cross_affiliation_warning(model, "SomeAgent", &ext),
                None,
                "{model:?} vs {ext:?} is compatible and must not warn"
            );
        }
    }

    /// The warning names the extension, the institution that owns its data and
    /// the institution the bound model is covered by — all three, because
    /// dropping any one leaves a sentence the user cannot act on.
    #[test]
    fn the_warning_names_the_extension_and_both_institutions() {
        let warning =
            cross_affiliation_warning(bound("ucsf"), "AtlantisAgent", &allowlist(&["stanford"]))
                .expect("a mismatch must warn");
        assert!(warning.contains("AtlantisAgent"), "{warning}");
        assert!(warning.contains("stanford"), "{warning}");
        assert!(warning.contains("ucsf"), "{warning}");
        assert!(
            warning.contains("UCSF"),
            "the published display name: {warning}"
        );
    }

    /// An institution the registry does not publish has no display name, and the
    /// raw id is surfaced instead. Task 47's Step 2: failing open on a typo is
    /// how a real constraint disappears, so the unknown id must reach the user's
    /// eyes rather than being dropped for being unrenderable.
    #[test]
    fn an_institution_with_no_display_name_is_surfaced_raw() {
        assert_eq!(institution_display_name(inst("ucsf")), Some("UCSF"));
        assert_eq!(institution_display_name(inst("atlantis")), None);

        let warning =
            cross_affiliation_warning(bound("ucsf"), "AtlantisAgent", &allowlist(&["atlantis"]))
                .expect("a mismatch must warn");
        assert!(warning.contains("atlantis"), "{warning}");
    }

    /// Every owner is named, not just the first. An allowlist that omits the
    /// bound institution may still name several, and a user deciding whether a
    /// DUA covers the flow needs all of them.
    #[test]
    fn the_warning_names_every_owner_of_a_multi_institution_allowlist() {
        let warning = cross_affiliation_warning(
            bound("ucsf"),
            "SharedAgent",
            &allowlist(&["broad", "stanford"]),
        )
        .expect("a mismatch must warn");
        assert!(warning.contains("broad"), "{warning}");
        assert!(warning.contains("stanford"), "{warning}");
    }

    /// An empty allowlist permits nothing, and says so in words rather than
    /// producing a sentence with a blank where the institution should be.
    #[test]
    fn an_empty_allowlist_warns_without_naming_an_institution_it_does_not_have() {
        let empty = ExtensionAffiliation::Institutions(BTreeSet::new());
        let warning = cross_affiliation_warning(bound("ucsf"), "EmptyAgent", &empty)
            .expect("an empty allowlist admits nothing, so it must warn");
        assert!(warning.contains("EmptyAgent"), "{warning}");
        assert!(warning.contains("no institution"), "{warning}");
    }

    // ---------------------------------------------------------------- Fixtures

    /// Inputs chosen to break a normaliser: mixed case, interior and edge
    /// whitespace, non-ASCII with a multi-character lowercasing (`İ`), width
    /// variants, and a long one.
    fn fuzz_corpus() -> Vec<String> {
        let mut corpus: Vec<String> = [
            "",
            " ",
            "\t\n",
            "ucsf",
            "UCSF",
            " ucsf ",
            "UcSf",
            "u c s f",
            "stanford",
            "STANFORD",
            "ucsf-health",
            "ucsf_health",
            "ücsf",
            "ÜCSF",
            "ＵＣＳＦ",
            "İ",
            "i",
            "ß",
            "0",
            "-",
            "local",
            "Local",
            "any",
            "institution",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        corpus.push("u".repeat(4096));
        corpus.push("U".repeat(4096));
        corpus
    }

    struct Xorshift(u64);

    impl Xorshift {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn pick<'a>(&mut self, xs: &'a [String]) -> &'a str {
            &xs[(self.next() % xs.len() as u64) as usize]
        }
    }
}
