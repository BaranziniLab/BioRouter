//! BR-71: the `workspace` platform extension — the agent's tool surface over
//! the daemon's sessions and (when attached) the GUI's tabs. Design of record:
//! docs/agent-loop/designs/agent-workspace-control.md. Registered
//! `default_enabled: false`; enabling is an explicit user decision (§5).

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
// `EnabledExtensionsState::from_extension_data` is a PROVIDED METHOD of the
// `ExtensionState` trait (`session/extension_data.rs:66-71`), not an inherent
// one — the trait must be in scope or the call in `handle_list` is E0599.
use crate::session::{EnabledExtensionsState, ExtensionState};
use crate::workspace_services;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool, ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

/// The machine **identifier**, which **must** normalize to this extension's
/// `PLATFORM_EXTENSIONS` registry key (`"workspace"`). The human-readable label
/// is [`EXTENSION_TITLE`] — the two are deliberately separate.
///
/// `normalize` (`extension_manager.rs:159`) strips whitespace and lowercases,
/// and two already-landed pieces of the system depend on the result:
///
/// * the platform spawn path looks the def up by `normalize(config.name)`
///   (`extension_manager.rs:808-817`), and `/ext:` resolution builds that config
///   from *this* string (`resolve_bundled_extension` → `into_config`), so a name
///   that normalizes to anything else makes `/ext:workspace` fail with
///   "Unknown platform extension" — the exact issue-#48 failure the pre-existing
///   test `resolves_every_bundled_extension_to_its_owning_registry` pins;
/// * the advertised tool prefix is `{normalize(name)}__{tool}`, and
///   `workspace_inspector::is_set_tools_call` already pins
///   `workspace__workspace_set_tools`.
///
/// The design's longer label "Workspace Control" therefore cannot be the *name*
/// (it normalizes to `workspacecontrol`); it is carried by [`EXTENSION_TITLE`].
pub static EXTENSION_NAME: &str = "Workspace";

/// The human-readable label, exactly as the design of record specifies it
/// (`docs/agent-loop/designs/agent-workspace-control.md` §4.1: "display name
/// **Workspace Control**").
///
/// This is what MCP's `Implementation.title` carries — the field whose whole
/// purpose is "a human-readable name, for UI, when `name` is a programmatic
/// identifier". Deriving it from [`EXTENSION_NAME`] instead would silently
/// downgrade the specified display name to the normalization-constrained
/// identifier, which is the one thing the identifier may not also be.
pub static EXTENSION_TITLE: &str = "Workspace Control";

