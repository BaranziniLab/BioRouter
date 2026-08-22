//! The mirror marker: how a coding-agent provider says *"this tool call already
//! ran — show it, do not run it"*.
//!
//! # Why a marker exists at all
//!
//! Every other provider hands the agent loop a `ToolRequest` as a *request*: the
//! loop inspects it, gates it, dispatches it, and appends the `ToolResponse` it
//! gets back. The two coding-agent providers are the exception. Their child
//! executed the call already — over the tool bridge, where Biorouter ran it
//! behind the same inspectors, permission mode, `.biorouterignore`, vault and
//! privacy Gate C ([`super::bridge`]) — and by the time the frame describing it
//! reaches this crate the work is done and the result is back in the child.
//!
//! Surfacing that as a bare `ToolRequest` would be a correctness bug, not a
//! cosmetic one: a `ToolRequest` in the turn's response message is dispatched by
//! the loop (`categorize_tools`, `agents/agent.rs`), and
//! `categorize_tool_requests` (`agents/reply_parts.rs`) filters on **content
//! only** — it never looks at metadata. So an unmarked mirrored request is either
//! a `Tool '…' not found` error row (with the `mcp__biorouter__` prefix intact)
//! or a genuine **second execution** of a call that already ran. A shell command
//! run twice is not a display glitch.
//!
//! # Where the marker lives, and where it deliberately does not
//!
//! It is a reserved key in the existing per-tool [`ProviderMetadata`] on
//! [`ToolRequest`] / [`ToolResponse`] — a `serde_json::Map` those types already
//! carry and already serialize. Nothing new enters the message schema, and the
//! GUI keeps rendering the pair with the components it uses for every other
//! provider.
//!
//! It is **not** a [`crate::conversation::MessageProvenance`] variant. That type
//! is BR-71's security-purposed cross-session stamp, whose presence has a
//! specific meaning to `is_provenance_boundary` in merges and to the subagent
//! surfaces, and whose unknown `kind` deliberately degrades to `None`. Reusing
//! it would overload a security signal with a display one and would silently
//! lose the stamp on an older reader.
//!
//! # The fail-safe direction
//!
//! Losing the marker must never cause a double execution, so the loop's question
//! is [`contains_provider_executed`] — *does this message carry **any** mirrored
//! tool content?* — and not *are they all mirrored?* A message that somehow mixed
//! marked and unmarked content therefore dispatches **nothing**: the worst case
//! is a card whose tool did not run (visible, and a decoder bug), never a shell
//! command that ran twice (invisible, and unrecoverable). The decoders never mix
//! — [`super::claude_stream`] and [`super::codex_stream`] mint mirrored content
//! only — and the tests below pin the fail-safe direction anyway.
//!
//! The marker is honoured on the **live stream path only**. Persisted rows are
//! replayed as history and are never re-dispatched, so a reader that does not
//! understand the key loses attribution, never safety.

// ⚠ `ProviderMetadata` here is the per-tool `serde_json::Map` on a
// `ToolRequest`/`ToolResponse` (`conversation::message`), **not** the identically
// named provider-configuration type in `providers::base`. Importing the wrong one
// compiles nowhere near here and reads as if it should.
use crate::conversation::message::{
    Message, MessageContent, ProviderMetadata, ToolRequest, ToolResponse,
};

/// The reserved `ProviderMetadata` key. Namespaced, because the map is shared
/// with whatever a provider chooses to record there.
pub const PROVIDER_EXECUTED_KEY: &str = "biorouterProviderExecuted";

/// Who actually ran a mirrored tool call — and therefore which guarantees hold.
///
/// This is a real distinction, not a label: [`Self::Bridged`] passed every gate
/// Biorouter has, [`Self::Child`] passed none of them. The GUI is expected to
/// say so on the card, because a user reading a command card is entitled to know
/// whether Biorouter approved it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Execution {
    /// Ran on Biorouter's side of the tool bridge: inspectors, permission mode,
    /// `.biorouterignore`, vault and privacy Gate C all applied.
    Bridged,
    /// Ran inside the child agent's own sandbox and never reached Biorouter —
    /// Codex's `exec`/`apply_patch` under its read-only sandbox, and any MCP
    /// server the user configured in their own `~/.codex/config.toml`. Displayed
    /// for honesty; explicitly *not* something Biorouter vouched for.
    Child,
}

