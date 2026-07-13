//! BR-7: externalize an oversized tool result out of `messages.content_json`.
//!
//! Every message is persisted as one `content_json` TEXT column holding the
//! whole serialized `Vec<MessageContent>` — including every byte of every tool
//! response. A single multi-megabyte result therefore lands *inline* in the
//! `messages` row, and every read of that session (`get_conversation`, the FTS
//! backfill, chat recall's `json_each` scan, `list_sessions`' join) drags it
//! back off disk and through serde. Session DBs bloat and loads get slower for
//! the rest of the session's life.
//!
//! The BR-6 large-response handler bounds what a *live* tool call puts in
//! context, but it runs only on the extension-manager dispatch path — platform
//! tools, subagent results, frontend tool results and anything imported from a
//! legacy session still reach the DB whole. This module is the storage-layer
//! floor underneath it.
//!
//! ## How it works
//!
//! On write ([`externalize`]) any *text* content item inside a tool response
//! that is larger than [`threshold_bytes`] is moved into the `message_blobs`
//! side table and replaced, in the persisted content, by a compact **stub**:
//! a machine-readable marker line, a size summary, a bounded head/tail preview
//! (the BR-11 [`truncate_middle_out`] helper, shared with compaction), and the
//! blob handle.
//!
//! On read the default is to [`hydrate`] the stubs straight back — an exact,
//! byte-for-byte round trip — so every existing consumer (the UI transcript,
//! exports, the agent's resumed conversation) sees precisely what it saw
//! before, while the `messages` rows themselves stay small.
//!
//! Setting `BIOROUTER_SESSION_BLOB_LAZY_LOAD=true` opts into the *lazy* read:
//! `get_conversation` leaves the stub in place, so a resumed session never
//! deserializes the payload at all, and the model pulls (or greps) the parts it
//! needs back with the `platform__read_session_blob` tool.
//!
//! Only text inside `ToolResponse` is externalized — user prose stays inline
//! (it is what chat recall searches) and images stay inline (a stub is not
//! something a vision model can read back).

use crate::config::Config;
use crate::context_mgmt::truncate_middle_out;
use crate::conversation::message::MessageContent;
use rmcp::model::{CallToolResult, Content};
use std::collections::HashMap;
use uuid::Uuid;

/// Size, in bytes of UTF-8 text, above which one tool-response text item is
/// moved to the blob table. Comfortably above anything the BR-6 handler lets
/// through (~25k tokens), so in practice only payloads that bypassed it — or
/// predate it — are externalized. Override with
/// `BIOROUTER_SESSION_BLOB_THRESHOLD_BYTES`; `0` disables externalization.
pub const DEFAULT_BLOB_THRESHOLD_BYTES: usize = 64 * 1024;

/// How much of the payload the stub keeps inline as a head/tail preview. Small
/// on purpose: the stub's whole job is to keep the `messages` row tiny.
pub const DEFAULT_STUB_PREVIEW_CHARS: usize = 600;

/// Opening delimiter of the stub's marker line. The uid and `>>>` follow.
const MARKER_OPEN: &str = "<<<biorouter-session-blob:";
const MARKER_CLOSE: &str = ">>>";

/// A payload lifted out of a message, waiting to be written to `message_blobs`
/// in the same transaction as the message row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingBlob {
    pub uid: String,
    pub content: String,
}

impl PendingBlob {
    pub fn bytes(&self) -> i64 {
        self.content.len() as i64
    }
}

/// Resolved externalization threshold in bytes (`0` = disabled).
pub fn threshold_bytes() -> usize {
    Config::global()
        .get_param::<usize>("BIOROUTER_SESSION_BLOB_THRESHOLD_BYTES")
        .unwrap_or(DEFAULT_BLOB_THRESHOLD_BYTES)
}

/// True when `get_conversation` should leave stubs in place instead of
/// hydrating them (opt-in; see the module docs).
pub fn lazy_load_enabled() -> bool {
    Config::global()
        .get_param::<bool>("BIOROUTER_SESSION_BLOB_LAZY_LOAD")
        .unwrap_or(false)
}

