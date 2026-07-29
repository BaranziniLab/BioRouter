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

/// The display name, which **must** normalize to this extension's
/// `PLATFORM_EXTENSIONS` registry key (`"workspace"`).
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
/// (it normalizes to `workspacecontrol`); it lives in the registry description
/// and in the `description` field of the `ExtensionConfig::Platform` the GUI
/// writes.
pub static EXTENSION_NAME: &str = "Workspace";

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
                title: Some(EXTENSION_NAME.to_string()),
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
            // Tasks 13-17 and 19/24 append: workspace_read_conversation,
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
            "workspace_read_conversation" => Err("not implemented until Task 13".to_string()),
            "workspace_send_prompt" => Err("not implemented until Task 14".to_string()),
            "workspace_set_tools" => Err("not implemented until Task 15".to_string()),
            "workspace_close" => Err("not implemented until Task 16".to_string()),
            "workspace_watch" => Err("not implemented until Task 17".to_string()),
            "workspace_open" => Err("not implemented until Task 24".to_string()),
            // BR-71 decision 22: the spawn tool is advertised here but
            // dispatched by the agent loop (it needs the parent's TaskConfig).
            // Reachable only if that interception is ever removed.
            crate::agents::subagent_tool::SUBAGENT_TOOL_NAME => {
                Err("`subagent` is dispatched by the agent loop, not by this extension".to_string())
            }
            _ => Err(format!("Unknown tool: {name}")),
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

        let args: rmcp::model::JsonObject =
            serde_json::from_value(serde_json::json!({ "scope": "all" })).unwrap();
        let result = c
            .call_tool(
                "workspace_list",
                Some(args),
                crate::agents::mcp_client::McpMeta::new("caller"),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains(&parent.id));
        assert!(text.contains("\"gui_attached\": false"));
        // §4.1: per-session enabled extensions + KBs are part of the row.
        assert!(text.contains("\"extensions\""));
        assert!(text.contains("\"knowledge_bases\""));
        // Post-#45: the write target is reported alongside the set. Without it a
        // model can set a primary and never read it back.
        assert!(text.contains("\"primary_kb\""));
        // Decision 17: paging metadata is always present.
        assert!(text.contains("\"has_more\""));
        assert!(text.contains("\"total_matching\""));
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
        let call = |offset: u32, limit: u32| {
            let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
                "scope": "all", "offset": offset, "limit": limit
            }))
            .unwrap();
            args
        };
        let first = c
            .call_tool(
                "workspace_list",
                Some(call(0, 2)),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let text = first.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("\"returned\": 2"), "got: {text}");
        assert!(text.contains("\"has_more\": true"));

        let second = c
            .call_tool(
                "workspace_list",
                Some(call(2, 2)),
                test_meta(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let second_text = second.content[0].as_text().unwrap().text.clone();
        assert!(second_text.contains("\"offset\": 2"));
        // The two pages must not overlap.
        for id in ["paged-0", "paged-1"] {
            if text.contains(id) {
                assert!(!second_text.contains(id), "{id} appeared on both pages");
            }
        }
    }

    /// Decision 23: the `subagent_status` list mode, re-expressed.
    #[tokio::test]
    async fn workspace_list_filters_by_parent_and_by_subagent_type() {
        let c = client();
        let sm = c.context.session_manager.clone();
        let parent = sm
            .create_session(
                std::env::temp_dir(),
                "p".into(),
                crate::session::session_manager::SessionType::User,
            )
            .await
            .unwrap();
        let child = sm
            .create_session(
                std::env::temp_dir(),
                "c".into(),
                crate::session::session_manager::SessionType::SubAgent,
            )
            .await
            .unwrap();
        sm.update(&child.id)
            .parent_session_id(Some(parent.id.clone()))
            .apply()
            .await
            .unwrap();

        let args: rmcp::model::JsonObject = serde_json::from_value(serde_json::json!({
            "scope": "all", "parent_session_id": parent.id, "only_subagents": true
        }))
        .unwrap();
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
        // Assert on the ROW SET, not on substrings. Every child row carries
        // `"parent_session_id": "<parent id>"` (see Step 3's `rows.push`), so a
        // naive `assert!(!text.contains(&parent.id))` is false by construction —
        // the parent's id is present as a FIELD of the matched child.
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let ids: Vec<&str> = v["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["session_id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec![child.id.as_str()],
            "the parent is not its own subagent"
        );
    }
}
