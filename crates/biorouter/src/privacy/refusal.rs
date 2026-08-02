//! The refusals the privacy gates return (issue #56).
//!
//! One module rather than one per gate, because a refusal is the *same object*
//! at three different altitudes: a typed Rust error the caller can branch on, a
//! typed HTTP body the GUI renders a repair card from, and — for Gate C — a
//! string the model reads. Keeping them together is what lets §14.4's rule
//! ("never leak content in a refusal": name the tool and the tier, never a
//! session title or a working directory) be checked by reading one file.
//!
//! Later tasks add one item each and say which:
//!
//! | Task | Adds |
//! |---|---|
//! | 12 (this one) | [`PrivacyRefusal`] and its two accessors |
//! | 13 | `turn_refusal(&Session) -> String`, and Task 10's `CHATRECALL_LOAD_REFUSAL` moves here |
//! | 14 | `privacy_refusal(extension, extension_tier, caller_tier) -> Option<ErrorData>` |
//! | 23 | the `PrivateChildOfPublicParent` variant and `PrivacyRefusal::spawn_upgrade` |

use super::{ProviderTier, SessionClassification};
use crate::session::session_manager::Session;
use rmcp::model::{ErrorCode, ErrorData};

/// A privacy boundary refused an operation.
///
/// It is an ordinary `std::error::Error` carried inside `anyhow::Error`, so
/// every existing caller keeps its `Result<()>` signature and the one place
/// that needs to tell a refusal from a database failure — the
/// `/agent/update_provider` handler, which owes the GUI a 409 rather than a
/// 500 — asks with `downcast_ref`.
///
/// ⚠ The payload is deliberately thin: a session **id** and a provider **name**.
/// Refusals reach the model's context (Gate C) and the user's screen, and §14.4
/// forbids either carrying conversation content.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrivacyRefusal {
    /// Gate A: a public model was offered to a session already classified
    /// private. The classification is a permanent ratchet, so this is not
    /// retryable — the way forward is a private model, or declassifying the
    /// session.
    #[error(
        "this chat is private, so it cannot be switched to `{provider}`, which is a public model"
    )]
    PublicModelOnPrivateSession {
        session_id: String,
        provider: String,
    },
}

impl PrivacyRefusal {
    /// The classification of the session that refused. Half of the pair the
    /// GUI's card names ("this chat is **private**, that model is **public**").
    pub fn session_classification(&self) -> SessionClassification {
        match self {
            Self::PublicModelOnPrivateSession { .. } => SessionClassification::Private,
        }
    }

    /// The tier of the thing that was refused.
    pub fn provider_tier(&self) -> ProviderTier {
        match self {
            Self::PublicModelOnPrivateSession { .. } => ProviderTier::Public,
        }
    }

    /// The session the refusal is about. Kept as an accessor rather than
    /// destructured at every call site so a future variant without one is a
    /// compile error here instead of at six handlers.
    pub fn session_id(&self) -> &str {
        match self {
            Self::PublicModelOnPrivateSession { session_id, .. } => session_id,
        }
    }
}

/// The one sentence every turn refusal contains, and the reason it is a
/// constant rather than a phrase inlined at the format site: two independent
/// readers key on it — the desktop client, which must be able to tell a refusal
/// from an ordinary assistant reply, and `agents::agent::gate_b_turn_tests`,
/// whose whole discrimination between "refused" and "ran" is this substring. A
/// reworded refusal that dropped it would leave both of them silently reading
/// every refusal as a completed turn.
pub const TURN_REFUSAL_MARKER: &str = "this turn was not sent";

/// Gate B: the turn was refused because the session is classified private and
/// the model it is bound to is public, with no private model recorded on the
/// row to repair to.
///
/// §14.4: this string is rendered straight into the transcript, so it may name
/// the tier and the provider and nothing else. It takes the whole [`Session`]
/// rather than the two strings it uses so that a future variant needing the
/// classification does not have to change every call site — but note that the
/// row carries the session's *name* and *working directory*, which are CONTENT
/// and must never reach the sentence. The unit test below is what holds that.
pub fn turn_refusal(session: &Session) -> String {
    format!(
        "This chat is private, so only a private model — one hosted inside the institution — \
         may run in it. The model this chat is set to (`{provider}`) is public, so \
         {TURN_REFUSAL_MARKER}. Switch this chat to a private model (Settings → Models, or the \
         model chip in the composer) and send it again. Nothing about the chat has been changed.",
        provider = session.provider_name.as_deref().unwrap_or("no model"),
    )
}

