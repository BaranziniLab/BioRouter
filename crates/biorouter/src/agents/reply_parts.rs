use anyhow::Result;
use std::sync::Arc;

use async_stream::try_stream;
use futures::stream::StreamExt;
use serde_json::{json, Value};
use tracing::debug;

use super::super::agents::Agent;
use crate::conversation::message::{Message, MessageContent, ToolRequest};
use crate::conversation::Conversation;
use crate::providers::base::{
    stream_from_single_message, MessageStream, Provider, ProviderSteerReceiver, ProviderUsage,
};
use crate::providers::errors::ProviderError;
use crate::providers::toolshim::{
    augment_message_with_tool_calls, convert_tool_messages_to_text,
    modify_system_prompt_for_tool_json, OllamaInterpreter,
};

use crate::agents::code_execution_extension::EXTENSION_NAME as CODE_EXECUTION_EXTENSION;
use crate::agents::subagent_tool::{SUBAGENT_TOOL_NAME, SUBAGENT_TOOL_PREFIXED};
use crate::session::SessionType;

const SUBAGENT_STEERING_INSTRUCTIONS: &str = "A human can send a new user message directly into this subagent while it is running. At the next safe boundary, the newest direct user message supersedes any conflicting earlier task instruction. Act on it immediately before continuing prior work; do not merely record it or finish the old task first.";
use crate::session::session_manager::UsageLedgerEntry;
use rmcp::model::Tool;

fn attach_skill_inventory(
    extensions_info: &mut [crate::agents::extension::ExtensionInfo],
    inventory: &str,
) -> bool {
    let Some(skills) = extensions_info.iter_mut().find(|info| {
        info.classification == crate::agents::extension::ExtensionClassification::Capability
            && info.name.eq_ignore_ascii_case("skills")
    }) else {
        return false;
    };
    if !inventory.is_empty() {
        skills.instructions.push_str("\n\n");
        skills.instructions.push_str(inventory);
    }
    true
}

pub(super) fn attach_effective_tool_rosters(
    extensions_info: &mut [crate::agents::extension::ExtensionInfo],
    available_tools: &[Tool],
    directly_callable_tools: &[Tool],
) {
    for info in extensions_info {
        info.tool_roster_known = true;
        let prefix = format!("{}__", info.name);
        info.available_tools = available_tools
            .iter()
            .filter_map(|tool| tool.name.strip_prefix(&prefix).map(str::to_string))
            .collect();
        info.directly_callable_tools = directly_callable_tools
            .iter()
            .filter_map(|tool| tool.name.strip_prefix(&prefix).map(str::to_string))
            .collect();
        info.available_tools.sort();
        info.directly_callable_tools.sort();

        if info.classification == crate::agents::extension::ExtensionClassification::Capability
            && info.name == crate::agents::workspace_extension::EXTENSION_NAME
            && !info
                .available_tools
                .iter()
                .any(|tool| tool == "workspace_open")
        {
            info.instructions = "Workspace delegation-only mode. Use only the exact effective roster above to spawn, inspect, steer, watch, or close work. Panel control, opening arbitrary conversations, and changing another conversation's tools are unavailable unless their tools are explicitly listed.".to_string();
        }
        if info.classification == crate::agents::extension::ExtensionClassification::Capability
            && info.name == CODE_EXECUTION_EXTENSION
            && !info
                .available_tools
                .iter()
                .any(|tool| tool == "execute_code")
        {
            info.instructions = "Code Execution is restricted to the effective inspection tools above. Do not run code or hide other direct tools unless `execute_code` is present.".to_string();
        }
    }
}

pub(super) fn add_core_platform_capability(
    extensions_info: &mut Vec<crate::agents::extension::ExtensionInfo>,
    tools: &[Tool],
) {
    if tools.iter().any(|tool| tool.name.starts_with("platform__"))
        && !extensions_info.iter().any(|info| info.name == "platform")
    {
        extensions_info.push(crate::agents::extension::ExtensionInfo::capability(
            "platform",
            "Core Biorouter operations supplied by the current application session. Use only the exact effective roster; knowledge ingestion appears here only while Knowledge is enabled.",
            false,
        ));
    }
}

