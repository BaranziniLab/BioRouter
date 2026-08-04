//! Issue #56 Task 58 / [#47]: how far a caller reaches into a session it merely
//! *named*.
//!
//! ⚠ **`session_id` is a request parameter, not a credential.** Authorization is
//! one daemon-wide shared secret and the daemon has no principal, so no
//! session-scoped route can tell whether the caller has any relationship to the
//! session it addresses. AR-11 measured that secret to be recoverable by the
//! agent, so a public-tier agent could recover it and then read a private chat's
//! transcript, or run a turn inside it — defeating every tier gate without
//! touching one.
//!
//! ⚠ **This module does NOT fix [#47], and nothing here should be read as
//! claiming it does.** #47 says in its own words that this is *"a property of
//! the daemon's API surface as a whole, not of one endpoint"*, and the general
//! problem is that the daemon has no principal — an authorization redesign, not
//! a release fix. What this closes is the **privacy slice**: a session-addressing
//! route that reaches a **private** session must carry the user-action proof.
//! #47 stays open with its residual narrowed to, and stated so it cannot be
//! read as smaller than it is:
//!
//! * a caller holding the daemon secret still reaches every **public** session
//!   it can name — that is not a privacy boundary and this gate is deliberately
//!   inert there;
//! * it still reaches every session-addressing route NOT on
//!   [the gated list](self#the-gated-list). `POST /interrupt` (inject text into
//!   the turn running in a named chat), `POST /agent/cancel`, `POST
//!   /agent/resume`, `GET /sessions/{id}/extensions`, `PUT /sessions/{id}/name`
//!   and `DELETE /sessions/{id}` are all still open. The list is the five the
//!   ruling named, and `/reply` dominates them, but "dominates" is an argument
//!   about capability rather than a proof about every route;
//! * and the daemon still has no principal, which is the actual subject of #47.
//!
//! # The gated list
//!
//! | Route | Why it is on the list |
//! |---|---|
//! | `POST /reply` | Runs an agent turn, with tools, in the named session. It **strictly dominates** the rest: a caller who can run a turn in a session can already do anything that session can do. |
//! | `GET /sessions/{session_id}` | Returns the transcript. |
//! | `POST /agent/update_working_dir` | Repoints the session at a directory of the caller's choosing and restarts its agent. |
//! | `POST /agent/add_extension` | Attaches tools to the session. |
//! | `POST /knowledge/active` | Repoints the session's knowledge bases and its write target. |
//!
//! # Why `X-User-Action` and not a new mechanism
//!
//! The instrument already exists ([`biorouter_server::auth::user_action_proof`],
//! Task 18A) and it is the right one here for a reason worth writing down: the
//! daemon holds only the **digest**, while the key lives in the Electron main
//! process and is never in the daemon's environment. So the very recoverability
//! AR-11 measured does **not** hand an agent this proof. That asymmetry is the
//! whole reason the mechanism works, and it must not be undone by caching the
//! key daemon-side for convenience.
//!
//! [#47]: https://github.com/BaranziniLab/biorouter/issues/47

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use biorouter::privacy::SessionClassification;
use biorouter::session::session_manager::SessionManager;
// Issue #56 DR-16. `src/routes/` is compiled into the `biorouterd` binary as
// well as the lib and cannot name `crate::auth`, so this is the shared
// direction — the same import `routes::session` and `routes::knowledge` use.
use biorouter_server::auth::{user_action_proof, UserActionProof};

/// What an unproven caller is told when it names a session it may not reach.
///
/// ⚠ **ONE sentence for "that chat is private" and for "there is no such
/// chat", deliberately.** An unproven caller must not learn whether a session
/// exists, or anything about it, from the shape of the refusal — otherwise the
/// refusal itself becomes an oracle that enumerates the machine's private chats
/// one id at a time. §14.4's content rule holds: it names the boundary and
/// nothing about the chat — no id, no title, no working directory, no tier.
///
/// It forecloses the retry for the reason every refusal in this feature does: a
/// model that reads a refusal as transient loops on it.
pub const SESSION_OUT_OF_REACH: &str =
    "That chat is private, or there is no chat with that id — this request carried no proof it \
     came from the person at the keyboard, and the two answers are deliberately the same so that \
     nothing about the chat is disclosed. Nothing was read and nothing was changed. Do not retry; \
     the same call will be refused again, and no setting, hook or permission mode changes it. If \
     this task genuinely needs that chat, stop and ask the user to open it for you.";