impl Execution {
    /// The wire value. Stable — it is persisted in session rows.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Execution::Bridged => "bridged",
            Execution::Child => "child",
        }
    }

    /// Parse the wire value. An unrecognised value is `None`, which the loop
    /// treats as "not mirrored" — the fail-safe direction is to dispatch nothing
    /// only when we positively recognise the marker.
    #[must_use]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "bridged" => Some(Execution::Bridged),
            "child" => Some(Execution::Child),
            _ => None,
        }
    }
}

fn stamp(metadata: &mut Option<ProviderMetadata>, exec: Execution) {
    metadata.get_or_insert_with(serde_json::Map::new).insert(
        PROVIDER_EXECUTED_KEY.to_string(),
        serde_json::Value::String(exec.as_str().to_string()),
    );
}

fn read(metadata: Option<&ProviderMetadata>) -> Option<Execution> {
    metadata
        .and_then(|m| m.get(PROVIDER_EXECUTED_KEY))
        .and_then(serde_json::Value::as_str)
        .and_then(Execution::from_str)
}

/// Stamp a request the child already made.
pub fn mark_request(request: &mut ToolRequest, exec: Execution) {
    stamp(&mut request.metadata, exec);
}

/// Stamp the response that closes a mirrored request.
///
/// The response is stamped too, so a row read on its own — by the transcript
/// flattener, by a session export, by the GUI — carries its own provenance
/// rather than depending on finding its partner.
pub fn mark_response(response: &mut ToolResponse, exec: Execution) {
    stamp(&mut response.metadata, exec);
}

/// The execution kind recorded on a request, if it is mirrored.
#[must_use]
pub fn request_execution(request: &ToolRequest) -> Option<Execution> {
    read(request.metadata.as_ref())
}

/// The execution kind recorded on a response, if it is mirrored.
#[must_use]
pub fn response_execution(response: &ToolResponse) -> Option<Execution> {
    read(response.metadata.as_ref())
}

/// Does this message carry **any** mirrored tool content?
///
/// This is the question the agent loop asks, and the `any` rather than `all` is
/// the fail-safe choice explained in the module header: a mixed message
/// dispatches nothing.
#[must_use]
pub fn contains_provider_executed(message: &Message) -> bool {
    message.content.iter().any(|content| match content {
        MessageContent::ToolRequest(r) => request_execution(r).is_some(),
        MessageContent::ToolResponse(r) => response_execution(r).is_some(),
        _ => false,
    })
}

