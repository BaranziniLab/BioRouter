//! The user's own knowledge-base tier control (issue #56, DR-18) — the one
//! writer in the tree permitted to LOWER a base's tier.
//!
//! Everywhere else the classification is a ratchet: [`super::tier::raise_unlocked`]
//! is monotone by construction and physically cannot express a lowering,
//! whatever the caller passes. That is the property Tasks 10A–10D rest on, so
//! the escape hatch is exactly one function, in its own file, with a type-level
//! proof of user in its signature and the provenance written in the same entry.
//!
//! ⚠ **There is no `kb_set_tier` tool and there must never be one.** A model
//! raises a tier as a side effect of writing (Task 10B) and can do nothing else.
//! Every wrong implementation of this feature is a route or a tool that lets the
//! model choose, which is why the audits below enumerate rather than count.

use crate::knowledge::tier;
use crate::knowledge::types::KbTier;
use anyhow::Result;
use std::path::Path;

/// The `reason` a base carries after a user released it.
pub const PUBLICIZED_BY_USER: &str = "publicized_by_user";
/// …and after a user pulled it back. A separate word on purpose: a base a user
/// privatized is not a base a user released, and one string for both would say
/// the wrong thing half the time.
pub const PRIVATIZED_BY_USER: &str = "privatized_by_user";

/// Proof that a human asked. A ZST with a **private field**, so the tuple
/// literal `UserKbTierChange(())` is unavailable outside this module and the
/// named constructor below is the only door.
///
/// It is constructed in exactly one place — the `POST
/// /knowledge/bases/{id}/tier` handler, after `auth` has matched the user-action
/// proof Task 18A issues. No MCP server, no `#[tool]` handler, no macro, no
/// `KbToolDispatch` and no CLI subcommand can construct one.
///
/// This is the session-side proof-of-user in `biorouter::privacy::declassify`,
/// for bases instead of sessions, and it is a **separate type on purpose**: one
/// proof must not be spendable on the other subject.
///
/// ⚠ Its name is deliberately not spelled out here. That module's own audit
/// asserts the set of files naming it is exactly `{routes/session.rs}` — the
/// same shape as the audit below — so a doc comment mentioning it would turn
/// this file into a second member and fail a test that is doing its job.
///
/// ⚠ **What Rust enforces here, stated precisely.** The single caller lives in
/// `biorouter-server`, a different crate, and `pub(in path)` does not cross a
/// crate boundary — so the constructor is `pub`, and the language guarantees
/// only that a caller cannot fabricate the proof by writing the struct literal.
/// What caps the number of call sites at one is
/// [`tests::the_proof_of_user_is_constructed_in_exactly_one_place`], a repo walk
/// asserting the set of files that construct it is exactly `{routes/knowledge.rs}`
/// and that they do so once. An MCP server or a CLI subcommand reaching for this
/// would have to name the type, and the build turns red.
pub struct UserKbTierChange(());

impl UserKbTierChange {
    /// Mint the proof. Call this **only** after `auth::user_action_proof` has
    /// returned `Proven`; see the handler, which is the sole call site.
    pub fn from_user_action() -> Self {
        Self(())
    }

    /// The same proof, for tests that exercise the writer rather than the route.
    #[cfg(test)]
    fn for_test() -> Self {
        Self(())
    }
}