/// Gate D: the model asked to load a chat history more private than the model
/// itself.
///
/// Moved here from `agents::chatrecall_extension`, which is where Task 10 put
/// it before this module existed. Constant on purpose: a model that sees a
/// different string on retry concludes the refusal is transient and loops. It
/// names no target — not the session, not its working directory — because
/// §11.4 classifies both as CONTENT.
pub const fn chatrecall_load_refusal() -> &'static str {
    "This chat history is private: it was created under a model hosted inside the institution, \
     so only a private model may read it. This session is running on a public model. Ask the user \
     to switch this chat to a private model — Settings → Models, or the model chip in the \
     composer — and try again. Do not retry with a different session id or through another tool; \
     the boundary is the same everywhere."
}

/// Gate C's refusal. Returns `None` when the call is permitted, so the caller
/// reads as `if let Some(err) = privacy_refusal(..) { return Err(err.into()); }`.
///
/// Pure: no config, no session, no provider, no I/O. That is what lets the
/// dispatch choke point ask it while holding nothing, and what lets its whole
/// behaviour be pinned by a table-driven unit test.
///
/// `ErrorData` directly, **not** a `ToolInspector`. `Agent::handle_denied_tools`
/// passes a real reason through for exactly three inspector names — the hook
/// inspector, `"security"` and the repetition inspector — and everything else
/// falls to `DECLINED_RESPONSE`, which the code itself calls "actively
/// misleading": it tells the model *the user* refused. An inspector-shaped Gate
/// C would also be invisible to `POST /agent/call_tool`, to the `execute_code`
/// bridge and to `Agent::call_prefetch_tool`, none of which run inspectors.
///
/// §14.4: the string reaches the model's context, so it names the extension and
/// the two tiers and nothing else — no session id, no title, no working
/// directory.
pub fn privacy_refusal(
    extension: &str,
    extension_tier: ProviderTier,
    caller_tier: ProviderTier,
) -> Option<ErrorData> {
    if extension_tier != ProviderTier::Private || caller_tier == ProviderTier::Private {
        return None;
    }
    Some(ErrorData::new(
        ErrorCode::INVALID_REQUEST,
        format!(
            "`{extension}` is a private extension: it reaches data held inside the institution, \
             so only a private model may call it. This session is running on a public model. \
             Ask the user to switch this chat to a private model — Settings > Models, or the \
             model chip in the composer — and then try again. This is a data-protection \
             boundary set by the Biorouter marketplace, not something to work around: do not \
             retry with a different tool name, through code execution, or through a resource \
             read."
        ),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gate C's refusal, exercised the way `check_enable_allowed`'s four tests
    /// exercise it (`extension_manager_extension.rs`): no global config, no
    /// session, no provider — because the function is pure. Same register, too:
    /// name the state, name the reason, foreclose the workaround, name the
    /// human action.
    #[test]
    fn the_refusal_is_pure_deterministic_and_forecloses_the_workaround() {
        use ProviderTier::{Private, Public};

        // The three permitted combinations. Only "private extension, public
        // caller" is a boundary crossing.
        assert!(privacy_refusal("ucsfomopagent", Private, Private).is_none());
        assert!(privacy_refusal("developer", Public, Public).is_none());
        assert!(privacy_refusal("developer", Public, Private).is_none());

        let e = privacy_refusal("ucsfomopagent", Private, Public).unwrap();
        let m = e.message.to_string();
        assert!(m.contains("ucsfomopagent"), "{m}");
        assert!(m.contains("private"), "{m}");
        assert!(m.contains("marketplace"), "{m}"); // names the grantor (R11)
        assert!(m.contains("Settings"), "{m}"); // names the human action
        assert!(m.contains("do not"), "{m}"); // forecloses the workaround

        // Deterministic: a model that sees a different string on retry
        // concludes the refusal is transient and loops.
        assert_eq!(
            m,
            privacy_refusal("ucsfomopagent", Private, Public)
                .unwrap()
                .message
                .to_string()
        );
    }

    /// §14.4 again, for the one refusal that reaches the MODEL's context: it may
    /// name the extension and the tier and nothing else. There is no session in
    /// the signature, so this is a statement about what the function *cannot*
    /// say rather than about what it happens not to.
    #[test]
    fn gate_c_names_the_extension_and_carries_no_session_content() {
        let m = privacy_refusal("ucsfomopagent", ProviderTier::Private, ProviderTier::Public)
            .unwrap()
            .message
            .to_string();
        for content in ["20260801_7", "Patient MRN 4471 workup", "phi/cohort-3"] {
            assert!(!m.contains(content), "{m}");
        }
    }

    #[test]
    fn a_refusal_survives_a_round_trip_through_anyhow() {
        // The whole reason this is a typed error: the route handler has an
        // `anyhow::Error` and owes a 409 rather than a 500.
        let err: anyhow::Error = PrivacyRefusal::PublicModelOnPrivateSession {
            session_id: "s1".into(),
            provider: "anthropic".into(),
        }
        .into();
        let refusal = err
            .downcast_ref::<PrivacyRefusal>()
            .expect("a privacy refusal must survive anyhow");
        assert_eq!(
            refusal.session_classification(),
            SessionClassification::Private
        );
        assert_eq!(refusal.provider_tier(), ProviderTier::Public);
        assert_eq!(refusal.session_id(), "s1");
    }

    /// §14.4: refusals are rendered to the user and (for Gate C) to the model,
    /// so the text may name the tool and the tier and nothing else. The id is
    /// carried as data for the handler; it must not be in the sentence.
    #[test]
    fn the_message_names_the_provider_and_the_tier_and_no_session_content() {
        let refusal = PrivacyRefusal::PublicModelOnPrivateSession {
            session_id: "20260801_7".into(),
            provider: "anthropic".into(),
        };
        let message = refusal.to_string();
        assert!(message.contains("anthropic"), "{message}");
        assert!(message.contains("private"), "{message}");
        assert!(
            !message.contains("20260801_7"),
            "a refusal must not carry the session id into text the user or the model reads: \
             {message}"
        );
    }

    /// The same §14.4 rule for Gate B's turn refusal, which is the one refusal
    /// built from a whole `Session` row — so it is the one with the session's
    /// name and working directory within arm's reach of the format string.
    #[test]
    fn a_turn_refusal_names_the_model_and_no_session_content() {
        let session = Session {
            id: "20260801_7".into(),
            name: "Patient MRN 4471 workup".into(),
            working_dir: std::path::PathBuf::from("/Users/someone/phi/cohort-3"),
            provider_name: Some("anthropic".into()),
            privacy_tier: SessionClassification::Private,
            ..Default::default()
        };
        let text = turn_refusal(&session);
        assert!(text.contains("anthropic"), "{text}");
        assert!(text.contains("private"), "{text}");
        for content in ["20260801_7", "Patient MRN 4471 workup", "phi/cohort-3"] {
            assert!(
                !text.contains(content),
                "a refusal carried session content ({content}) into text the user reads: {text}"
            );
        }
    }

    /// The marker is what the desktop client and `gate_b_turn_tests` key on to
    /// tell a refusal from a completed turn. A reword that dropped it would
    /// make both of them read every refusal as a turn that ran.
    #[test]
    fn every_turn_refusal_carries_the_marker() {
        for provider in [Some("anthropic".to_string()), None] {
            let session = Session {
                provider_name: provider,
                privacy_tier: SessionClassification::Private,
                ..Default::default()
            };
            let text = turn_refusal(&session);
            assert!(text.contains(TURN_REFUSAL_MARKER), "{text}");
        }
    }

    /// Gate D's refusal is a constant for a reason (a model that sees a
    /// different string on retry concludes the refusal is transient and loops),
    /// and it is subject to the same content rule.
    #[test]
    fn the_chatrecall_refusal_is_stable_and_names_no_target() {
        assert_eq!(chatrecall_load_refusal(), chatrecall_load_refusal());
        assert!(chatrecall_load_refusal().contains("private"));
        assert!(!chatrecall_load_refusal().contains("session id\": "));
    }
}
