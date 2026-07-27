//! Why a turn ended without doing its job.
//!
//! Before this existed, a turn that *failed* and a turn that *succeeded* were
//! indistinguishable to every consumer of the agent stream. A provider 403 was
//! downgraded into an assistant chat message ("Ran into this error: …") and the
//! stream then ended **normally**, so:
//!
//!   * `biorouter run` exited **0**,
//!   * `--output-format json` reported `"status": "completed"`,
//!   * telemetry logged the failed run as a **success**,
//!   * and the desktop/app runtimes rendered a finished turn.
//!
//! The only way to recover "did the turn actually run?" was to regex the English
//! prose the model-facing layer emitted. This makes the failure a typed, terminal
//! event that every boundary — CLI exit code, JSON status, SSE frame, WS frame —
//! can check mechanically.
//!
//! Wave 4 adds [`TurnAbortCode::ToolLoop`] and [`TurnAbortCode::WorkerTimeout`]
//! through the same channel.

use serde::{Deserialize, Serialize};

use crate::providers::errors::ProviderErrorKind;

/// The reason a turn was aborted. Serialized into SSE/WebSocket error frames and
/// mapped to process exit codes by the CLI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum TurnAbortCode {
    /// The provider call failed (auth, rate limit, network, server error).
    ProviderFailure { kind: ProviderErrorKind },
    /// The session store (SQLite) failed while persisting or reading the
    /// turn's messages (#31/#41). Distinct from `ProviderFailure` so wire
    /// frames and exit codes stop blaming the provider for a local disk/db
    /// problem — the operator remediation is completely different.
    SessionStore,
    /// The model called the same tool with the same arguments until the loop
    /// guard terminated the turn (Wave 4).
    ToolLoop { tool: String },
    /// A worker profile consulted by the main agent never answered (Wave 4).
    WorkerTimeout { agent: String, elapsed_s: u64 },
}

impl TurnAbortCode {
    /// Stable machine-readable code for wire frames (`{"type":"error","code":…}`).
    pub fn wire_code(&self) -> &'static str {
        match self {
            Self::ProviderFailure { .. } => "provider_failure",
            Self::SessionStore => "session_store_failure",
            Self::ToolLoop { .. } => "tool_loop",
            Self::WorkerTimeout { .. } => "worker_timeout",
        }
    }

    /// The process exit code a CLI run should end with.
    ///
    /// These are distinct from the generic `1` that every existing `anyhow` path
    /// already returns, so a harness can tell "the agent ran and disagreed with
    /// you" from "the agent never ran".
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::ProviderFailure { kind } if kind.is_auth() => exit::PROVIDER_AUTH,
            Self::ProviderFailure { .. } => exit::PROVIDER_FAILED,
            Self::SessionStore => exit::SESSION_STORE,
            Self::ToolLoop { .. } => exit::TOOL_LOOP,
            Self::WorkerTimeout { .. } => exit::WORKER_TIMEOUT,
        }
    }
}

/// The machine-checkable abort code for a raw agent error — shared by the
/// CLI's reply-construction failure path and its mid-stream `Err` branch so
/// both report identically in structured output (#31/#41).
///
/// Classified by downcast, not by string matching: an error whose chain
/// contains a [`sqlx::Error`] is a session-store failure (the only sqlx in
/// the turn path is the session SQLite store), everything else falls back to
/// the provider classification. Before this, a failed `add_message` inside
/// `Agent::reply` was reported as `provider_failure` — the wire code and the
/// process exit code blamed the provider for a local db problem.
pub fn classify_agent_error(e: &anyhow::Error) -> TurnAbortCode {
    if e.chain().any(|cause| cause.is::<sqlx::Error>()) {
        return TurnAbortCode::SessionStore;
    }
    let kind = e
        .downcast_ref::<crate::providers::errors::ProviderError>()
        .map(crate::providers::errors::ProviderError::kind)
        .unwrap_or(ProviderErrorKind::Other);
    TurnAbortCode::ProviderFailure { kind }
}

/// A turn that ended without doing its work, as an `Error` — so it can travel
/// through the CLI's existing `anyhow::Result` plumbing without changing every
/// signature, and be recovered at the top with `downcast_ref` to pick the exit
/// code.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct TurnFailed {
    pub code: TurnAbortCode,
    pub message: String,
}

