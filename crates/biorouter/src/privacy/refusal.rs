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

#[cfg(test)]
mod tests {
    use super::*;

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
}
