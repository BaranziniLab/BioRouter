//! Hook event types and the JSON payload delivered to hooks on stdin.
//!
//! Field names follow Claude Code's hook input schema (snake_case) so that
//! existing hook scripts can be ported to Biorouter unchanged.

use serde::{Deserialize, Serialize};

/// Lifecycle events that can trigger hooks.
///
/// Serialized as PascalCase strings matching Claude Code event names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PermissionRequest,
    UserPromptSubmit,
    Stop,
    SubagentStart,
    SubagentStop,
    SessionStart,
    SessionEnd,
    Notification,
    PreCompact,
    PostCompact,
}

impl HookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::PreToolUse => "PreToolUse",
            HookEvent::PostToolUse => "PostToolUse",
            HookEvent::PostToolUseFailure => "PostToolUseFailure",
            HookEvent::PermissionRequest => "PermissionRequest",
            HookEvent::UserPromptSubmit => "UserPromptSubmit",
            HookEvent::Stop => "Stop",
            HookEvent::SubagentStart => "SubagentStart",
            HookEvent::SubagentStop => "SubagentStop",
            HookEvent::SessionStart => "SessionStart",
            HookEvent::SessionEnd => "SessionEnd",
            HookEvent::Notification => "Notification",
            HookEvent::PreCompact => "PreCompact",
            HookEvent::PostCompact => "PostCompact",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "PreToolUse" => Some(HookEvent::PreToolUse),
            "PostToolUse" => Some(HookEvent::PostToolUse),
            "PostToolUseFailure" => Some(HookEvent::PostToolUseFailure),
            "PermissionRequest" => Some(HookEvent::PermissionRequest),
            "UserPromptSubmit" => Some(HookEvent::UserPromptSubmit),
            "Stop" => Some(HookEvent::Stop),
            "SubagentStart" => Some(HookEvent::SubagentStart),
            "SubagentStop" => Some(HookEvent::SubagentStop),
            "SessionStart" => Some(HookEvent::SessionStart),
            "SessionEnd" => Some(HookEvent::SessionEnd),
            "Notification" => Some(HookEvent::Notification),
            "PreCompact" => Some(HookEvent::PreCompact),
            "PostCompact" => Some(HookEvent::PostCompact),
            _ => None,
        }
    }

    /// Whether a blocking decision from a hook is honored for this event.
    pub fn supports_blocking(&self) -> bool {
        matches!(
            self,
            HookEvent::PreToolUse
                | HookEvent::PermissionRequest
                | HookEvent::UserPromptSubmit
                | HookEvent::Stop
        )
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The JSON document piped to a hook's stdin (and embedded in prompt-hook
/// judge requests).
#[derive(Debug, Clone, Default, Serialize)]
pub struct HookPayload {
    pub session_id: String,
    /// The session working directory; hooks also run with this as their cwd.
    pub cwd: String,
    pub hook_event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<serde_json::Value>,
    /// Error text for PostToolUseFailure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The user prompt for UserPromptSubmit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Compaction trigger: "manual" or "auto".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    /// SessionStart source ("startup" | "resume") or SessionEnd reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// True when a Stop hook already blocked once this turn-exit sequence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_hook_active: Option<bool>,
    /// Recent conversation messages (role-prefixed, truncated) so Stop-hook
    /// judges can evaluate what the agent actually did this turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_tail: Option<String>,
    /// Notification text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_id: Option<String>,
    /// Biorouter-specific extra detail (e.g. "context_overflow" compactions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl HookPayload {
    pub fn new(event: HookEvent, session_id: impl Into<String>, cwd: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            cwd: cwd.into(),
            hook_event_name: event.as_str().to_string(),
            ..Default::default()
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_round_trips_through_strings() {
        for event in [
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::PostToolUseFailure,
            HookEvent::PermissionRequest,
            HookEvent::UserPromptSubmit,
            HookEvent::Stop,
            HookEvent::SubagentStart,
            HookEvent::SubagentStop,
            HookEvent::SessionStart,
            HookEvent::SessionEnd,
            HookEvent::Notification,
            HookEvent::PreCompact,
            HookEvent::PostCompact,
        ] {
            assert_eq!(HookEvent::from_str_opt(event.as_str()), Some(event));
        }
        assert_eq!(HookEvent::from_str_opt("FileChanged"), None);
    }

    #[test]
    fn payload_omits_unset_fields() {
        let payload = HookPayload::new(HookEvent::PreToolUse, "sess-1", "/tmp");
        let json: serde_json::Value = serde_json::from_str(&payload.to_json()).unwrap();
        assert_eq!(json["hook_event_name"], "PreToolUse");
        assert_eq!(json["session_id"], "sess-1");
        assert!(json.get("tool_name").is_none());
        assert!(json.get("prompt").is_none());
    }
}
