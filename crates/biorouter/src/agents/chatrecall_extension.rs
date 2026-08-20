use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use crate::privacy::{CallCapability, ProviderTier};
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

/// How much of one matched message SEARCH prints.
///
/// ⚠ Recall is a *locator*, not a transcript reader: the model gets session ids
/// and reads the interesting one with LOAD (or `workspace_read_conversation`).
/// Printing whole messages made the answer scale with how much the user had
/// written rather than with how many things matched — measured at 779,488
/// characters (~195k tokens) for one ordinary query at `limit: 50`, roughly 8x
/// the 25k-token inline cap, which pushes the entire result into a file the
/// model then has to go and grep. At this width a full 50-hit search stays
/// inside the cap and remains directly readable.
const MAX_EXCERPT_CHARS: usize = 1200;

/// How much of one message LOAD prints. Wider than [`MAX_EXCERPT_CHARS`] because
/// LOAD shows at most six messages and exists so the model can actually read
/// them — but still bounded, because a rendered tool call includes its arguments
/// and a `text_editor` write carries an entire file in those.
const MAX_LOAD_MESSAGE_CHARS: usize = 4000;

/// One matched message, clipped to [`MAX_EXCERPT_CHARS`] on a char boundary.
/// The marker is load-bearing: silent truncation would let the model conclude a
/// message does not mention something when it simply was not shown.
fn excerpt(content: &str) -> String {
    clip(
        content,
        MAX_EXCERPT_CHARS,
        "load this session for the full message",
    )
}

