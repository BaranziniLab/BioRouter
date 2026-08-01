use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use crate::privacy::CallCapability;
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

pub static EXTENSION_NAME: &str = "Chat Recall";

/// Issue #56 Gate D. Moved into `crates/biorouter/src/privacy/refusal.rs` by
/// Task 13, which is the first task that has that module. Constant on purpose:
/// a model that sees a different string on retry concludes the refusal is
/// transient and loops. It names no target — not the session, not its working
/// directory — because §11.4 classifies both as CONTENT.
const CHATRECALL_LOAD_REFUSAL: &str = "This chat history is private: it was created under a model \
     hosted inside the institution, so only a private model may read it. This session is running \
     on a public model. Ask the user to switch this chat to a private model — Settings → Models, \
     or the model chip in the composer — and try again. Do not retry with a different session id \
     or through another tool; the boundary is the same everywhere.";

/// Parameters for the chatrecall tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ChatRecallParams {
    /// Search keywords. Use multiple related terms/synonyms (e.g., 'database postgres sql'). Mutually exclusive with session_id.
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    /// Session ID to load. Returns first/last 3 messages. Mutually exclusive with query.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    /// Max results (default: 10, max: 50). Search mode only.
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<i64>,
    /// ISO 8601 date (e.g., '2025-10-01T00:00:00Z'). Search mode only.
    #[serde(skip_serializing_if = "Option::is_none")]
    after_date: Option<String>,
    /// ISO 8601 date (e.g., '2025-10-15T23:59:59Z'). Search mode only.
    #[serde(skip_serializing_if = "Option::is_none")]
    before_date: Option<String>,
}

pub struct ChatRecallClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
}