/// …and when this daemon was handed no user-action key at all.
///
/// A separate sentence, per Task 18A's open question 23: reporting "this daemon
/// cannot verify a human" as "you are not a human" sends the person at the
/// keyboard hunting for a permission they can never obtain. `just run-server`, a
/// hand-run `biorouterd agent` and every headless deployment land here, and
/// private sessions are unreachable over HTTP on such a daemon. That is the
/// fail-closed direction open question 23 already accepted, and it must not be
/// softened with an env-var escape — the daemon's environment is exactly what
/// AR-11 measured to be recoverable.
///
/// ⚠ It says nothing about the named chat either, so it is no more of an oracle
/// than [`SESSION_OUT_OF_REACH`]: it separates *credential states of the
/// caller*, which the caller already knows, never *states of the session*.
pub const SESSION_REACH_NO_KEY: &str =
    "This daemon was started without a user-action key, so it cannot verify that a request came \
     from the person at the keyboard — and reaching into a private chat requires that proof. \
     Nothing was read and nothing was changed. This control is unavailable on this daemon; use \
     the desktop app.";

/// The named session, reduced to the one bit this gate turns on.
///
/// Three states rather than two because the third has to be *represented* in
/// order to be provably answered the same way as `Private`; folding it in at the
/// type level would make the indistinguishability a definition rather than a
/// tested claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetTier {
    /// The row was read, and it is public.
    Public,
    /// The row was read, and it is private.
    Private,
    /// The row could not be read: no such session, a recycled id, a store
    /// error. **Answered exactly as `Private` is.**
    Unreadable,
}

/// A refusal from [`refuse_unless_reachable`].
///
/// Carries a `&'static str` rather than a `String` so the two constants above
/// are the only two things it can ever say — which is what makes "these two
/// inputs produce the identical response" checkable by equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOutOfReach {
    pub status: StatusCode,
    pub message: &'static str,
}

