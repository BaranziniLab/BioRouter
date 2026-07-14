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
            Self::ToolLoop { .. } => exit::TOOL_LOOP,
            Self::WorkerTimeout { .. } => exit::WORKER_TIMEOUT,
        }
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