impl ChatRecallClient {
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
            instructions: Some(indoc! {r#"
                Chat Recall

                Search past conversations and load session summaries when the user expects some memory or context.

                Two modes:
                - Search mode: Use query with keywords/synonyms to find relevant messages
                - Load mode: Use session_id to get first and last messages of a specific session
            "#}.to_string()),
        };

        Ok(Self { info, context })
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_chatrecall(
        &self,
        current_session_id: &str,
        cap: CallCapability,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let arguments = arguments.ok_or("Missing arguments")?;

        let target_session_id = arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(sid) = target_session_id {
            // LOAD MODE: Get session summary (first and last few messages)
            match self.context.session_manager.get_session(&sid, true).await {
                Ok(loaded_session) => {
                    // Issue #56 Gate D (LOAD). BEFORE the header string is
                    // built, so neither the session name nor the working
                    // directory can escape — both are CONTENT under §11.4.
                    //
                    // `cap` was sampled once, at the entry that admitted this
                    // call. It is NOT re-derived here: this code runs inside
                    // the driven future, on the far side of
                    // `tool_dispatch_limits::acquire`, where the provider may
                    // already be a different one.
                    if cap.enforced()
                        && !crate::privacy::visible_to(cap.tier(), loaded_session.privacy_tier)
                    {
                        return Ok(vec![Content::text(CHATRECALL_LOAD_REFUSAL)]);
                    }

                    let conversation = loaded_session.conversation.as_ref();

                    if conversation.is_none() {
                        return Ok(vec![Content::text(format!(
                            "Session {} has no conversation.",
                            sid
                        ))]);
                    }

                    let msgs = conversation.unwrap().messages();
                    let total = msgs.len();

                    if total == 0 {
                        return Ok(vec![Content::text(format!(
                            "Session {} has no messages.",
                            sid
                        ))]);
                    }

                    let mut output = format!(
                        "Session: {} (ID: {})\nWorking Dir: {}\nTotal Messages: {}\n\n",
                        loaded_session.name,
                        sid,
                        loaded_session.working_dir.display(),
                        total
                    );

                    // Show first 3 messages
                    let first_count = std::cmp::min(3, total);
                    output.push_str("--- First Few Messages ---\n\n");
                    for (idx, msg) in msgs.iter().take(first_count).enumerate() {
                        output.push_str(&format!("{}. [{:?}] ", idx + 1, msg.role));
                        for content in &msg.content {
                            if let Some(text) = content.as_text() {
                                output.push_str(text);
                                output.push('\n');
                            }
                        }
                        output.push('\n');
                    }

                    // Show last 3 messages (if different from first)
                    if total > first_count {
                        output.push_str("--- Last Few Messages ---\n\n");
                        let last_count = std::cmp::min(3, total);
                        let skip_count = total.saturating_sub(last_count);
                        for (idx, msg) in msgs.iter().skip(skip_count).enumerate() {
                            output.push_str(&format!(
                                "{}. [{:?}] ",
                                skip_count + idx + 1,
                                msg.role
                            ));
                            for content in &msg.content {
                                if let Some(text) = content.as_text() {
                                    output.push_str(text);
                                    output.push('\n');
                                }
                            }
                            output.push('\n');
                        }
                    }

                    Ok(vec![Content::text(output)])
                }
                Err(e) => Err(format!("Failed to load session: {}", e)),
            }
        } else {
            // SEARCH MODE: Search across all sessions
            let query = arguments
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or("Missing required parameter: query or session_id")?
                .to_string();

            let limit = arguments
                .get("limit")
                .and_then(|v| v.as_i64())
                .map(|l| l as usize)
                .unwrap_or(10)
                .min(50);

            let after_date = arguments
                .get("after_date")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            let before_date = arguments
                .get("before_date")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc));

            // Exclude current session from results to avoid self-referential loops
            let exclude_session_id = Some(current_session_id.to_string());

            match self
                .context
                .session_manager
                .search_chat_history(
                    &query,
                    Some(limit),
                    after_date,
                    before_date,
                    exclude_session_id,
                )
                .await
            {
                Ok(results) => {
                    let formatted_results = if results.total_matches == 0 {
                        format!("No results found for query: '{}'", query)
                    } else {
                        let mut output = format!(
                            "Found {} matching message(s) across {} session(s) for query: '{}'\n\n",
                            results.total_matches,
                            results.results.len(),
                            query
                        );
                        for (idx, result) in results.results.iter().enumerate() {
                            output.push_str(&format!(
                                "{}. Session: {} (ID: {})\n   Working Dir: {}\n   Last Activity: {}\n   Showing {} of {} total message(s) in session:\n\n",
                                idx + 1,
                                result.session_description,
                                result.session_id,
                                result.session_working_dir,
                                result.last_activity.format("%Y-%m-%d"),
                                result.messages.len(),
                                result.total_messages_in_session
                            ));

                            for (msg_idx, message) in result.messages.iter().enumerate() {
                                output.push_str(&format!(
                                    "   {}.{} [{}]\n   {}\n\n",
                                    idx + 1,
                                    msg_idx + 1,
                                    message.role,
                                    message
                                        .content
                                        .lines()
                                        .map(|line| format!("   {}", line))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                ));
                            }
                        }
                        output
                    };
                    Ok(vec![Content::text(formatted_results)])
                }
                Err(e) => Err(format!("Chat recall failed: {}", e)),
            }
        }
    }

    fn get_tools() -> Vec<Tool> {
        // Generate JSON schema from the ChatRecallParams struct
        let schema = schema_for!(ChatRecallParams);
        let schema_value =
            serde_json::to_value(schema).expect("Failed to serialize ChatRecallParams schema");

        let input_schema = schema_value
            .as_object()
            .expect("Schema should be an object")
            .clone();

        vec![Tool::new(
            "chatrecall".to_string(),
            indoc! {r#"
                Search past chat or load session summaries. Use when it is clear user expects some memory or context.

                search mode (query): Use multiple keywords/synonyms. Returns messages grouped by session, ordered by recency. Supports date filters.
                load mode (session_id): Returns first/last 3 messages of a session.
            "#}
            .to_string(),
            input_schema,
        )
        .annotate(ToolAnnotations {
            title: Some("Recall past conversations".to_string()),
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(false),
        })]
    }
}