/// Cheap probe used on the read path: does this stored `content_json` reference
/// any blob at all? A session that never externalized anything (the common
/// case) therefore never pays for a blob query.
pub fn content_json_has_stub(content_json: &str) -> bool {
    content_json.contains(MARKER_OPEN)
}

/// Rewrite a message's content for storage, lifting every oversized tool-response
/// text item into a blob.
///
/// Returns `None` — and clones nothing — when the message is under the
/// threshold, which is the overwhelmingly common case.
pub fn externalize(content: &[MessageContent]) -> Option<(Vec<MessageContent>, Vec<PendingBlob>)> {
    externalize_with(content, threshold_bytes())
}

/// [`externalize`] with an explicit threshold (the seam the tests drive).
pub fn externalize_with(
    content: &[MessageContent],
    threshold: usize,
) -> Option<(Vec<MessageContent>, Vec<PendingBlob>)> {
    if threshold == 0 || !content.iter().any(|c| has_oversized_text(c, threshold)) {
        return None;
    }

    let mut blobs = Vec::new();
    let rewritten = content
        .iter()
        .map(|item| match item {
            MessageContent::ToolResponse(response) if has_oversized_text(item, threshold) => {
                let mut response = response.clone();
                if let Ok(result) = response.tool_result.as_mut() {
                    externalize_result(result, threshold, &mut blobs);
                }
                MessageContent::ToolResponse(response)
            }
            other => other.clone(),
        })
        .collect();

    // `has_oversized_text` said yes, so this cannot be empty; guard anyway
    // rather than persist a stub whose blob was never recorded.
    if blobs.is_empty() {
        return None;
    }
    Some((rewritten, blobs))
}

fn externalize_result(result: &mut CallToolResult, threshold: usize, blobs: &mut Vec<PendingBlob>) {
    for item in &mut result.content {
        let Some(text) = item.as_text() else { continue };
        if text.text.len() <= threshold || blob_uid_of(&text.text).is_some() {
            continue;
        }
        let payload = text.text.clone();
        let uid = Uuid::now_v7().to_string();
        *item = Content::text(stub_text(&uid, &payload));
        blobs.push(PendingBlob {
            uid,
            content: payload,
        });
    }
}

fn has_oversized_text(item: &MessageContent, threshold: usize) -> bool {
    let MessageContent::ToolResponse(response) = item else {
        return false;
    };
    let Ok(result) = response.tool_result.as_ref() else {
        return false;
    };
    result.content.iter().any(|c| {
        c.as_text()
            // An already-stubbed item is never re-externalized: a
            // re-persisted conversation (compaction, diverge, copy) must not
            // grow a stub-of-a-stub.
            .is_some_and(|t| t.text.len() > threshold && blob_uid_of(&t.text).is_none())
    })
}

/// The inline replacement: a marker line the storage layer parses, then a
/// human/model-readable summary with a bounded head+tail preview and the handle
/// to pull the rest.
fn stub_text(uid: &str, payload: &str) -> String {
    let preview = truncate_middle_out(payload, DEFAULT_STUB_PREVIEW_CHARS);
    format!(
        "{MARKER_OPEN}{uid}{MARKER_CLOSE}\n\
         This tool result was large ({bytes} bytes, {lines} lines) and is stored outside the \
         conversation, so it does not consume context on every turn. The full output is kept \
         with this session under blob id `{uid}`.\n\
         Do NOT re-run the tool to see the rest — read it back with `platform__read_session_blob` \
         (`blob_id: \"{uid}\"`), optionally with `pattern` to grep it or `offset`/`limit` for a \
         line range.\n\
         \n--- preview: head and tail ---\n{preview}\n--- end of preview ---",
        bytes = payload.len(),
        lines = payload.lines().count(),
    )
}

/// The blob uid a stub refers to, or `None` when this text is not a stub.
///
/// The marker must open the text, keeping the check O(1) and making it
/// impossible for a marker quoted *inside* a payload to be mistaken for one.
pub fn blob_uid_of(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(MARKER_OPEN)?;
    let (uid, _) = rest.split_once(MARKER_CLOSE)?;
    // Only a well-formed uid we could have minted; a payload that happens to
    // start with the marker text cannot address someone else's blob.
    Uuid::parse_str(uid).ok()?;
    Some(uid)
}

