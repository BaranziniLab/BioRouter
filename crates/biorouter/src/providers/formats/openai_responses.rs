use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;
use crate::providers::base::{ProviderUsage, Usage};
use crate::providers::formats::audience;
use crate::providers::formats::openai::model_reasoning_effort;
use anyhow::{anyhow, Error};
use async_stream::try_stream;
use chrono;
use futures::Stream;
use rmcp::model::{object, CallToolRequestParams, ErrorCode, ErrorData, Role, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: String,
    pub model: String,
    pub output: Vec<ResponseOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ResponseReasoningInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponseOutputItem {
    Reasoning {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<Vec<String>>,
    },
    Message {
        id: String,
        status: String,
        role: String,
        content: Vec<ResponseContentBlock>,
    },
    FunctionCall {
        id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponseContentBlock {
    OutputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Vec<Value>>,
    },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseReasoningInfo {
    pub effort: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub total_tokens: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<ResponseInputTokensDetails>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseInputTokensDetails {
    pub cached_tokens: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponsesStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated {
        sequence_number: i32,
        response: ResponseMetadata,
    },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        sequence_number: i32,
        response: ResponseMetadata,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        sequence_number: i32,
        output_index: i32,
        item: ResponseOutputItemInfo,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        part: ContentPart,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        sequence_number: i32,
        output_index: i32,
        item: ResponseOutputItemInfo,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        part: ContentPart,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<Value>>,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        sequence_number: i32,
        response: ResponseMetadata,
    },
    #[serde(rename = "response.failed")]
    ResponseFailed { sequence_number: i32, error: Value },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        arguments: String,
    },
    #[serde(rename = "error")]
    Error { error: Value },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMetadata {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: String,
    pub model: String,
    pub output: Vec<ResponseOutputItemInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ResponseReasoningInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponseOutputItemInfo {
    Reasoning {
        id: String,
        summary: Vec<String>,
    },
    Message {
        id: String,
        status: String,
        role: String,
        content: Vec<ContentPart>,
    },
    FunctionCall {
        id: String,
        status: String,
        call_id: String,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ContentPart {
    OutputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Vec<Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<Value>>,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
}

fn add_conversation_history(input_items: &mut Vec<Value>, messages: &[Message]) {
    for message in messages.iter().filter(|m| m.is_agent_visible()) {
        let has_only_tool_content = message.content.iter().all(|c| {
            matches!(
                c,
                MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
            )
        });

        if has_only_tool_content {
            continue;
        }

        if message.role != Role::User && message.role != Role::Assistant {
            continue;
        }

        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        let mut content_items = Vec::new();
        for content in &message.content {
            match content {
                MessageContent::Text(text) if !text.text.is_empty() => {
                    let content_type = if message.role == Role::Assistant {
                        "output_text"
                    } else {
                        "input_text"
                    };
                    content_items.push(json!({
                        "type": content_type,
                        "text": text.text
                    }));
                }
                MessageContent::Image(image) if message.role == Role::User => {
                    // Responses API user-message image format: data URL inline.
                    // Assistant messages don't carry image inputs.
                    content_items.push(json!({
                        "type": "input_image",
                        "image_url": format!(
                            "data:{};base64,{}",
                            image.mime_type, image.data
                        ),
                    }));
                }
                _ => {}
            }
        }

        if !content_items.is_empty() {
            input_items.push(json!({
                "role": role,
                "content": content_items
            }));
        }
    }
}

fn add_function_calls(input_items: &mut Vec<Value>, messages: &[Message]) {
    for message in messages.iter().filter(|m| m.is_agent_visible()) {
        if message.role == Role::Assistant {
            for content in &message.content {
                if let MessageContent::ToolRequest(request) = content {
                    if let Ok(tool_call) = &request.tool_call {
                        let arguments_str = tool_call
                            .arguments
                            .as_ref()
                            .map(|args| {
                                serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string())
                            })
                            .unwrap_or_else(|| "{}".to_string());

                        tracing::debug!(
                            "Replaying function_call with call_id: {}, name: {}",
                            request.id,
                            tool_call.name
                        );
                        input_items.push(json!({
                            "type": "function_call",
                            "call_id": request.id,
                            "name": tool_call.name,
                            "arguments": arguments_str
                        }));
                    }
                }
            }
        }
    }
}

fn add_function_call_outputs(input_items: &mut Vec<Value>, messages: &[Message]) {
    for message in messages.iter().filter(|m| m.is_agent_visible()) {
        for content in &message.content {
            if let MessageContent::ToolResponse(response) = content {
                match &response.tool_result {
                    Ok(contents) => {
                        let text_content: Vec<String> = contents
                            .content
                            .iter()
                            // Send only what the tool addressed to the model.
                            .filter(|c| audience::is_for_model(c))
                            .filter_map(audience::flattened_text)
                            .collect();

                        // Emitted even when nothing survives the filter. The Err
                        // arm below already knows why: a `function_call` with no
                        // matching output is the "No tool output found" error,
                        // and the Responses API takes an empty string happily.
                        // A tool that addresses every block to the user alone,
                        // or returns only an image, used to skip this push and
                        // fail the whole request one turn later.
                        tracing::debug!(
                            "Sending function_call_output with call_id: {}",
                            response.id
                        );
                        input_items.push(json!({
                            "type": "function_call_output",
                            "call_id": response.id,
                            "output": text_content.join("\n")
                        }));
                    }
                    Err(error_data) => {
                        // Handle error responses - must send them back to the API
                        // to avoid "No tool output found" errors
                        tracing::debug!(
                            "Sending function_call_output error with call_id: {}",
                            response.id
                        );
                        input_items.push(json!({
                            "type": "function_call_output",
                            "call_id": response.id,
                            "output": format!("Error: {}", error_data.message)
                        }));
                    }
                }
            }
        }
    }
}

pub fn create_responses_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> anyhow::Result<Value, Error> {
    let mut input_items = Vec::new();

    if !system.is_empty() {
        input_items.push(json!({
            "role": "system",
            "content": [{
                "type": "input_text",
                "text": system
            }]
        }));
    }

    add_conversation_history(&mut input_items, messages);
    add_function_calls(&mut input_items, messages);
    add_function_call_outputs(&mut input_items, messages);

    let mut payload = json!({
        "model": model_config.model_name,
        "input": input_items,
        "store": false,  // Don't store responses on server (we replay history ourselves)
    });

    if !tools.is_empty() {
        let tools_spec: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                })
            })
            .collect();

        payload
            .as_object_mut()
            .unwrap()
            .insert("tools".to_string(), json!(tools_spec));
    }

    if let Some(temp) = model_config.temperature {
        payload
            .as_object_mut()
            .unwrap()
            .insert("temperature".to_string(), json!(temp));
    }

    if let Some(tokens) = model_config.max_tokens {
        payload
            .as_object_mut()
            .unwrap()
            .insert("max_output_tokens".to_string(), json!(tokens));
    }

    // BR-63: the Responses API takes the effort as `reasoning.effort`, not the
    // chat-completions top-level `reasoning_effort`. Omitted entirely unless the
    // turn pinned an effort, so the model's own default is untouched.
    if let Some(effort) = model_config
        .reasoning_effort
        .and_then(|effort| effort.provider_effort())
        .and_then(|effort| model_reasoning_effort(&model_config.model_name, effort))
    {
        payload
            .as_object_mut()
            .unwrap()
            .insert("reasoning".to_string(), json!({ "effort": effort }));
    }

    Ok(payload)
}

pub fn responses_api_to_message(response: &ResponsesApiResponse) -> anyhow::Result<Message> {
    let mut content = Vec::new();

    for item in &response.output {
        match item {
            ResponseOutputItem::Reasoning { .. } => {
                continue;
            }
            ResponseOutputItem::Message {
                content: msg_content,
                ..
            } => {
                for block in msg_content {
                    match block {
                        ResponseContentBlock::OutputText { text, .. } => {
                            if !text.is_empty() {
                                content.push(MessageContent::text(text));
                            }
                        }
                        ResponseContentBlock::ToolCall { id, name, input } => {
                            content.push(MessageContent::tool_request(
                                id.clone(),
                                Ok(CallToolRequestParams {
                                    task: None,
                                    name: name.clone().into(),
                                    arguments: Some(object(input.clone())),
                                    meta: None,
                                }),
                            ));
                        }
                    }
                }
            }
            ResponseOutputItem::FunctionCall {
                id,
                name,
                arguments,
                ..
            } => {
                tracing::debug!("Received FunctionCall with id: {}, name: {}", id, name);
                let parsed_args = if arguments.is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(arguments).unwrap_or_else(|_| json!({}))
                };

                content.push(MessageContent::tool_request(
                    id.clone(),
                    Ok(CallToolRequestParams {
                        task: None,
                        name: name.clone().into(),
                        arguments: Some(object(parsed_args)),
                        meta: None,
                    }),
                ));
            }
        }
    }

    let mut message = Message::new(Role::Assistant, chrono::Utc::now().timestamp(), content);

    message = message.with_id(response.id.clone());

    Ok(message)
}