#[async_trait]
impl McpClientTrait for ChatRecallClient {
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
            // Issue #56: the capability arrives on the meta, sampled once by the
            // entry that admitted this call. There is no `Weak<ExtensionManager>`
            // upgrade on this path and no provider read — see `CallCapability`.
            "chatrecall" => {
                self.handle_chatrecall(session_id, meta.capability, arguments)
                    .await
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::agent::{seams, Agent, AgentConfig};
    use crate::agents::extension::PlatformExtensionContext;
    use crate::agents::extension::{ExtensionConfig, PLATFORM_EXTENSIONS};
    use crate::conversation::message::Message as ConvMessage;
    use crate::model::ModelConfig;
    use crate::privacy::{CallCapability, ProviderTier, SessionClassification};
    use crate::providers::base::{Provider, ProviderMetadata, ProviderUsage};
    use crate::providers::errors::ProviderError;
    use crate::session::session_manager::{Session, SessionType};
    use crate::session::SessionManager;
    use rmcp::model::Tool as McpTool;
    use std::sync::Arc;

    /// A provider whose only interesting property is its tier. `complete_*` is
    /// never reached: every test here dispatches a tool, none runs a turn.
    struct TierProvider(ProviderTier);

    #[async_trait]
    impl Provider for TierProvider {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new(
                "tier",
                "Tier",
                "tier test provider",
                "m",
                vec!["m"],
                "",
                vec![],
            )
        }
        fn get_name(&self) -> &str {
            "tier-test-provider"
        }
        fn tier(&self) -> ProviderTier {
            self.0
        }
        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[ConvMessage],
            _tools: &[McpTool],
        ) -> Result<(ConvMessage, ProviderUsage), ProviderError> {
            unreachable!("no test here runs a turn")
        }
        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("m")
        }
    }

    fn public_provider() -> Arc<dyn Provider> {
        Arc::new(TierProvider(ProviderTier::Public))
    }

    fn private_provider() -> Arc<dyn Provider> {
        Arc::new(TierProvider(ProviderTier::Private))
    }

    /// One isolated session store plus the extension under test.
    ///
    /// ⚠ The plan wrote the fixtures as free functions (`private_session_named`,
    /// `load_via_public_capability_caller`, `agent_on`). They are methods here
    /// because every one of them has to reach the SAME `SessionManager` — a
    /// target created in one free function and loaded by another would be two
    /// different temp databases. Every assertion below is the plan's, verbatim.
    struct Harness {
        _temp: tempfile::TempDir,
        sm: Arc<SessionManager>,
        client: ChatRecallClient,
    }

    impl Harness {
        async fn new() -> Self {
            let _temp = tempfile::TempDir::new().unwrap();
            let sm = Arc::new(SessionManager::new(_temp.path().to_path_buf()));
            let client = ChatRecallClient::new(PlatformExtensionContext {
                extension_manager: None,
                session_manager: Arc::clone(&sm),
            })
            .unwrap();
            Self { _temp, sm, client }
        }

        async fn session_named(&self, name: &str, dir: &str, private: bool) -> Session {
            let s = self
                .sm
                .create_session(
                    std::path::PathBuf::from(dir),
                    name.to_string(),
                    SessionType::User,
                )
                .await
                .unwrap();
            self.sm
                .add_message(&s.id, &ConvMessage::user().with_text("hello"))
                .await
                .unwrap();
            if private {
                self.sm
                    .update(&s.id)
                    .raise_privacy(SessionClassification::Private, "turn:test")
                    .apply()
                    .await
                    .unwrap();
            }
            s
        }

        async fn private_session_named(&self, name: &str, dir: &str) -> Session {
            self.session_named(name, dir, true).await
        }

        async fn public_session_named(&self, name: &str, dir: &str) -> Session {
            self.session_named(name, dir, false).await
        }

        async fn load_via(
            &self,
            cap: CallCapability,
            target: &str,
        ) -> Result<Vec<Content>, String> {
            let mut args = JsonObject::new();
            args.insert(
                "session_id".into(),
                serde_json::Value::String(target.into()),
            );
            self.client
                .handle_chatrecall("caller-session", cap, Some(args))
                .await
        }

        async fn load_via_public_capability_caller(
            &self,
            target: &str,
        ) -> Result<Vec<Content>, String> {
            self.load_via(CallCapability::for_test(ProviderTier::Public, true), target)
                .await
        }

        async fn load_via_private_capability_caller(
            &self,
            target: &str,
        ) -> Result<Vec<Content>, String> {
            self.load_via(
                CallCapability::for_test(ProviderTier::Private, true),
                target,
            )
            .await
        }

        /// A real `Agent` on this harness's store, with the chatrecall platform
        /// extension loaded, plus a caller session to dispatch from.
        async fn agent_on(&self, provider: Arc<dyn Provider>) -> (Arc<Agent>, Session) {
            let agent = Arc::new(Agent::with_config(AgentConfig::new(
                Arc::clone(&self.sm),
                crate::config::permission::PermissionManager::instance(),
                None,
                crate::config::BioRouterMode::Auto,
            )));
            let caller = self
                .sm
                .create_session(
                    self._temp.path().to_path_buf(),
                    "caller".into(),
                    SessionType::User,
                )
                .await
                .unwrap();
            *agent.provider.lock().await = Some(provider);
            agent
                .extension_manager
                .add_extension(ExtensionConfig::Platform {
                    name: "chatrecall".to_string(),
                    description: PLATFORM_EXTENSIONS["chatrecall"].description.to_string(),
                    bundled: None,
                    available_tools: Vec::new(),
                })
                .await
                .unwrap();
            (agent, caller)
        }
    }

    /// The real dispatch path: `Agent::dispatch_tool_call` samples the
    /// capability, `ExtensionManager` carries it, chatrecall reads it.
    async fn chatrecall_load(
        agent: &Arc<Agent>,
        caller: &Session,
        target_id: &str,
    ) -> Result<Vec<Content>, String> {
        let mut args = JsonObject::new();
        args.insert(
            "session_id".into(),
            serde_json::Value::String(target_id.to_string()),
        );
        let (_req, dispatched) = agent
            .dispatch_tool_call(
                rmcp::model::CallToolRequestParams {
                    meta: None,
                    name: "chatrecall__chatrecall".into(),
                    arguments: Some(args),
                    task: None,
                },
                "req".to_string(),
                None,
                caller,
            )
            .await;
        let dispatched = match dispatched {
            Ok(d) => d,
            Err(e) => return Err(e.message.to_string()),
        };
        dispatched
            .result
            .await
            .map(|r| r.content)
            .map_err(|e| e.message.to_string())
    }

    #[tokio::test]
    async fn load_refuses_a_private_session_without_naming_it() {
        // The leak is in the STRING, not the return value: a guard placed after
        // the header `format!` at :113 returns an error whose text already carries
        // the session name and the working directory. §11.4 classifies both as
        // CONTENT — a title in this product is LLM-generated from the conversation,
        // and a working dir routinely names a cohort, a study or a population.
        let h = Harness::new().await;
        let target = h
            .private_session_named(
                "OMOP diabetes cohort characterisation",
                "/data/phi/cohort-2026-dm2",
            )
            .await;
        let out = h
            .load_via_public_capability_caller(&target.id)
            .await
            .unwrap();
        let text = out[0].as_text().unwrap().text.clone();

        assert!(text.contains("private"), "must say why: {text}");
        assert!(!text.contains("OMOP"), "leaked the session name: {text}");
        assert!(
            !text.contains("diabetes"),
            "leaked the session name: {text}"
        );
        assert!(
            !text.contains("cohort-2026-dm2"),
            "leaked the working dir: {text}"
        );
        assert!(
            !text.contains("/data/phi"),
            "leaked the working dir: {text}"
        );
    }

    #[tokio::test]
    async fn load_still_works_for_a_private_caller_and_for_public_targets() {
        let h = Harness::new().await;
        let priv_target = h.private_session_named("OMOP cohort", "/data/phi/x").await;
        let pub_target = h.public_session_named("weekly notes", "/tmp/notes").await;
        assert!(h
            .load_via_private_capability_caller(&priv_target.id)
            .await
            .unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .contains("OMOP cohort"));
        assert!(h
            .load_via_public_capability_caller(&pub_target.id)
            .await
            .unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .contains("weekly notes"));
    }

    /// The sample the call was ADMITTED on is the sample the gate reads — even
    /// though the tool ran minutes later, behind the dispatch semaphore.
    ///
    /// This is the test the `Weak<ExtensionManager>` design could not pass and
    /// which nothing in rounds 1-3 forced. Under that design chatrecall re-derived
    /// the tier from the provider mutex *inside the driven future*
    /// (`agent.rs`'s `tool_dispatch_limits::acquire` is the park point), so a
    /// call admitted as Public read Private there and returned the transcript.
    #[tokio::test]
    async fn a_swap_after_admission_does_not_change_what_this_call_may_load() {
        let h = Harness::new().await;
        let (agent, s) = h.agent_on(public_provider()).await;
        let target = h.private_session_named("OMOP cohort", "/data/phi/x").await;

        // Park the call AFTER `Agent::dispatch_tool_call` has returned its future
        // and BEFORE anything drives it — i.e. exactly where a real queued call
        // sits.
        let held = seams::hold_dispatch_queue();
        let call = tokio::spawn({
            let agent = agent.clone();
            let caller = s.clone();
            let id = target.id.clone();
            async move { chatrecall_load(&agent, &caller, &id).await }
        });
        let release = held.await.unwrap();

        agent
            .update_provider(private_provider(), &s.id)
            .await
            .unwrap();
        release.send(()).unwrap();

        let text = call.await.unwrap().unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .clone();
        assert!(
            text.contains("private"),
            "a call admitted as public loaded a private transcript"
        );
        assert!(!text.contains("OMOP"), "leaked the session name: {text}");
    }
}