impl TurnFailed {
    pub fn new(code: TurnAbortCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn exit_code(&self) -> u8 {
        self.code.exit_code()
    }
}

/// Process exit codes for a turn that did not complete its work.
pub mod exit {
    /// The turn ran to completion.
    pub const OK: u8 = 0;
    /// Any pre-existing `anyhow` failure path. Unchanged.
    pub const GENERIC: u8 = 1;
    /// The provider call failed (network, 5xx, rate limit).
    pub const PROVIDER_FAILED: u8 = 70;
    /// The provider rejected our credentials (401/403).
    pub const PROVIDER_AUTH: u8 = 75;
    /// The model looped on one tool until the guard terminated the turn.
    pub const TOOL_LOOP: u8 = 76;
    /// A consulted worker profile never answered.
    pub const WORKER_TIMEOUT: u8 = 77;
    /// The session store (SQLite) failed while persisting the turn (#31/#41).
    pub const SESSION_STORE: u8 = 78;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #31/#41: a session-store failure inside `Agent::reply` must classify
    /// as its own abort code — wire code and exit code stop blaming the
    /// provider for a local db problem. Classified by downcast anywhere in
    /// the anyhow chain, exactly how the store's errors travel (a root
    /// `sqlx::Error` under `context(...)` wrappers).
    #[test]
    fn store_errors_classify_as_session_store_not_provider_failure() {
        use anyhow::Context as _;

        let store_error = anyhow::Error::new(sqlx::Error::PoolClosed)
            .context("Failed to add message to session");
        assert_eq!(classify_agent_error(&store_error), TurnAbortCode::SessionStore);
        assert_eq!(
            classify_agent_error(&store_error).wire_code(),
            "session_store_failure"
        );
        assert_eq!(
            classify_agent_error(&store_error).exit_code(),
            exit::SESSION_STORE
        );

        // A typed provider error keeps its kind…
        let provider_error = anyhow::Error::new(
            crate::providers::errors::ProviderError::Authentication("403".to_string()),
        );
        assert_eq!(
            classify_agent_error(&provider_error),
            TurnAbortCode::ProviderFailure {
                kind: ProviderErrorKind::Auth
            }
        );

        // …and anything else stays the conservative provider/Other fallback.
        let opaque = anyhow::anyhow!("something else entirely");
        assert_eq!(
            classify_agent_error(&opaque),
            TurnAbortCode::ProviderFailure {
                kind: ProviderErrorKind::Other
            }
        );
    }

    #[test]
    fn auth_failures_get_their_own_exit_code() {
        let auth = TurnAbortCode::ProviderFailure {
            kind: ProviderErrorKind::Auth,
        };
        assert_eq!(auth.exit_code(), exit::PROVIDER_AUTH);

        let server = TurnAbortCode::ProviderFailure {
            kind: ProviderErrorKind::Server,
        };
        assert_eq!(server.exit_code(), exit::PROVIDER_FAILED);
    }

    /// Every abort code must be nonzero — the whole point is that a failed turn
    /// stops looking like a successful one.
    #[test]
    fn no_abort_code_maps_to_success() {
        for code in [
            TurnAbortCode::ProviderFailure {
                kind: ProviderErrorKind::Network,
            },
            TurnAbortCode::ToolLoop {
                tool: "ui_describe".into(),
            },
            TurnAbortCode::WorkerTimeout {
                agent: "fine_mapper".into(),
                elapsed_s: 120,
            },
        ] {
            assert_ne!(code.exit_code(), exit::OK, "{code:?} must not exit 0");
            assert!(!code.wire_code().is_empty());
        }
    }

    #[test]
    fn wire_codes_round_trip_through_serde() {
        let code = TurnAbortCode::ToolLoop {
            tool: "ui_describe".into(),
        };
        let json = serde_json::to_value(&code).unwrap();
        assert_eq!(json["reason"], "tool_loop");
        assert_eq!(json["tool"], "ui_describe");
        let back: TurnAbortCode = serde_json::from_value(json).unwrap();
        assert_eq!(back, code);
    }
}