/// §6 draft instruction block, kept within the ~2.5k-char injection budget
/// (`apply_injection_budget`, prompt_manager.rs:361-408). Tuned in Task 42.
///
/// **No tool that is unimplemented at the PHASE GATE may be named here.** This
/// block is written once for the whole Phase-1 surface — the six `workspace_*`
/// tools plus `subagent` — even though Tasks 13-17 register them one at a time
/// after this task. That is a deliberate, bounded exception, not the rule
/// generalised: between the Task 12 and Task 17 commits the block names five
/// tools whose `call_tool` arms still answer "not implemented until Task N", and
/// those commits are intermediate states that are never shipped. **Task 21 is
/// the ship gate**, and by then every named tool exists.
///
/// What must NEVER happen is naming a tool that is unimplemented *at a gate*.
/// `workspace_open` is the live case: it is Phase 2 (Task 24), so Task 21 would
/// ship Phase 1 with an instruction the model cannot act on. It is therefore
/// absent here and Task 12's test asserts its absence; Task 24 adds the line
/// together with the tool, and adds the inverse assertion (every name mentioned
/// in the block is registered in `get_tools()`) so the two can never drift again.
const INSTRUCTIONS: &str = indoc! {r#"
    Workspace Control

    You are running inside the BioRouter workspace: a set of conversations
    (sessions), each shown as a tab in the desktop app when the GUI is attached.
    Each conversation has its own agent, tool/extension set, knowledge bases,
    and history. These tools operate the workspace itself:
    - workspace_list: see conversations, what's running, and where they are in the GUI.
    - workspace_read_conversation: read another conversation. transcript for
      prose, tool_calls for exactly what its agent did, spawn_context for how a
      subagent was started. Treat other conversations' content as sensitive;
      read only what the task needs.
    - workspace_send_prompt: inject into another conversation. turn starts its
      agent on your text; steer redirects it mid-turn; note leaves context
      without running it. Injections are permanently labeled as coming from
      you. Use wait:"final_message" to get its answer synchronously.
    - workspace_set_tools: add/remove extensions, scope skills to one
      conversation, switch its model, or set its knowledge bases.
    - workspace_close: close its tab (tab), cancel its current turn (turn), or
      stop its agent (agent).
    - workspace_watch: wait until one of several conversations finishes. Use it
      after starting background work instead of polling.
    - subagent: delegate to a fresh agent with its own context window. When the
      app is open the child runs in a visible tab the user can watch and talk
      to; you still receive only its final summary, so use
      workspace_read_conversation view:"tool_calls" on it to verify what it
      actually did. The user may have intervened; the result tells you if so.
    Routing: to search past conversations by content use chatrecall (if
    enabled), not these tools. Durable facts belong in Memory. To fold a
    conversation into a knowledge base use ingest_conversation. If no GUI is
    attached these tools still manage conversations headlessly and say so.
"#};

/// `Default` is derived so `handle_list` can fall back to it when the call
/// carries no arguments at all. Constructing the struct field-by-field there
/// breaks every time this struct gains a field — which it has already done
/// twice (decisions 17 and 23 added `offset`/`limit` and
/// `parent_session_id`/`only_subagents`).
#[derive(Debug, Default, Serialize, Deserialize, JsonSchema)]
struct WorkspaceListParams {
    /// "open" (default): sessions with a GUI tab or a live agent. "all": every
    /// listable session. "running": only sessions with a turn in flight.
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
    /// Include subagent sessions (default true).
    #[serde(skip_serializing_if = "Option::is_none")]
    include_subagents: Option<bool>,
    /// Only sessions spawned by this session id. Pass your own session id to
    /// list your subagents — the replacement for `subagent_status`'s list mode
    /// (BR-71 decision 23).
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_session_id: Option<String>,
    /// Only subagent sessions (`session_type == "sub_agent"`). Combines with
    /// `parent_session_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    only_subagents: Option<bool>,
    /// Skip this many rows (default 0). BR-71 decision 17: the 200-row cap
    /// alone was rejected, so the tool pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<u32>,
    /// Return at most this many rows (default 50, max 200).
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WorkspaceReadParams {
    session_id: String,
    /// "transcript" (default) | "tool_calls" | "summary" | "spawn_context".
    #[serde(skip_serializing_if = "Option::is_none")]
    view: Option<String>,
    /// Only the last N messages (transcript/tool_calls views).
    #[serde(skip_serializing_if = "Option::is_none")]
    last: Option<usize>,
    /// Start from the message with this durable msg_uid (BR-45 identity;
    /// design §4.1 `range: { from_msg_uid }`). Combines with `last` (uid slice
    /// first, then tail).
    #[serde(skip_serializing_if = "Option::is_none")]
    from_msg_uid: Option<String>,
    /// Cap on returned characters (default 20000, max 200000). Oversized
    /// results above the BR-7 blob threshold are externalized by the caller's
    /// own persist path — see the note in the handler — never silently
    /// truncated at a raised cap.
    #[serde(skip_serializing_if = "Option::is_none")]
    max_chars: Option<usize>,
}

/// The tools [`INSTRUCTIONS`] names whose handler is still a placeholder, each
/// with the task that lands it.
///
/// A data table rather than one match arm per tool so the invariant is
/// *checkable*: `get_tools()` must advertise none of these (a tool the model can
/// see but not use is worse than one it cannot see), and every `workspace_*` name
/// in the instruction block must be either advertised or listed here. Both are
/// asserted in `advertises_no_tool_whose_handler_is_still_a_placeholder`.
///
/// The task that implements a tool deletes its row here — it must, or its own
/// dispatch arm is shadowed by nothing and the surface test fails — and adds it
/// to `get_tools()` in the same commit.
const PENDING_TOOLS: &[(&str, &str)] = &[
    ("workspace_send_prompt", "Task 14"),
    ("workspace_set_tools", "Task 15"),
    ("workspace_close", "Task 16"),
    ("workspace_watch", "Task 17"),
    ("workspace_open", "Task 24"),
];

pub struct WorkspaceClient {
    info: InitializeResult,
    pub(crate) context: PlatformExtensionContext,
}

impl WorkspaceClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities {
                tasks: None,
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: None,
                prompts: None,
                completions: None,
                experimental: None,
                logging: None,
            },
            server_info: Implementation {
                name: EXTENSION_NAME.to_string(),
                title: Some(EXTENSION_TITLE.to_string()),
                version: "1.0.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(INSTRUCTIONS.to_string()),
        };
        Ok(Self { info, context })
    }

    fn tool(name: &str, description: &str, schema: serde_json::Value, read_only: bool) -> Tool {
        let input_schema = schema.as_object().expect("schema is an object").clone();
        Tool::new(name.to_string(), description.to_string(), input_schema).annotate(
            ToolAnnotations {
                title: Some(name.replace('_', " ")),
                read_only_hint: Some(read_only),
                destructive_hint: Some(!read_only),
                idempotent_hint: Some(read_only),
                open_world_hint: Some(false),
            },
        )
    }

    fn get_tools() -> Vec<Tool> {
        vec![
            Self::tool(
                "workspace_list",
                "List conversations in the workspace: id, name, type, running \
                 state, parent, enabled extensions, active knowledge base, and \
                 GUI tab placement when a GUI is attached.",
                serde_json::to_value(schema_for!(WorkspaceListParams)).unwrap(),
                true,
            ),
            Self::tool(
                "workspace_read_conversation",
                "Structured read of any conversation. view: transcript (prose), \
                 tool_calls (exactly what its agent did), summary (head/tail), \
                 spawn_context (how a subagent was started). Refuses hidden sessions.",
                serde_json::to_value(schema_for!(WorkspaceReadParams)).unwrap(),
                true,
            ),
            // Tasks 14-17 and 19/24 append:
            // workspace_send_prompt, workspace_set_tools, workspace_close,
            // workspace_watch, workspace_open, and `subagent` (advertised only;
            // the spawn dispatch lives in agent.rs — see Task 19).
        ]
    }

    async fn handle_list(
        &self,
        _caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceListParams = match arguments {
            Some(a) => serde_json::from_value(serde_json::Value::Object(a))
                .map_err(|e| format!("invalid arguments: {e}"))?,
            // Derived, not written out: a field-by-field literal here is a
            // compile error every time the params struct grows.
            None => WorkspaceListParams::default(),
        };
        let scope = args.scope.as_deref().unwrap_or("open");
        let include_subagents = args.include_subagents.unwrap_or(true);
        // Decision 17: real paging, not a silent 200-row truncation. The page is
        // applied AFTER scope filtering, so `offset` walks the rows the model
        // actually sees.
        let offset = args.offset.unwrap_or(0) as usize;
        let limit = (args.limit.unwrap_or(50) as usize).clamp(1, 200);

        let services = workspace_services::get();
        let gui_attached = services.as_ref().is_some_and(|s| s.gui_attached());
        let layout = services.as_ref().and_then(|s| s.layout_snapshot());

        // SCAN the store in chunks rather than reading one fixed window.
        //
        // Decision 17 rejected a silent cap, and a single
        // `list_session_summaries(1000, 0, …)` reintroduces one a decimal place
        // higher: `total_matching` and `has_more` would be computed over at most
        // 1000 rows, so `offset >= 1000` returns nothing and the paging metadata
        // — the whole point of the decision — lies on any workspace with more
        // sessions than that. Paging the STORAGE query directly is not
        // equivalent either: scope filtering happens here, so a storage page
        // yields short, ragged tool pages.
        //
        // So: walk the store `SCAN_CHUNK` rows at a time, filter, and stop at
        // `MAX_SCAN_ROWS`. If the ceiling is ever hit the payload says so
        // explicitly (`scan_truncated`) instead of quietly under-reporting.
        const SCAN_CHUNK: u32 = 500;
        const MAX_SCAN_ROWS: usize = 20_000;

        // NOTE for the unit tests: `AgentManager::instance()` resolves
        // `Paths::data_dir()` and the process-global `SessionManager::instance()`,
        // and its first initialization runs `run_first_run_init` (seeds built-in
        // skills, installs the Soul KB + a 3 AM schedule). Run every test that
        // reaches this handler under a sandboxed `BIOROUTER_PATH_ROOT` (or
        // `XDG_CONFIG_HOME`) so it cannot touch the developer's real
        // `~/.config/biorouter`. The same caveat applies to Tasks 14, 15 and 17.
        let agent_manager = crate::execution::manager::AgentManager::instance()
            .await
            .map_err(|e| format!("agent manager unavailable: {e}"))?;

        let mut rows = Vec::new();
        let mut matched = 0usize;
        let mut scanned = 0usize;
        let mut scan_truncated = false;
        let mut db_offset: u32 = 0;
        'scan: loop {
            let summaries = self
                .context
                .session_manager
                // `include_empty: true` (Task 4): the sidebar's INNER JOIN on
                // `messages` hides a session that has none, and `workspace_open`
                // (Task 24) creates exactly that — a session with a working dir
                // and no message yet. `workspace_list` must be able to see it.
                .list_session_summaries(SCAN_CHUNK, db_offset, include_subagents, true)
                .await
                .map_err(|e| format!("failed to list sessions: {e}"))?;
            if summaries.is_empty() {
                break;
            }
            db_offset += summaries.len() as u32;
            for s in summaries {
                scanned += 1;
                if scanned > MAX_SCAN_ROWS {
                    scan_truncated = true;
                    break 'scan;
                }
                let running = services
                    .as_ref()
                    .is_some_and(|svc| svc.is_turn_active(&s.id));
                let live = agent_manager.has_session(&s.id).await;
                let gui_placement = gui_tab_for(layout.as_ref(), &s.id);
                let in_scope = match scope {
                    "running" => running,
                    "all" => true,
                    // "open": a conversation is open if it has a live agent, a
                    // turn in flight, or a GUI tab. `running` is load-bearing
                    // here, not redundant: a glass-box subagent is registered in
                    // `AgentManager`'s PINNED sidecar (Task 33), never in the
                    // `sessions` LRU that `has_session` reads, and a background
                    // child holds no GUI tab — so without it every running
                    // subagent is invisible in the DEFAULT scope, which is the
                    // scope decision 23's migration note tells prompts to use.
                    _ /* "open" */ => live || running || gui_placement.is_some(),
                };
                // Decision 23: these two filters are what `subagent_status`'s
                // list mode becomes. `parent_session_id` answers "my
                // subagents"; `only_subagents` answers "every delegation in the
                // workspace".
                let parent_matches = args
                    .parent_session_id
                    .as_deref()
                    .is_none_or(|want| s.parent_session_id.as_deref() == Some(want));
                let type_matches = !args.only_subagents.unwrap_or(false)
                    || s.session_type.as_deref() == Some("sub_agent");
                if !in_scope || !parent_matches || !type_matches {
                    continue;
                }
                matched += 1;
                if matched <= offset || rows.len() >= limit {
                    // Still counted in `matched` — that is what makes
                    // `total_matching` / `has_more` honest rather than
                    // page-local.
                    continue;
                }
                // §4.1 required row fields: enabled extension names + active
                // KBs. Read per INCLUDED row only (the summary row has no
                // extension_data), exactly the GET /sessions/{id}/extensions
                // fallback logic — `get_session_extensions`, whose two lines are
                // `EnabledExtensionsState::from_extension_data(&session.extension_data)`
                // `.unwrap_or_else(biorouter::config::get_enabled_extensions)`.
                // Best-effort: a read failure yields an empty list, never fails
                // the listing.
                let extensions: Vec<String> =
                    match self.context.session_manager.get_session(&s.id, false).await {
                        Ok(full) => {
                            EnabledExtensionsState::from_extension_data(&full.extension_data)
                                .map(|st| st.extensions.iter().map(|e| e.name()).collect())
                                .unwrap_or_else(|| {
                                    // No session-specific state → global config, the
                                    // exact fallback GET /sessions/{id}/extensions
                                    // performs (`from_extension_data` returns Option).
                                    crate::config::get_enabled_extensions()
                                        .iter()
                                        .map(|e| e.name())
                                        .collect()
                                })
                        }
                        Err(_) => Vec::new(),
                    };
                // Post-#45: ONE call returning set + write target together
                // (Task 9). `primary_kb` is on the row because a model that can
                // SET a write target and cannot READ it back will thrash — it
                // has no way to tell "already correct" from "not applied".
                let kbs = services
                    .as_ref()
                    .map(|svc| svc.knowledge_selection(&s.id))
                    .unwrap_or_default();
                rows.push(json!({
                    "session_id": s.id,
                    "name": s.name,
                    "session_type": s.session_type,
                    "working_dir": s.working_dir,
                    "running": running,
                    "parent_session_id": s.parent_session_id,
                    "extensions": extensions,
                    "knowledge_bases": kbs.kb_ids,
                    // `null` means "no write target chosen", which is a real and
                    // distinct state from "no knowledge bases" — a session can
                    // have several bases and no primary, and a KB-less write then
                    // fails. Do not collapse it to the first id here; the service
                    // owns promotion (`repair_primary_unlocked`).
                    "primary_kb": kbs.primary_kb,
                    "gui": gui_placement,
                }));
            }
        }

        let mut payload = json!({
            "gui_attached": gui_attached,
            "scope": scope,
            // Paging metadata, so the model can walk the list instead of
            // guessing whether it saw everything (decision 17).
            "offset": offset,
            "limit": limit,
            "returned": rows.len(),
            "total_matching": matched,
            "has_more": matched > offset + rows.len(),
            "sessions": rows,
        });
        if scan_truncated {
            // The one case where `total_matching` is a lower bound. Say so in
            // the payload rather than letting the model believe a floor is a
            // total — the failure decision 17 exists to prevent.
            payload["scan_truncated"] = json!(true);
            payload["scanned"] = json!(MAX_SCAN_ROWS);
            payload["note"] = json!(format!(
                "Stopped after scanning {MAX_SCAN_ROWS} conversations; \
                 total_matching is a lower bound. Narrow the query with \
                 scope, parent_session_id or only_subagents."
            ));
        }
        Ok(vec![Content::text(
            serde_json::to_string_pretty(&payload).unwrap(),
        )])
    }

    async fn handle_read_conversation(
        &self,
        caller_session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args: WorkspaceReadParams = parse_args(arguments)?;
        let view = args.view.as_deref().unwrap_or("transcript");
        let max_chars = args.max_chars.unwrap_or(20_000).min(200_000);

        let session = self
            .context
            .session_manager
            .get_session(&args.session_id, true)
            .await
            .map_err(|e| format!("failed to load session: {e}"))?;

        // §5 "no covert reads": Hidden sessions honor the same visibility rules
        // as the session list. The read itself is auditable — it IS a tool call
        // in the caller's transcript.
        if session.session_type == crate::session::session_manager::SessionType::Hidden {
            return Err("this session is hidden and cannot be read".to_string());
        }
        tracing::info!(
            caller = caller_session_id,
            target = %args.session_id,
            view,
            "workspace cross-session read"
        );

        let messages: Vec<_> = session
            .conversation
            .as_ref()
            .map(|c| c.messages().to_vec())
            .unwrap_or_default();
        // BR-45 range: slice from the named msg_uid (message ids ARE the durable
        // uids — #41 add_message_adopting_uid), then apply `last` as a tail.
        let from_start = match &args.from_msg_uid {
            Some(uid) => messages
                .iter()
                .position(|m| m.id.as_deref() == Some(uid.as_str()))
                .ok_or_else(|| format!("no message with msg_uid '{uid}' in this session"))?,
            None => 0,
        };
        let ranged = &messages[from_start..];
        let tail = |n: Option<usize>| -> &[crate::conversation::message::Message] {
            match n {
                Some(n) if n < ranged.len() => &ranged[ranged.len() - n..],
                _ => ranged,
            }
        };

        let body = match view {
            "tool_calls" => project_tool_calls(tail(args.last)),
            "summary" => project_summary(&session, &messages),
            "spawn_context" => project_spawn_context(&messages)
                .ok_or("this session has no recorded spawn context")?,
            _ => project_transcript(tail(args.last)),
        };

        // Oversized-result handling (§4.1 "session-blob mechanism, never silent
        // truncation"): this tool RESULT is persisted into the CALLER's session,
        // where BR-7's externalization (`message_blobs::externalize`; the
        // threshold is `DEFAULT_BLOB_THRESHOLD_BYTES` in message_blobs.rs)
        // stores payloads above the blob threshold as session blobs readable via
        // platform__read_session_blob — so a raised max_chars round-trips intact
        // instead of bloating context. The tool-level cap is model-facing
        // pagination; when it clips, the marker names the narrowing controls
        // rather than dropping data silently.
        let clipped = if body.chars().count() > max_chars {
            let cut: String = body.chars().take(max_chars).collect();
            format!(
                "{cut}\n… [clipped at {max_chars} chars — narrow with `last` or \
                 `from_msg_uid`, or raise `max_chars` (up to 200000; oversized \
                 results are stored as a session blob, not lost)]"
            )
        } else {
            body
        };
        Ok(vec![Content::text(format!(
            "Session {} ({}, {:?})\n\n{}",
            session.id, session.name, session.session_type, clipped
        ))])
    }
}

