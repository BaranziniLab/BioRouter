//! Serializable paused-run state for human-in-the-loop approvals + recovery.
//!
//! When a tool needs approval, the run is snapshotted to a [`RunState`],
//! persisted into the session, and an `approval` frame is surfaced. The live
//! decision (`approve`/`reject`) is delivered over the connection that is parked
//! awaiting it. Persistence makes the pause **observable** after a reconnect
//! (an app can re-surface "approval pending" via `GET /apps/{id}/runstate`).
//!
//! Note: re-driving a *resolved* snapshot from a fresh connection/process (full
//! cross-process resume) would require replaying the agent loop from the
//! snapshot and is intentionally out of scope here — the snapshot carries enough
//! to support that later (session handle + pending tool + remaining budget).
//!
//! The snapshot is intentionally **lightweight**: the conversation already lives
//! in the durable session (Phase 1), so RunState only carries what's needed to
//! resume — the session handle, the pending tool call, and the remaining turn
//! budget. That keeps it trivially serializable (no embedding/ deriving serde on
//! the heavy conversation types) and the resume path just reloads the session.

use serde::{Deserialize, Serialize};

/// Current schema version of a serialized [`RunState`] (independent of the
/// session-DB schema), so a resumer can detect/upgrade older snapshots.
pub const RUN_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Paused, waiting for a human to approve/reject the pending tool.
    AwaitingApproval,
    /// Approved; ready to resume by dispatching the pending tool.
    Approved,
    /// Rejected; resume by feeding the model a declined-tool result.
    Rejected,
    /// Already resumed (terminal — guards against double-resume).
    Resumed,
}

/// The tool call that triggered the approval pause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTool {
    /// The tool-request id, so the resumed loop can splice the result/denial back.
    pub request_id: String,
    pub name: String,
    /// Arguments as JSON (serde-friendly; no engine types embedded).
    pub args: serde_json::Value,
}

/// A serializable snapshot of a paused run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    pub version: u32,
    /// Opaque id surfaced to the client as the approval id.
    pub run_id: String,
    /// The durable session this run belongs to (carries the conversation).
    pub session_id: String,
    pub app_id: String,
    pub status: RunStatus,
    pub pending_tool: PendingTool,
    /// Why approval is required (shown to the human).
    pub reason: String,
    /// Turns left in the agent's budget when it paused (so resume is bounded).
    pub remaining_turns: u32,
    /// If the human rejected, the message fed to the model in the declined result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
}

impl RunState {
    /// Snapshot a run paused awaiting approval of `pending`.
    pub fn awaiting_approval(
        run_id: impl Into<String>,
        session_id: impl Into<String>,
        app_id: impl Into<String>,
        pending: PendingTool,
        reason: impl Into<String>,
        remaining_turns: u32,
    ) -> Self {
        RunState {
            version: RUN_STATE_VERSION,
            run_id: run_id.into(),
            session_id: session_id.into(),
            app_id: app_id.into(),
            status: RunStatus::AwaitingApproval,
            pending_tool: pending,
            reason: reason.into(),
            remaining_turns,
            reject_reason: None,
        }
    }

    /// Mark approved. No-op if already resolved.
    pub fn approve(&mut self) -> bool {
        if self.status == RunStatus::AwaitingApproval {
            self.status = RunStatus::Approved;
            true
        } else {
            false
        }
    }

    /// Mark rejected with an optional human reason. No-op if already resolved.
    pub fn reject(&mut self, reason: Option<String>) -> bool {
        if self.status == RunStatus::AwaitingApproval {
            self.status = RunStatus::Rejected;
            self.reject_reason = reason;
            true
        } else {
            false
        }
    }

    /// Mark resumed (terminal); guards against a second resume.
    pub fn mark_resumed(&mut self) {
        self.status = RunStatus::Resumed;
    }

    /// Whether this state still needs a human decision.
    pub fn is_pending(&self) -> bool {
        self.status == RunStatus::AwaitingApproval
    }

    /// Serialize to JSON for persistence / cross-process transfer.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Restore from JSON; rejects an unknown future schema version.
    pub fn from_json(s: &str) -> anyhow::Result<Self> {
        let state: RunState = serde_json::from_str(s)?;
        if state.version > RUN_STATE_VERSION {
            anyhow::bail!(
                "RunState schema v{} is newer than supported v{}",
                state.version,
                RUN_STATE_VERSION
            );
        }
        Ok(state)
    }