fn coerce_value(s: &str, schema: &Value) -> Value {
    let type_str = schema.get("type");

    match type_str {
        Some(Value::String(t)) => match t.as_str() {
            "number" | "integer" => try_coerce_number(s),
            "boolean" => try_coerce_boolean(s),
            _ => Value::String(s.to_string()),
        },
        Some(Value::Array(types)) => {
            // Try each type in order
            for t in types {
                if let Value::String(type_name) = t {
                    match type_name.as_str() {
                        "number" | "integer" if s.parse::<f64>().is_ok() => {
                            return try_coerce_number(s)
                        }
                        "boolean" if matches!(s.to_lowercase().as_str(), "true" | "false") => {
                            return try_coerce_boolean(s)
                        }
                        _ => continue,
                    }
                }
            }
            Value::String(s.to_string())
        }
        _ => Value::String(s.to_string()),
    }
}

fn try_coerce_number(s: &str) -> Value {
    if let Ok(n) = s.parse::<f64>() {
        if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
            json!(n as i64)
        } else {
            json!(n)
        }
    } else {
        Value::String(s.to_string())
    }
}

fn try_coerce_boolean(s: &str) -> Value {
    match s.to_lowercase().as_str() {
        "true" => json!(true),
        "false" => json!(false),
        _ => Value::String(s.to_string()),
    }
}

fn coerce_tool_arguments(
    arguments: Option<serde_json::Map<String, Value>>,
    tool_schema: &Value,
) -> Option<serde_json::Map<String, Value>> {
    let args = arguments?;

    let properties = tool_schema.get("properties").and_then(|p| p.as_object())?;

    let mut coerced = serde_json::Map::new();

    for (key, value) in args.iter() {
        let coerced_value =
            if let (Value::String(s), Some(prop_schema)) = (value, properties.get(key)) {
                coerce_value(s, prop_schema)
            } else {
                value.clone()
            };
        coerced.insert(key.clone(), coerced_value);
    }

    Some(coerced)
}

/// Survives the code-execution tool filter?
///
/// When the `code_execution` extension is active the model is meant to reach
/// ordinary tools by *writing code*, so the tool list collapses to
/// `code_execution__*`. Two families are exempt because code cannot express
/// them: spawning a subagent (it runs its own agent loop, not a function call),
/// and BR-71's workspace control (it operates the daemon and the GUI, not the
/// sandbox). Both name forms of the spawn tool are kept — models strip prefixes.
pub(crate) fn survives_code_execution_filter(tool_name: &str, code_exec_prefix: &str) -> bool {
    tool_name.starts_with(code_exec_prefix)
        || tool_name == SUBAGENT_TOOL_NAME
        || tool_name == SUBAGENT_TOOL_PREFIXED
        // BR-40's separate poll tool used to need its own clause here, so a
        // model that spawned a background child could still collect it. BR-71
        // decision 23 deleted that tool: collecting a background child is now
        // `workspace_watch` / `workspace_read_conversation` / `workspace_close`,
        // which the `workspace__` prefix below already exempts.
        || tool_name.starts_with("workspace__")
}

fn code_execution_mode_is_active(loaded: bool, tools: &[Tool]) -> bool {
    loaded
        && tools
            .iter()
            .any(|tool| tool.name == format!("{CODE_EXECUTION_EXTENSION}__execute_code"))
}

async fn toolshim_postprocess(
    response: Message,
    toolshim_tools: &[Tool],
) -> Result<Message, ProviderError> {
    let interpreter = OllamaInterpreter::new().map_err(|e| {
        ProviderError::ExecutionError(format!("Failed to create OllamaInterpreter: {}", e))
    })?;

    augment_message_with_tool_calls(&interpreter, response, toolshim_tools)
        .await
        .map_err(|e| ProviderError::ExecutionError(format!("Failed to augment message: {}", e)))
}

