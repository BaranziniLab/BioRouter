//! Declassification (issue #56, §12.4/§12.5) — the one writer in the tree
//! permitted to LOWER `sessions.privacy_tier`.
//!
//! Everywhere else the classification is a permanent ratchet: writes go through
//! [`crate::session::session_manager::SessionUpdateBuilder::raise_privacy`],
//! whose emission is a monotone `CASE WHEN` that physically cannot express a
//! lowering, whatever the caller passes. That is the property the whole feature
//! rests on, so the escape hatch is exactly one function, in its own module,
//! with a type-level proof of user in its signature and a ledger row for every
//! use.

use crate::privacy::SessionClassification;
use crate::session::session_manager::SessionManager;
use anyhow::Result;

/// The `privacy_reason` a declassified session carries afterwards.
///
/// It replaces the provenance (`mcp:…`/`turn:…`) rather than clearing it, so a
/// row that reads Public is still distinguishable from one that was born Public
/// — and the storage layer treats a non-`mcp:` reason as displaceable, so a
/// later raise records the new provenance normally
/// (`session_manager.rs`'s `raise_privacy` doc comment spells this out).
pub const DECLASSIFIED_BY_USER: &str = "declassified_by_user";

/// §12.4's graded confirmation, as a predicate over the stored provenance.
///
/// `true` → the user must retype the last six characters of the session id.
/// `false` → a single click, with a five-second undo.
///
/// **Phrased as an exception, not as a match**, and that is the whole design:
/// the weak control is granted only when the provenance says, in as many words,
/// that this chat merely ran a turn against a private endpoint. Everything else
/// gets the strong one.
///
/// The alternative — `starts_with("mcp:")` → strong — is what §12.4 literally
/// says and it fails open twice over. `privacy_reason`'s vocabulary is `turn:*`,
/// `mcp:*`, `inherited:<parent>`, `diverged:<parent>`, `backfill:<provider>`,
/// `imported` and `declassified_by_user`; a chat branched out of an OMOP session
/// carries `diverged:<parent>`, not `mcp:*`, so a match-on-`mcp:` rule hands the
/// single-click control to a copy of exactly the conversation §12.4 wants
/// protected. §12.4 acknowledges this ("or inherited from an `mcp:*` ancestor")
/// and would need an ancestor walk to implement as written; reading the absence
/// of a `turn:` prefix as "unknown, so protect it" gets the same answer for
/// every inherited case without one, and gets it right for a reason that is
/// absent (a projection bug, a future vocabulary entry) too.
///
/// `backfill:*` is turn-like in origin and still lands on the strong control:
/// the migration inferred that tier from a bound provider rather than observing
/// a turn, so what the chat actually reached is unknown.
pub fn requires_typed_confirmation(privacy_reason: Option<&str>) -> bool {
    !privacy_reason.is_some_and(|reason| reason.starts_with("turn:"))
}

/// Proof that a human confirmed.
///
/// A ZST with a **private field**, so the tuple literal `UserConfirmation(())`
/// is unavailable outside this module and the named constructor below is the
/// only door.
///
/// ⚠ **What Rust enforces here, stated precisely, because the design overstates
/// it.** §12.4 describes the constructor as `pub(in …)`, "invoked in exactly one
/// place". It cannot be: the single caller is
/// `biorouter-server::routes::session::declassify_session`, in a different
/// crate, and `pub(in path)` does not cross a crate boundary. So the constructor
/// is `pub`, and the language guarantees only that a caller cannot fabricate the
/// proof by writing the struct literal — it does not, by itself, cap the number
/// of call sites at one.
///
/// What caps it is `the_proof_of_user_is_constructed_in_exactly_one_place`
/// below: a repo walk asserting the set of files outside this one that so much
/// as *name* `UserConfirmation` is exactly `{routes/session.rs}`. An MCP server,
/// a `ToolRouter`, a `workspace_*` handler or a CLI subcommand that reached for
/// this would have to name the type, and the build turns red. That is a weaker
/// mechanism than the design claims and a stronger one than a route that is
/// merely undocumented — and it is the strongest available without moving
/// `declassify` into the server crate, where it would lose the `pub(crate)`
/// storage access it needs.
pub struct UserConfirmation(());