pub fn get_responses_usage(response: &ResponsesApiResponse) -> Usage {
    response
        .usage
        .as_ref()
        .map_or_else(Usage::default, response_usage_to_usage)
}

fn response_usage_to_usage(usage: &ResponseUsage) -> Usage {
    let cached_tokens = usage
        .input_tokens_details
        .as_ref()
        .map(|details| details.cached_tokens)
        .filter(|cached| *cached > 0)
        .map(|cached| cached.min(usage.input_tokens.max(0)));
    let input_tokens = usage.input_tokens - cached_tokens.unwrap_or(0);

    Usage::new(
        Some(input_tokens),
        Some(usage.output_tokens),
        Some(usage.total_tokens),
    )
    .with_cache(cached_tokens, None)
}

fn process_streaming_output_items(
    output_items: Vec<(ResponseOutputItemInfo, bool)>,
    is_text_response: bool,
) -> Vec<MessageContent> {
    let mut content = Vec::new();

    for (item, completion_confirmed) in output_items {
        match item {
            ResponseOutputItemInfo::Reasoning { .. } => {
                // Skip reasoning items
            }
            ResponseOutputItemInfo::Message {
                status,
                content: parts,
                ..
            } => {
                let completion_confirmed = completion_confirmed && status == "completed";
                for part in parts {
                    match part {
                        ContentPart::OutputText { text, .. } => {
                            if !text.is_empty() && !is_text_response {
                                content.push(MessageContent::text(&text));
                            }
                        }
                        ContentPart::ToolCall {
                            id,
                            name,
                            arguments,
                        } => {
                            content.push(streaming_tool_content(
                                id,
                                name,
                                &arguments,
                                completion_confirmed,
                            ));
                        }
                    }
                }
            }
            ResponseOutputItemInfo::FunctionCall {
                status,
                call_id,
                name,
                arguments,
                ..
            } => {
                content.push(streaming_tool_content(
                    call_id,
                    name,
                    &arguments,
                    completion_confirmed && status == "completed",
                ));
            }
        }
    }

    content
}