    /// Persist this run state into a session's `extension_data` (which the
    /// session DB already round-trips), so a paused approval survives a reload —
    /// no dedicated schema migration needed.
    pub fn store_into(&self, data: &mut crate::session::ExtensionData) {
        if let Ok(v) = serde_json::to_value(self) {
            data.set_extension_state(RUN_STATE_KEY, RUN_STATE_VER, v);
        }
    }

    /// Load a persisted run state from a session's `extension_data` (if any, and
    /// not a too-new schema). Returns `None` when absent or cleared.
    pub fn load_from(data: &crate::session::ExtensionData) -> Option<RunState> {
        let v = data.get_extension_state(RUN_STATE_KEY, RUN_STATE_VER)?;
        if v.is_null() {
            return None;
        }
        let state: RunState = serde_json::from_value(v.clone()).ok()?;
        (state.version <= RUN_STATE_VERSION).then_some(state)
    }

    /// Clear any persisted run state (after the approval is resolved).
    pub fn clear(data: &mut crate::session::ExtensionData) {
        data.set_extension_state(RUN_STATE_KEY, RUN_STATE_VER, serde_json::Value::Null);
    }
}

/// Pseudo-extension name under which a paused run is stashed in a session's
/// `extension_data` (avoids a dedicated `sessions` column / schema migration).
const RUN_STATE_KEY: &str = "brsdk_run_state";
const RUN_STATE_VER: &str = "1";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pending() -> PendingTool {
        PendingTool {
            request_id: "tr_1".into(),
            name: "send_email".into(),
            args: json!({ "to": "lab@ucsf.edu", "body": "results attached" }),
        }
    }

    fn state() -> RunState {
        RunState::awaiting_approval("run_1", "20240101_7", "spoke-app", pending(), "send_email requires approval", 12)
    }

    #[test]
    fn roundtrips_through_json_across_a_process_boundary() {
        let s = state();
        let json = s.to_json().unwrap();
        // Simulate a different process loading it.
        let back = RunState::from_json(&json).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.pending_tool.args["to"], "lab@ucsf.edu");
        assert_eq!(back.remaining_turns, 12);
        assert!(back.is_pending());
    }

    #[test]
    fn approve_transitions_once() {
        let mut s = state();
        assert!(s.approve());
        assert_eq!(s.status, RunStatus::Approved);
        assert!(!s.is_pending());
        // A second decision is a no-op (can't approve an already-approved run).
        assert!(!s.approve());
        assert!(!s.reject(None));
    }

    #[test]
    fn reject_records_reason() {
        let mut s = state();
        assert!(s.reject(Some("wrong recipient".into())));
        assert_eq!(s.status, RunStatus::Rejected);
        assert_eq!(s.reject_reason.as_deref(), Some("wrong recipient"));
    }

    #[test]
    fn mark_resumed_is_terminal() {
        let mut s = state();
        s.approve();
        s.mark_resumed();
        assert_eq!(s.status, RunStatus::Resumed);
        assert!(!s.approve(), "cannot re-approve a resumed run");
    }

    #[test]
    fn rejects_newer_schema_version() {
        let mut s = state();
        s.version = RUN_STATE_VERSION + 1;
        let json = s.to_json().unwrap();
        assert!(RunState::from_json(&json).is_err());
    }

    #[test]
    fn persists_into_and_loads_from_extension_data() {
        use crate::session::ExtensionData;
        let mut data = ExtensionData::new();
        // Absent → None.
        assert!(RunState::load_from(&data).is_none());

        // Store → load roundtrips.
        let s = state();
        s.store_into(&mut data);
        let back = RunState::load_from(&data).expect("stored run state");
        assert_eq!(back, s);
        assert!(back.is_pending());

        // Clear → None again (so a reconnect won't re-show a resolved approval).
        RunState::clear(&mut data);
        assert!(RunState::load_from(&data).is_none());
    }

    #[test]
    fn load_from_rejects_a_too_new_persisted_schema() {
        use crate::session::ExtensionData;
        let mut data = ExtensionData::new();
        let mut s = state();
        s.version = RUN_STATE_VERSION + 1;
        s.store_into(&mut data);
        assert!(
            RunState::load_from(&data).is_none(),
            "a future-schema persisted run state must not load"
        );
    }
}
