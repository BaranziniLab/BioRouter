//! Affiliation across the crate boundary (issue #56, DR-26 / Task 50 Step 0).
//!
//! DR-26's third axis lives in `biorouter::privacy::affiliation`, and this crate
//! **cannot see it**: `biorouter-mcp` does not depend on `biorouter` (see
//! [`super::tier`]'s header, which records the same constraint for the tier).
//! Until this module existed exactly **one bit** crossed — the
//! `biorouter-capability-tier` `_meta` key, a `"private"`/`"public"` word — so an
//! affiliation added to `CallCapability` reached no MCP server at all.
//!
//! # Why a SECOND key rather than a richer value on the first
//!
//! DR-26 Step 0 offers both. This takes the second key, because widening
//! `biorouter-capability-tier`'s grammar fails **open** on the axis that already
//! works: [`super::tier::caller_is_private`] compares the value against the exact
//! word `private`, so any binary that has not been updated — and this tree ships
//! a separately-installed PATH CLI that routinely lags the app, see the "Runtime
//! CLI-vs-app drift" note in `CLAUDE.md` — would read `private:institution:ucsf`
//! as PUBLIC and hand a private caller a public model's reach.
//!
//! A second key is additive in the safe direction: a reader that does not know
//! it ignores it, and a reader that does know it treats absence as
//! [`CallerAffiliation::Unstated`], which is the **most restrictive** caller in
//! the table below.
//!
//! # The table, and why it is not a copy of `affiliation::compatible`
//!
//! The extension side of DR-26 is an **allowlist**: `Institutions([a, b])` names
//! who may reach it, so a model in the list is compatible. A knowledge base's set
//! is the opposite — DR-26 Task 50 Step 1 defines it as "the union of what they
//! ingested", i.e. a set of **owners**, and says in as many words that "a model
//! matching only one of them is a mismatch". So a KB owned by `{ucsf, stanford}`
//! is reachable only from a model that covers *both*, which no single-institution
//! model does.
//!
//! That is not a second implementation of the extension table; it is that table
//! applied **once per owner and conjoined** — a KB owned by `S` is reachable iff
//! the model clears `Institution(o)` for every `o` in `S`. That relationship is
//! pinned across the crate boundary by
//! `biorouter::privacy::affiliation::tests::the_knowledge_base_rule_is_dr_26s_table_conjoined_over_owners`,
//! which drives both crates' functions over one corpus — this crate cannot
//! import the other, so the audit lives on the side that can see both.
//!
//! ⚠ **[`CallerAffiliation::Local`] returns before any comparison**, for the
//! reason DR-26 states and an implementer gets wrong: a local model reaches
//! everything private because **no transfer occurs at all**. It is the most
//! permissive value, not a peer of the institutions.

use std::collections::BTreeSet;

/// The meta key the daemon writes the caller's **affiliation** into, beside
/// [`super::tier::CAPABILITY_TIER_META_KEY`].
///
/// Defined here rather than in the daemon for the reason the tier key is:
/// the writer is in `biorouter` and the reader is in this crate, and two
/// spellings of one key is exactly how a barrier silently stops working.
pub const CAPABILITY_AFFILIATION_META_KEY: &str = "biorouter-capability-affiliation";

/// The wire word for a model that runs on this machine.
const LOCAL: &str = "local";

/// The wire prefix for a model covered by a named institution's agreements.
const INSTITUTION_PREFIX: &str = "institution:";

/// Whose agreements cover the model bound to the calling session, as this crate
/// can see it.
///
/// The mirror of `biorouter::privacy::affiliation::ModelAffiliation` plus the
/// `None` that trait's `Option` carries, flattened into one enum because the
/// wire has no way to distinguish "absent because public" from "absent because
/// the provider stated nothing" — and both must be read the same restrictive
/// way here.
/// ⚠ **[`Self::Unstated`] is the `Default`**, which is what makes an argument
/// struct that forgets this axis fail *closed*. `Local` would be the opposite
/// and is the one value that must never be reached by omission — it is the most
/// permissive in the model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CallerAffiliation {
    /// Runs on this machine; the data never leaves it. The **most** permissive
    /// value — see the module header.
    Local,
    /// Covered by that institution's agreements. The id is normalised by
    /// [`normalize_institution`].
    Institution(String),
    /// No affiliation reached this server: an older daemon, a non-built-in
    /// transport, a public caller, a provider that overrode `tier()` and forgot
    /// `affiliation()`, or an unrecognised wire value.
    ///
    /// ⚠ **The restrictive answer, and it must stay that way.** DR-26 gives a
    /// private model with no stated affiliation the same treatment as a foreign
    /// institution's: it mismatches every claimed base. Reading absence as "no
    /// constraint" is the fail-open this axis exists to prevent.
    #[default]
    Unstated,
}

