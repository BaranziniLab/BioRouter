use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use crate::session::extension_data;
use crate::session::extension_data::{ExtensionState, TodoState, TodoStatus};
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool, ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "todo";

/// Default cap on the number of items a checklist may hold (per session).
const DEFAULT_MAX_ITEMS: usize = 200;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TodoWriteParams {
    /// The full checklist as markdown. One item per line, e.g.
    /// `- [ ] task`, `- [~] in progress`, `- [x] done`. Replaces the whole
    /// list — prefer `todo_add`/`todo_update` for incremental changes.
    content: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TodoAddParams {
    /// New tasks to append. Each becomes a pending item with a fresh id.
    items: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TodoUpdateParams {
    /// The id (the `#N` shown in the checklist) of the item to update.
    id: String,
    /// New status: `pending`, `in_progress`, or `completed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    /// Optional replacement text for the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct PlanWriteParams {
    /// The living plan: a maintained, step-by-step plan you keep updated as you
    /// work. Pass an empty string to clear it.
    plan: String,
}

pub struct TodoClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
    /// Serializes [`TodoClient::with_state`]'s read-modify-write.
    ///
    /// H6: until the client-wide `McpClientBox` mutex was removed, every
    /// `call_tool` on this extension was serialized for free, which is what made
    /// `with_state` safe. It loads the session's `extension_data`, mutates it,
    /// and writes the WHOLE blob back — with two `.await` points in between, so
    /// two concurrent `todo_add`s could both read `{a}`, then write `{a,x}` and
    /// `{a,y}`: a lost update. Now that tool calls on one extension overlap,
    /// this extension carries its own lock instead. It is deliberately narrow —
    /// it guards only the session-metadata round trip (sub-millisecond, local),
    /// not the MCP dispatch path, so it costs nothing and cannot reintroduce H6.
    state_lock: tokio::sync::Mutex<()>,
}

impl TodoClient {
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
                title: Some("Todo".to_string()),
                version: "1.1.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                indoc! {r#"
                Your plan and todo checklist are automatically re-injected into
                your context each turn, so you don't need to repeat them.

                Structured, per-item workflow:
                - Start: `plan_write` the approach, then `todo_write` the initial
                  checklist (one `- [ ]` line per task).
                - During: `todo_update` a single item's status as you go
                  (`in_progress` when you pick it up, `completed` when done).
                  Prefer `todo_add`/`todo_update` over rewriting the whole list.
                - End: verify every item is `completed` (or explain why not).

                Statuses: pending, in_progress, completed. Items are addressed by
                the `#N` id shown next to each line.
            "#}
                .to_string(),
            ),
        };

        Ok(Self {
            info,
            context,
            state_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Load the session's todo state (migrating any legacy blob), let `f`
    /// mutate it, then persist. Returns `f`'s success message.
    async fn with_state<F>(&self, session_id: &str, f: F) -> Result<String, String>
    where
        F: FnOnce(&mut TodoState) -> Result<String, String>,
    {
        // Held across the whole read-modify-write; see `state_lock`.
        let _state_guard = self.state_lock.lock().await;

        let manager = &self.context.session_manager;
        let mut session = manager
            .get_session(session_id, false)
            .await
            .map_err(|_| "Failed to read session metadata".to_string())?;

        let mut state = TodoState::load(&session.extension_data).unwrap_or_default();
        let message = f(&mut state)?;

        state
            .to_extension_data(&mut session.extension_data)
            .map_err(|_| "Failed to serialize TODO state".to_string())?;

        manager
            .update(session_id)
            .extension_data(session.extension_data)
            .apply()
            .await
            .map_err(|_| "Failed to update session metadata".to_string())?;

        Ok(message)
    }

    fn max_items() -> usize {
        std::env::var("BIOROUTER_TODO_MAX_ITEMS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_ITEMS)
    }

    async fn handle_write(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let content = string_arg(&arguments, "content")?;

        let char_count = content.chars().count();
        let max_chars = std::env::var("BIOROUTER_TODO_MAX_CHARS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50_000);
        if max_chars > 0 && char_count > max_chars {
            return Err(format!(
                "Todo list too large: {} chars (max: {})",
                char_count, max_chars
            ));
        }

        let max_items = Self::max_items();
        let message = self
            .with_state(session_id, move |state| {
                state.set_from_markdown(&content);
                if max_items > 0 && state.items.len() > max_items {
                    return Err(format!(
                        "Todo list too long: {} items (max: {})",
                        state.items.len(),
                        max_items
                    ));
                }
                Ok(format!("Todo list set: {} item(s)", state.items.len()))
            })
            .await?;
        Ok(vec![Content::text(message)])
    }

    async fn handle_add(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let items = string_array_arg(&arguments, "items")?;
        if items.is_empty() {
            return Err("Missing required parameter: items".to_string());
        }
        let max_items = Self::max_items();
        let message = self
            .with_state(session_id, move |state| {
                let ids = state.add_items(items);
                if ids.is_empty() {
                    return Err("No non-empty items to add".to_string());
                }
                if max_items > 0 && state.items.len() > max_items {
                    return Err(format!(
                        "Todo list too long: {} items (max: {})",
                        state.items.len(),
                        max_items
                    ));
                }
                Ok(format!("Added {} item(s): #{}", ids.len(), ids.join(", #")))
            })
            .await?;
        Ok(vec![Content::text(message)])
    }

    async fn handle_update(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args = arguments.as_ref().ok_or("Missing arguments")?;
        let displayed_id = args
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: id")?;
        let id = displayed_id
            .strip_prefix('#')
            .unwrap_or(displayed_id)
            .to_string();

        let status = match args.get("status").and_then(|v| v.as_str()) {
            Some(raw) => Some(TodoStatus::parse(raw).ok_or_else(|| {
                format!("Unknown status: {raw} (use pending/in_progress/completed)")
            })?),
            None => None,
        };
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if status.is_none() && text.is_none() {
            return Err("Provide a new status and/or text to update".to_string());
        }

        let message = self
            .with_state(session_id, move |state| {
                if state.update_item(&id, status, text) {
                    Ok(format!("Updated item #{id}"))
                } else {
                    Err(format!("No todo item with id #{id}"))
                }
            })
            .await?;
        Ok(vec![Content::text(message)])
    }

    async fn handle_plan_write(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let plan = string_arg(&arguments, "plan")?;

        let char_count = plan.chars().count();
        let max_chars = std::env::var("BIOROUTER_TODO_MAX_CHARS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50_000);
        if max_chars > 0 && char_count > max_chars {
            return Err(format!(
                "Plan too large: {} chars (max: {})",
                char_count, max_chars
            ));
        }

        let message = self
            .with_state(session_id, move |state| {
                let trimmed = plan.trim();
                if trimmed.is_empty() {
                    state.plan = None;
                    Ok("Plan cleared".to_string())
                } else {
                    state.plan = Some(trimmed.to_string());
                    Ok("Plan updated".to_string())
                }
            })
            .await?;
        Ok(vec![Content::text(message)])
    }

    fn get_tools() -> Vec<Tool> {
        vec![
            tool_from_schema::<TodoWriteParams>(
                "todo_write",
                indoc! {r#"
                    Replace the entire todo checklist with a markdown checklist.

                    Use this to seed the initial checklist. One item per line:
                    `- [ ] task`, `- [~] in progress`, `- [x] done`. For
                    incremental changes prefer `todo_add` (append) and
                    `todo_update` (flip a single item's status) so you never
                    accidentally truncate the list.
                "#},
                true,
            ),
            tool_from_schema::<TodoAddParams>(
                "todo_add",
                indoc! {r#"
                    Append one or more new pending items to the checklist without
                    rewriting the existing ones. Each item gets a fresh `#N` id.
                "#},
                false,
            ),
            tool_from_schema::<TodoUpdateParams>(
                "todo_update",
                indoc! {r#"
                    Update a single checklist item by its `#N` id: change its
                    status (pending/in_progress/completed) and/or its text,
                    without touching the rest of the list.
                "#},
                false,
            ),
            tool_from_schema::<PlanWriteParams>(
                "plan_write",
                indoc! {r#"
                    Set or update the living plan: a maintained, step-by-step
                    plan you keep current as you work. Re-injected into your
                    context each turn alongside the checklist. Empty string
                    clears it.
                "#},
                false,
            ),
        ]
    }
}

/// Read a required string argument.
fn string_arg(arguments: &Option<JsonObject>, key: &str) -> Result<String, String> {
    arguments
        .as_ref()
        .ok_or("Missing arguments")?
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {key}"))
        .map(|s| s.to_string())
}

/// Read a required array-of-strings argument.
fn string_array_arg(arguments: &Option<JsonObject>, key: &str) -> Result<Vec<String>, String> {
    let value = arguments
        .as_ref()
        .ok_or("Missing arguments")?
        .get(key)
        .ok_or_else(|| format!("Missing required parameter: {key}"))?;
    let array = value
        .as_array()
        .ok_or_else(|| format!("Parameter {key} must be an array of strings"))?;
    Ok(array
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect())
}

fn tool_from_schema<T: JsonSchema>(name: &str, description: &str, destructive: bool) -> Tool {
    let schema = schema_for!(T);
    let schema_value = serde_json::to_value(schema).expect("Failed to serialize schema");
    Tool::new(
        name.to_string(),
        description.to_string(),
        schema_value.as_object().unwrap().clone(),
    )
    .annotate(ToolAnnotations {
        title: Some(name.to_string()),
        read_only_hint: Some(false),
        destructive_hint: Some(destructive),
        idempotent_hint: Some(false),
        open_world_hint: Some(false),
    })
}

#[async_trait]
impl McpClientTrait for TodoClient {
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
        let session_id = &meta.session_id;
        let content = match name {
            "todo_write" => self.handle_write(session_id, arguments).await,
            "todo_add" => self.handle_add(session_id, arguments).await,
            "todo_update" => self.handle_update(session_id, arguments).await,
            "plan_write" => self.handle_plan_write(session_id, arguments).await,
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {}",
                error
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_moim(&self, session_id: &str) -> Option<String> {
        let metadata = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .ok()?;

        // Only the live plan/task state belongs here; the behavioral rule (plan
        // up front, keep a todo list) lives in system.md so it holds even
        // without this extension. See BR-4 / BR-60.
        let state = extension_data::TodoState::load(&metadata.extension_data)?;
        if state.is_empty() {
            return None;
        }
        Some(format!("Current tasks and notes:\n{}", state.render()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::privacy::{CallCapability, ProviderTier};
    use crate::session::{SessionManager, SessionType};

    fn meta(session_id: &str) -> McpMeta {
        McpMeta::new(
            session_id,
            CallCapability::for_test(ProviderTier::Public, true),
        )
    }

    async fn call(
        client: &TodoClient,
        session_id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> CallToolResult {
        client
            .call_tool(
                name,
                args.as_object().cloned(),
                meta(session_id),
                CancellationToken::default(),
            )
            .await
            .unwrap()
    }

    fn text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content| content.as_text().map(|text| text.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn all_advertised_todo_tools_dispatch_and_reinject_their_state() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(SessionManager::new(temp.path().join("sessions")));
        let session = manager
            .create_session(
                temp.path().to_path_buf(),
                "todo dispatch".into(),
                SessionType::User,
            )
            .await
            .unwrap();
        let client = TodoClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::clone(&manager),
        })
        .unwrap();

        let listed = client
            .list_tools(None, CancellationToken::default())
            .await
            .unwrap();
        let mut names = listed
            .tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(
            names,
            ["plan_write", "todo_add", "todo_update", "todo_write"]
        );

        assert!(text(
            &call(
                &client,
                &session.id,
                "plan_write",
                serde_json::json!({"plan": "Build, verify, report"}),
            )
            .await
        )
        .contains("Plan updated"));
        assert!(text(
            &call(
                &client,
                &session.id,
                "todo_write",
                serde_json::json!({"content": "- [ ] build\n- [ ] verify"}),
            )
            .await
        )
        .contains("2 item"));
        assert!(text(
            &call(
                &client,
                &session.id,
                "todo_add",
                serde_json::json!({"items": ["report"]}),
            )
            .await
        )
        .contains("#3"));
        assert!(text(
            &call(
                &client,
                &session.id,
                "todo_update",
                serde_json::json!({"id": "1", "status": "completed"}),
            )
            .await
        )
        .contains("#1"));

        let reinjected = client.get_moim(&session.id).await.unwrap();
        assert!(reinjected.contains("Build, verify, report"), "{reinjected}");
        assert!(reinjected.contains("build"), "{reinjected}");
        assert!(reinjected.contains("report"), "{reinjected}");
        assert!(reinjected.contains("[x]"), "{reinjected}");

        let invalid = call(
            &client,
            &session.id,
            "todo_update",
            serde_json::json!({"id": "999", "status": "completed"}),
        )
        .await;
        assert_eq!(invalid.is_error, Some(true));
        assert!(text(&invalid).contains("No todo item"));
    }

    #[tokio::test]
    async fn todo_update_accepts_the_displayed_hash_prefixed_id() {
        let temp = tempfile::tempdir().unwrap();
        let manager = Arc::new(SessionManager::new(temp.path().join("sessions")));
        let session = manager
            .create_session(
                temp.path().to_path_buf(),
                "todo displayed id".into(),
                SessionType::User,
            )
            .await
            .unwrap();
        let client = TodoClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::clone(&manager),
        })
        .unwrap();

        call(
            &client,
            &session.id,
            "todo_write",
            serde_json::json!({"content": "- [ ] verify displayed id"}),
        )
        .await;
        let updated = call(
            &client,
            &session.id,
            "todo_update",
            serde_json::json!({"id": "#1", "status": "completed"}),
        )
        .await;

        assert_eq!(updated.is_error, Some(false), "{}", text(&updated));
        assert!(text(&updated).contains("Updated item #1"));
        let reinjected = client.get_moim(&session.id).await.unwrap();
        assert!(
            reinjected.contains("- [x] (#1) verify displayed id"),
            "{reinjected}"
        );
    }
}