fn parse_args<T: serde::de::DeserializeOwned>(arguments: Option<JsonObject>) -> Result<T, String> {
    let args = arguments.ok_or("Missing arguments")?;
    serde_json::from_value(serde_json::Value::Object(args))
        .map_err(|e| format!("invalid arguments: {e}"))
}

/// The `tool_calls` projection (§4.1): ToolRequest/ToolResponse pairs only,
/// correlated by their shared id — "what did that agent actually do".
fn project_tool_calls(messages: &[crate::conversation::message::Message]) -> String {
    use crate::conversation::message::MessageContent;
    let mut out = String::new();
    for message in messages {
        for content in &message.content {
            match content {
                MessageContent::ToolRequest(req) => {
                    out.push_str(&format!("→ [{}] {}\n", req.id, req.to_readable_string()));
                }
                MessageContent::ToolResponse(resp) => {
                    let digest = match &resp.tool_result {
                        Ok(result) => {
                            let text: String = result
                                .content
                                .iter()
                                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                                .collect::<Vec<_>>()
                                .join(" ");
                            let short: String = text.chars().take(400).collect();
                            format!("ok: {short}")
                        }
                        Err(e) => format!("error: {e}"),
                    };
                    out.push_str(&format!("← [{}] {digest}\n", resp.id));
                }
                _ => {}
            }
        }
    }
    if out.is_empty() {
        "No tool calls in range.".to_string()
    } else {
        out
    }
}