/// Every blob uid the stored content of one message refers to. Drives both
/// hydration and the orphan sweep on a conversation rewrite.
pub fn referenced_uids(content: &[MessageContent]) -> Vec<String> {
    let mut uids = Vec::new();
    for item in content {
        let MessageContent::ToolResponse(response) = item else {
            continue;
        };
        let Ok(result) = response.tool_result.as_ref() else {
            continue;
        };
        for c in &result.content {
            if let Some(uid) = c.as_text().and_then(|t| blob_uid_of(&t.text)) {
                uids.push(uid.to_string());
            }
        }
    }
    uids
}

/// Put the payloads back: the exact inverse of [`externalize`]. A stub whose
/// blob is missing (a hand-edited DB, a partially restored backup) keeps its
/// stub text — which still carries the size summary and the preview — rather
/// than failing the whole session load.
///
/// Returns true when anything was hydrated.
pub fn hydrate(content: &mut [MessageContent], blobs: &HashMap<String, String>) -> bool {
    if blobs.is_empty() {
        return false;
    }
    let mut hydrated = false;
    for item in content.iter_mut() {
        let MessageContent::ToolResponse(response) = item else {
            continue;
        };
        let Ok(result) = response.tool_result.as_mut() else {
            continue;
        };
        for c in &mut result.content {
            let Some(payload) = c
                .as_text()
                .and_then(|t| blob_uid_of(&t.text))
                .and_then(|uid| blobs.get(uid))
            else {
                continue;
            };
            *c = Content::text(payload.clone());
            hydrated = true;
        }
    }
    hydrated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ToolResponse;
    use rmcp::model::{ErrorCode, ErrorData};

    fn tool_response(items: Vec<Content>) -> MessageContent {
        MessageContent::ToolResponse(ToolResponse {
            id: "call_1".to_string(),
            tool_result: Ok(CallToolResult {
                content: items,
                structured_content: None,
                is_error: Some(false),
                meta: None,
            }),
            metadata: None,
        })
    }

    fn big(marker: &str) -> String {
        (0..4_000)
            .map(|i| format!("{marker} line {i} of a very large tool result\n"))
            .collect()
    }

    #[test]
    fn small_tool_response_is_left_inline() {
        let content = vec![tool_response(vec![Content::text("all good")])];
        assert!(externalize_with(&content, 1024).is_none());
    }

    #[test]
    fn oversized_tool_text_becomes_a_stub_and_round_trips_exactly() {
        let payload = big("a");
        let content = vec![tool_response(vec![Content::text(payload.clone())])];

        let (stored, blobs) = externalize_with(&content, 1024).expect("should externalize");
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0].content, payload);

        // The stored row is tiny, self-describing, and names the handle.
        let stub = stored_text(&stored, 0);
        assert!(stub.len() < payload.len() / 10);
        assert!(stub.contains(&blobs[0].uid));
        assert!(stub.contains("platform__read_session_blob"));
        assert!(stub.contains("a line 0 of a very large tool result"));
        assert!(stub.contains("a line 3999 of a very large tool result"));
        assert_eq!(referenced_uids(&stored), vec![blobs[0].uid.clone()]);

        // ...and hydration restores the payload byte for byte.
        let map: HashMap<_, _> = blobs
            .iter()
            .map(|b| (b.uid.clone(), b.content.clone()))
            .collect();
        let mut restored = stored;
        assert!(hydrate(&mut restored, &map));
        assert_eq!(restored, content);
    }

    /// The `ToolResponse` wrapper (and its `id`) must survive: dropping it would
    /// orphan the paired `ToolRequest` and break every provider's tool protocol.
    #[test]
    fn the_tool_response_envelope_is_preserved() {
        let content = vec![tool_response(vec![Content::text(big("b"))])];
        let (stored, _) = externalize_with(&content, 1024).unwrap();
        let MessageContent::ToolResponse(response) = &stored[0] else {
            panic!("the tool response must stay a tool response");
        };
        assert_eq!(response.id, "call_1");
        assert_eq!(response.tool_result.as_ref().unwrap().content.len(), 1);
    }

    #[test]
    fn each_oversized_item_gets_its_own_blob_and_small_ones_stay_inline() {
        let content = vec![tool_response(vec![
            Content::text("summary line"),
            Content::text(big("c")),
            Content::text(big("d")),
        ])];
        let (stored, blobs) = externalize_with(&content, 1024).unwrap();
        assert_eq!(blobs.len(), 2);
        assert_eq!(stored_text(&stored, 0), "summary line");

        let map: HashMap<_, _> = blobs
            .iter()
            .map(|b| (b.uid.clone(), b.content.clone()))
            .collect();
        let mut restored = stored;
        hydrate(&mut restored, &map);
        assert_eq!(restored, content);
    }

    /// Re-persisting an already-stubbed conversation (compaction, diverge, copy
    /// all DELETE + re-INSERT) must keep the same handle, not wrap the stub in
    /// another stub.
    #[test]
    fn a_stub_is_not_re_externalized() {
        let content = vec![tool_response(vec![Content::text(big("e"))])];
        let (stored, blobs) = externalize_with(&content, 1024).unwrap();
        assert!(externalize_with(&stored, 1024).is_none());
        assert_eq!(referenced_uids(&stored), vec![blobs[0].uid.clone()]);
    }

    #[test]
    fn user_text_and_images_are_never_externalized() {
        let content = vec![
            MessageContent::text(big("f")),
            MessageContent::image(big("g"), "image/png"),
        ];
        assert!(externalize_with(&content, 1024).is_none());
    }

    #[test]
    fn an_errored_tool_response_is_left_alone() {
        let content = vec![MessageContent::ToolResponse(ToolResponse {
            id: "call_2".to_string(),
            tool_result: Err(ErrorData::new(ErrorCode::INTERNAL_ERROR, big("h"), None)),
            metadata: None,
        })];
        assert!(externalize_with(&content, 1024).is_none());
    }

    #[test]
    fn a_zero_threshold_disables_externalization() {
        let content = vec![tool_response(vec![Content::text(big("i"))])];
        assert!(externalize_with(&content, 0).is_none());
    }

    /// A payload that merely *quotes* the marker cannot address a blob: the uid
    /// must parse, and the marker must open the text.
    #[test]
    fn a_forged_marker_does_not_resolve_to_a_blob() {
        assert!(blob_uid_of("<<<biorouter-session-blob:not-a-uuid>>>\nhi").is_none());
        assert!(blob_uid_of("preamble <<<biorouter-session-blob:0192.>>>").is_none());

        let real = Uuid::now_v7().to_string();
        let forged = format!("some output mentioning {MARKER_OPEN}{real}{MARKER_CLOSE}");
        let content = vec![tool_response(vec![Content::text(forged.clone())])];
        let mut restored = content.clone();
        let map = HashMap::from([(real, "SECRET".to_string())]);
        assert!(!hydrate(&mut restored, &map));
        assert_eq!(restored, content);
    }

    #[test]
    fn a_missing_blob_leaves_the_stub_in_place_instead_of_failing() {
        let content = vec![tool_response(vec![Content::text(big("j"))])];
        let (stored, _) = externalize_with(&content, 1024).unwrap();
        let mut restored = stored.clone();
        assert!(!hydrate(&mut restored, &HashMap::new()));
        assert_eq!(restored, stored);
    }

    #[test]
    fn content_json_probe_only_fires_on_a_real_stub() {
        let content = vec![tool_response(vec![Content::text(big("k"))])];
        let (stored, _) = externalize_with(&content, 1024).unwrap();
        let json = serde_json::to_string(&stored).unwrap();
        assert!(content_json_has_stub(&json));
        assert!(!content_json_has_stub(
            &serde_json::to_string(&content).unwrap()
        ));
    }

    fn stored_text(content: &[MessageContent], idx: usize) -> String {
        let MessageContent::ToolResponse(response) = &content[0] else {
            panic!("expected a tool response");
        };
        response.tool_result.as_ref().unwrap().content[idx]
            .as_text()
            .unwrap()
            .text
            .clone()
    }
}