fn streaming_tool_content(
    call_id: String,
    name: String,
    arguments: &str,
    completion_confirmed: bool,
) -> MessageContent {
    if !completion_confirmed {
        return MessageContent::tool_request(
            call_id,
            Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Tool-call stream completion was not confirmed; the call was not executed. Emit a new, complete tool call.",
                Some(json!({"biorouterToolCallFailure":"incomplete_stream"})),
            )),
        );
    }

    let parsed = if arguments.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str::<Value>(arguments).map_err(|error| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Could not interpret tool use parameters: {error}"),
                Some(json!({"biorouterToolCallFailure":"invalid_arguments"})),
            )
        })
    }
    .and_then(|value| {
        if value.is_object() {
            Ok(value)
        } else {
            Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Tool arguments must be a JSON object; the call was not executed.",
                Some(json!({"biorouterToolCallFailure":"invalid_arguments"})),
            ))
        }
    });

    match parsed {
        Ok(arguments) => MessageContent::tool_request(
            call_id,
            Ok(CallToolRequestParams {
                task: None,
                name: name.into(),
                arguments: Some(object(arguments)),
                meta: None,
            }),
        ),
        Err(error) => MessageContent::tool_request(call_id, Err(error)),
    }
}

fn output_item_id(item: &ResponseOutputItemInfo) -> &str {
    match item {
        ResponseOutputItemInfo::Reasoning { id, .. }
        | ResponseOutputItemInfo::Message { id, .. }
        | ResponseOutputItemInfo::FunctionCall { id, .. } => id,
    }
}