/// The ONLY writer in the tree permitted to LOWER a base's tier.
///
/// [`tier::raise_unlocked`] is monotone by construction and stays that way; this
/// bypasses it with its own write, and
/// [`tests::exactly_one_writer_outside_the_ratchet_saves_the_tier_store`] asserts
/// exactly one such writer exists.
///
/// Writes the provenance in the same entry, before returning: `reason` and
/// `changed_at`, so a released base is never indistinguishable from one that was
/// always public. The two land in **one** `save`, so a crash leaves either both
/// or neither — the same "audit before the write, in the same transaction"
/// ordering `privacy::declassify` uses for a session.
///
/// It writes both directions, including a "change" that is already the current
/// value: re-publicizing an already-public base refreshes `changed_at` rather
/// than silently doing nothing, because the user did ask and the record should
/// say when. It is not a no-op worth optimising — this is a human clicking a
/// button, not a hot path.
///
/// ⚠ **Caller must hold the root lock.** [`crate::knowledge::service::KnowledgeService::set_tier_by_user`]
/// is the wrapper that takes it. Task 10A decision (5b): calling that wrapper
/// from inside `create_base` / `import_brkb` / `delete_base`, which already hold
/// the lock, deadlocks — `FileLockGuard::acquire` opens a fresh fd and `flock`s
/// exclusively, so a second acquire in the same process blocks forever.
pub(super) fn set_unlocked(
    root: &Path,
    kb_id: &str,
    new_tier: KbTier,
    _ok: &UserKbTierChange,
) -> Result<()> {
    // Refuses an unreadable store rather than replacing it. This is the lowering
    // writer, so it is the one where "just overwrite it" is most tempting and
    // most destructive: a silent replacement erases every ratchet in the file.
    let mut store = tier::load_for_write(root)?;
    let word = if new_tier.is_private() {
        tier::PRIVATE
    } else {
        tier::PUBLIC
    };
    let reason = if new_tier.is_private() {
        PRIVATIZED_BY_USER
    } else {
        PUBLICIZED_BY_USER
    };
    store.bases.insert(kb_id.to_string(), word.to_string());
    store.provenance.insert(
        kb_id.to_string(),
        tier::Provenance {
            reason: reason.to_string(),
            changed_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    tier::save(root, &store)?;
    tracing::info!(
        kb_id,
        tier = word,
        "knowledge base tier changed by the user (issue #56 DR-18)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::service::KnowledgeService;
    use crate::knowledge::tier::{entry as tier_entry, TierEntry};
    use crate::knowledge::types::KbTier;

    /// Returns the `TempDir` alongside the service: dropping it deletes the
    /// knowledge root, so a bare `let svc = svc_with_base(..)` would unlink the
    /// tree before the first assertion ran. Same reason `tier.rs`'s
    /// `tempdir_with_bases` returns a pair.
    fn svc_with_base(id: &str) -> (tempfile::TempDir, KnowledgeService) {
        let d = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(d.path().to_path_buf());
        svc.create_base(id, id, None).unwrap();
        (d, svc)
    }

    fn tier_of(svc: &KnowledgeService, kb_id: &str) -> KbTier {
        tier_entry(svc.root(), kb_id).tier
    }

    fn entry(svc: &KnowledgeService, kb_id: &str) -> TierEntry {
        tier_entry(svc.root(), kb_id)
    }

    #[test]
    fn only_a_user_proof_can_lower_a_base_and_a_model_cannot_construct_one() {
        // The mirror of Task 29's `only_a_user_confirmation_can_lower_the_tier`.
        // `UserKbTierChange` is a ZST with a private field, so the KB MCP server,
        // the three macros, `KbToolDispatch` and every `#[tool]` handler have no
        // path to one — pinned across the tree by
        // `the_proof_of_user_is_constructed_in_exactly_one_place`.
        let (_d, svc) = svc_with_base("omop");
        svc.raise_tier("omop", /* caller_is_private */ true)
            .unwrap();
        assert_eq!(tier_of(&svc, "omop"), KbTier::Private);

        svc.set_tier_by_user("omop", KbTier::Public, &UserKbTierChange::for_test())
            .unwrap();
        assert_eq!(tier_of(&svc, "omop"), KbTier::Public);

        // …and the other direction, which is the cheap one.
        svc.set_tier_by_user("omop", KbTier::Private, &UserKbTierChange::for_test())
            .unwrap();
        assert_eq!(tier_of(&svc, "omop"), KbTier::Private);
    }

    #[test]
    fn a_publicized_base_is_not_indistinguishable_from_one_that_was_always_public() {
        // The `.kb-tiers` entry records how the value got there, exactly as
        // `privacy_reason` does for a session. Without this, a user cannot tell a
        // base they released from one that was never private, and neither can a
        // support conversation six months later.
        let (_d, svc) = svc_with_base("omop");
        svc.raise_tier("omop", true).unwrap();
        svc.set_tier_by_user("omop", KbTier::Public, &UserKbTierChange::for_test())
            .unwrap();
        let e = entry(&svc, "omop");
        assert_eq!(e.tier, KbTier::Public);
        assert_eq!(e.reason.as_deref(), Some("publicized_by_user"));
        assert!(e.changed_at.is_some());

        // The other direction is recorded too, and with its OWN word: a base a
        // user pulled back is not a base a user released, and one reason string
        // for both would say the wrong thing half the time.
        svc.set_tier_by_user("omop", KbTier::Private, &UserKbTierChange::for_test())
            .unwrap();
        let e = entry(&svc, "omop");
        assert_eq!(e.tier, KbTier::Private);
        assert_eq!(e.reason.as_deref(), Some("privatized_by_user"));

        // A base nobody ever moved carries no provenance at all, which is what
        // makes the assertions above mean something.
        let (_d2, plain) = svc_with_base("notes");
        assert_eq!(entry(&plain, "notes").reason, None);
        assert_eq!(entry(&plain, "notes").changed_at, None);
    }

    #[test]
    fn the_ratchet_still_ratchets_after_a_publicize() {
        // Publicizing is not an exemption. The next private write raises it again,
        // and the user is not silently left on a base that stopped ratcheting.
        let (_d, svc) = svc_with_base("omop");
        svc.raise_tier("omop", true).unwrap();
        svc.set_tier_by_user("omop", KbTier::Public, &UserKbTierChange::for_test())
            .unwrap();
        svc.raise_tier("omop", /* caller_is_private */ true)
            .unwrap();
        assert_eq!(tier_of(&svc, "omop"), KbTier::Private);

        // …and the provenance goes with it. A row reading Private under
        // `publicized_by_user` would be an audit trail that says the opposite of
        // what happened.
        assert_eq!(entry(&svc, "omop").reason, None);
    }

    #[test]
    fn a_user_change_survives_a_reader_that_cannot_parse_the_store() {
        // The one direction `load_for_write` must NOT be relaxed in: an
        // unreadable store is refused rather than replaced, because silently
        // rewriting it would erase every ratchet in it. This is the lowering
        // writer, so it is the one where "just overwrite it" is most tempting.
        let (_d, svc) = svc_with_base("omop");
        std::fs::write(
            crate::knowledge::paths::kb_tiers_path(svc.root()),
            "{not json",
        )
        .unwrap();
        assert!(svc
            .set_tier_by_user("omop", KbTier::Public, &UserKbTierChange::for_test())
            .is_err());
        // And the base still reads private, which is where an unparseable store
        // leaves every base with a directory on disk.
        assert_eq!(tier_of(&svc, "omop"), KbTier::Private);
    }

    /// The proof-of-user is a cross-crate `pub` constructor, because the only
    /// caller lives in `biorouter-server` and `pub(in …)` cannot cross a crate
    /// boundary. Rust therefore cannot restrict it to one call site on its own —
    /// this test is what does, and it is Task 29's
    /// `the_proof_of_user_is_constructed_in_exactly_one_place` for bases.
    ///
    /// It pins two things and it takes both. The set of FILES outside this one
    /// that so much as NAME the type must be exactly the service wrapper (which
    /// takes it by reference and cannot construct it — the field is private) and
    /// the HTTP handler; and the number of times the constructor is CALLED must
    /// be exactly one, in the handler.
    ///
    /// ⚠ The plan's Step 5 gate (2) expected `UserKbTierChange` to appear in
    /// **no** file but `tier_user.rs` and `routes/knowledge.rs`. That cannot hold
    /// alongside its own gate (1), which requires `set_tier_by_user` to live in
    /// `service.rs` — a wrapper cannot take an argument whose type it may not
    /// name. The property that matters is CONSTRUCTION, not mention, so that is
    /// what the second assertion counts.
    #[test]
    fn the_proof_of_user_is_constructed_in_exactly_one_place() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let crates = root.join("crates");
        assert!(
            crates.is_dir(),
            "the audit walks {} — if that path is wrong every assertion below \
             passes for the wrong reason",
            crates.display()
        );

        // Composed rather than written out, so this file does not match its own
        // audit and so a reader cannot mistake the needle for a call site.
        let named = concat!("User", "KbTierChange");
        let minted = concat!("User", "KbTierChange::from_user_action(");

        let mut naming: Vec<String> = vec![];
        let mut constructions: std::collections::BTreeMap<String, usize> = Default::default();
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
            scanned += 1;
            // This file declares the type and names it throughout its own tests;
            // counting it would make every number below larger and
            // indistinguishable from a real second site.
            if rel == "crates/biorouter-mcp/src/knowledge/tier_user.rs" {
                continue;
            }
            let src = std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
            if src.contains(named) {
                naming.push(rel.clone());
            }
            for line in src.lines() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                let hits = code.matches(minted).count();
                if hits > 0 {
                    *constructions.entry(rel.clone()).or_default() += hits;
                }
            }
        }
        assert!(
            scanned >= 400,
            "only {scanned} .rs files were scanned. A broken walk reports the same \
             empty set as a clean tree."
        );
        naming.sort();
        assert_eq!(
            naming,
            vec![
                "crates/biorouter-mcp/src/knowledge/service.rs".to_string(),
                "crates/biorouter-server/src/routes/knowledge.rs".to_string(),
            ],
            "a new file names the proof-of-user. The whole claim that a model \
             cannot publicize a knowledge base rests on this set: the service \
             wrapper, which takes the proof and cannot make one, and the route \
             behind the user-action header."
        );
        let called: Vec<(String, usize)> = constructions.into_iter().collect();
        assert_eq!(
            called,
            vec![(
                "crates/biorouter-server/src/routes/knowledge.rs".to_string(),
                1
            )],
            "the proof-of-user is minted more than once. A second construction \
             site is a second way to lower a base's tier, and only the one inside \
             the tier handler is known to sit behind the user-action guard."
        );
    }

    /// The whole audit surface for "can a base's tier be lowered anywhere else".
    ///
    /// `tier::raise_unlocked` is monotone by construction and physically cannot
    /// express a lowering, so every writer that is not [`set_unlocked`] is
    /// incapable of one — except for the raw store write those two share. This
    /// pins the set of files that call it.
    ///
    /// ⚠ A tripwire, not a proof: it matches one spelling. What it reliably
    /// catches is the realistic case — a second bypass added by someone who
    /// copied this one.
    #[test]
    fn exactly_one_writer_outside_the_ratchet_saves_the_tier_store() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let needle = concat!("tier::", "save(");
        let mut callers: Vec<String> = vec![];
        let mut scanned = 0usize;
        for entry in walkdir::WalkDir::new(root.join("crates")) {
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
            scanned += 1;
            let src = std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
            if src
                .lines()
                .any(|l| !l.trim_start().starts_with("//") && l.contains(needle))
            {
                callers.push(rel);
            }
        }
        assert!(scanned >= 400, "only {scanned} .rs files were scanned");
        callers.sort();
        assert_eq!(
            callers,
            vec!["crates/biorouter-mcp/src/knowledge/tier_user.rs".to_string()],
            "a second module writes the tier store directly, bypassing the \
             monotone ratchet. `tier::save` is `pub(super)` so only the knowledge \
             module can reach it at all; this is what keeps that reach at one."
        );
    }
}