impl IntoResponse for SessionOutOfReach {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

impl From<SessionOutOfReach> for super::errors::ErrorResponse {
    fn from(refusal: SessionOutOfReach) -> Self {
        Self {
            status: refusal.status,
            message: refusal.message.to_string(),
        }
    }
}

/// May a caller in this credential state reach a session in this state?
///
/// ⚠ **Extracted so the claim is asserted rather than grepped for.** None of the
/// five gated handlers can be driven from a unit test cheaply — `AppState::new()`
/// opens the developer's REAL session database — so a scan for
/// `session_reach(` would keep passing against a call whose result was
/// discarded. This mapping is pure, so every corner of it is driven for real by
/// [`tests`], and the one thing a pure function cannot see — that the gate runs
/// before the route touches anything — stays a source scan and says so.
///
/// `enforced` is DR-15's master opt-out, taken as an argument rather than read
/// here so that "the switch is off" is a corner this function can be tested at.
/// With tiers off the gate is entirely inert — including for `Unreadable`, so a
/// user who opted out still gets their 404 rather than a 403 for a chat that
/// simply is not there.
pub fn refuse_unless_reachable(
    enforced: bool,
    tier: TargetTier,
    proof: UserActionProof,
) -> Result<(), SessionOutOfReach> {
    if !enforced {
        return Ok(());
    }
    match tier {
        // A public chat is reachable by anything holding the daemon secret,
        // exactly as it was before this gate existed. A barrier that fired on
        // every chat is one people route around, and it would break every
        // client that has never sent this header.
        TargetTier::Public => Ok(()),
        TargetTier::Private | TargetTier::Unreadable => match proof {
            UserActionProof::Proven => Ok(()),
            UserActionProof::Unproven => Err(SessionOutOfReach {
                status: StatusCode::FORBIDDEN,
                message: SESSION_OUT_OF_REACH,
            }),
            UserActionProof::NoKeyInstalled => Err(SessionOutOfReach {
                status: StatusCode::FORBIDDEN,
                message: SESSION_REACH_NO_KEY,
            }),
        },
    }
}

/// Read the named session's tier, failing closed on anything that is not a
/// readable public row.
///
/// Metadata only (`with_messages: false`): resolving the tier must not be a way
/// to load the very transcript the gate is about to refuse.
pub async fn target_tier(manager: &SessionManager, session_id: &str) -> TargetTier {
    match manager.get_session(session_id, false).await {
        Ok(session) if session.privacy_tier == SessionClassification::Private => {
            TargetTier::Private
        }
        Ok(_) => TargetTier::Public,
        Err(_) => TargetTier::Unreadable,
    }
}

/// The whole gate, as one call: **resolve the target session's tier first, and
/// require the user-action proof when that tier is Private.**
///
/// ⚠ **Call this before the handler does anything else.** Task 49's grant route
/// establishes the ordering and says why; this is the same rule with a session
/// as its subject. A handler that fetched the agent, or validated the request,
/// or took the turn lock first would hand an unproven caller a side channel —
/// "this chat is busy", "this extension is not enabled here", "no such session"
/// — that discloses exactly what the refusal is worded to withhold.
pub async fn session_reach(
    manager: &SessionManager,
    session_id: &str,
    headers: &HeaderMap,
) -> Result<(), SessionOutOfReach> {
    // DR-15's master opt-out, read INSIDE the gate. A direct read, not a
    // `CallCapability`: an HTTP request naming a session is not a tool call and
    // has no admitted capability to inherit.
    let enforced = biorouter::privacy::privacy_tiers_enabled();
    // Short-circuit BEFORE the store read, so the opt-out costs nothing per
    // request rather than a database round trip per request.
    if !enforced {
        return Ok(());
    }
    refuse_unless_reachable(
        enforced,
        target_tier(manager, session_id).await,
        user_action_proof(headers),
    )
}

/// `POST /knowledge/active` — the one gated route whose router does not have an
/// [`AppState`](crate::state::AppState) to resolve a tier with.
///
/// ⚠ **A middleware rather than a line in the handler, and that is a plumbing
/// constraint rather than a design preference.** `knowledge::router` is
/// deliberately state-typed on `Arc<KnowledgeService>` so it can be tested
/// without constructing an `AppState` (and `crates/biorouter-server/tests/
/// knowledge_routes.rs` does exactly that — 46 tests on this branch, measured,
/// not carried over). Widening that state would touch every one of its ~35
/// handlers and every one of those tests, to gate a single route. So the gate is
/// layered onto the nested router instead, where it is the first thing the
/// request meets.
///
/// It buffers the body ONLY for the route it gates. Everything else under
/// `/knowledge` — including 25 MB multipart ingests — passes through untouched.
pub async fn gate_knowledge_active(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    // `nest` strips the prefix before the inner router sees the request, and a
    // layer added to that inner router runs inside it — so the path here is
    // `/active`. The prefixed spelling is accepted too, so that moving this
    // layer to the outer router (or a change in how `nest` rewrites the URI)
    // cannot silently turn a security gate into a no-op. Which spelling arrives
    // is pinned by `the_knowledge_active_gate_is_actually_wired`.
    let path = request.uri().path();
    let gated = request.method() == axum::http::Method::POST
        && (path == "/active" || path == "/knowledge/active");
    if !gated {
        return next.run(request).await;
    }

    let (parts, body) = request.into_parts();
    // A selection edit is a handful of ids. The cap is not a policy, it is a
    // refusal to buffer something unbounded in a middleware.
    let Ok(bytes) = axum::body::to_bytes(body, 1024 * 1024).await else {
        return (StatusCode::BAD_REQUEST, "request body too large").into_response();
    };
    // A body this does not understand is passed on unchanged: the handler owns
    // the 400, and answering it here would make the gate a second, divergent
    // parser of the same request.
    if let Some(session_id) = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|body| {
            body.get("session_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
    {
        if let Err(refusal) =
            session_reach(state.session_manager(), &session_id, &parts.headers).await
        {
            return refusal.into_response();
        }
    }

    next.run(axum::extract::Request::from_parts(
        parts,
        axum::body::Body::from(bytes),
    ))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::body_of;

    const PROOFS: [UserActionProof; 3] = [
        UserActionProof::Proven,
        UserActionProof::Unproven,
        UserActionProof::NoKeyInstalled,
    ];

    /// The whole rule, at every corner, in the direction that matters: a private
    /// target is out of reach unless the request proves it came from the person
    /// at the keyboard.
    #[test]
    fn a_private_target_is_out_of_reach_without_the_users_proof() {
        assert!(
            refuse_unless_reachable(true, TargetTier::Private, UserActionProof::Proven).is_ok()
        );
        for proof in [UserActionProof::Unproven, UserActionProof::NoKeyInstalled] {
            assert!(
                refuse_unless_reachable(true, TargetTier::Private, proof).is_err(),
                "a caller holding nothing but the daemon secret reached a private chat ({proof:?})"
            );
        }
    }

    /// …and a public target is completely unaffected, for every caller.
    ///
    /// This is the half a gate written only for the refusal loses. A barrier
    /// that fired on every chat would break every client that has never sent
    /// this header — which is all of them, for every public chat — and DR-16's
    /// posture is a CONDITION, not a wall in front of the user.
    #[test]
    fn a_public_target_is_reachable_by_every_caller() {
        for proof in PROOFS {
            assert!(
                refuse_unless_reachable(true, TargetTier::Public, proof).is_ok(),
                "the gate is a wall in front of the user, not a condition ({proof:?})"
            );
        }
    }

    /// Issue #56 Task 58, Step 4.3. An unproven caller cannot distinguish "no
    /// such session" from "private session" **from the response**.
    ///
    /// Byte-for-byte on both the status and the message, because either half
    /// alone is an oracle: a 403 and a 404 enumerate the machine's private chats
    /// just as well as two different sentences do.
    #[test]
    fn no_such_session_and_a_private_session_are_the_same_refusal() {
        for proof in PROOFS {
            assert_eq!(
                refuse_unless_reachable(true, TargetTier::Unreadable, proof),
                refuse_unless_reachable(true, TargetTier::Private, proof),
                "the refusal tells an unproven caller whether the chat exists ({proof:?})"
            );
        }
    }

    /// Open question 23. A daemon that was handed no user-action key refuses
    /// rather than allows — including the person at the keyboard — and says so
    /// in different words, because reporting "this daemon cannot verify a human"
    /// as "you are not a human" sends them hunting for a permission they can
    /// never obtain.
    #[test]
    fn a_keyless_daemon_refuses_rather_than_allows_and_says_which() {
        let keyless =
            refuse_unless_reachable(true, TargetTier::Private, UserActionProof::NoKeyInstalled)
                .expect_err("a keyless daemon must refuse");
        let unproven =
            refuse_unless_reachable(true, TargetTier::Private, UserActionProof::Unproven)
                .expect_err("an unproven caller must be refused");
        assert_eq!(keyless.message, SESSION_REACH_NO_KEY);
        assert_eq!(unproven.message, SESSION_OUT_OF_REACH);
        assert_ne!(
            keyless.message, unproven.message,
            "a daemon with no user-action key must not be told it is the model"
        );
        // …and neither of them is a way to learn the tier: both say the same
        // thing whether the chat is private or absent, which is the assertion
        // above, and both are 403.
        assert_eq!(keyless.status, StatusCode::FORBIDDEN);
        assert_eq!(unproven.status, StatusCode::FORBIDDEN);
    }

    /// DR-15's master opt-out turns the whole gate off — including for an
    /// `Unreadable` target, so a user who opted out still gets their 404 for a
    /// chat that simply is not there rather than a 403 for one that is.
    #[test]
    fn the_master_switch_turns_the_whole_gate_off() {
        for tier in [
            TargetTier::Public,
            TargetTier::Private,
            TargetTier::Unreadable,
        ] {
            for proof in PROOFS {
                assert!(
                    refuse_unless_reachable(false, tier, proof).is_ok(),
                    "the master opt-out did not reach this gate ({tier:?}, {proof:?})"
                );
            }
        }
    }

    /// §14.4's content rule, and the marker rule beside it.
    ///
    /// The refusal must name the boundary and nothing about the chat, and it
    /// must not claim to be one of the other two user-proof refusals: their
    /// toasts say *switch this chat's model* and *branch it from the chat
    /// window*, and both would send the user somewhere that cannot help.
    #[test]
    fn the_refusal_names_the_boundary_and_nothing_about_the_chat() {
        for message in [SESSION_OUT_OF_REACH, SESSION_REACH_NO_KEY] {
            assert!(
                !message.contains(biorouter::privacy::refusal::USER_ACTION_REFUSAL_MARKER),
                "this refusal is claiming to be the model picker's: {message}"
            );
            assert!(
                !message.contains(crate::routes::session::COPY_OF_PRIVATE_REFUSAL_MARKER),
                "this refusal is claiming to be the branch refusal's: {message}"
            );
        }
        // The retry is foreclosed in the arm a model actually reaches. The
        // keyless arm is the human's, and tells them where to go instead.
        assert!(SESSION_OUT_OF_REACH.contains("Do not retry"));
    }

    /// Issue #56 Task 58, Step 3: **resolve the tier before doing anything
    /// else.** The half [`refuse_unless_reachable`]'s own tests cannot see.
    ///
    /// Ordering, not merely presence: a gate placed after the turn lock, after
    /// the agent fetch or after the transcript read hands an unproven caller the
    /// side channel the refusal is worded to withhold. Each row names the FIRST
    /// thing its handler does that touches the session, and the assertion is
    /// that the gate is earlier in the body than that.
    ///
    /// A source scan because none of these handlers can be driven cheaply from a
    /// unit test — `AppState::new()` opens the developer's REAL session
    /// database. The two that can be are driven, over HTTP, by
    /// [`super::bypass_tests`]; this is what holds the other three, and the
    /// ordering of all five.
    #[test]
    fn every_gated_route_resolves_the_tier_before_it_touches_the_session() {
        let session_rs = include_str!("session.rs");
        let reply_rs = include_str!("reply.rs");
        let agent_rs = include_str!("agent.rs");
        for (src, func, first_touch, what) in [
            (
                reply_rs,
                "pub async fn reply",
                "try_begin_turn_idempotent(",
                "the turn lock, whose 409 says whether this chat is busy",
            ),
            (
                session_rs,
                "async fn get_session(",
                "get_session(&session_id, true)",
                "the transcript read",
            ),
            (
                agent_rs,
                "async fn agent_add_extension",
                "get_agent(",
                "the agent fetch, which creates one if absent",
            ),
            (
                agent_rs,
                "async fn update_working_dir",
                "try_begin_turn_idempotent(",
                "the turn lock, whose 409 says whether this chat is busy",
            ),
        ] {
            let handler = body_of(src, func);
            let gate = handler.find("session_reach(").unwrap_or_else(|| {
                panic!("{func} does not consult the session-reach gate (`session_reach(`)")
            });
            let touch = handler
                .find(first_touch)
                .unwrap_or_else(|| panic!("`{first_touch}` is no longer in {func}"));
            assert!(
                gate < touch,
                "{func} reaches {what} before it resolves the target session's tier"
            );
        }

        // The negative controls, so the scan is provably not vacuous: a handler
        // in each file that is NOT on the gated list must come back without the
        // gate, or `body_of` is over-reading past a function end and every
        // assertion above is passing on someone else's body.
        //
        // BOTH sides in `agent.rs`: `agent_remove_extension` sits after the two
        // gated handlers' neighbourhood and `update_agent_provider` before it,
        // and a control on one side only passes against an extractor that
        // over-reads towards the other.
        for (src, control) in [
            (reply_rs, "pub async fn interrupt"),
            (session_rs, "async fn get_session_extensions"),
            (agent_rs, "async fn agent_remove_extension"),
            (agent_rs, "async fn update_agent_provider"),
        ] {
            assert!(
                !body_of(src, control).contains("session_reach("),
                "the body scan is over-reading: {control} is not on the gated list and \
                 reported the gate"
            );
        }
    }

    /// The knowledge route's gate is a middleware, so the scan above cannot see
    /// it — but the wiring can still be lost in a refactor of `configure`, and a
    /// layer that is never applied is a security control that silently does
    /// nothing.
    ///
    /// `the_knowledge_active_gate_is_actually_wired` is what proves it FIRES;
    /// this is the cheap tripwire that survives a move of that test.
    #[test]
    fn the_knowledge_router_carries_the_reach_gate() {
        let mod_rs = include_str!("mod.rs");
        let configure = body_of(mod_rs, "pub fn configure");
        assert!(
            configure.contains("session_reach::gate_knowledge_active"),
            "the knowledge router no longer carries the session-reach gate"
        );
    }
}

#[cfg(test)]
mod bypass_tests {
    use super::*;
    use crate::routes::session::diverge_tests::{
        install_test_user_action_key, TEST_USER_ACTION_KEY,
    };
    use crate::state::AppState;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use biorouter::conversation::message::Message;
    use biorouter::model::ModelConfig;
    use biorouter::session::SessionType;
    use serial_test::serial;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tower::ServiceExt;

    /// A string that appears in the seeded chat and nowhere else, so "the
    /// transcript came back" is an assertion rather than an impression.
    ///
    /// ⚠ **Unmistakably a fixture, and deliberately not shaped like a record.**
    /// These tests seed into the developer's REAL session database —
    /// `AppState::new()` opens it — so a row that ever escapes [`SeededChat`]'s
    /// cleanup lands in their own sidebar. A marker that read like a patient
    /// identifier would then be a privacy incident invented by the test suite of
    /// the privacy feature.
    const MARKER_IN_THE_TRANSCRIPT: &str = "task58-transcript-marker-not-real-data";

    async fn get_session_with(
        state: Arc<AppState>,
        session_id: &str,
        user_action: Option<&str>,
    ) -> (StatusCode, String) {
        let app = crate::routes::session::routes(state);
        let mut builder = Request::builder()
            .method("GET")
            .uri(format!("/sessions/{session_id}"));
        if let Some(key) = user_action {
            builder = builder.header("X-User-Action", key);
        }
        let res = app
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// `POST /reply`.
    ///
    /// ⚠ The caller MUST already hold this session's turn lock. A `/reply` that
    /// gets past every check spawns a real agent turn against the developer's
    /// real configuration — which on this machine means real provider
    /// credentials in the Keychain. Holding the lock makes "the gate let it
    /// through" observable as a 409 from the turn lock, with nothing spawned and
    /// no streaming body to drain.
    async fn post_reply_with(
        state: Arc<AppState>,
        session_id: &str,
        user_action: Option<&str>,
    ) -> (StatusCode, String) {
        let app = crate::routes::reply::routes(state);
        let mut builder = Request::builder()
            .method("POST")
            .uri("/reply")
            .header("content-type", "application/json");
        if let Some(key) = user_action {
            builder = builder.header("X-User-Action", key);
        }
        let body = serde_json::json!({
            "session_id": session_id,
            "user_message": Message::user().with_text("summarise this chat"),
        });
        let res = app
            .oneshot(
                builder
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// `POST /agent/update_working_dir`.
    ///
    /// ⚠ Like [`post_reply_with`], the caller MUST already hold this session's
    /// turn lock: the first thing this handler does after the gate is claim that
    /// lock, and a request that gets past both repoints a real chat at a
    /// directory and RESTARTS its agent there. Holding the lock makes "the gate
    /// let it through" observable as a 409, with nothing moved.
    async fn post_update_working_dir(
        state: Arc<AppState>,
        session_id: &str,
        user_action: Option<&str>,
    ) -> (StatusCode, String) {
        post_json(
            crate::routes::agent::routes(state),
            "/agent/update_working_dir",
            serde_json::json!({
                "session_id": session_id,
                "working_dir": "/tmp/task58_session_reach",
            }),
            user_action,
        )
        .await
    }

    /// `POST /agent/add_extension`.
    ///
    /// The command names nothing that exists, so an accepted request has no MCP
    /// server to spawn — but it would still reach `get_agent`, which MINTS an
    /// agent from the developer's own configuration. That is why only the
    /// refusing arm of this route is driven, and it is refused before that call.
    async fn post_add_extension(
        state: Arc<AppState>,
        session_id: &str,
        user_action: Option<&str>,
    ) -> (StatusCode, String) {
        post_json(
            crate::routes::agent::routes(state),
            "/agent/add_extension",
            serde_json::json!({
                "session_id": session_id,
                "config": {
                    "type": "stdio",
                    "name": "task58-probe",
                    "description": "",
                    "cmd": "/nonexistent/task58",
                    "args": [],
                    "timeout": null,
                },
            }),
            user_action,
        )
        .await
    }

    /// One JSON POST at `router`, with the proof-of-user header when supplied.
    async fn post_json(
        router: axum::Router,
        uri: &str,
        body: serde_json::Value,
        user_action: Option<&str>,
    ) -> (StatusCode, String) {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(key) = user_action {
            builder = builder.header("X-User-Action", key);
        }
        let res = router
            .oneshot(
                builder
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn post_knowledge_active(
        state: Arc<AppState>,
        body: serde_json::Value,
        user_action: Option<&str>,
    ) -> (StatusCode, String) {
        let app = crate::routes::configure(state, "task-58-secret".to_string());
        let mut builder = Request::builder()
            .method("POST")
            .uri("/knowledge/active")
            .header("content-type", "application/json");
        if let Some(key) = user_action {
            builder = builder.header("X-User-Action", key);
        }
        let res = app
            .oneshot(
                builder
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// A chat seeded by this module, which deletes itself when the test's scope
    /// ends — **including on a panic**, which a `delete_session` at the end of
    /// the test body cannot do.
    ///
    /// ⚠ The store here is the developer's REAL session database, so a row that
    /// outlives a failing assertion is a chat in their own sidebar, forever, with
    /// no obvious provenance. `block_in_place` is what lets an async delete run
    /// from `Drop`, and it is only legal on a multi-threaded runtime — so
    /// `#[tokio::test(flavor = "multi_thread")]` on every test below is a
    /// requirement of this type, not a habit.
    struct SeededChat {
        state: Arc<AppState>,
        id: String,
    }

    impl SeededChat {
        fn id(&self) -> &str {
            &self.id
        }
    }

    impl Drop for SeededChat {
        fn drop(&mut self) {
            let state = self.state.clone();
            let id = std::mem::take(&mut self.id);
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    // Reported, never panicked: on the unwind path a second panic
                    // aborts the process and takes the real failure's message with
                    // it, which would turn a legible assertion into a bare abort.
                    if let Err(e) = state.session_manager().delete_session(&id).await {
                        eprintln!("task 58: could not clean up the seeded chat {id}: {e}");
                    }
                })
            });
        }
    }

    /// One chat at `tier` with a marker message in it. A private one is raised
    /// the way a real one gets there — by binding a private provider — rather
    /// than by writing the column, so what these tests refuse is the same state
    /// a user's own chat reaches. The returned guard deletes it.
    async fn seed_chat(
        state: &Arc<AppState>,
        label: &str,
        tier: SessionClassification,
    ) -> SeededChat {
        let manager = state.session_manager();
        let session = manager
            .create_session(
                PathBuf::from("/tmp/task58_session_reach"),
                label.to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        manager
            .add_message(
                &session.id,
                &Message::user().with_text(MARKER_IN_THE_TRANSCRIPT),
            )
            .await
            .unwrap();
        if tier == SessionClassification::Private {
            manager
                .update(&session.id)
                .provider_name("versa_azure")
                .model_config(ModelConfig::new("gpt-4o").unwrap())
                .raise_privacy(SessionClassification::Private, "turn:versa_azure")
                .apply()
                .await
                .unwrap();
        }
        SeededChat {
            state: state.clone(),
            id: session.id,
        }
    }

    async fn seed_private_chat(state: &Arc<AppState>, label: &str) -> SeededChat {
        seed_chat(state, label, SessionClassification::Private).await
    }

    /// Issue #56 Task 58 / #47. **The bypass itself, as a named regression
    /// test.** This is the test that would have caught the hole, and it is the
    /// one that must never be deleted.
    ///
    /// Hold nothing but the daemon secret — which AR-11 measured the agent can
    /// recover — name a private session, and try the two things that dominate
    /// every other session-addressing route: read its transcript, and run a turn
    /// in it. A caller who can run a turn in a session can already do anything
    /// that session can do.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn holding_the_secret_you_cannot_read_a_private_transcript_or_run_a_turn_in_it() {
        install_test_user_action_key();
        let state = AppState::new().await.unwrap();
        let private = seed_private_chat(&state, "Task 58 private (test fixture)").await;
        let private_id = private.id();

        // 1. READ. A caller holding only the daemon secret must not get the
        //    transcript.
        let (status, body) = get_session_with(state.clone(), private_id, None).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a caller holding only the daemon secret read a private transcript"
        );
        assert!(
            !body.contains(MARKER_IN_THE_TRANSCRIPT),
            "the refusal carried the private conversation in its body"
        );

        // 2. …and the person at the keyboard still can.
        let (status, body) =
            get_session_with(state.clone(), private_id, Some(TEST_USER_ACTION_KEY)).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the user cannot read their own chat"
        );
        assert!(
            body.contains(MARKER_IN_THE_TRANSCRIPT),
            "the user's own read did not return the conversation"
        );

        // 3. RUN A TURN. `/reply` dominates every other route on the list.
        //    The turn lock is held for the whole exchange so that a request the
        //    gate lets through stops at a 409 instead of spawning a real turn
        //    against real provider credentials.
        let turn_guard = state
            .try_begin_turn_idempotent(
                private_id,
                tokio_util::sync::CancellationToken::new(),
                None,
            )
            .expect("no turn is running in a session created a moment ago");

        let (status, _) = post_reply_with(state.clone(), private_id, None).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a caller holding only the daemon secret ran an agent turn, with tools, in a \
             private chat"
        );

        let (status, _) = post_reply_with(state.clone(), private_id, Some(TEST_USER_ACTION_KEY))
            .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "the user's own turn was refused; 409 here is the turn lock this test holds, \
             i.e. the request got past the barrier as it should"
        );

        drop(turn_guard);
    }

    /// Issue #56 Task 58, Step 4.1: **a public target is unaffected.**
    ///
    /// The half a gate written only for its refusal never drives, and the
    /// expensive one to get wrong. [`super::target_tier`] resolves a real row out
    /// of the real store, so if it ever mis-read a valid public session — a
    /// column renamed, a default that flips, a transient store error swallowed
    /// into `Unreadable` — then EVERY header-less caller would be refused on
    /// EVERY public chat, and every other assertion in this module would still
    /// pass. [`super::tests::a_public_target_is_reachable_by_every_caller`] pins
    /// the decision; this pins that a real public row on this machine arrives at
    /// that decision as `Public`.
    ///
    /// Each route is driven to a status only its own body can produce, so
    /// "not a 403" is nowhere the assertion:
    ///
    /// * `GET /sessions/{id}` returns the transcript, and the marker is in it;
    /// * `POST /reply` and `POST /agent/update_working_dir` each stop at the turn
    ///   lock this test holds — the first thing each does after the gate — so
    ///   their 409 is a status the gate cannot produce. Holding it is also what
    ///   keeps a request that got through from spawning a real turn against real
    ///   provider credentials, or repointing a chat and restarting its agent.
    ///
    /// ⚠ `POST /agent/add_extension` is deliberately NOT driven on this arm, and
    /// that is a cost of the route rather than an omission: passing its gate
    /// means `get_agent` mints a real agent from the developer's own
    /// configuration and the handler then attaches an extension to it. Its
    /// refusing arm is driven by
    /// [`the_other_two_gated_routes_refuse_a_private_chat_over_http`], and there
    /// is nothing between its gate and its handler that the other three do not
    /// also have — it is the same `session_reach(` call, with `Public` returning
    /// `Ok(())` unconditionally.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn a_public_chat_is_reachable_over_http_by_a_caller_that_proves_nothing() {
        install_test_user_action_key();
        let state = AppState::new().await.unwrap();
        let public = seed_chat(
            &state,
            "Task 58 public (test fixture)",
            SessionClassification::Public,
        )
        .await;

        // READ. The whole transcript, to a caller carrying nothing but the
        // daemon secret — which is exactly the request the gate must not touch.
        let (status, body) = get_session_with(state.clone(), public.id(), None).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the gate is a wall in front of the user: it refused a PUBLIC chat ({body})"
        );
        assert!(
            body.contains(MARKER_IN_THE_TRANSCRIPT),
            "a public read came back without the conversation: {body}"
        );

        let turn_guard = state
            .try_begin_turn_idempotent(
                public.id(),
                tokio_util::sync::CancellationToken::new(),
                None,
            )
            .expect("no turn is running in a session created a moment ago");

        let (status, body) = post_reply_with(state.clone(), public.id(), None).await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "/reply refused an unproven caller on a PUBLIC chat; 409 here is the turn lock \
             this test holds, i.e. the request reached the handler as it should ({body})"
        );