/// Clip `content` to `max` CHARACTERS (never bytes — slicing by byte offset
/// would panic mid-codepoint on any non-ASCII transcript).
fn clip(content: &str, max: usize, hint: &str) -> String {
    if content.chars().count() <= max {
        return content.to_string();
    }
    let clipped: String = content.chars().take(max).collect();
    format!("{clipped}… [truncated; {hint}]")
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
                    //
                    // The `cap.enforced()` conjunct is DR-15's master opt-out,
                    // and it is deliberately part of this predicate — the plan's
                    // snippet for this guard omitted it. Every gate reads the
                    // toggle, and reading it from the same sample that carried
                    // the tier is what keeps the two halves of one decision at
                    // one instant. It is a no-op while the toggle is still the
                    // `const fn … { true }` stub, and is the whole guard once
                    // Task 30 makes it settable. Do not "simplify" it away.
                    //
                    // The toggle's function name is deliberately not spelled
                    // here: a Step 5 gate counts that token tree-wide and must
                    // see exactly one — the read inside `CallCapability`.
                    if cap.enforced()
                        && !crate::privacy::visible_to(cap.tier(), loaded_session.privacy_tier)
                    {
                        // Issue #56 Gate D. The string itself lives in
                        // `privacy::refusal`, which owns every refusal in the
                        // tree — so §14.4's "never leak content in a refusal"
                        // rule can be checked by reading one file, and so no
                        // second copy of this sentence can drift from it.
                        return Ok(vec![Content::text(
                            crate::privacy::refusal::chatrecall_load_refusal(),
                        )]);
                    }

                    // Issue #56 DR-26 / Task 50 Step 3, on the SAME line as the
                    // tier gate above and for the same reason it is here: the
                    // header string below carries the session's name and working
                    // directory, which are CONTENT under §11.4.
                    //
                    // ⚠ **Both endpoints are private here**, so every tier gate
                    // this campaign built says yes and only the third axis
                    // refuses — the operator's case, arriving through recall
                    // instead of through a tool call. A chat that queried the
                    // UCSF OMOP connector holds UCSF's data in its transcript
                    // just as surely as the connector does.
                    //
                    // ⚠ **An unreadable answer is not an empty set.** A store
                    // error means we cannot tell which institutions this chat
                    // reached, and DR-26's discipline for unknown is
                    // restrictive — the same direction `KbAffiliation::Unknown`
                    // takes. So the `Err` arm refuses rather than falling
                    // through, which is why `session_affiliations` returns a
                    // `Result` instead of defaulting.
                    if cap.enforced() {
                        let owners = self
                            .context
                            .session_manager
                            .session_affiliations(&sid)
                            .await;
                        let refusal = match owners {
                            Ok(owners) => crate::privacy::affiliation::cross_affiliation_owners(
                                cap.affiliation(),
                                "this chat history",
                                &owners,
                            )
                            .map(|finding| finding.warning),
                            Err(error) => {
                                tracing::warn!(
                                    session_id = %sid,
                                    %error,
                                    "could not read this chat's institutional affiliations; \
                                     refusing the recall"
                                );
                                Some(
                                    "Cross-institutional data flow. Which institutions this chat \
                                     history reached could not be determined, so this build \
                                     cannot vouch that your model's agreements cover it."
                                        .to_string(),
                                )
                            }
                        };
                        if let Some(warning) = refusal {
                            return Ok(vec![Content::text(
                                crate::privacy::refusal::chatrecall_cross_affiliation_refusal(
                                    &warning,
                                ),
                            )]);
                        }
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

                    // ⚠ Render EVERY content part, not just `as_text()`.
                    // A tool call, a tool response and a thinking block all
                    // return `None` there, so a message that carried only tool
                    // traffic used to print its header and an empty body —
                    // 62% of messages in a real store. `chat_fts::searchable_parts`
                    // is the same flattening SEARCH mode already renders and the
                    // FTS index already stores, so the two halves of this tool
                    // can no longer disagree about what a message says.
                    let render = |msg: &crate::conversation::message::Message| -> String {
                        let parts = crate::session::chat_fts::searchable_parts(&msg.content);
                        if parts.is_empty() {
                            "[no renderable content]".to_string()
                        } else {
                            clip(
                                &parts.join("\n"),
                                MAX_LOAD_MESSAGE_CHARS,
                                "read this session with workspace_read_conversation for the full text",
                            )
                        }
                    };

                    // Show first 3 messages
                    let first_count = std::cmp::min(3, total);
                    output.push_str("--- First Few Messages ---\n\n");
                    for (idx, msg) in msgs.iter().take(first_count).enumerate() {
                        output.push_str(&format!("{}. [{:?}] ", idx + 1, msg.role));
                        output.push_str(&render(msg));
                        output.push_str("\n\n");
                    }

                    // Show the last few messages that the first block did not
                    // already print.
                    //
                    // ⚠ `skip_count` must never fall BELOW `first_count`, or the
                    // two blocks overlap and the same message is printed twice
                    // under two different numbers. `total - min(3, total)` alone
                    // does exactly that at total = 4 (repeats #2 and #3) and
                    // total = 5 (repeats #3) — the only two sizes where the
                    // windows meet, and 244 sessions in a real store are in that
                    // range, which is why "it looked fine" for 3 and for 6.
                    let skip_count = std::cmp::max(first_count, total.saturating_sub(3));
                    if skip_count < total {
                        output.push_str("--- Last Few Messages ---\n\n");
                        for (idx, msg) in msgs.iter().skip(skip_count).enumerate() {
                            output.push_str(&format!(
                                "{}. [{:?}] ",
                                skip_count + idx + 1,
                                msg.role
                            ));
                            output.push_str(&render(msg));
                            output.push_str("\n\n");
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

            // Issue #56 Gate D (SEARCH). The filter is pushed into SQL, ahead of
            // the `LIMIT`, rather than applied to the returned rows: a public
            // caller must still get its full quota of public hits when private
            // sessions outrank them.
            //
            // `cap` is the sample this call was ADMITTED on, exactly as in the
            // LOAD arm above — never a fresh provider read from inside the
            // driven future.
            //
            // The collapse to a bare tier is where DR-15's master opt-out is
            // applied: `restricts_private_data()` is the conjunction of the
            // toggle and the reach, so with the toggle off this searches with
            // full reach. Passing `cap.tier()` straight through would keep
            // filtering after the user opted out. Same correction the LOAD arm
            // carries, for the same reason.
            let reach = if cap.restricts_private_data() {
                ProviderTier::Public
            } else {
                ProviderTier::Private
            };

            match self
                .context
                .session_manager
                .search_chat_history(
                    &query,
                    Some(limit),
                    after_date,
                    before_date,
                    exclude_session_id,
                    crate::session::chat_history_search::SearchReach {
                        tier: reach,
                        // Issue #56 DR-26 / Task 50 Step 3. Off the SAME
                        // admitted sample as the tier above, never a fresh
                        // provider read from inside the driven future. Unlike
                        // the tier, it is NOT collapsed by the master opt-out
                        // here — the clause reads the toggle itself
                        // (`ChatHistorySearch::filters_by_affiliation`), so the
                        // rule holds for any caller of `search_chat_history`,
                        // capability or not.
                        affiliation: cap.affiliation(),
                    },
                )
                .await
            {
                Ok(results) => {
                    let formatted_results = if results.total_matches == 0 {
                        format!("No results found for query: '{}'", query)
                    } else {
                        // ⚠ `total_matches` is the count of rows that came back
                        // AFTER `LIMIT`, not the number that matched. Printing it
                        // as "Found N" told the model N was the whole answer
                        // whenever the search hit its cap, which is exactly when
                        // that is least true. Say "at least" and name the lever.
                        let capped = results.total_matches >= limit;
                        let mut output = format!(
                            "Found {}{} matching message(s) across {} session(s) for query: '{}'\n{}\n",
                            if capped { "at least " } else { "" },
                            results.total_matches,
                            results.results.len(),
                            query,
                            if capped {
                                format!(
                                    "(the {limit}-message limit was reached, so there are more \
                                     matches than are shown; narrow the query or raise `limit`)\n"
                                )
                            } else {
                                String::new()
                            }
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
                                    excerpt(&message.content)
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

                search mode (query): Use multiple keywords/synonyms; any of them may match. Returns messages grouped by session, best match first, each message clipped to an excerpt. `limit` caps MESSAGES, not sessions, so a broad query can return few sessions — narrow it rather than raising the limit. Date filters are inclusive of their own day.
                load mode (session_id): Returns the first and last few messages of one session, each clipped to a long excerpt.
                Mutually exclusive: if both are given, session_id wins and query is ignored.
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
            self.session_containing(name, dir, private, "hello").await
        }

        async fn session_containing(
            &self,
            name: &str,
            dir: &str,
            private: bool,
            text: &str,
        ) -> Session {
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
                .add_message(&s.id, &ConvMessage::user().with_text(text))
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

        async fn search_via(
            &self,
            cap: CallCapability,
            query: &str,
        ) -> Result<Vec<Content>, String> {
            let mut args = JsonObject::new();
            args.insert("query".into(), serde_json::Value::String(query.into()));
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

    /// Gate D's other half. LOAD names one session; SEARCH sweeps every session
    /// in the store, so it is the wider hole of the two.
    #[tokio::test]
    async fn search_hides_a_private_session_from_a_public_caller() {
        let h = Harness::new().await;
        h.session_containing(
            "OMOP diabetes cohort characterisation",
            "/data/phi/cohort-2026-dm2",
            true,
            "the mitochondrion count is private",
        )
        .await;
        h.session_containing(
            "weekly notes",
            "/tmp/notes",
            false,
            "the mitochondrion count is public",
        )
        .await;

        let public = h
            .search_via(
                CallCapability::for_test(ProviderTier::Public, true),
                "mitochondrion",
            )
            .await
            .unwrap();
        let text = public[0].as_text().unwrap().text.clone();
        // The renderer prints `session_description` and `session_working_dir`;
        // `create_session` fills `name`, not `description`, so the working dir
        // is what identifies a session in this output. §11.4 classifies it as
        // CONTENT — "/data/phi/cohort-2026-dm2" names a cohort on its own.
        // `session_description` itself is covered from the SQL side, in
        // `chat_history_search`'s `no_content_field_of_a_private_row_survives`.
        assert!(
            text.contains("/tmp/notes"),
            "the public session must still be found: {text}"
        );
        assert!(
            !text.contains("/data/phi"),
            "leaked the working dir: {text}"
        );
        assert!(
            !text.contains("cohort-2026-dm2"),
            "leaked the working dir: {text}"
        );
        assert!(
            !text.contains("is private"),
            "leaked the message body: {text}"
        );

        let private = h
            .search_via(
                CallCapability::for_test(ProviderTier::Private, true),
                "mitochondrion",
            )
            .await
            .unwrap();
        let text = private[0].as_text().unwrap().text.clone();
        assert!(
            text.contains("/data/phi/cohort-2026-dm2"),
            "a private caller still sees it: {text}"
        );
        assert!(text.contains("is private"), "{text}");
        assert!(text.contains("/tmp/notes"), "{text}");
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
        //
        // Keyed on this caller session and this tool name, so the rendezvous can
        // only be taken by the dispatch below. An unkeyed one is worse than no
        // test: another test's dispatch takes it, this call runs un-parked
        // BEFORE the swap, and every assertion still passes — the ordering under
        // test quietly stops being exercised.
        let held = seams::hold_dispatch_queue(&s.id, "chatrecall__chatrecall");
        let call = tokio::spawn({
            let agent = agent.clone();
            let caller = s.clone();
            let id = target.id.clone();
            async move { chatrecall_load(&agent, &caller, &id).await }
        });
        // And under a timeout, so a key or a hold point that has drifted fails
        // with that sentence instead of hanging the test binary.
        let release = tokio::time::timeout(std::time::Duration::from_secs(60), held)
            .await
            .expect(
                "the chatrecall dispatch never reached the seam: the rendezvous key or the \
                 hold point has drifted",
            )
            .unwrap();

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

    // ─────────── DR-26 / Task 50 Step 3: the third axis on chat recall ───────

    fn bound_to(name: &str) -> CallCapability {
        CallCapability::for_test_affiliated(
            ProviderTier::Private,
            true,
            Some(crate::privacy::affiliation::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new(name),
            )),
        )
    }

    /// The operator's case, arriving through recall instead of through a tool
    /// call: **both** endpoints are private, so every tier gate in this campaign
    /// says yes, and only DR-26 refuses.
    ///
    /// The refusal names the institution — DR-26 requires a warning specific
    /// enough to act on — and still names no session, no title and no working
    /// directory, because §11.4 classifies all three as content.
    #[tokio::test]
    async fn a_chat_that_reached_one_institution_is_not_recallable_from_another() {
        let h = Harness::new().await;
        let target = h
            .private_session_named(
                "OMOP diabetes cohort characterisation",
                "/data/phi/cohort-2026-dm2",
            )
            .await;
        h.sm.record_session_affiliation(
            &target.id,
            crate::privacy::affiliation::InstitutionId::new("ucsf"),
        )
        .await
        .unwrap();

        let text = h.load_via(bound_to("stanford"), &target.id).await.unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .clone();
        assert!(text.contains("Cross-institutional"), "{text}");
        assert!(text.contains("ucsf"), "the owning institution: {text}");
        assert!(text.contains("stanford"), "the bound institution: {text}");
        assert!(!text.contains("OMOP"), "leaked the session name: {text}");
        assert!(
            !text.contains("cohort-2026-dm2"),
            "leaked the working dir: {text}"
        );

        // The institution's own model still reads it, or the gate is just
        // "refuse everyone".
        assert!(h.load_via(bound_to("ucsf"), &target.id).await.unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .contains("OMOP diabetes cohort"));
        // …and so does a local model, which transfers nothing.
        assert!(h
            .load_via_private_capability_caller(&target.id)
            .await
            .unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .contains("OMOP diabetes cohort"));
    }

    /// The union rule, at the surface. A chat that reached BOTH institutions'
    /// connectors is recallable from neither of their models — the row an
    /// implementation reaching for `contains` gets wrong, and the one Task 50's
    /// gate names.
    #[tokio::test]
    async fn a_chat_that_reached_two_institutions_is_recallable_from_neither() {
        let h = Harness::new().await;
        let target = h.private_session_named("joint study", "/data/joint").await;
        for institution in ["ucsf", "stanford"] {
            h.sm.record_session_affiliation(
                &target.id,
                crate::privacy::affiliation::InstitutionId::new(institution),
            )
            .await
            .unwrap();
        }

        for institution in ["ucsf", "stanford"] {
            let text = h.load_via(bound_to(institution), &target.id).await.unwrap()[0]
                .as_text()
                .unwrap()
                .text
                .clone();
            assert!(
                text.contains("Cross-institutional"),
                "a model covered by {institution} alone read a chat that reached both: {text}"
            );
            assert!(!text.contains("joint study"), "leaked the name: {text}");
        }
        // Local reaches it; it never compares.
        assert!(h
            .load_via_private_capability_caller(&target.id)
            .await
            .unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .contains("joint study"));
    }

    /// A chat that touched no institution's extension is recallable from every
    /// private model — the Missing direction. Without this the gate would refuse
    /// every recall in an ordinary chat, which is the failure that gets a
    /// control turned off.
    #[tokio::test]
    async fn an_unaffiliated_private_chat_is_recallable_from_any_private_model() {
        let h = Harness::new().await;
        let target = h.private_session_named("ordinary notes", "/tmp/n").await;
        for cap in [bound_to("ucsf"), bound_to("stanford")] {
            assert!(h.load_via(cap, &target.id).await.unwrap()[0]
                .as_text()
                .unwrap()
                .text
                .contains("ordinary notes"));
        }
    }

    /// SEARCH is the wider hole of the two: LOAD names one session, SEARCH
    /// sweeps every session in the store and returns snippets. The filter is in
    /// SQL, ahead of the `LIMIT`, exactly as the tier filter beside it.
    #[tokio::test]
    async fn search_hides_another_institutions_chat_and_keeps_its_own() {
        let h = Harness::new().await;
        let ucsf = h
            .session_containing("ucsf work", "/data/u", true, "SENTINELWORD cohort")
            .await;
        h.sm.record_session_affiliation(
            &ucsf.id,
            crate::privacy::affiliation::InstitutionId::new("ucsf"),
        )
        .await
        .unwrap();
        let unaffiliated = h
            .session_containing("plain work", "/data/p", true, "SENTINELWORD notes")
            .await;

        let text = h
            .search_via(bound_to("stanford"), "SENTINELWORD")
            .await
            .unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .clone();
        // The rendered result carries the working dir and the transcript, not
        // the (LLM-generated) name, so those are what a leak would show.
        assert!(
            !text.contains("/data/u") && !text.contains("cohort"),
            "a Stanford-covered model saw a UCSF chat's transcript: {text}"
        );
        assert!(
            text.contains("/data/p") && text.contains("notes"),
            "the filter must not hide a chat no institution claims: {text}"
        );
        let _ = unaffiliated;

        // UCSF's own model sees both, or the filter is just "hide everything".
        let mine = h
            .search_via(bound_to("ucsf"), "SENTINELWORD")
            .await
            .unwrap()[0]
            .as_text()
            .unwrap()
            .text
            .clone();
        assert!(
            mine.contains("/data/u") && mine.contains("cohort"),
            "{mine}"
        );
        assert!(mine.contains("/data/p"), "{mine}");
    }
}