/// Whose content a knowledge base holds.
///
/// ⚠ **Not an allowlist.** DR-26 Task 50 Step 1: a base's affiliation is the
/// **union of what it ingested**, so every institution in the set is an owner
/// whose agreements the reader must be covered by. See the module header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KbAffiliation {
    /// The institutions whose sessions have written into this base. **Empty is
    /// permissive**: no institution claims the content, so every private model
    /// may read it — the exact opposite of an empty allowlist on the extension
    /// side, and the reason these are two types rather than one.
    Owners(BTreeSet<String>),
    /// The store could not be read, so whose content this base holds is not
    /// known.
    ///
    /// ⚠ **This is the "fail closed" half of Step 0's migration warning.** A
    /// schema-2 reader that collapsed an unreadable store onto
    /// `Owners(∅)` would read an unrecognised affiliation as "no constraint",
    /// inverting the discipline [`super::tier`] already keeps (a `Missing` file
    /// reads public because the migration has not run; an `Unreadable` one reads
    /// private because the answer is unknown). Only [`CallerAffiliation::Local`]
    /// — which transfers nothing — clears it.
    Unknown,
}

impl KbAffiliation {
    /// The institutions named, or `None` when the answer is unknown. For the
    /// refusal, which names them.
    pub fn owners(&self) -> Option<&BTreeSet<String>> {
        match self {
            Self::Owners(owners) => Some(owners),
            Self::Unknown => None,
        }
    }
}