impl Agent {
    pub async fn prepare_tools_and_prompt(
        &self,
        session_id: &str,
        working_dir: &std::path::Path,
    ) -> Result<(Vec<Tool>, Vec<Tool>, String)> {
        // Get tools from extension manager
        let mut tools = self.list_tools(session_id, None).await;

        // Add frontend tools
        let frontend_tools = self.frontend_tools.lock().await;
        for frontend_tool in frontend_tools.values() {
            tools.push(frontend_tool.tool.clone());
        }
        drop(frontend_tools);

        let code_execution_loaded = self
            .extension_manager
            .is_extension_enabled(CODE_EXECUTION_EXTENSION)
            .await;
        let effective_tools = tools.clone();
        let code_execution_active =
            code_execution_mode_is_active(code_execution_loaded, &effective_tools);
        if code_execution_active {
            let code_exec_prefix = format!("{CODE_EXECUTION_EXTENSION}__");
            tools.retain(|tool| survives_code_execution_filter(&tool.name, &code_exec_prefix));
        }

        // Stable tool ordering is important for multi session prompt caching.
        tools.sort_by(|a, b| a.name.cmp(&b.name));

        // BR-18: re-grade the risk registry from *this* tool list — the exact set
        // the model can call from. Doing it here (rather than off the extension
        // manager alone) means platform, frontend, subagent and final-output tools
        // are graded too, and a tool that just disappeared (extension disabled,
        // code-execution filter applied) stops being auto-approvable. Cheap: a
        // hashmap rebuild over a few dozen already-materialised tools, on a path
        // that is already doing a `list_tools` + prompt render.
        self.tool_risks.refresh_from_tools(&tools);

        // Prepare system prompt
        let mut extensions_info = self.extension_manager.get_extensions_info().await;
        add_core_platform_capability(&mut extensions_info, &effective_tools);
        attach_effective_tool_rosters(&mut extensions_info, &effective_tools, &tools);
        if extensions_info.iter().any(|info| {
            info.classification == crate::agents::extension::ExtensionClassification::Capability
                && info.name.eq_ignore_ascii_case("skills")
                && !info.available_tools.is_empty()
        }) {
            let inventory = crate::agents::skills_extension::session_skill_inventory_instructions(
                self.config.session_manager.as_ref(),
                session_id,
            )
            .await?;
            attach_skill_inventory(&mut extensions_info, &inventory);
        }

        // Get model name from provider
        let provider = self.provider().await?;
        let model_config = provider.get_model_config();

        // BR-3: pick the system-prompt variant for this provider/model so small
        // local models get extra scaffolding while strong models stay lean.
        let prompt_variant = crate::agents::prompt_manager::PromptVariant::select(
            provider.get_name(),
            &model_config.model_name,
        );

        let prompt_manager = self.prompt_manager.lock().await;
        let mut system_prompt = prompt_manager
            .builder()
            .with_extensions(extensions_info.into_iter())
            .with_frontend_instructions(self.frontend_instructions.lock().await.clone())
            .with_code_execution_mode(code_execution_active)
            .with_hints(working_dir)
            .with_enable_subagents(self.subagents_enabled(session_id).await)
            .with_prompt_variant(prompt_variant)
            .build();

        let is_subagent = matches!(
            self.config
                .session_manager
                .get_session(session_id, false)
                .await
                .ok()
                .map(|session| session.session_type),
            Some(SessionType::SubAgent)
        );
        if is_subagent {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(SUBAGENT_STEERING_INSTRUCTIONS);
        }

        // Handle toolshim if enabled
        let mut toolshim_tools = vec![];
        if model_config.toolshim {
            // If tool interpretation is enabled, modify the system prompt
            system_prompt = modify_system_prompt_for_tool_json(&system_prompt, &tools);
            // Make a copy of tools before emptying
            toolshim_tools = tools.clone();
            // Empty the tools vector for provider completion
            tools = vec![];
        }

        Ok((tools, toolshim_tools, system_prompt))
    }

