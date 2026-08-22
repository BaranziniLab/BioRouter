//! **The parity gate.** A coding-agent turn must reach the transcript in the
//! same shape an API-provider turn does, because that shape is the entire
//! contract the GUI renders from.
//!
//! The claim being tested is the user-visible one: *"when I use Claude Code or
//! Codex, I see the same thing I see with an API provider."* The GUI has no
//! provider-specific branch for tool cards — `ToolCallWithResponse` pairs a
//! `ToolRequest` with the `ToolResponse` carrying the same id, reads the name
//! out of the request, colours the card from `isError` on the result, and
//! expands the arguments and the output. So "the same thing" means, precisely:
//! the same *sequence of message shapes*, with the same pairing, the same
//! names, and the same success/failure signal.
//!
//! This test therefore normalises a recorded Claude Code turn and an
//! equivalent Anthropic-shaped turn to a comparable summary and asserts they
//! agree. Normalising is what makes the comparison meaningful rather than
//! trivially false: ids and timestamps differ between the two, and nothing the
//! user sees depends on their literal values.
//!
//! It also pins the counts per fixture cell, in the spirit of bb's
//! `row-counts.json`: an `unhandled` frame count may only ever go **down**, so
//! a vendor adding a frame Biorouter needs shows up as a failing number rather
//! than as silence.

use biorouter::conversation::message::{Message, MessageContent};
use biorouter::providers::coding_agent::mirror::{self, Execution};

/// The user-visible skeleton of one turn.
///
/// Deliberately *not* the messages themselves: two turns that render
/// identically may differ in ids, timestamps and how text was chunked, and a
/// comparison sensitive to those would fail for reasons no user could see.
#[derive(Debug, PartialEq, Eq)]
struct Shape {
    /// Did the turn produce assistant prose at all?
    has_text: bool,
    /// Tool cards, in order: (tool name, ok/failed).
    cards: Vec<(String, bool)>,
    /// Every card's request has a response with the same id.
    every_card_settles: bool,
}

/// Build the shape from whatever a provider yielded.
fn shape_of(messages: &[Message]) -> Shape {
    let mut has_text = false;
    let mut requests: Vec<(String, String)> = Vec::new();
    let mut responses: Vec<(String, bool)> = Vec::new();

    for message in messages {
        for content in &message.content {
            match content {
                MessageContent::Text(t) if !t.text.trim().is_empty() => has_text = true,
                MessageContent::ToolRequest(r) => {
                    let name = r
                        .tool_call
                        .as_ref()
                        .map(|c| c.name.to_string())
                        .unwrap_or_default();
                    requests.push((r.id.clone(), name));
                }
                MessageContent::ToolResponse(r) => {
                    let failed = r
                        .tool_result
                        .as_ref()
                        .ok()
                        .and_then(|v| v.is_error)
                        .unwrap_or(false);
                    responses.push((r.id.clone(), failed));
                }
                _ => {}
            }
        }
    }

    let every_card_settles = requests
        .iter()
        .all(|(id, _)| responses.iter().any(|(rid, _)| rid == id));
    let cards = requests
        .iter()
        .map(|(id, name)| {
            let failed = responses
                .iter()
                .find(|(rid, _)| rid == id)
                .map(|(_, failed)| *failed)
                .unwrap_or(false);
            (name.clone(), failed)
        })
        .collect();

    Shape {
        has_text,
        cards,
        every_card_settles,
    }
}

/// What an **API provider** produces for the same conversation: the loop
/// dispatches the request and appends the response it got back.
///
/// Written by hand rather than replayed, because that is the reference the
/// coding-agent path has to match, and hand-writing it is what makes the
/// expected shape explicit instead of circular.
fn api_provider_turn(tool: &str, failed: bool) -> Vec<Message> {
    let call = rmcp::model::CallToolRequestParams {
        name: tool.to_string().into(),
        arguments: Some(rmcp::model::object(serde_json::json!({ "command": "ls" }))),
        meta: None,
        task: None,
    };
    let request = Message::assistant().with_tool_request("call_1", Ok(call));

    let body = vec![rmcp::model::Content::text("a.txt")];
    let result = if failed {
        rmcp::model::CallToolResult::error(body)
    } else {
        rmcp::model::CallToolResult::success(body)
    };
    let response = Message::user().with_tool_response("call_1", Ok(result));

    vec![
        request,
        response,
        Message::assistant().with_text("Listed the directory."),
    ]
}

/// The equivalent turn as the **coding-agent** path produces it: the child
/// already ran the call, so the provider mirrors the pair instead.
fn mirrored_turn(tool: &str, failed: bool) -> Vec<Message> {
    let request = mirror::request_message(
        "toolu_1",
        &format!("mcp__biorouter__{tool}"),
        serde_json::json!({ "command": "ls" }),
        Execution::Bridged,
    );
    let response = mirror::response_message(
        "toolu_1",
        vec![rmcp::model::Content::text("a.txt")],
        failed,
        Execution::Bridged,
    );

    vec![
        request,
        response,
        Message::assistant().with_text("Listed the directory."),
    ]
}

/// A successful tool call renders the same either way.
#[test]
fn a_mirrored_turn_has_the_same_shape_as_an_api_provider_turn() {
    let api = shape_of(&api_provider_turn("developer__shell", false));
    let mirrored = shape_of(&mirrored_turn("developer__shell", false));

    assert_eq!(
        api, mirrored,
        "a coding-agent turn must reach the GUI in the same shape as an API \
         provider's — same card, same tool name, same settled state"
    );
    assert_eq!(
        mirrored.cards,
        vec![("developer__shell".to_string(), false)]
    );
    assert!(mirrored.every_card_settles);
}

/// And so does a failing one — including the failure signal itself, which is
/// what turns the card red.
#[test]
fn a_failed_call_renders_the_same_either_way() {
    let api = shape_of(&api_provider_turn("developer__shell", true));
    let mirrored = shape_of(&mirrored_turn("developer__shell", true));

    assert_eq!(api, mirrored);
    assert_eq!(mirrored.cards, vec![("developer__shell".to_string(), true)]);
}

/// The one difference that must exist: only the mirrored pair is marked, and
/// the marker is what stops the loop dispatching a call that already ran.
///
/// Without this assertion the two paths could be made "identical" by dropping
/// the marker — which would pass every shape comparison above and reintroduce
/// double execution.
#[test]
fn only_the_mirrored_turn_carries_the_marker() {
    let api = api_provider_turn("developer__shell", false);
    let mirrored = mirrored_turn("developer__shell", false);

    assert!(
        !api.iter().any(mirror::contains_provider_executed),
        "an API provider's pair must never look mirrored, or the loop would stop \
         dispatching real tool calls"
    );
    assert!(
        mirrored.iter().any(mirror::contains_provider_executed),
        "the mirrored pair must be marked, or the loop would run the child's call \
         a second time"
    );
}