/// Normalise an institution id: strip whitespace, lowercase.
///
/// ⚠ **The same transform as `biorouter::config::extensions::name_to_key`**,
/// which is what `InstitutionId::new` on the other side of the boundary uses.
/// It is re-implemented rather than imported because this crate cannot see that
/// one, and the two are pinned equal by
/// `biorouter::privacy::affiliation::tests::the_two_crates_normalise_an_institution_id_identically`.
/// Two normalisers is how `UCSF` and `ucsf` become two institutions and an
/// institution mismatches itself — which fails open by training users to click
/// through the warning.
///
/// It is deliberately **not** lossy in any other way: truncating or stripping
/// would rewrite two distinct institutions into one id, which grants a flow that
/// should have warned.
pub fn normalize_institution(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// The value the daemon writes under [`CAPABILITY_AFFILIATION_META_KEY`], or
/// `None` when there is nothing to say and the key is omitted.
///
/// ONE spelling, one function, both sides — the correction
/// [`super::tier::capability_meta_value`] records for the tier key, applied to
/// this one from the start.
pub fn capability_meta_value(affiliation: &CallerAffiliation) -> Option<String> {
    match affiliation {
        CallerAffiliation::Local => Some(LOCAL.to_string()),
        CallerAffiliation::Institution(id) => Some(format!("{INSTITUTION_PREFIX}{id}")),
        // Absence IS the encoding of `Unstated`, so a daemon that has nothing to
        // say writes nothing and an older one is indistinguishable from it.
        CallerAffiliation::Unstated => None,
    }
}

/// The caller's affiliation, [`CallerAffiliation::Unstated`] unless the meta says
/// otherwise.
///
/// Every unrecognised value reads `Unstated`, which is the restrictive answer —
/// see that variant.
pub fn caller_affiliation(meta: &rmcp::model::Meta) -> CallerAffiliation {
    let raw = match meta
        .0
        .get(CAPABILITY_AFFILIATION_META_KEY)
        .and_then(|v| v.as_str())
    {
        Some(raw) => raw,
        None => return CallerAffiliation::Unstated,
    };
    if raw == LOCAL {
        return CallerAffiliation::Local;
    }
    match raw.strip_prefix(INSTITUTION_PREFIX) {
        Some(id) if !normalize_institution(id).is_empty() => {
            CallerAffiliation::Institution(normalize_institution(id))
        }
        // An empty institution id, or a word from a grammar this build does not
        // know. Unknown is not "no constraint".
        _ => CallerAffiliation::Unstated,
    }
}

/// The institutions a caller's content belongs to, as a set — what the ratchet
/// adds when this caller writes into a base.
///
/// ONE definition of "which callers contribute an owner", because both
/// [`super::tier::raise_affiliation_unlocked`] and `import_brkb`'s union need
/// it and two copies is how they come to disagree.
///
/// [`CallerAffiliation::Local`] and [`CallerAffiliation::Unstated`] contribute
/// **nothing**, for two different reasons that happen to agree — see
/// [`super::tier::raise_affiliation_unlocked`], which spells both out. In
/// particular a sentinel owner for `Unstated` would make the base permanently
/// unreachable from every institutional model with no declassification path.
pub fn contributed_owners(caller: &CallerAffiliation) -> BTreeSet<String> {
    match caller {
        CallerAffiliation::Institution(id) => std::iter::once(id.clone()).collect(),
        CallerAffiliation::Local | CallerAffiliation::Unstated => BTreeSet::new(),
    }
}

/// **The** affiliation comparison for a knowledge base. May a session whose
/// model is covered by `caller` read or write a base owned by `kb`?
///
/// `false` is a *mismatch*, which DR-26 answers with a warning and — where a
/// grant path exists — the user's explicit approval. It is not a policy decision
/// made here; [`super::tier::assert_reachable`] is the barrier.
///
/// | caller | base | |
/// |---|---|---|
/// | `Local` | anything | compatible |
/// | `Institution(X)` | `Owners(∅)` | compatible |
/// | `Institution(X)` | `Owners({X})` | compatible |
/// | `Institution(X)` | `Owners({X, Y})` | **mismatch** — ⚠ do not stop at the first match |
/// | `Institution(X)` | `Owners({Y})` | **mismatch** |
/// | `Institution(X)` | `Unknown` | **mismatch** |
/// | `Unstated` | `Owners(∅)` | compatible |
/// | `Unstated` | anything claimed | **mismatch** |
pub fn reachable(caller: &CallerAffiliation, kb: &KbAffiliation) -> bool {
    if matches!(caller, CallerAffiliation::Local) {
        // Returns BEFORE any comparison, and the reason is not a rule about
        // which sets contain what: a local model may reach everything private
        // because **no transfer occurs at all**. There is no disclosure, so
        // there is nothing for an agreement to govern. Writing this as equality,
        // or as membership in a set that happens to contain `Local`, makes the
        // local model match only itself and breaks the single most important
        // case while every other row of the table still passes. See DR-26.
        return true;
    }
    let owners = match kb {
        // Whose content this is cannot be established, so only the caller that
        // transfers nothing may have it.
        KbAffiliation::Unknown => return false,
        KbAffiliation::Owners(owners) => owners,
    };
    match caller {
        CallerAffiliation::Local => true,
        // EVERY owner, not the first that matches: a base holding both
        // institutions' content is reachable from neither institution's model.
        CallerAffiliation::Institution(bound) => owners.iter().all(|owner| owner == bound),
        CallerAffiliation::Unstated => owners.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owners(names: &[&str]) -> KbAffiliation {
        KbAffiliation::Owners(names.iter().map(|n| n.to_string()).collect())
    }

    fn bound(name: &str) -> CallerAffiliation {
        CallerAffiliation::Institution(name.to_string())
    }

    fn meta_with(value: &str) -> rmcp::model::Meta {
        let mut meta = rmcp::model::Meta::default();
        meta.0.insert(
            CAPABILITY_AFFILIATION_META_KEY.to_string(),
            serde_json::Value::String(value.to_string()),
        );
        meta
    }

    /// Row 1, and the mistake DR-26 says an implementer will make. An equality
    /// test — or set membership that happens to include `Local` — passes every
    /// other row and fails exactly here.
    #[test]
    fn a_local_model_reaches_every_knowledge_base() {
        for kb in [
            owners(&[]),
            owners(&["ucsf"]),
            owners(&["ucsf", "stanford"]),
            KbAffiliation::Unknown,
        ] {
            assert!(
                reachable(&CallerAffiliation::Local, &kb),
                "a local model must reach {kb:?}: no transfer occurs at all"
            );
        }
    }

    /// An unclaimed base is reachable from every private model. This is the
    /// direction that is NOT symmetric with the extension side, where an empty
    /// allowlist permits nothing.
    #[test]
    fn a_base_no_institution_claims_is_reachable_from_every_private_model() {
        for caller in [
            CallerAffiliation::Local,
            bound("ucsf"),
            bound("stanford"),
            CallerAffiliation::Unstated,
        ] {
            assert!(reachable(&caller, &owners(&[])), "{caller:?}");
        }
    }

    /// The single-owner rows.
    #[test]
    fn a_single_owner_base_is_reachable_only_from_that_institutions_model() {
        assert!(reachable(&bound("ucsf"), &owners(&["ucsf"])));
        assert!(!reachable(&bound("stanford"), &owners(&["ucsf"])));
    }

    /// ⚠ **The headline row of Task 50's gate**: "a two-institution KB
    /// mismatches a model matching only one". An implementation that stopped at
    /// the first match — `owners.contains(bound)`, the obvious reading, and the
    /// one the extension side actually uses — passes every other test here.
    #[test]
    fn a_two_institution_base_mismatches_a_model_matching_only_one_of_them() {
        let both = owners(&["stanford", "ucsf"]);
        assert!(
            !reachable(&bound("ucsf"), &both),
            "a UCSF model reached a base that also holds Stanford's content"
        );
        assert!(
            !reachable(&bound("stanford"), &both),
            "a Stanford model reached a base that also holds UCSF's content"
        );
        // …and the local model still reaches it, because it never compares.
        assert!(reachable(&CallerAffiliation::Local, &both));
    }

    /// A private model that states no affiliation mismatches every **claimed**
    /// base, exactly as `affiliation::unstated_model` does on the extension
    /// side. Absence is not a permit.
    #[test]
    fn an_unstated_caller_reaches_nothing_any_institution_claims() {
        assert!(!reachable(&CallerAffiliation::Unstated, &owners(&["ucsf"])));
        assert!(!reachable(
            &CallerAffiliation::Unstated,
            &owners(&["ucsf", "stanford"])
        ));
        assert!(!reachable(
            &CallerAffiliation::Unstated,
            &KbAffiliation::Unknown
        ));
    }

    /// The fail-closed half of Step 0's migration warning: an unreadable store
    /// is not "no constraint".
    #[test]
    fn an_unknown_base_affiliation_is_reachable_only_from_a_local_model() {
        assert!(reachable(
            &CallerAffiliation::Local,
            &KbAffiliation::Unknown
        ));
        for caller in [
            bound("ucsf"),
            bound("stanford"),
            CallerAffiliation::Unstated,
        ] {
            assert!(
                !reachable(&caller, &KbAffiliation::Unknown),
                "{caller:?} reached a base whose owners could not be read"
            );
        }
    }

    /// The wire grammar round-trips, and every value this build writes is one it
    /// reads back to the same thing. A formatter and a parser that disagree is a
    /// barrier that silently stops working.
    #[test]
    fn every_value_the_daemon_writes_parses_back_to_what_it_meant() {
        for caller in [
            CallerAffiliation::Local,
            bound("ucsf"),
            bound("some-institution-this-build-never-heard-of"),
        ] {
            let wire = capability_meta_value(&caller).expect("a stated affiliation has a value");
            assert_eq!(caller_affiliation(&meta_with(&wire)), caller, "{wire}");
        }
        // `Unstated` is encoded by the key's ABSENCE, which is what makes an
        // older daemon indistinguishable from a provider that states nothing.
        assert_eq!(capability_meta_value(&CallerAffiliation::Unstated), None);
        assert_eq!(
            caller_affiliation(&rmcp::model::Meta::default()),
            CallerAffiliation::Unstated
        );
    }

    /// Every unrecognised wire value reads `Unstated` — the restrictive answer
    /// — rather than being ignored or read as `Local`.
    #[test]
    fn an_unrecognised_wire_value_reads_as_unstated_not_as_permission() {
        for raw in [
            "",
            "Local",
            "private",
            "institution:",
            "institution:  ",
            "institution",
            "ucsf",
            "local:ucsf",
            "{}",
        ] {
            assert_eq!(
                caller_affiliation(&meta_with(raw)),
                CallerAffiliation::Unstated,
                "{raw:?} was read as something other than Unstated"
            );
        }
    }

    /// The wire id is normalised on the way in, so a hand-written
    /// `institution:UCSF` matches a base owned by `ucsf` instead of mismatching
    /// itself.
    #[test]
    fn a_wire_institution_id_is_normalised_on_parse() {
        assert_eq!(
            caller_affiliation(&meta_with("institution: UCSF ")),
            bound("ucsf")
        );
        assert!(reachable(
            &caller_affiliation(&meta_with("institution:UCSF")),
            &owners(&["ucsf"])
        ));
    }

    /// The normaliser is not lossy in any direction beyond case and whitespace:
    /// two institutions that differ elsewhere must never collapse into one, or a
    /// cross-institutional flow is granted that should have warned.
    #[test]
    fn distinct_institution_names_never_collide() {
        let names = ["ucsf", "ucsfhealth", "ucsf-health", "ucsf_health", "ücsf"];
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                assert_ne!(
                    normalize_institution(a),
                    normalize_institution(b),
                    "{a:?} and {b:?} became one institution"
                );
            }
        }
        assert_eq!(normalize_institution(" UCSF\t"), "ucsf");
        assert_eq!(normalize_institution("U c S f"), "ucsf");
    }
}