struct PendingFunctionCall {
    call_id: String,
    name: String,
}

struct PendingOutputItem {
    item_id: String,
    calls: BTreeMap<i32, PendingFunctionCall>,
}

enum StreamingOutputItemState {
    Pending(PendingOutputItem),
    Unconfirmed(ResponseOutputItemInfo),
    Completed(ResponseOutputItemInfo),
}

pub fn responses_api_to_streaming_message<S>(
    mut stream: S,
) -> impl Stream<Item = anyhow::Result<crate::providers::base::ProviderStreamItem>> + 'static
where
    S: Stream<Item = anyhow::Result<String>> + Unpin + Send + 'static,
{
    try_stream! {
        use futures::StreamExt;

        let mut accumulated_text = String::new();
        let mut response_id: Option<String> = None;
        let mut model_name: Option<String> = None;
        let mut final_usage: Option<ProviderUsage> = None;
        let mut output_items: BTreeMap<i32, StreamingOutputItemState> = BTreeMap::new();
        let mut is_text_response = false;

        'outer: while let Some(response) = stream.next().await {
            let response_str = response?;

            // Skip empty lines
            if response_str.trim().is_empty() {
                continue;
            }

            let data_line = if let Some(data) = response_str.strip_prefix("data:") {
                data.trim_start()
            } else if response_str.starts_with("event:") {
                // Skip event type lines
                continue;
            } else {
                // Try to parse as-is in case there's no prefix
                &response_str
            };

            if data_line.trim() == "[DONE]" {
                break 'outer;
            }

            let event: ResponsesStreamEvent = serde_json::from_str(data_line)
                .map_err(|error| anyhow!("Failed to parse Responses stream event: {error}"))?;

            match event {
                ResponsesStreamEvent::ResponseCreated { response, .. } |
                ResponsesStreamEvent::ResponseInProgress { response, .. } => {
                    response_id = Some(response.id);
                    model_name = Some(response.model);
                }

                ResponsesStreamEvent::OutputTextDelta { delta, .. } => {
                    is_text_response = true;
                    accumulated_text.push_str(&delta);

                    // Yield incremental text updates for true streaming
                    let mut content = Vec::new();
                    if !delta.is_empty() {
                        content.push(MessageContent::text(&delta));
                    }
                    let mut msg = Message::new(Role::Assistant, chrono::Utc::now().timestamp(), content);

                    // Add ID so desktop client knows these deltas are part of the same message
                    if let Some(id) = &response_id {
                        msg = msg.with_id(id.clone());
                    }

                    yield (Some(msg), None, None);
                }

                ResponsesStreamEvent::OutputItemAdded {
                    output_index,
                    item,
                    ..
                } => {
                    let pending = match item {
                        ResponseOutputItemInfo::FunctionCall {
                            id,
                            call_id,
                            name,
                            ..
                        } => Some(PendingOutputItem {
                            item_id: id,
                            calls: [(0, PendingFunctionCall { call_id, name })]
                                .into_iter()
                                .collect(),
                        }),
                        ResponseOutputItemInfo::Message { id, content, .. } => {
                            let calls = content
                                .into_iter()
                                .enumerate()
                                .filter_map(|part| match part {
                                    (content_index, ContentPart::ToolCall { id, name, .. }) => {
                                        i32::try_from(content_index).ok().map(|content_index| {
                                            (
                                                content_index,
                                                PendingFunctionCall { call_id: id, name },
                                            )
                                        })
                                    }
                                    (_, ContentPart::OutputText { .. }) => None,
                                })
                                .collect::<BTreeMap<_, _>>();
                            (!calls.is_empty()).then_some(PendingOutputItem {
                                item_id: id,
                                calls,
                            })
                        }
                        ResponseOutputItemInfo::Reasoning { .. } => None,
                    };
                    if let Some(pending) = pending {
                        output_items
                            .entry(output_index)
                            .or_insert(StreamingOutputItemState::Pending(pending));
                    }
                }

                ResponsesStreamEvent::ContentPartAdded {
                    item_id,
                    output_index,
                    content_index,
                    part: ContentPart::ToolCall { id, name, .. },
                    ..
                } => {
                    let call = PendingFunctionCall { call_id: id, name };
                    match output_items.entry(output_index) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(StreamingOutputItemState::Pending(PendingOutputItem {
                                item_id,
                                calls: [(content_index, call)].into_iter().collect(),
                            }));
                        }
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            if let StreamingOutputItemState::Pending(pending) = entry.get_mut() {
                                if pending.item_id == item_id {
                                    pending.calls.entry(content_index).or_insert(call);
                                }
                            }
                        }
                    }
                }

                ResponsesStreamEvent::OutputItemDone {
                    output_index,
                    item,
                    ..
                } => {
                    let item_id = output_item_id(&item);
                    let completion_matches = match output_items.get(&output_index) {
                        None => true,
                        Some(StreamingOutputItemState::Pending(pending)) => {
                            pending.item_id == item_id
                        }
                        Some(StreamingOutputItemState::Unconfirmed(existing)) => {
                            output_item_id(existing) == item_id
                        }
                        Some(StreamingOutputItemState::Completed(_)) => false,
                    };
                    if completion_matches {
                        output_items.insert(
                            output_index,
                            StreamingOutputItemState::Completed(item),
                        );
                    }
                }

                ResponsesStreamEvent::OutputTextDone { .. } => {
                    // Text is already complete from deltas, this is just a summary event
                }

                ResponsesStreamEvent::ResponseCompleted { response, .. } => {
                    let model = model_name.as_ref().unwrap_or(&response.model);
                    let usage = response
                        .usage
                        .as_ref()
                        .map_or_else(Usage::default, response_usage_to_usage);
                    final_usage = Some(ProviderUsage {
                        usage,
                        model: model.clone(),
                        provider: None,
                        finish_reason: None,
                    });

                    for (output_index, item) in response.output.into_iter().enumerate() {
                        if let Ok(output_index) = i32::try_from(output_index) {
                            output_items
                                .entry(output_index)
                                .or_insert(StreamingOutputItemState::Unconfirmed(item));
                        }
                    }

                    break 'outer;
                }

                ResponsesStreamEvent::FunctionCallArgumentsDelta { .. } => {
                    // Function call arguments are being streamed, but we'll get the complete
                    // arguments in the OutputItemDone event, so we can ignore deltas for now
                }

                ResponsesStreamEvent::FunctionCallArgumentsDone { .. } => {
                    // Arguments are complete, will be in the OutputItemDone event
                }

                ResponsesStreamEvent::ResponseFailed { error, .. } => {
                    Err(anyhow!("Responses API failed: {:?}", error))?;
                }

                ResponsesStreamEvent::Error { error } => {
                    Err(anyhow!("Responses API error: {:?}", error))?;
                }

                _ => {
                    // Ignore remaining non-tool progress events.
                }
            }
        }

        let mut content = Vec::new();
        for item in output_items.into_values() {
            match item {
                StreamingOutputItemState::Pending(pending) => {
                    content.extend(pending.calls.into_values().map(|call| {
                        streaming_tool_content(call.call_id, call.name, "", false)
                    }));
                }
                StreamingOutputItemState::Unconfirmed(item) => {
                    content.extend(process_streaming_output_items(
                        vec![(item, false)],
                        is_text_response,
                    ));
                }
                StreamingOutputItemState::Completed(item) => {
                    content.extend(process_streaming_output_items(
                        vec![(item, true)],
                        is_text_response,
                    ));
                }
            }
        }

        if !content.is_empty() {
            let mut message = Message::new(Role::Assistant, chrono::Utc::now().timestamp(), content);
            if let Some(id) = response_id {
                message = message.with_id(id);
            }
            yield (Some(message), final_usage, None);
        } else if let Some(usage) = final_usage {
            yield (None, Some(usage), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::effort::ReasoningEffort;
    use futures::StreamExt;
    use rmcp::object;
    use tokio::pin;

    /// The single `function_call_output` produced for one tool response.
    fn function_call_output(content: Vec<rmcp::model::Content>) -> String {
        let message = Message::user().with_tool_response(
            "call-1",
            Ok(rmcp::model::CallToolResult {
                content,
                structured_content: None,
                is_error: Some(false),
                meta: None,
            }),
        );

        let mut items = Vec::new();
        add_function_call_outputs(&mut items, &[message]);
        assert_eq!(items.len(), 1, "one output per tool response");
        items[0]["output"]
            .as_str()
            .expect("output is a string")
            .to_string()
    }

    /// Every audience case, through the real Responses API formatter.
    ///
    /// `openai.rs` filtered and this path did not, so the same OpenAI account
    /// sent the model different content depending on which API the model name
    /// routed to. The two agree now.
    #[test]
    fn tool_result_blocks_reach_the_model_by_audience() {
        let sent = function_call_output(crate::providers::formats::audience::every_audience_case());

        assert_eq!(
            sent,
            crate::providers::formats::audience::MODEL_VISIBLE.join("\n"),
            "the Responses output must carry exactly the model-addressed blocks"
        );
        for withheld in crate::providers::formats::audience::MODEL_HIDDEN {
            assert!(!sent.contains(withheld), "{withheld} reached the model");
        }
    }

    /// A `text_editor view` through the real Responses API formatter: the file
    /// arrives as an embedded resource, so reading only text blocks would send
    /// the model an empty output for every file view.
    #[test]
    fn a_viewed_file_reaches_the_model_through_its_embedded_resource() {
        let sent =
            function_call_output(crate::providers::formats::audience::text_editor_view_result());

        assert_eq!(sent, crate::providers::formats::audience::VIEW_FOR_MODEL);
        assert!(
            !sent.contains(crate::providers::formats::audience::VIEW_FOR_USER),
            "the user's rendering reached the model"
        );
    }

    /// A tool that addresses every block to the user still owes the API an
    /// output for its call. Skipping the item is what raises "No tool output
    /// found", which fails the whole next request rather than losing one block.
    #[test]
    fn a_fully_withheld_result_still_answers_its_function_call() {
        let sent =
            function_call_output(vec![rmcp::model::Content::text("for the user")
                .with_audience(vec![rmcp::model::Role::User])]);

        assert_eq!(sent, "", "the output is present and empty, not absent");
    }

    // BR-63: the Responses API takes the effort nested under `reasoning`, not as
    // the chat-completions top-level `reasoning_effort` key.
    #[test]
    fn deep_effort_sets_reasoning_effort_high() -> anyhow::Result<()> {
        let model_config =
            ModelConfig::new_or_fail("gpt-5.5").with_reasoning_effort(Some(ReasoningEffort::Deep));

        let payload = create_responses_request(
            &model_config,
            "system",
            &[Message::user().with_text("hi")],
            &[],
        )?;

        assert_eq!(payload["reasoning"]["effort"], json!("high"));
        Ok(())
    }

    #[test]
    fn quick_effort_sets_reasoning_effort_low() -> anyhow::Result<()> {
        let model_config =
            ModelConfig::new_or_fail("gpt-5.5").with_reasoning_effort(Some(ReasoningEffort::Quick));

        let payload = create_responses_request(
            &model_config,
            "system",
            &[Message::user().with_text("hi")],
            &[],
        )?;

        assert_eq!(payload["reasoning"]["effort"], json!("low"));
        Ok(())
    }

    #[test]
    fn o4_mini_combines_function_tools_with_nested_reasoning_effort() -> anyhow::Result<()> {
        let model_config = ModelConfig::new_or_fail("o4-mini-2025-04-16")
            .with_reasoning_effort(Some(ReasoningEffort::Deep));
        let tool = Tool::new(
            "lookup_record",
            "Look up a record",
            object!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
            }),
        );

        let payload = create_responses_request(
            &model_config,
            "system",
            &[Message::user().with_text("Find record 42")],
            &[tool],
        )?;

        assert_eq!(payload["reasoning"]["effort"], json!("high"));
        assert_eq!(payload["tools"][0]["name"], json!("lookup_record"));
        assert!(payload.get("reasoning_effort").is_none());
        assert!(payload.get("messages").is_none());
        Ok(())
    }

    #[test]
    fn quick_effort_uses_the_minimum_supported_pro_effort() -> anyhow::Result<()> {
        for model_name in ["gpt-5.4-pro", "gpt-5.5-pro"] {
            let model_config = ModelConfig::new_or_fail(model_name)
                .with_reasoning_effort(Some(ReasoningEffort::Quick));
            let payload = create_responses_request(
                &model_config,
                "system",
                &[Message::user().with_text("hi")],
                &[],
            )?;

            assert_eq!(
                payload["reasoning"]["effort"],
                json!("medium"),
                "{model_name} does not accept low reasoning effort"
            );
        }
        Ok(())
    }

    #[test]
    fn responses_builder_omits_effort_for_non_reasoning_models() -> anyhow::Result<()> {
        let model_config =
            ModelConfig::new_or_fail("gpt-4.1").with_reasoning_effort(Some(ReasoningEffort::Deep));
        let payload = create_responses_request(
            &model_config,
            "system",
            &[Message::user().with_text("hi")],
            &[],
        )?;

        assert!(payload.get("reasoning").is_none());
        Ok(())
    }

    #[test]
    fn no_effort_leaves_the_request_untouched() -> anyhow::Result<()> {
        let model_config = ModelConfig::new_or_fail("gpt-5.5");

        let payload = create_responses_request(
            &model_config,
            "system",
            &[Message::user().with_text("hi")],
            &[],
        )?;

        assert!(payload.get("reasoning").is_none());
        Ok(())
    }

    #[test]
    fn non_streaming_usage_extracts_cached_input_tokens() {
        let response: ResponsesApiResponse = serde_json::from_value(json!({
            "id": "resp_cache",
            "object": "response",
            "created_at": 1,
            "status": "completed",
            "model": "gpt-5.4",
            "output": [],
            "usage": {
                "input_tokens": 1000,
                "output_tokens": 50,
                "total_tokens": 1050,
                "input_tokens_details": { "cached_tokens": 800 }
            }
        }))
        .expect("valid Responses API response");

        let usage = get_responses_usage(&response);
        assert_eq!(usage.input_tokens, Some(200));
        assert_eq!(usage.output_tokens, Some(50));
        assert_eq!(usage.cache_read_input_tokens, Some(800));
        assert_eq!(usage.cache_creation_input_tokens, None);
        assert_eq!(usage.total_tokens, Some(1050));
        assert_eq!(usage.billed_total(), Some(1050));
    }

    #[tokio::test]
    async fn streaming_usage_extracts_cached_input_tokens() -> anyhow::Result<()> {
        let lines = r#"
data: {"type":"response.completed","sequence_number":1,"response":{"id":"resp_cache","object":"response","created_at":1,"status":"completed","model":"gpt-5.4","output":[],"usage":{"input_tokens":1000,"output_tokens":50,"total_tokens":1050,"input_tokens_details":{"cached_tokens":800}}}}
data: [DONE]
"#;
        let response_stream = tokio_stream::iter(lines.lines().map(|line| Ok(line.to_string())));
        let messages = responses_api_to_streaming_message(response_stream);
        pin!(messages);

        let mut final_usage = None;
        while let Some(result) = messages.next().await {
            let (_, usage, _pending) = result?;
            if usage.is_some() {
                final_usage = usage;
            }
        }

        let usage = final_usage.expect("expected terminal usage");
        assert_eq!(usage.model, "gpt-5.4");
        assert_eq!(usage.usage.input_tokens, Some(200));
        assert_eq!(usage.usage.output_tokens, Some(50));
        assert_eq!(usage.usage.cache_read_input_tokens, Some(800));
        assert_eq!(usage.usage.cache_creation_input_tokens, None);
        assert_eq!(usage.usage.total_tokens, Some(1050));
        assert_eq!(usage.usage.billed_total(), Some(1050));
        Ok(())
    }
}