fn project_transcript(messages: &[crate::conversation::message::Message]) -> String {
    use crate::conversation::message::MessageContent;
    let mut out = String::new();
    for message in messages {
        // Tab-invisible bookkeeping rows (agent_only) are skipped; tool
        // payloads collapse to one-line stubs (§4.1).
        if !message.metadata.user_visible {
            continue;
        }
        out.push_str(&format!("[{:?}] ", message.role));
        if let Some(p) = &message.metadata.provenance {
            out.push_str(&format!("(injected: {:?}) ", p.kind));
        }
        for content in &message.content {
            match content {
                MessageContent::ToolRequest(req) => {
                    out.push_str(&format!("<tool call: {}>", req.to_readable_string()));
                }
                MessageContent::ToolResponse(resp) => {
                    out.push_str(&format!("<tool result: {}>", resp.id));
                }
                other => {
                    if let Some(text) = other.as_text() {
                        out.push_str(text);
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

fn project_summary(
    session: &crate::session::Session,
    messages: &[crate::conversation::message::Message],
) -> String {
    // chatrecall-load parity (§3.2): head 3 + tail 3, via the same data.
    let head = project_transcript(&messages[..messages.len().min(3)]);
    let tail_start = messages.len().saturating_sub(3).max(messages.len().min(3));
    let tail = project_transcript(&messages[tail_start..]);
    format!(
        "Working dir: {}\nMessages: {}\n\n--- First ---\n{head}\n--- Last ---\n{tail}",
        session.working_dir.display(),
        messages.len()
    )
}

fn project_spawn_context(messages: &[crate::conversation::message::Message]) -> Option<String> {
    use crate::conversation::message::ProvenanceKind;
    messages.iter().find_map(|m| {
        let p = m.metadata.provenance.as_ref()?;
        (p.kind == ProvenanceKind::SpawnContext).then(|| {
            m.content
                .iter()
                .filter_map(|c| c.as_text())
                .collect::<Vec<_>>()
                .join("\n")
        })
    })
}

/// Find `session_id` inside a layout echo (§4.3 `workspace_echo.layout`).
fn gui_tab_for(layout: Option<&serde_json::Value>, session_id: &str) -> Option<serde_json::Value> {
    let windows = layout?.as_array()?;
    for window in windows {
        let window_id = window.get("window_id")?.as_str().unwrap_or_default();
        for group in window.get("layout")?.as_array()? {
            for tab in group.get("tabs")?.as_array()? {
                if tab.get("session_id")?.as_str() == Some(session_id) {
                    return Some(json!({
                        "window_id": window_id,
                        "group_id": group.get("group_id"),
                        "tab_id": tab.get("tab_id"),
                        "focused": group.get("active_tab") == tab.get("tab_id"),
                    }));
                }
            }
        }
    }
    None
}

#[async_trait]
impl McpClientTrait for WorkspaceClient {
    async fn list_tools(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        meta: McpMeta,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let caller = &meta.session_id;
        let content = match name {
            "workspace_list" => self.handle_list(caller, arguments).await,
            "workspace_read_conversation" => self.handle_read_conversation(caller, arguments).await,
            // BR-71 decision 22: the spawn tool is advertised here but
            // dispatched by the agent loop (it needs the parent's TaskConfig).
            // Reachable only if that interception is ever removed.
            crate::agents::subagent_tool::SUBAGENT_TOOL_NAME => {
                Err("`subagent` is dispatched by the agent loop, not by this extension".to_string())
            }
            _ => match PENDING_TOOLS.iter().find(|(tool, _)| *tool == name) {
                Some((_, task)) => Err(format!("not implemented until {task}")),
                None => Err(format!("Unknown tool: {name}")),
            },
        };
        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {error}"
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension::PlatformExtensionContext;
    use crate::agents::mcp_client::McpClientTrait;
    use tokio_util::sync::CancellationToken;

    fn client() -> WorkspaceClient {
        let temp = tempfile::TempDir::new().unwrap();
        let session_manager = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        // Leak the tempdir for the test's lifetime so the DB stays alive.
        std::mem::forget(temp);
        WorkspaceClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager,
        })
        .unwrap()
    }

    fn test_meta() -> crate::agents::mcp_client::McpMeta {
        crate::agents::mcp_client::McpMeta::new("caller")
    }

    /// Call `workspace_list` and return the parsed payload.
    ///
    /// Every assertion below reads this rather than substring-matching the
    /// pretty-printed text: `text.contains("\"extensions\"")` is satisfied by an
    /// empty array, by the field appearing on the wrong object, and by the word
    /// turning up anywhere at all in a session name — so it pins nothing.
    async fn list(c: &WorkspaceClient, args: serde_json::Value) -> serde_json::Value {
        let args: rmcp::model::JsonObject = serde_json::from_value(args).unwrap();
        let result = c
            .call_tool(
                "workspace_list",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.clone();
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
    }

    /// The returned session ids, sorted so the assertion does not depend on the
    /// store's row order.
    fn sorted_ids(payload: &serde_json::Value) -> Vec<String> {
        let mut ids: Vec<String> = payload["sessions"]
            .as_array()
            .expect("sessions is an array")
            .iter()
            .map(|r| {
                r["session_id"]
                    .as_str()
                    .expect("row has a string id")
                    .to_string()
            })
            .collect();
        ids.sort();
        ids
    }

    fn sorted<S: AsRef<str>>(ids: Vec<S>) -> Vec<String> {
        let mut ids: Vec<String> = ids.into_iter().map(|s| s.as_ref().to_string()).collect();
        ids.sort();
        ids
    }

    /// The machine identifier and the human label are two different strings and
    /// must stay that way.
    ///
    /// `EXTENSION_NAME` is an IDENTIFIER: it is normalized into the
    /// `PLATFORM_EXTENSIONS` key by the platform spawn path
    /// (`extension_manager.rs`, `normalize(config.name)`) and into the advertised
    /// tool prefix `workspace__…`. `EXTENSION_TITLE` is the DISPLAY name the
    /// design of record specifies (agent-workspace-control.md §4.1, "display
    /// name **Workspace Control**") and is what the MCP `Implementation.title`
    /// carries. Collapsing the two — either way round — breaks one of them:
    /// "Workspace Control" normalizes to `workspacecontrol`, and "Workspace"
    /// is not the design's label.
    // `client()` builds a `SessionManager`, whose sqlx pool requires a Tokio
    // context even though nothing here awaits.
    #[tokio::test]
    async fn machine_name_normalizes_to_the_key_and_title_is_the_design_label() {
        assert_eq!(crate::agents::normalize(EXTENSION_NAME), "workspace");
        assert_eq!(EXTENSION_TITLE, "Workspace Control");
        let c = client();
        let info = c.get_info().unwrap();
        assert_eq!(info.server_info.name, EXTENSION_NAME);
        assert_eq!(info.server_info.title.as_deref(), Some(EXTENSION_TITLE));
    }

    /// The advertised surface and the placeholder surface must be disjoint, and
    /// together they must cover every tool [`INSTRUCTIONS`] names.
    ///
    /// This is the discriminating half of the surface check that
    /// `advertises_workspace_list_with_instructions` deliberately cannot be: that
    /// test asserts MEMBERSHIP (`contains`) because Tasks 13-17 each append one
    /// tool and re-run it expecting PASS, so a whole-vector equality there would
    /// be a fail-again-six-times gate (the plan puts its one exact-surface
    /// assertion in Task 24, the last task that changes `get_tools()`). But
    /// membership alone lets a premature tool — one whose handler still answers
    /// "not implemented until Task N" — be advertised without any test noticing,
    /// which is exactly the failure the phase-gate rule exists to prevent.
    ///
    /// The invariant below closes that hole without ever going stale: a task that
    /// implements a tool deletes its [`PENDING_TOOLS`] row (it must, or its own
    /// dispatch is unreachable) and adds it to `get_tools()`, and the two halves
    /// stay disjoint and complete by construction.
    #[tokio::test]
    async fn advertises_no_tool_whose_handler_is_still_a_placeholder() {
        let c = client();
        let advertised: Vec<String> = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools
            .iter()
            .map(|t| t.name.to_string())
            .collect();

        for (pending, task) in PENDING_TOOLS {
            assert!(
                !advertised.contains(&pending.to_string()),
                "{pending} is advertised but its handler answers \
                 'not implemented until {task}'"
            );
        }

        // Every `workspace_*` tool the instruction block tells the model about is
        // either live or explicitly pending — never a name nothing handles.
        let mentioned: std::collections::BTreeSet<String> = INSTRUCTIONS
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .filter(|tok| tok.starts_with("workspace_"))
            .map(|tok| tok.to_string())
            .collect();
        assert!(
            mentioned.contains("workspace_list"),
            "the token scan found nothing; got: {mentioned:?}"
        );
        for name in &mentioned {
            assert!(
                advertised.contains(name)
                    || PENDING_TOOLS.iter().any(|(pending, _)| pending == name),
                "the instructions name {name}, which is neither advertised nor \
                 listed in PENDING_TOOLS"
            );
        }
    }

    /// This task registers exactly ONE tool; Tasks 13-17 append the rest.
    ///
    /// **This assertion is deliberately ADDITIVE (`contains`), not exact
    /// (`assert_eq!` on the whole vector).** Six later tasks each append one
    /// entry to `get_tools()` — 13, 14, 15, 16, 17 and 18 — and every one of
    /// them re-runs this test under the filter
    /// `--lib agents::workspace_extension` with "Expected: PASS". A
    /// whole-vector equality here would therefore be a fail-again-six-times
    /// gate. Exactly ONE exact-surface assertion exists in the plan, and it
    /// lives in the LAST task that changes the surface (Task 24) so it can
    /// never go stale mid-phase.
    #[tokio::test]
    async fn advertises_workspace_list_with_instructions() {
        let c = client();
        let tools = c
            .list_tools(None, CancellationToken::new())
            .await
            .unwrap()
            .tools;
        let names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
        assert!(
            names.contains(&"workspace_list".to_string()),
            "got: {names:?}"
        );

        let info = c.get_info().unwrap();
        let instructions = info.instructions.as_deref().unwrap();
        assert!(instructions.contains("chatrecall"));
        assert!(instructions.len() <= 2500, "injection budget (§6)");
        // No tool that is unimplemented AT A PHASE GATE may be named. The block
        // is written once for the whole Phase-1 surface (see its doc comment),
        // but `workspace_open` is Phase 2 — Task 21 would otherwise ship Phase 1
        // telling the model to call a tool that answers "not implemented".
        assert!(
            !instructions.contains("workspace_open"),
            "workspace_open is not advertised until Task 24"
        );
    }

    #[tokio::test]
    async fn workspace_list_reports_headless_and_sessions() {
        let c = client();
        let parent = c
            .context
            .session_manager
            .create_session(
                std::env::temp_dir(),
                "listed".to_string(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        let v = list(&c, serde_json::json!({ "scope": "all" })).await;

        // Envelope: no GUI is attached in a unit test, and the paging metadata
        // (decision 17) is always present and honest, not merely present.
        assert_eq!(v["gui_attached"], serde_json::json!(false));
        assert_eq!(v["scope"], serde_json::json!("all"));
        assert_eq!(v["offset"], serde_json::json!(0));
        assert_eq!(v["returned"], serde_json::json!(1));
        assert_eq!(v["total_matching"], serde_json::json!(1));
        assert_eq!(v["has_more"], serde_json::json!(false));
        assert_eq!(sorted_ids(&v), vec![parent.id.clone()]);

        let row = &v["sessions"][0];
        assert_eq!(row["session_id"], serde_json::json!(parent.id));
        assert_eq!(row["name"], serde_json::json!("listed"));
        assert_eq!(row["session_type"], serde_json::json!("user"));
        assert_eq!(row["running"], serde_json::json!(false));
        assert_eq!(row["parent_session_id"], serde_json::Value::Null);
        assert!(
            !row["working_dir"].as_str().unwrap().is_empty(),
            "row: {row}"
        );
        // No GUI, so no tab placement — `null`, not a fabricated object.
        assert_eq!(row["gui"], serde_json::Value::Null);

        // §4.1: per-session enabled extensions. This session has no
        // session-specific extension state, so the row must carry the global
        // config — the same fallback `GET /sessions/{id}/extensions` performs.
        // Asserting the VALUE (not just the key) is what distinguishes the real
        // read from an empty placeholder array.
        //
        // Compared as a sorted set: the exact NAMES are the contract, their
        // order is an artifact of the config file's key order.
        let expected_extensions = sorted(
            crate::config::get_enabled_extensions()
                .iter()
                .map(|e| e.name())
                .collect::<Vec<String>>(),
        );
        assert!(
            !expected_extensions.is_empty(),
            "the fallback is empty, so this assertion would pin nothing"
        );
        let got_extensions = sorted(
            row["extensions"]
                .as_array()
                .expect("extensions is an array")
                .iter()
                .map(|n| n.as_str().expect("extension names are strings").to_string())
                .collect::<Vec<String>>(),
        );
        assert_eq!(got_extensions, expected_extensions);

        // Headless: no workspace services, so no knowledge selection.
        assert_eq!(row["knowledge_bases"], serde_json::json!([]));
        // Post-#45: the write target is reported alongside the set. Without it a
        // model can set a primary and never read it back. `null` is the correct
        // value here and is a DIFFERENT state from the key being absent, so the
        // presence check is separate from the value check.
        assert!(
            row.get("primary_kb").is_some(),
            "primary_kb is a required row field; row: {row}"
        );
        assert_eq!(row["primary_kb"], serde_json::Value::Null);
    }

    /// Decision 17: the page window is honoured and reported.
    #[tokio::test]
    async fn workspace_list_pages_instead_of_truncating() {
        let c = client();
        for i in 0..5 {
            c.context
                .session_manager
                .create_session(
                    std::env::temp_dir(),
                    format!("paged-{i}"),
                    crate::session::session_manager::SessionType::User,
                )
                .await
                .unwrap();
        }
        let page = |offset: u32, limit: u32| serde_json::json!({ "scope": "all", "offset": offset, "limit": limit });
        let first = list(&c, page(0, 2)).await;
        assert_eq!(first["returned"], serde_json::json!(2));
        assert_eq!(first["offset"], serde_json::json!(0));
        // `total_matching` counts every matching row, not just the page — that is
        // the whole point of decision 17.
        assert_eq!(first["total_matching"], serde_json::json!(5));
        assert_eq!(first["has_more"], serde_json::json!(true));

        let second = list(&c, page(2, 2)).await;
        assert_eq!(second["offset"], serde_json::json!(2));
        assert_eq!(second["returned"], serde_json::json!(2));
        assert_eq!(second["has_more"], serde_json::json!(true));

        let last = list(&c, page(4, 2)).await;
        assert_eq!(last["returned"], serde_json::json!(1));
        assert_eq!(last["has_more"], serde_json::json!(false));

        // The three pages must partition the five sessions: no overlap, no gap.
        let mut walked: Vec<String> = sorted_ids(&first);
        walked.extend(sorted_ids(&second));
        walked.extend(sorted_ids(&last));
        let unique: std::collections::BTreeSet<&String> = walked.iter().collect();
        assert_eq!(unique.len(), 5, "pages overlap or drop rows: {walked:?}");
        assert_eq!(sorted_ids(&list(&c, page(0, 200)).await), sorted(walked));
    }

    /// Decision 23: the `subagent_status` list mode, re-expressed.
    ///
    /// The fixture carries a DISTRACTOR for each filter, so the two are
    /// independently pinned. With only one parent and its one subagent, ignoring
    /// `parent_session_id` (→ every subagent) and ignoring `only_subagents` (→
    /// every child of that parent) both still yield the one expected row, and the
    /// test passes on an implementation that applies neither filter. Here:
    ///
    /// * `stray` is a subagent of a DIFFERENT parent — only `parent_session_id`
    ///   excludes it;
    /// * `masquerade` is a non-subagent child of the SAME parent — only
    ///   `only_subagents` excludes it.
    #[tokio::test]
    async fn workspace_list_filters_by_parent_and_by_subagent_type() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let new = |name: &'static str, kind| {
            let sm = sm.clone();
            async move {
                sm.create_session(std::env::temp_dir(), name.into(), kind)
                    .await
                    .unwrap()
            }
        };
        use crate::session::session_manager::SessionType;
        let parent = new("p", SessionType::User).await;
        let other_parent = new("other-p", SessionType::User).await;
        let child = new("c", SessionType::SubAgent).await;
        let stray = new("stray", SessionType::SubAgent).await;
        let masquerade = new("masquerade", SessionType::User).await;
        for (id, parent_id) in [
            (&child.id, &parent.id),
            (&stray.id, &other_parent.id),
            (&masquerade.id, &parent.id),
        ] {
            sm.update(id)
                .parent_session_id(Some(parent_id.clone()))
                .apply()
                .await
                .unwrap();
        }

        // Assert on the ROW SET, not on substrings. Every child row carries
        // `"parent_session_id": "<parent id>"`, so a naive
        // `assert!(!text.contains(&parent.id))` is false by construction — the
        // parent's id is present as a FIELD of the matched child.
        let both = list(
            &c,
            serde_json::json!({
                "scope": "all", "parent_session_id": parent.id, "only_subagents": true
            }),
        )
        .await;
        assert_eq!(
            sorted_ids(&both),
            sorted(vec![&child.id]),
            "the parent is not its own subagent, and neither are the distractors"
        );
        assert_eq!(both["total_matching"], serde_json::json!(1));

        // `parent_session_id` alone: both children of `parent`, whatever their
        // type — so dropping `only_subagents` is observable.
        let by_parent = list(
            &c,
            serde_json::json!({ "scope": "all", "parent_session_id": parent.id }),
        )
        .await;
        assert_eq!(
            sorted_ids(&by_parent),
            sorted(vec![&child.id, &masquerade.id])
        );

        // `only_subagents` alone: every subagent in the workspace, whoever spawned
        // it — so dropping `parent_session_id` is observable.
        let by_type = list(
            &c,
            serde_json::json!({ "scope": "all", "only_subagents": true }),
        )
        .await;
        assert_eq!(sorted_ids(&by_type), sorted(vec![&child.id, &stray.id]));

        // And unfiltered: all five, so neither filter is silently always-on.
        let all = list(&c, serde_json::json!({ "scope": "all" })).await;
        assert_eq!(
            sorted_ids(&all),
            sorted(vec![
                &parent.id,
                &other_parent.id,
                &child.id,
                &stray.id,
                &masquerade.id
            ])
        );
    }

    #[tokio::test]
    async fn read_conversation_projects_tool_calls_and_refuses_hidden() {
        use crate::conversation::message::{Message, MessageProvenance, ProvenanceKind};
        let c = client();
        let sm = c.context.session_manager.clone();

        let hidden = sm
            .create_session(
                std::env::temp_dir(),
                "h".into(),
                crate::session::session_manager::SessionType::Hidden,
            )
            .await
            .unwrap();
        let open = sm
            .create_session(
                std::env::temp_dir(),
                "o".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();

        // Seed: a user message, a tool request, and a spawn-context record.
        let mut m1 = Message::user().with_text("please compute");
        sm.add_message_adopting_uid(&open.id, &mut m1)
            .await
            .unwrap();
        // CallToolRequestParams derives NO Default — spell all four fields.
        // The citation is the DEPENDENCY's file, not this repo's:
        // ~/.cargo/registry/.../rmcp-0.14.0/src/model.rs:1887-1902 (derive
        // :1887, `pub struct CallToolRequestParams` :1890, fields
        // meta/name/arguments/task :1892-1901, `}` :1902). Re-verified
        // 2026-07-28 against the pinned `rmcp = "0.14.0"` (Cargo.toml:18):
        // the derive list is `Debug, Serialize, Deserialize, Clone, PartialEq`
        // — still no `Default`. Do NOT resolve this against
        // `crates/biorouter/src/model.rs`, which is 908 lines and unrelated.
        let mut m2 = Message::assistant().with_tool_request(
            "call-1",
            Ok(rmcp::model::CallToolRequestParams {
                meta: None,
                name: "shell".into(),
                arguments: Some(
                    serde_json::json!({"command": "ls"})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                task: None,
            }),
        );
        sm.add_message_adopting_uid(&open.id, &mut m2)
            .await
            .unwrap();
        let mut spawn = Message::user()
            .with_text("SPAWN CONTEXT …")
            .with_provenance(MessageProvenance {
                kind: ProvenanceKind::SpawnContext,
                from_session_id: None,
                from_session_name: None,
            });
        spawn.metadata.agent_visible = false;
        sm.add_message_adopting_uid(&open.id, &mut spawn)
            .await
            .unwrap();

        let call = |view: &str, sid: &str| {
            let args: rmcp::model::JsonObject =
                serde_json::from_value(serde_json::json!({ "session_id": sid, "view": view }))
                    .unwrap();
            (args,)
        };

        // Hidden sessions are refused (§5 "no covert reads").
        let (args,) = call("transcript", &hidden.id);
        let refused = c
            .call_tool(
                "workspace_read_conversation",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(refused.is_error, Some(true));

        // tool_calls view names the tool, not the prose.
        let (args,) = call("tool_calls", &open.id);
        let tc = c
            .call_tool(
                "workspace_read_conversation",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = tc.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("shell"));
        assert!(!text.contains("please compute"));

        // spawn_context view returns the provenance-marked record.
        let (args,) = call("spawn_context", &open.id);
        let sc = c
            .call_tool(
                "workspace_read_conversation",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(sc.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("SPAWN CONTEXT"));

        // BR-45 range: from_msg_uid slices the transcript from that message on.
        // m2's uid was adopted by add_message_adopting_uid (#41).
        let uid = m2.id.clone().expect("adopted uid");
        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "session_id": open.id, "view": "transcript", "from_msg_uid": uid
        }))
        .unwrap();
        let ranged = c
            .call_tool(
                "workspace_read_conversation",
                Some(args),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let rtext = ranged.content[0].as_text().unwrap().text.clone();
        assert!(
            !rtext.contains("please compute"),
            "messages before the uid are excluded"
        );
    }
}