    /// Stream a response from the LLM provider.
    /// Handles toolshim transformations if needed
    pub(crate) async fn stream_response_from_provider(
        provider: Arc<dyn Provider>,
        system_prompt: &str,
        messages: &[Message],
        tools: &[Tool],
        toolshim_tools: &[Tool],
        steering: Option<ProviderSteerReceiver>,
    ) -> Result<MessageStream, ProviderError> {
        let config = provider.get_model_config();

        // Convert tool messages to text if toolshim is enabled
        let messages_for_provider = if config.toolshim {
            convert_tool_messages_to_text(messages)
        } else {
            Conversation::new_unvalidated(messages.to_vec())
        };

        // Clone owned data to move into the async stream
        let system_prompt = system_prompt.to_owned();
        let tools = tools.to_owned();
        let toolshim_tools = toolshim_tools.to_owned();
        let provider = provider.clone();

        // Capture errors during stream creation and return them as part of the stream
        // so they can be handled by the existing error handling logic in the agent
        // Marker semantics (deliberately explicit — the previous names were
        // actively misleading, see the Stage 0 note in
        // `docs/history/streaming-tool-call-ui-2026-07/tool-call-ui-latency-investigation.md` §4.4).
        //
        // `provider.stream()` returns as soon as the response *headers* arrive;
        // token generation happens later, while the returned stream is polled.
        // So the OPEN -> OPENED span is request-serialize + connect + TTFB
        // ONLY, and is NOT comparable to the non-streaming pair below, which
        // brackets an entire blocking generation. Three distinct spans:
        //
        //   OPEN      -> OPENED     : connect + TTFB
        //   OPENED    -> EXHAUSTED  : generation (tokens streaming back)
        //   OPEN      -> EXHAUSTED  : total
        //
        // The OPENED -> EXHAUSTED == generation identity holds ONLY on the
        // streaming path. The non-streaming branch below funnels its single
        // completed message through `stream_from_single_message` into the same
        // `try_stream!`, so it also emits EXHAUSTED — but never an OPENED, and
        // its EXHAUSTED total is the same span FULL_GENERATION_END already
        // reported. Every marker therefore carries `streaming=<bool>` so a log
        // that mixes both paths (which a single session does — see the
        // investigation, §0) can be partitioned without inferring anything from
        // line adjacency. Pair OPENED with EXHAUSTED only within streaming=true.
        let streaming = provider.supports_streaming();
        let stream_open_start = std::time::Instant::now();
        let stream_result = if streaming {
            debug!(streaming, "WAITING_LLM_STREAM_OPEN");
            let result = match steering {
                Some(steering) => {
                    provider
                        .stream_with_steering(
                            system_prompt.as_str(),
                            messages_for_provider.messages(),
                            &tools,
                            steering,
                        )
                        .await
                }
                None => {
                    provider
                        .stream(
                            system_prompt.as_str(),
                            messages_for_provider.messages(),
                            &tools,
                        )
                        .await
                }
            };
            debug!(
                streaming,
                open_ms = stream_open_start.elapsed().as_millis() as u64,
                "WAITING_LLM_STREAM_OPENED"
            );
            result
        } else {
            // Non-streaming: this pair brackets the FULL generation (request +
            // connect + all tokens), unlike the streaming OPEN/OPENED pair.
            debug!(streaming, "WAITING_LLM_FULL_GENERATION_START");
            let complete_result = provider
                .complete(
                    system_prompt.as_str(),
                    messages_for_provider.messages(),
                    &tools,
                )
                .await;
            debug!(
                streaming,
                total_ms = stream_open_start.elapsed().as_millis() as u64,
                "WAITING_LLM_FULL_GENERATION_END"
            );

            match complete_result {
                Ok((message, usage)) => Ok(stream_from_single_message(message, usage)),
                Err(e) => Err(e),
            }
        };

        // If there was an error creating the stream, return a stream that yields that error
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                // Return a stream that immediately yields the error
                // This allows the error to be caught by existing error handling in agent.rs
                return Ok(Box::pin(try_stream! {
                    yield Err(e)?;
                }));
            }
        };

        Ok(Box::pin(try_stream! {
            while let Some(result) = stream.next().await {
                let (mut message, usage, pending) = result?;

                // Store the model information in the global store
                if let Some(usage) = usage.as_ref() {
                    crate::providers::base::set_current_model(&usage.model);
                }

                // Post-process / structure the response only if tool interpretation is enabled
                if message.is_some() && config.toolshim {
                    message = Some(toolshim_postprocess(message.unwrap(), &toolshim_tools).await?);
                }

                // `pending` is a display-only tool-call notification. It travels
                // its own slot, never becoming a `Message`, so it can never be
                // dispatched. Forwarded verbatim to the agent loop.
                yield (message, usage, pending);
            }

            // The stream is drained: generation is genuinely finished. This is
            // the marker the old instrumentation lacked entirely, which is why
            // generation time was invisible and prior investigations attributed
            // the whole model wait to the (tiny) stream-open span.
            //
            // Emitted on BOTH paths — on the non-streaming path this is a
            // one-element stream and the total duplicates FULL_GENERATION_END.
            // `streaming` is what makes the two distinguishable; it also lets a
            // report count OPENED vs EXHAUSTED per path, which is how a turn
            // aborted before the stream drained (no EXHAUSTED) shows up.
            debug!(
                streaming,
                total_ms = stream_open_start.elapsed().as_millis() as u64,
                "WAITING_LLM_STREAM_EXHAUSTED"
            );
        }))
    }

    /// Categorize tool requests from the response into different types
    /// Returns:
    /// - frontend_requests: Tool requests that should be handled by the frontend
    /// - other_requests: All other tool requests (including requests to enable extensions)
    /// - filtered_message: The original message with frontend tool requests removed
    pub(crate) async fn categorize_tool_requests(
        &self,
        response: &Message,
        tools: &[Tool],
    ) -> (Vec<ToolRequest>, Vec<ToolRequest>, Message) {
        // First collect all tool requests with coercion applied
        let tool_requests: Vec<ToolRequest> = response
            .content
            .iter()
            .filter_map(|content| {
                if let MessageContent::ToolRequest(req) = content {
                    let mut coerced_req = req.clone();

                    if let Ok(ref mut tool_call) = coerced_req.tool_call {
                        if let Some(tool) = tools.iter().find(|t| t.name == tool_call.name) {
                            let schema_value = Value::Object(tool.input_schema.as_ref().clone());
                            tool_call.arguments =
                                coerce_tool_arguments(tool_call.arguments.clone(), &schema_value);

                            if let Some(ref meta) = tool.meta {
                                coerced_req.tool_meta = serde_json::to_value(meta).ok();
                            }
                        }
                    }

                    Some(coerced_req)
                } else {
                    None
                }
            })
            .collect();

        // Create a filtered message with frontend tool requests removed
        let mut filtered_content = Vec::new();
        let mut tool_request_index = 0;

        for content in &response.content {
            match content {
                MessageContent::ToolRequest(_) => {
                    if tool_request_index < tool_requests.len() {
                        let coerced_req = &tool_requests[tool_request_index];
                        tool_request_index += 1;

                        let should_include = if let Ok(tool_call) = &coerced_req.tool_call {
                            !self.is_frontend_tool(&tool_call.name).await
                        } else {
                            true
                        };

                        if should_include {
                            filtered_content.push(MessageContent::ToolRequest(coerced_req.clone()));
                        }
                    }
                }
                _ => {
                    filtered_content.push(content.clone());
                }
            }
        }

        let mut filtered_message =
            Message::new(response.role.clone(), response.created, filtered_content);

        // Preserve the ID if it exists
        if let Some(id) = response.id.clone() {
            filtered_message = filtered_message.with_id(id);
        }

        // Categorize tool requests
        let mut frontend_requests = Vec::new();
        let mut other_requests = Vec::new();

        for request in tool_requests {
            if let Ok(tool_call) = &request.tool_call {
                if self.is_frontend_tool(&tool_call.name).await {
                    frontend_requests.push(request);
                } else {
                    other_requests.push(request);
                }
            } else {
                // If there's an error in the tool call, add it to other_requests
                other_requests.push(request);
            }
        }

        (frontend_requests, other_requests, filtered_message)
    }

    pub(crate) async fn update_session_metrics(
        &self,
        session_config: &crate::agents::types::SessionConfig,
        usage: &ProviderUsage,
        is_compaction_usage: bool,
        event_key: &str,
    ) -> Result<()> {
        let provider_name = match usage.provider.clone() {
            Some(provider) => Some(provider),
            None => self
                .provider()
                .await
                .ok()
                .map(|provider| provider.get_name().to_string()),
        };
        apply_session_metrics(
            &self.config.session_manager,
            session_config,
            usage,
            is_compaction_usage,
            event_key,
            provider_name,
        )
        .await
    }
}