impl UserConfirmation {
    /// Mint the proof. Call this **only** after matching a confirmation the user
    /// typed; see §12.4's grading, which the route implements.
    pub fn from_typed_confirmation() -> Self {
        Self(())
    }

    /// The same proof, for tests that exercise the writer rather than the route.
    #[cfg(test)]
    fn for_test() -> Self {
        Self(())
    }
}

/// What a declassification actually did. The route turns these into 200 / 200 /
/// 404 — a "no such session" must not read as a successful declassification, and
/// an already-public row must not read as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclassifyOutcome {
    /// The row was Private (to the fail-closed reader) and is now Public. One
    /// ledger row was written.
    Declassified,
    /// The row was already exactly `public`. Nothing was written — a
    /// double-clicked confirm button must not leave a second ledger entry
    /// claiming a transition that never happened.
    AlreadyPublic,
    /// No row with that id.
    SessionNotFound,
}

/// The ONLY writer in the tree permitted to lower `privacy_tier`.
///
/// Every other write goes through the session update builder, whose emission is
/// the monotone `CASE WHEN` and physically cannot lower it; this bypasses the
/// builder with its own `UPDATE`.
/// `exactly_one_statement_in_the_tree_assigns_a_public_classification` asserts
/// that this is the only such statement outside the migration.
///
/// **The audit row is written in the SAME transaction, BEFORE the `UPDATE`**, so
/// it observes the pre-change state — which the session row itself will not hold
/// a moment later, because the declassification overwrites `privacy_reason`. A
/// crash between the two leaves either both or neither.
///
/// The bound provider is deliberately untouched. A public chat may run a private
/// model; that direction was never restricted (`bind_allowed` admits any
/// provider on a public session), so clearing it here would break a working chat
/// to enforce a rule that does not exist.
pub async fn declassify(
    sm: &SessionManager,
    session_id: &str,
    _ok: UserConfirmation,
) -> Result<DeclassifyOutcome> {
    let pool = sm.storage().pool().await?;
    let mut tx = pool.begin().await?;

    // Inside the transaction, so the state the ledger records is the state the
    // UPDATE below overwrites — not a snapshot from before some concurrent
    // ratchet.
    let Some((raw_tier, reason_before, provider_name)) =
        sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "SELECT privacy_tier, privacy_reason, provider_name FROM sessions WHERE id = ?1",
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(DeclassifyOutcome::SessionNotFound);
    };

    // The tier the READER sees, not the raw bytes. `from_stored` fails closed, so
    // a row holding `PUBLIC` (a hand-edited database, a restored backup) is
    // Private to every gate in the tree and is unassignable through the ratchet.
    // This writer canonicalises it — otherwise the user is shown a private badge
    // above a control that silently does nothing.
    let from = SessionClassification::from_stored(&raw_tier);
    if from == SessionClassification::Public {
        return Ok(DeclassifyOutcome::AlreadyPublic);
    }

    let message_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE session_id = ?1")
            .bind(session_id)
            .fetch_one(&mut *tx)
            .await?;

    // `actor` and `actor_kind` are both "user" because this daemon has no
    // principal: `check_token` compares one machine-wide bearer, so there is no
    // identity to record (AR-11/AR-15). What the row can say honestly is the
    // KIND of actor, and the kind is the whole point — the route that mints the
    // proof is the only construction site, and it is behind the user-action
    // header. Writing a fabricated username here would be worse than writing
    // none.
    sqlx::query(
        "INSERT INTO classification_audit ( \
            session_id, from_classification, to_classification, reason, actor, actor_kind, \
            app_version, provider_name_at_change, privacy_reason_before, message_count_at_change \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(session_id)
    .bind(from.as_sql())
    .bind(SessionClassification::Public.as_sql())
    .bind(DECLASSIFIED_BY_USER)
    .bind("user")
    .bind("user")
    .bind(env!("CARGO_PKG_VERSION"))
    .bind(provider_name)
    .bind(reason_before)
    .bind(message_count)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE sessions \
            SET privacy_tier = 'public', privacy_reason = ?2, updated_at = datetime('now') \
          WHERE id = ?1",
    )
    .bind(session_id)
    .bind(DECLASSIFIED_BY_USER)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    tracing::info!(
        session_id,
        from = from.as_sql(),
        "declassified by the user (issue #56 §12.5)"
    );
    Ok(DeclassifyOutcome::Declassified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelConfig;
    use crate::privacy::SessionClassification;
    use crate::session::session_manager::{SessionManager, SessionType};

    /// One audit row, in the shape §12.5 stores it.
    #[derive(Debug, sqlx::FromRow)]
    struct AuditRow {
        from_classification: String,
        to_classification: String,
        reason: String,
        actor: String,
        actor_kind: String,
        app_version: String,
        provider_name_at_change: Option<String>,
        privacy_reason_before: Option<String>,
        message_count_at_change: Option<i64>,
    }

    /// A private session carrying `reason` as its provenance and `versa_azure`
    /// as its bound provider.
    async fn private_session_with_reason(sm: &SessionManager, reason: &str) -> String {
        let s = sm
            .create_session(
                std::env::temp_dir(),
                "a cohort chat".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        sm.add_message(
            &s.id,
            &crate::conversation::message::Message::user().with_text("patient MRN 12345"),
        )
        .await
        .unwrap();
        sm.update(&s.id)
            .provider_name("versa_azure")
            .model_config(ModelConfig::new("gpt-4o").unwrap())
            .raise_privacy(SessionClassification::Private, reason)
            .apply()
            .await
            .unwrap();
        s.id
    }

    async fn audit_rows(sm: &SessionManager, session_id: &str) -> Vec<AuditRow> {
        let pool = sm.storage().pool().await.unwrap();
        sqlx::query_as::<_, AuditRow>(
            "SELECT from_classification, to_classification, reason, actor, actor_kind, \
             app_version, provider_name_at_change, privacy_reason_before, \
             message_count_at_change FROM classification_audit WHERE session_id = ?1 \
             ORDER BY id",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn only_a_user_confirmation_can_lower_the_tier() {
        // UserConfirmation is a ZST whose only constructor is invoked in exactly
        // one place: the HTTP handler, after it has matched the typed
        // confirmation. No MCP server, no ToolRouter, no workspace_* handler and
        // no CLI subcommand constructs one — the field is private, so the tuple
        // literal is unavailable outside this module, and the named constructor
        // is pinned to its single call site by
        // [`the_proof_of_user_is_constructed_in_exactly_one_place`].
        let temp = tempfile::TempDir::new().unwrap();
        let sm = SessionManager::new(temp.path().to_path_buf());
        let id = private_session_with_reason(&sm, "mcp:ucsfomopagent").await;

        declassify(&sm, &id, UserConfirmation::for_test())
            .await
            .unwrap();

        let row = sm.get_session(&id, false).await.unwrap();
        assert_eq!(row.privacy_tier, SessionClassification::Public);
        assert_eq!(row.privacy_reason.as_deref(), Some("declassified_by_user"));
        // A declassified session must never be indistinguishable from one that
        // was always public.
        let rows = audit_rows(&sm, &id).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].actor_kind, "user");
        assert_eq!(rows[0].actor, "user");
        assert_eq!(rows[0].from_classification, "private");
        assert_eq!(rows[0].to_classification, "public");
        assert_eq!(rows[0].reason, "declassified_by_user");
        assert_eq!(rows[0].app_version, env!("CARGO_PKG_VERSION"));
        // The pre-change state, which the row on `sessions` no longer holds: the
        // provenance is overwritten by the declassification itself, so the audit
        // is the ONLY remaining record of what this chat had reached.
        assert_eq!(
            rows[0].privacy_reason_before.as_deref(),
            Some("mcp:ucsfomopagent")
        );
        assert_eq!(
            rows[0].provider_name_at_change.as_deref(),
            Some("versa_azure")
        );
        assert_eq!(rows[0].message_count_at_change, Some(1));

        // The bound provider is left exactly as it was: a public chat may run a
        // private model, and that direction was never restricted.
        assert_eq!(row.provider_name.as_deref(), Some("versa_azure"));
    }

    #[tokio::test]
    async fn declassifying_twice_writes_one_row_and_a_missing_session_is_reported() {
        // A double-click on the confirm button sends two requests. The second
        // must not leave a second ledger entry claiming a private→public
        // transition that never happened.
        let temp = tempfile::TempDir::new().unwrap();
        let sm = SessionManager::new(temp.path().to_path_buf());
        let id = private_session_with_reason(&sm, "turn:versa_azure").await;

        assert_eq!(
            declassify(&sm, &id, UserConfirmation::for_test())
                .await
                .unwrap(),
            DeclassifyOutcome::Declassified
        );
        assert_eq!(
            declassify(&sm, &id, UserConfirmation::for_test())
                .await
                .unwrap(),
            DeclassifyOutcome::AlreadyPublic
        );
        assert_eq!(audit_rows(&sm, &id).await.len(), 1);

        assert_eq!(
            declassify(&sm, "29990101_00000", UserConfirmation::for_test())
                .await
                .unwrap(),
            DeclassifyOutcome::SessionNotFound
        );
    }

    #[tokio::test]
    async fn a_value_the_reader_refuses_is_repaired_rather_than_treated_as_public() {
        // `from_stored` maps anything that is not exactly `public` to Private, so
        // a row holding `PUBLIC` is private to the whole Rust tree while being
        // unassignable through the ratchet
        // (`a_stored_tier_the_reader_refuses_cannot_be_assigned_away`). This is
        // the one writer that can canonicalise it, and it must — otherwise the
        // user is shown a private badge with a control that silently does
        // nothing.
        let temp = tempfile::TempDir::new().unwrap();
        let sm = SessionManager::new(temp.path().to_path_buf());
        let id = private_session_with_reason(&sm, "turn:versa_azure").await;
        {
            let pool = sm.storage().pool().await.unwrap();
            sqlx::query("UPDATE sessions SET privacy_tier = 'PUBLIC' WHERE id = ?1")
                .bind(&id)
                .execute(pool)
                .await
                .unwrap();
        }
        assert_eq!(
            sm.get_session(&id, false).await.unwrap().privacy_tier,
            SessionClassification::Private,
            "the reader fails closed, so this row is Private to every gate"
        );

        assert_eq!(
            declassify(&sm, &id, UserConfirmation::for_test())
                .await
                .unwrap(),
            DeclassifyOutcome::Declassified
        );
        assert_eq!(
            sm.get_session(&id, false).await.unwrap().privacy_tier,
            SessionClassification::Public
        );
        // …and the ledger records the tier the READER saw, not the raw bytes,
        // because that is the classification the gates were enforcing.
        assert_eq!(audit_rows(&sm, &id).await[0].from_classification, "private");
    }

    #[test]
    fn the_weak_control_is_granted_only_to_a_chat_that_merely_ran_a_turn() {
        assert!(!requires_typed_confirmation(Some("turn:versa_azure")));
        assert!(requires_typed_confirmation(Some("mcp:ucsfomopagent")));
        // The cases a match-on-`mcp:` rule hands the weak control to. A branch
        // of an OMOP session is the one that matters: it carries the SAME
        // conversation and none of the `mcp:` spelling.
        assert!(requires_typed_confirmation(Some(
            "diverged:20260101_120000"
        )));
        assert!(requires_typed_confirmation(Some(
            "inherited:20260101_120000"
        )));
        assert!(requires_typed_confirmation(Some("imported")));
        assert!(requires_typed_confirmation(Some("backfill:versa_azure")));
        // Absent, and anything a future task adds to the vocabulary.
        assert!(requires_typed_confirmation(None));
        assert!(requires_typed_confirmation(Some("")));
        assert!(requires_typed_confirmation(Some("something_new")));
        // Not a prefix match on a longer word: `turned:` is not `turn:`.
        assert!(requires_typed_confirmation(Some("turned_private")));
    }

    /// The whole audit surface for "can the ratchet be reversed".
    ///
    /// The plan's shell gate greps for any comparison of `privacy_tier` against
    /// the public literal and expects one hit. That spelling is not the property:
    /// this tree already contained FOUR such occurrences before a line of this
    /// task existed — Gate A's `WHERE` clause in `bind_provider_if_allowed` and
    /// chat recall's two visibility filters, all of them READS — so a count over
    /// them says nothing about writes and the gate's stated expectation cannot
    /// hold. What is pinned here instead is the set of files containing an
    /// ASSIGNMENT (a SQL `SET` of that column to that literal), which must be
    /// exactly this one.
    ///
    /// The needle is composed at runtime rather than written out, so this file
    /// does not match its own audit — see below.
    #[test]
    fn exactly_one_statement_in_the_tree_assigns_a_public_classification() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let crates = root.join("crates");
        assert!(
            crates.is_dir(),
            "the audit walks {} — if that path is wrong every assertion below passes \
             for the wrong reason",
            crates.display()
        );

        // Composed rather than written out, so this file does not match its own
        // audit. Spelling the needle literally would give `declassify.rs` two
        // hits — the statement and the scanner — and "2" is then indistinguishable
        // from a real second bypass landing here.
        let needle = format!("SET privacy_tier = '{}'", SessionClassification::PUBLIC_SQL);
        let mut assignments: std::collections::BTreeMap<String, usize> = Default::default();
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
            let src = std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
            for line in src.lines() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                let hits = code.matches(needle.as_str()).count();
                if hits > 0 {
                    *assignments.entry(rel.clone()).or_default() += hits;
                }
            }
        }
        assert!(
            scanned >= 400,
            "only {scanned} .rs files were scanned. A broken walk reports the same empty \
             set as a clean tree."
        );
        let found: Vec<(String, usize)> = assignments.into_iter().collect();
        assert_eq!(
            found,
            vec![("crates/biorouter/src/privacy/declassify.rs".to_string(), 1)],
            "the set of statements that lower a session's classification changed. Every \
             other write goes through `SessionUpdateBuilder`, whose monotone `CASE WHEN` \
             physically cannot lower the tier; a second bypass is a design change."
        );
    }

    /// The proof-of-user is a cross-crate `pub` constructor, because the only
    /// caller lives in `biorouter-server` and `pub(in …)` cannot cross a crate
    /// boundary. Rust therefore cannot restrict it to one call site on its own —
    /// this test is what does, by pinning the set of files that so much as name
    /// the type.
    #[test]
    fn the_proof_of_user_is_constructed_in_exactly_one_place() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let mut naming: Vec<String> = vec![];
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
            if rel == "crates/biorouter/src/privacy/declassify.rs" {
                continue;
            }
            let src = std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("the audit could not read {rel}: {e}"));
            if src.contains("UserConfirmation") {
                naming.push(rel);
            }
        }
        assert!(scanned >= 400, "only {scanned} .rs files were scanned");
        naming.sort();
        assert_eq!(
            naming,
            vec!["crates/biorouter-server/src/routes/session.rs".to_string()],
            "a second construction site for the proof-of-user appeared. The whole claim \
             that an agent cannot declassify a chat rests on this set having one member, \
             and that member being a route behind the user-action header."
        );
    }
}