        let (status, body) = post_update_working_dir(state.clone(), public.id(), None).await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "/agent/update_working_dir refused an unproven caller on a PUBLIC chat; 409 here \
             is the turn lock this test holds ({body})"
        );

        drop(turn_guard);
    }

    /// The two routes on the gated list that
    /// [`holding_the_secret_you_cannot_read_a_private_transcript_or_run_a_turn_in_it`]
    /// does not drive, on their refusing arm, over HTTP.
    ///
    /// Neither is additional capability — `/reply` dominates both, and it is
    /// already covered. They are here because "the gate is on this route" is
    /// otherwise only [`super::tests::every_gated_route_resolves_the_tier_before_it_touches_the_session`],
    /// and a source scan cannot see a `session_reach(` call whose result was
    /// discarded.
    ///
    /// The refusal is asserted by its full text, not by its status, because these
    /// two return it through `ErrorResponse` — a JSON envelope — while
    /// `GET /sessions/{id}` returns it as plain text. One sentence has to survive
    /// both shapes, or a client cannot recognise the boundary it hit.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn the_other_two_gated_routes_refuse_a_private_chat_over_http() {
        install_test_user_action_key();
        let state = AppState::new().await.unwrap();
        let private = seed_private_chat(&state, "Task 58 routes (test fixture)").await;

        // Held for the same reason as everywhere else here: if the gate ever
        // stopped firing, this request would repoint a real chat and restart its
        // agent rather than merely returning the wrong status.
        let turn_guard = state
            .try_begin_turn_idempotent(
                private.id(),
                tokio_util::sync::CancellationToken::new(),
                None,
            )
            .expect("no turn is running in a session created a moment ago");

        let (status, body) = post_update_working_dir(state.clone(), private.id(), None).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a caller holding only the daemon secret repointed a private chat at a directory \
             of its choosing and restarted its agent there ({body})"
        );
        assert!(
            body.contains(SESSION_OUT_OF_REACH),
            "the refusal did not survive the JSON envelope: {body}"
        );

        drop(turn_guard);

        let (status, body) = post_add_extension(state.clone(), private.id(), None).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a caller holding only the daemon secret attached tools to a private chat ({body})"
        );
        assert!(
            body.contains(SESSION_OUT_OF_REACH),
            "the refusal did not survive the JSON envelope: {body}"
        );
    }

    /// Issue #56 Task 58, Step 4.3, over HTTP: the refusal is not an oracle.
    ///
    /// [`super::tests::no_such_session_and_a_private_session_are_the_same_refusal`]
    /// pins the decision; this pins that the ROUTE does not add a distinguisher
    /// of its own on the way out — a different status, a different body, a
    /// validation answer that only a real id could produce.
    ///
    /// And the other direction, which is the part that keeps this from being
    /// satisfied by a route that refuses everything: a caller who DOES prove it
    /// is the user is told the truth, 200 for the one that exists and 404 for
    /// the one that does not.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn an_unproven_caller_cannot_tell_a_private_chat_from_one_that_does_not_exist() {
        install_test_user_action_key();
        let state = AppState::new().await.unwrap();
        let chat = seed_private_chat(&state, "Task 58 oracle (test fixture)").await;
        // Syntactically valid (so `is_valid_session_id` cannot answer for the
        // store) and not a session on this machine.
        let absent_id = "task58-no-such-session-0000";

        let private = get_session_with(state.clone(), chat.id(), None).await;
        let absent = get_session_with(state.clone(), absent_id, None).await;
        assert_eq!(
            private, absent,
            "an unproven caller can tell a private chat from one that does not exist"
        );
        assert_eq!(private.0, StatusCode::FORBIDDEN);

        // The user is told the difference, which is what makes the equality
        // above a property of the *refusal* rather than of a route that answers
        // 403 to everything.
        let (status, _) =
            get_session_with(state.clone(), chat.id(), Some(TEST_USER_ACTION_KEY)).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) =
            get_session_with(state.clone(), absent_id, Some(TEST_USER_ACTION_KEY)).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "the person at the keyboard is entitled to know the chat is not there"
        );
    }

    /// The knowledge gate is a middleware, and a middleware that is layered onto
    /// the wrong router — or matched against the wrong spelling of the path,
    /// which is exactly what `nest` makes ambiguous — is a security control that
    /// silently does nothing. So it is exercised through the REAL router tree
    /// (`routes::configure`), not through a hand-built one.
    ///
    /// ⚠ Only the refusing arm is driven here, deliberately. The accepting arm
    /// writes a selection pointer into the developer's real knowledge directory;
    /// the refusing arm is decided before the handler runs and writes nothing.
    /// That a **public** session is unaffected is
    /// [`super::tests::a_public_target_is_reachable_by_every_caller`], and the
    /// middleware has no branch of its own between that decision and the
    /// handler.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn the_knowledge_active_gate_is_actually_wired() {
        install_test_user_action_key();
        let state = AppState::new().await.unwrap();
        let private = seed_private_chat(&state, "Task 58 knowledge (test fixture)").await;

        let (status, body) = post_knowledge_active(
            state.clone(),
            serde_json::json!({ "session_id": private.id(), "hidden_kbs": [] }),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a caller holding only the daemon secret repointed a private chat's knowledge \
             bases: {body}"
        );

        // A body naming NO session addresses the machine-wide scope, not a
        // chat, so the gate has nothing to resolve and must let it through to
        // the handler that owns it. Any answer but 403 shows the middleware did
        // not invent a session to refuse.
        let (status, _) =
            post_knowledge_active(state.clone(), serde_json::json!({ "hidden_kbs": [] }), None)
                .await;
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "the gate refused a request that names no chat at all"
        );
    }
}