pub(crate) async fn apply_session_metrics(
    manager: &crate::session::SessionManager,
    session_config: &crate::agents::types::SessionConfig,
    usage: &ProviderUsage,
    is_compaction_usage: bool,
    event_key: &str,
    provider_name: Option<String>,
) -> Result<()> {
    let session_id = session_config.id.as_str();
    let (current_total, current_input, current_output) = if is_compaction_usage {
        let new_input = usage.usage.output_tokens;
        (new_input, new_input, None)
    } else {
        (
            usage.usage.total_tokens,
            usage.usage.input_tokens,
            usage.usage.output_tokens,
        )
    };

    let billed_total = usage.usage.billed_total();
    if billed_total.is_some() || usage.usage.total_tokens.is_some() {
        manager
            .apply_usage_event(UsageLedgerEntry {
                event_key: event_key.to_string(),
                session_id: session_id.to_string(),
                schedule_id: session_config.schedule_id.clone(),
                current_total_tokens: current_total,
                current_input_tokens: current_input,
                current_output_tokens: current_output,
                billed_total_tokens: billed_total,
                input_tokens: usage.usage.input_tokens,
                output_tokens: usage.usage.output_tokens,
                model_id: Some(usage.model.clone()),
                provider: provider_name,
                cache_read_tokens: Some(usage.usage.cache_read_input_tokens.unwrap_or(0)),
                cache_creation_tokens: Some(usage.usage.cache_creation_input_tokens.unwrap_or(0)),
            })
            .await?;
    } else {
        manager
            .update(session_id)
            .schedule_id(session_config.schedule_id.clone())
            .apply()
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use crate::model::ModelConfig;
    use crate::providers::base::{Provider, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use async_trait::async_trait;
    use rmcp::{
        handler::server::router::tool::ToolRouter, object, tool, tool_handler, tool_router,
    };

    #[derive(Clone)]
    struct MockProvider {
        model_config: ModelConfig,
    }

    #[async_trait]
    impl Provider for MockProvider {
        fn metadata() -> crate::providers::base::ProviderMetadata {
            crate::providers::base::ProviderMetadata::empty()
        }

        fn get_name(&self) -> &str {
            "mock"
        }

        fn get_model_config(&self) -> ModelConfig {
            self.model_config.clone()
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> anyhow::Result<(Message, ProviderUsage), ProviderError> {
            Ok((
                Message::assistant().with_text("ok"),
                ProviderUsage::new("mock".to_string(), Usage::default()),
            ))
        }
    }

    #[test]
    fn live_skill_inventory_attaches_only_to_the_enabled_skills_capability() {
        use crate::agents::extension::{ExtensionClassification, ExtensionInfo};

        let mut enabled = vec![ExtensionInfo::capability("skills", "static", false)];
        assert!(attach_skill_inventory(&mut enabled, "live generation 2"));
        assert_eq!(enabled[0].instructions, "static\n\nlive generation 2");

        let mut installed_extension = vec![ExtensionInfo::classified(
            "skills",
            "third party",
            false,
            ExtensionClassification::Extension,
        )];
        assert!(!attach_skill_inventory(
            &mut installed_extension,
            "must not attach"
        ));
        assert_eq!(installed_extension[0].instructions, "third party");
    }

    #[test]
    fn effective_tool_rosters_distinguish_module_access_from_direct_calls() {
        use crate::agents::extension::ExtensionInfo;

        let mut entries = vec![ExtensionInfo::capability("developer", "guidance", false)];
        let available = vec![
            Tool::new("developer__shell", "shell", object!({"type": "object"})),
            Tool::new(
                "developer__text_editor",
                "editor",
                object!({"type": "object"}),
            ),
        ];
        let direct = vec![available[0].clone()];

        attach_effective_tool_rosters(&mut entries, &available, &direct);
        assert_eq!(entries[0].available_tools, ["shell", "text_editor"]);
        assert_eq!(entries[0].directly_callable_tools, ["shell"]);

        let platform = vec![Tool::new(
            "platform__ingest_conversation".to_string(),
            "ingest".to_string(),
            object!({"type": "object"}),
        )];
        add_core_platform_capability(&mut entries, &platform);
        attach_effective_tool_rosters(&mut entries, &platform, &platform);
        let core = entries.iter().find(|info| info.name == "platform").unwrap();
        assert_eq!(core.available_tools, ["ingest_conversation"]);
        assert_eq!(core.directly_callable_tools, ["ingest_conversation"]);
    }

    #[tokio::test]
    async fn prepare_tools_returns_sorted_tools_including_frontend() -> anyhow::Result<()> {
        let agent = crate::agents::Agent::new();

        let session = agent
            .config
            .session_manager
            .create_session(
                std::path::PathBuf::default(),
                "test-prepare-tools".to_string(),
                SessionType::Hidden,
            )
            .await?;

        let model_config = ModelConfig::new("test-model").unwrap();
        let provider = std::sync::Arc::new(MockProvider { model_config });
        agent.update_provider(provider, &session.id).await?;

        // Add unsorted frontend tools
        let frontend_tools = vec![
            Tool::new(
                "frontend__z_tool".to_string(),
                "Z tool".to_string(),
                object!({ "type": "object", "properties": { } }),
            ),
            Tool::new(
                "frontend__a_tool".to_string(),
                "A tool".to_string(),
                object!({ "type": "object", "properties": { } }),
            ),
        ];

        agent
            .add_extension(crate::agents::extension::ExtensionConfig::Frontend {
                name: "frontend".to_string(),
                description: "desc".to_string(),
                tools: frontend_tools,
                instructions: None,
                bundled: None,
                available_tools: vec![],
            })
            .await
            .unwrap();

        let working_dir = std::env::current_dir()?;
        let (tools, _toolshim_tools, _system_prompt) = agent
            .prepare_tools_and_prompt(&session.id, &working_dir)
            .await?;

        let names: Vec<String> = tools.iter().map(|t| t.name.clone().into_owned()).collect();
        assert!(names.iter().any(|n| n == "frontend__a_tool"));
        assert!(names.iter().any(|n| n == "frontend__z_tool"));

        // Verify the names are sorted ascending
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);

        Ok(())
    }

    #[tokio::test]
    async fn subagent_prompt_makes_the_latest_direct_human_message_authoritative(
    ) -> anyhow::Result<()> {
        let agent = crate::agents::Agent::new();
        let session = agent
            .config
            .session_manager
            .create_session(
                std::path::PathBuf::default(),
                format!("steerable-subagent-{}", uuid::Uuid::new_v4()),
                SessionType::SubAgent,
            )
            .await?;
        agent
            .update_provider(
                Arc::new(MockProvider {
                    model_config: ModelConfig::new("test-model").unwrap(),
                }),
                &session.id,
            )
            .await?;

        let (_, _, system_prompt) = agent
            .prepare_tools_and_prompt(&session.id, std::path::Path::new("."))
            .await?;

        assert!(system_prompt.contains(SUBAGENT_STEERING_INSTRUCTIONS));
        assert!(system_prompt.contains("supersedes any conflicting earlier task instruction"));
        Ok(())
    }

    #[test]
    fn the_code_execution_filter_keeps_the_prefixed_spawn_tool_and_workspace_tools() {
        let prefix = format!("{CODE_EXECUTION_EXTENSION}__");
        // Kept: the sandbox itself …
        assert!(survives_code_execution_filter(
            &format!("{prefix}execute_code"),
            &prefix
        ));
        // … both spellings of the spawn tool — a filter that knows only the
        // bare name deletes delegation from every default session …
        assert!(survives_code_execution_filter("subagent", &prefix));
        assert!(survives_code_execution_filter(
            "workspace__subagent",
            &prefix
        ));
        // … and the whole workspace surface, or enabling Workspace Control
        // silently does nothing in the default configuration.
        assert!(survives_code_execution_filter(
            "workspace__workspace_list",
            &prefix
        ));
        assert!(survives_code_execution_filter(
            "workspace__workspace_send_prompt",
            &prefix
        ));
        // Dropped: everything the model is supposed to reach through code.
        assert!(!survives_code_execution_filter("developer__shell", &prefix));
        assert!(!survives_code_execution_filter("memory__remember", &prefix));
    }

    #[test]
    fn code_execution_presence_without_executor_does_not_hide_direct_tools() {
        let inspection_only = vec![Tool::new(
            "code_execution__read_module".to_string(),
            "read".to_string(),
            object!({ "type": "object", "properties": {} }),
        )];
        assert!(!code_execution_mode_is_active(true, &inspection_only));

        let executor = vec![Tool::new(
            "code_execution__execute_code".to_string(),
            "execute".to_string(),
            object!({ "type": "object", "properties": {} }),
        )];
        assert!(code_execution_mode_is_active(true, &executor));
        assert!(!code_execution_mode_is_active(false, &executor));
    }

    // ------------------------------------------------------------------
    // Issue #56 Gate F2: a private server's own INSTRUCTIONS.
    //
    // `get_extensions_info` hands every installed extension's instruction
    // text to the prompt builder, and the result is the system prompt of
    // EVERY turn. Gate E filters `filter_tools`, a different function on a
    // different path, so a private connector's tool names could be hidden
    // while its prose — table names, cohort semantics, credential scope —
    // still shipped to a public model on every request.
    // ------------------------------------------------------------------

    /// One of the two extensions the compiled-in BAAM baseline calls private.
    const PRIVATE_EXTENSION: &str = "ucsfomopagent";
    const SENTINEL: &str = "SENTINEL-INSTRUCTIONS";

    /// An in-process MCP server whose only distinguishing feature is the
    /// instruction text the manager hands the prompt builder. Injected through
    /// the real admission point, so the tier under test is the one
    /// `classify_extension` resolves rather than one poked into the record.
    #[derive(Clone)]
    struct InstructedServer {
        tool_router: ToolRouter<Self>,
    }

    #[tool_router(router = tool_router)]
    impl InstructedServer {
        fn new() -> Self {
            Self {
                tool_router: Self::tool_router(),
            }
        }

        #[tool(description = "Private extension fixture tool")]
        fn fixture_tool(&self) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::Content::text("ok"),
            ]))
        }
    }

    #[tool_handler(router = self.tool_router)]
    impl rmcp::ServerHandler for InstructedServer {
        fn get_info(&self) -> rmcp::model::ServerInfo {
            rmcp::model::ServerInfo {
                capabilities: rmcp::model::ServerCapabilities::builder()
                    .enable_tools()
                    .build(),
                instructions: Some(SENTINEL.to_string()),
                ..Default::default()
            }
        }
    }

    struct TieredProvider {
        tier: crate::privacy::ProviderTier,
    }

    #[async_trait]
    impl Provider for TieredProvider {
        fn metadata() -> crate::providers::base::ProviderMetadata {
            crate::providers::base::ProviderMetadata::empty()
        }

        fn get_name(&self) -> &str {
            "plain"
        }

        fn tier(&self) -> crate::privacy::ProviderTier {
            self.tier
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail("plain-model")
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> anyhow::Result<(Message, ProviderUsage), ProviderError> {
            Ok((
                Message::assistant().with_text("ok"),
                ProviderUsage::new("plain-model".to_string(), Usage::default()),
            ))
        }
    }

    /// Bind a model of `tier` and render the system prompt the way a turn
    /// does. The assertion is on the RENDERED PROMPT, not on the tool list:
    /// asserting on the tools is the wrong-implementation trap, because Gate E
    /// already hides those and the instructions still ship.
    async fn system_prompt_under(
        agent: &crate::agents::Agent,
        session_id: &str,
        working_dir: &std::path::Path,
        tier: crate::privacy::ProviderTier,
    ) -> String {
        agent
            .update_provider(Arc::new(TieredProvider { tier }), session_id)
            .await
            .expect("binding a model of this tier is legal on a public session");
        let (_tools, _toolshim_tools, system_prompt) = agent
            .prepare_tools_and_prompt(session_id, working_dir)
            .await
            .expect("the real system-prompt path");
        system_prompt
    }

    #[tokio::test]
    async fn a_private_servers_instructions_do_not_reach_a_public_system_prompt() {
        use crate::privacy::ProviderTier::{Private, Public};

        let dir = tempfile::tempdir().unwrap();
        let agent = crate::agents::Agent::with_config(crate::agents::AgentConfig::new(
            Arc::new(crate::session::SessionManager::new(
                dir.path().to_path_buf(),
            )),
            Arc::new(crate::config::permission::PermissionManager::new(
                dir.path().to_path_buf(),
            )),
            None,
            crate::config::BioRouterMode::Auto,
        ));
        let session = agent
            .config
            .session_manager
            .create_session(
                dir.path().to_path_buf(),
                "gate-f".to_string(),
                SessionType::Hidden,
            )
            .await
            .unwrap();
        agent
            .extension_manager
            .add_inprocess_server(PRIVATE_EXTENSION, InstructedServer::new())
            .await
            .expect("inject the private extension");

        // Public first: the session is public, so both binds below are legal
        // and the order cannot be reversed.
        let public_prompt = system_prompt_under(&agent, &session.id, dir.path(), Public).await;
        assert!(
            !public_prompt.contains(SENTINEL),
            "a private server's instructions reached a public model's system prompt"
        );

        let private_prompt = system_prompt_under(&agent, &session.id, dir.path(), Private).await;
        assert!(
            private_prompt.contains(SENTINEL),
            "the model entitled to the extension must still get its instructions"
        );
    }

    #[tokio::test]
    async fn test_stream_error_propagation() {
        use futures::StreamExt;

        type StreamItem = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>;
        let stream = futures::stream::iter(vec![
            Ok((Some(Message::assistant().with_text("chunk1")), None)),
            Ok((Some(Message::assistant().with_text("chunk2")), None)),
            Err(ProviderError::RequestFailed(
                "simulated stream error".to_string(),
            )),
        ] as Vec<StreamItem>);

        let mut pinned = Box::pin(stream);
        let mut results = Vec::new();
        let mut error_seen = false;

        while let Some(result) = pinned.next().await {
            match result {
                Ok((message, _usage)) => {
                    if let Some(msg) = message {
                        results.push(msg.as_concat_text());
                    }
                }
                Err(_e) => {
                    error_seen = true;
                    break;
                }
            }
        }

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], "chunk1");
        assert_eq!(results[1], "chunk2");
        assert!(
            error_seen,
            "Error should have been propagated, not silently ignored"
        );
    }
}