/// The execution kind for a message, when every mirrored item agrees.
///
/// Used for display and for tests; the loop's gate is
/// [`contains_provider_executed`]. Returns `None` for a message with no mirrored
/// content, and for one whose items disagree (which a decoder must never
/// produce).
#[must_use]
pub fn message_execution(message: &Message) -> Option<Execution> {
    let mut seen: Option<Execution> = None;
    for content in &message.content {
        let found = match content {
            MessageContent::ToolRequest(r) => request_execution(r),
            MessageContent::ToolResponse(r) => response_execution(r),
            _ => None,
        };
        match (seen, found) {
            (_, None) => {}
            (None, Some(e)) => seen = Some(e),
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => return None,
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolRequestParams, CallToolResult, Content};
    use rmcp::object;

    fn request(id: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams {
                name: "developer__shell".into(),
                arguments: Some(object!({ "command": "ls" })),
                meta: None,
                task: None,
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    fn response(id: &str) -> ToolResponse {
        ToolResponse {
            id: id.to_string(),
            tool_result: Ok(CallToolResult::success(vec![Content::text("ok")])),
            metadata: None,
        }
    }

    fn message_with(content: Vec<MessageContent>) -> Message {
        Message::new(
            rmcp::model::Role::Assistant,
            chrono::Utc::now().timestamp(),
            content,
        )
    }

    #[test]
    fn a_marked_request_round_trips_through_metadata() {
        let mut r = request("toolu_1");
        assert_eq!(request_execution(&r), None, "unmarked to begin with");
        mark_request(&mut r, Execution::Bridged);
        assert_eq!(request_execution(&r), Some(Execution::Bridged));

        let mut r2 = request("toolu_2");
        mark_request(&mut r2, Execution::Child);
        assert_eq!(request_execution(&r2), Some(Execution::Child));
    }

    #[test]
    fn a_marked_response_round_trips_through_metadata() {
        let mut r = response("toolu_1");
        assert_eq!(response_execution(&r), None);
        mark_response(&mut r, Execution::Bridged);
        assert_eq!(response_execution(&r), Some(Execution::Bridged));
    }

    /// The marker must survive the session store, which round-trips messages
    /// through serde. A marker that only exists in memory would be honoured on
    /// the live stream and lost on reload, which is exactly the drift the
    /// metadata home was chosen to avoid.
    #[test]
    fn the_marker_survives_a_serde_round_trip() {
        let mut r = request("toolu_1");
        mark_request(&mut r, Execution::Child);
        let msg = message_with(vec![MessageContent::ToolRequest(r)]);

        let json = serde_json::to_string(&msg).expect("serialize");
        let back: Message = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(message_execution(&back), Some(Execution::Child));
        assert!(contains_provider_executed(&back));
    }

    /// Stamping must not discard whatever the provider already recorded there.
    #[test]
    fn stamping_preserves_other_provider_metadata() {
        let mut r = request("toolu_1");
        let mut existing = serde_json::Map::new();
        existing.insert("vendorId".to_string(), serde_json::json!("abc"));
        r.metadata = Some(existing);

        mark_request(&mut r, Execution::Bridged);

        let meta = r.metadata.as_ref().expect("metadata");
        assert_eq!(meta.get("vendorId").and_then(|v| v.as_str()), Some("abc"));
        assert_eq!(request_execution(&r), Some(Execution::Bridged));
    }

    /// An ordinary API-provider message must never look mirrored — this is the
    /// assertion that keeps the loop's new branch off every other provider's
    /// path.
    #[test]
    fn an_unmarked_message_is_not_provider_executed() {
        let msg = message_with(vec![MessageContent::ToolRequest(request("toolu_1"))]);
        assert!(!contains_provider_executed(&msg));
        assert_eq!(message_execution(&msg), None);
    }

    /// A value nobody wrote (a newer marker kind, a corrupted row) is not
    /// recognised, so the message dispatches normally rather than being silently
    /// swallowed.
    #[test]
    fn an_unrecognised_marker_value_is_not_mirrored() {
        let mut r = request("toolu_1");
        let mut meta = serde_json::Map::new();
        meta.insert(
            PROVIDER_EXECUTED_KEY.to_string(),
            serde_json::json!("teleported"),
        );
        r.metadata = Some(meta);

        assert_eq!(request_execution(&r), None);
        assert!(!contains_provider_executed(&message_with(vec![
            MessageContent::ToolRequest(r)
        ])));
    }

    /// The fail-safe direction, stated as a test: a message mixing a mirrored
    /// call with an unmirrored one still answers "yes" to the loop's question,
    /// so **nothing** in it is dispatched. Not running a tool is recoverable;
    /// running one twice is not.
    #[test]
    fn a_mixed_message_still_suppresses_dispatch() {
        let mut marked = request("toolu_1");
        mark_request(&mut marked, Execution::Bridged);
        let msg = message_with(vec![
            MessageContent::ToolRequest(marked),
            MessageContent::ToolRequest(request("toolu_2")),
        ]);

        assert!(
            contains_provider_executed(&msg),
            "any mirrored content suppresses dispatch for the whole message"
        );
        assert_eq!(
            message_execution(&msg),
            Some(Execution::Bridged),
            "one marked item, so the kinds do not disagree"
        );
    }

    /// Disagreeing kinds have no single answer for display, and saying so is
    /// better than picking one.
    #[test]
    fn disagreeing_kinds_have_no_message_execution() {
        let mut a = request("toolu_1");
        mark_request(&mut a, Execution::Bridged);
        let mut b = request("toolu_2");
        mark_request(&mut b, Execution::Child);
        let msg = message_with(vec![
            MessageContent::ToolRequest(a),
            MessageContent::ToolRequest(b),
        ]);

        assert!(contains_provider_executed(&msg));
        assert_eq!(message_execution(&msg), None);
    }

    #[test]
    fn text_only_messages_are_never_mirrored() {
        let msg = message_with(vec![MessageContent::text("hello")]);
        assert!(!contains_provider_executed(&msg));
        assert_eq!(message_execution(&msg), None);
    }

    #[test]
    fn the_wire_values_are_stable() {
        assert_eq!(Execution::Bridged.as_str(), "bridged");
        assert_eq!(Execution::Child.as_str(), "child");
        assert_eq!(Execution::from_str("bridged"), Some(Execution::Bridged));
        assert_eq!(Execution::from_str("child"), Some(Execution::Child));
        assert_eq!(Execution::from_str("Bridged"), None);
    }
}
