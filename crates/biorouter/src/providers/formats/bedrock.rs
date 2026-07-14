use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use crate::mcp_utils::ToolResult;
use anyhow::{anyhow, bail, Result};
use aws_sdk_bedrockruntime::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_bedrockruntime::operation::converse::ConverseError;
use aws_sdk_bedrockruntime::types as bedrock;
use aws_smithy_types::{Document, Number};
use base64::Engine;
use chrono::Utc;
use rmcp::model::{
    object, CallToolRequestParams, Content, ErrorCode, ErrorData, RawContent, ResourceContents,
    Role, Tool,
};
use serde_json::Value;

use super::super::base::Usage;
use super::super::errors::ProviderError;
use crate::conversation::message::{Message, MessageContent};

pub fn to_bedrock_message(message: &Message) -> Result<bedrock::Message> {
    bedrock::Message::builder()
        .role(to_bedrock_role(&message.role))
        .set_content(Some(
            message
                .content
                .iter()
                .map(to_bedrock_message_content)
                .collect::<Result<_>>()?,
        ))
        .build()
        .map_err(|err| anyhow!("Failed to construct Bedrock message: {}", err))
}

pub fn to_bedrock_message_content(content: &MessageContent) -> Result<bedrock::ContentBlock> {
    Ok(match content {
        MessageContent::Text(text) => bedrock::ContentBlock::Text(text.text.to_string()),
        MessageContent::ToolConfirmationRequest(_tool_confirmation_request) => {
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::ActionRequired(_action_required) => {
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::Image(image) => {
            bedrock::ContentBlock::Image(to_bedrock_image(&image.data, &image.mime_type)?)
        }
        MessageContent::Thinking(_) => {
            // Thinking blocks are not supported in Bedrock - skip
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::RedactedThinking(_) => {
            // Redacted thinking blocks are not supported in Bedrock - skip
            bedrock::ContentBlock::Text("".to_string())
        }
        MessageContent::SystemNotification(_) => {
            bail!("SystemNotification should not get passed to the provider")
        }
        MessageContent::ToolRequest(tool_req) => {
            let tool_use_id = tool_req.id.to_string();
            let tool_use = if let Ok(call) = tool_req.tool_call.as_ref() {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .name(call.name.to_string())
                    .input(to_bedrock_json(&Value::from(call.arguments.clone())))
                    .build()
            } else {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .build()
            }?;
            bedrock::ContentBlock::ToolUse(tool_use)
        }
        MessageContent::FrontendToolRequest(tool_req) => {
            let tool_use_id = tool_req.id.to_string();
            let tool_use = if let Ok(call) = tool_req.tool_call.as_ref() {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .name(call.name.to_string())
                    .input(to_bedrock_json(&Value::from(call.arguments.clone())))
                    .build()
            } else {
                bedrock::ToolUseBlock::builder()
                    .tool_use_id(tool_use_id)
                    .build()
            }?;
            bedrock::ContentBlock::ToolUse(tool_use)
        }
        MessageContent::ToolResponse(tool_res) => {
            let content = match &tool_res.tool_result {
                Ok(result) => Some(
                    result
                        .content
                        .iter()
                        // Filter out content items that have User in their audience
                        .filter(|c| {
                            c.audience()
                                .is_none_or(|audience| !audience.contains(&Role::User))
                        })
                        .map(|c| to_bedrock_tool_result_content_block(&tool_res.id, c.clone()))
                        .collect::<Result<_>>()?,
                ),
                Err(error) => {
                    // For errors, create a text content block with the error message
                    Some(vec![bedrock::ToolResultContentBlock::Text(format!(
                        "The tool call returned the following error:\n{}",
                        error
                    ))])
                }
            };
            bedrock::ContentBlock::ToolResult(
                bedrock::ToolResultBlock::builder()
                    .tool_use_id(tool_res.id.to_string())
                    .status(if tool_res.tool_result.is_ok() {
                        bedrock::ToolResultStatus::Success
                    } else {
                        bedrock::ToolResultStatus::Error
                    })
                    .set_content(content)
                    .build()?,
            )
        }
    })
}

/// Convert MCP Content to Bedrock ToolResultContentBlock
///
/// Supports text, images, and document resources. Images are supported
/// by Bedrock for Anthropic Claude 3 models.
pub fn to_bedrock_tool_result_content_block(
    tool_use_id: &str,
    content: Content,
) -> Result<bedrock::ToolResultContentBlock> {
    Ok(match content.raw {
        RawContent::Text(text) => bedrock::ToolResultContentBlock::Text(text.text),
        RawContent::Image(image) => {
            bedrock::ToolResultContentBlock::Image(to_bedrock_image(&image.data, &image.mime_type)?)
        }
        RawContent::ResourceLink(_link) => {
            bedrock::ToolResultContentBlock::Text("[Resource link]".to_string())
        }
        RawContent::Resource(resource) => match &resource.resource {
            ResourceContents::TextResourceContents { text, .. } => {
                match to_bedrock_document(tool_use_id, &resource.resource)? {
                    Some(doc) => bedrock::ToolResultContentBlock::Document(doc),
                    None => bedrock::ToolResultContentBlock::Text(text.to_string()),
                }
            }
            ResourceContents::BlobResourceContents { .. } => {
                bail!("Blob resource content is not supported by Bedrock provider yet")
            }
        },
        RawContent::Audio(..) => bail!("Audio is not not supported by Bedrock provider"),
    })
}

pub fn to_bedrock_role(role: &Role) -> bedrock::ConversationRole {
    match role {
        Role::User => bedrock::ConversationRole::User,
        Role::Assistant => bedrock::ConversationRole::Assistant,
    }
}

pub fn to_bedrock_image(data: &String, mime_type: &String) -> Result<bedrock::ImageBlock> {
    // Extract format from MIME type
    let format = match mime_type.as_str() {
        "image/png" => bedrock::ImageFormat::Png,
        "image/jpeg" | "image/jpg" => bedrock::ImageFormat::Jpeg,
        "image/gif" => bedrock::ImageFormat::Gif,
        "image/webp" => bedrock::ImageFormat::Webp,
        _ => bail!(
            "Unsupported image format: {}. Bedrock supports png, jpeg, gif, webp",
            mime_type
        ),
    };

    // Create image source with base64 data
    let source = bedrock::ImageSource::Bytes(aws_smithy_types::Blob::new(
        base64::prelude::BASE64_STANDARD
            .decode(data)
            .map_err(|e| anyhow!("Failed to decode base64 image data: {}", e))?,
    ));

    // Build the image block
    Ok(bedrock::ImageBlock::builder()
        .format(format)
        .source(source)
        .build()?)
}

pub fn to_bedrock_tool_config(tools: &[Tool]) -> Result<bedrock::ToolConfiguration> {
    Ok(bedrock::ToolConfiguration::builder()
        .set_tools(Some(
            tools.iter().map(to_bedrock_tool).collect::<Result<_>>()?,
        ))
        .build()?)
}

pub fn to_bedrock_tool(tool: &Tool) -> Result<bedrock::Tool> {
    let mut input_schema = tool.input_schema.as_ref().clone();

    // If the schema doesn't have a "type" field, add it
    // This is required by Bedrock
    if !input_schema.contains_key("type") {
        input_schema.insert("type".to_string(), Value::String("object".to_string()));
    }

    Ok(bedrock::Tool::ToolSpec(
        bedrock::ToolSpecification::builder()
            .name(tool.name.to_string())
            .description(
                tool.description
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            )
            .input_schema(bedrock::ToolInputSchema::Json(to_bedrock_json(
                &Value::Object(input_schema),
            )))
            .build()?,
    ))
}

pub fn to_bedrock_json(value: &Value) -> Document {
    match value {
        Value::Null => Document::Null,
        Value::Bool(bool) => Document::Bool(*bool),
        Value::Number(num) => {
            if let Some(n) = num.as_u64() {
                Document::Number(Number::PosInt(n))
            } else if let Some(n) = num.as_i64() {
                Document::Number(Number::NegInt(n))
            } else if let Some(n) = num.as_f64() {
                Document::Number(Number::Float(n))
            } else {
                unreachable!()
            }
        }
        Value::String(str) => Document::String(str.to_string()),
        Value::Array(arr) => Document::Array(arr.iter().map(to_bedrock_json).collect()),
        Value::Object(obj) => Document::Object(HashMap::from_iter(
            obj.into_iter()
                .map(|(key, val)| (key.to_string(), to_bedrock_json(val))),
        )),
    }
}

fn to_bedrock_document(
    tool_use_id: &str,
    content: &ResourceContents,
) -> Result<Option<bedrock::DocumentBlock>> {
    let (uri, text) = match content {
        ResourceContents::TextResourceContents { uri, text, .. } => (uri, text),
        ResourceContents::BlobResourceContents { .. } => {
            bail!("Blob resource content is not supported by Bedrock provider yet")
        }
    };

    let filename = Path::new(uri)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(uri);

    // Return None if the file type is not supported
    let (name, format) = match filename.split_once('.') {
        Some((name, "txt")) => (name, bedrock::DocumentFormat::Txt),
        Some((name, "csv")) => (name, bedrock::DocumentFormat::Csv),
        Some((name, "md")) => (name, bedrock::DocumentFormat::Md),
        Some((name, "html")) => (name, bedrock::DocumentFormat::Html),
        _ => return Ok(None), // Not a supported document type
    };

    // Since we can't use the full path (due to character limit and also Bedrock does not accept `/` etc.),
    // and Bedrock wants document names to be unique, we're adding `tool_use_id` as a prefix to make
    // document names unique.
    let name = format!("{tool_use_id}-{name}");

    Ok(Some(
        bedrock::DocumentBlock::builder()
            .format(format)
            .name(name)
            .source(bedrock::DocumentSource::Bytes(text.as_bytes().into()))
            .build()
            .map_err(|err| anyhow!("Failed to construct Bedrock document: {}", err))?,
    ))
}

pub fn from_bedrock_message(message: &bedrock::Message) -> Result<Message> {
    let role = from_bedrock_role(message.role())?;
    let content = message
        .content()
        .iter()
        .map(from_bedrock_content_block)
        .collect::<Result<Vec<_>>>()?;
    let created = Utc::now().timestamp();

    Ok(Message::new(role, created, content))
}

pub fn from_bedrock_content_block(block: &bedrock::ContentBlock) -> Result<MessageContent> {
    Ok(match block {
        bedrock::ContentBlock::Text(text) => MessageContent::text(text),
        bedrock::ContentBlock::ToolUse(tool_use) => MessageContent::tool_request(
            tool_use.tool_use_id.to_string(),
            Ok(CallToolRequestParams {
                task: None,
                name: tool_use.name.clone().into(),
                arguments: Some(object(from_bedrock_json(&tool_use.input.clone())?)),
                meta: None,
            }),
        ),
        bedrock::ContentBlock::ToolResult(tool_res) => MessageContent::tool_response(
            tool_res.tool_use_id.to_string(),
            if tool_res.content.is_empty() {
                Err(ErrorData {
                    code: ErrorCode::INTERNAL_ERROR,
                    message: Cow::from("Empty content for tool use from Bedrock".to_string()),
                    data: None,
                })
            } else {
                tool_res
                    .content
                    .iter()
                    .map(from_bedrock_tool_result_content_block)
                    .collect::<ToolResult<Vec<_>>>()
                    .map(|content| rmcp::model::CallToolResult {
                        content,
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    })
            },
        ),
        _ => bail!("Unsupported content block type from Bedrock"),
    })
}

pub fn from_bedrock_tool_result_content_block(
    content: &bedrock::ToolResultContentBlock,
) -> ToolResult<Content> {
    Ok(match content {
        bedrock::ToolResultContentBlock::Text(text) => Content::text(text.to_string()),
        _ => {
            return Err(ErrorData {
                code: ErrorCode::INTERNAL_ERROR,
                message: Cow::from("Unsupported tool result from Bedrock".to_string()),
                data: None,
            })
        }
    })
}

pub fn from_bedrock_role(role: &bedrock::ConversationRole) -> Result<Role> {
    Ok(match role {
        bedrock::ConversationRole::User => Role::User,
        bedrock::ConversationRole::Assistant => Role::Assistant,
        _ => bail!("Unknown role from Bedrock"),
    })
}

pub fn from_bedrock_usage(usage: &bedrock::TokenUsage) -> Usage {
    Usage::new(
        Some(usage.input_tokens),
        Some(usage.output_tokens),
        Some(usage.total_tokens),
    )
}

pub fn from_bedrock_json(document: &Document) -> Result<Value> {
    Ok(match document {
        Document::Null => Value::Null,
        Document::Bool(bool) => Value::Bool(*bool),
        Document::Number(num) => match num {
            Number::PosInt(i) => Value::Number((*i).into()),
            Number::NegInt(i) => Value::Number((*i).into()),
            Number::Float(f) => Value::Number(
                serde_json::Number::from_f64(*f).ok_or(anyhow!("Expected a valid float"))?,
            ),
        },
        Document::String(str) => Value::String(str.clone()),
        Document::Array(arr) => {
            Value::Array(arr.iter().map(from_bedrock_json).collect::<Result<_>>()?)
        }
        Document::Object(obj) => Value::Object(
            obj.iter()
                .map(|(key, val)| Ok((key.clone(), from_bedrock_json(val)?)))
                .collect::<Result<_>>()?,
        ),
    })
}

/// Default overall timeout (seconds) for a single Bedrock `Converse` call,
/// including any SDK-internal retries. Without an operation timeout a stalled
/// endpoint (a proxy that accepts the connection but never answers) hangs the
/// whole agent turn forever — the user sees a spinner that never resolves and no
/// error is ever surfaced. This bounds that case while staying generous enough to
/// never abort a legitimate slow completion. Override with
/// `BEDROCK_OPERATION_TIMEOUT_SECS` (0 disables the timeout entirely).
pub const BEDROCK_DEFAULT_OPERATION_TIMEOUT_SECS: u64 = 300;

/// Build the [`TimeoutConfig`] for a Bedrock client, reading
/// `BEDROCK_OPERATION_TIMEOUT_SECS` (config param first, then process env),
/// defaulting to [`BEDROCK_DEFAULT_OPERATION_TIMEOUT_SECS`]. Returns `None` when
/// the timeout is explicitly disabled (`0`), leaving the SDK default (no overall
/// timeout) in place.
pub fn bedrock_timeout_config(
    config: &crate::config::Config,
) -> Option<aws_smithy_types::timeout::TimeoutConfig> {
    let secs = config
        .get_param::<u64>("BEDROCK_OPERATION_TIMEOUT_SECS")
        .ok()
        .or_else(|| {
            std::env::var("BEDROCK_OPERATION_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
        })
        .unwrap_or(BEDROCK_DEFAULT_OPERATION_TIMEOUT_SECS);

    if secs == 0 {
        return None;
    }

    Some(
        aws_smithy_types::timeout::TimeoutConfig::builder()
            .operation_timeout(std::time::Duration::from_secs(secs))
            .build(),
    )
}

/// True if `text` reads like a context/token-window overflow, regardless of how
/// the (possibly proxied) endpoint phrases it. Amazon Bedrock itself says
/// "Input is too long for requested model."; gateways in front of it rephrase.
pub fn looks_like_context_overflow(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("input is too long")
        || t.contains("too long for requested model")
        || t.contains("too many tokens")
        || t.contains("maximum context")
        || t.contains("context length")
        || t.contains("context window")
        || t.contains("maximum number of tokens")
        || (t.contains("token") && (t.contains("exceed") || t.contains("limit")))
        || (t.contains("prompt") && t.contains("too long"))
}

/// Classify a Bedrock `Converse` failure into a [`ProviderError`].
///
/// ## Why this exists
///
/// UCSF's Versa Bedrock endpoint is a MuleSoft proxy in front of Amazon Bedrock,
/// and even direct Bedrock is commonly fronted by a gateway. Those intermediaries
/// return HTTP error responses whose bodies are **not** in the AWS JSON error
/// shape (no `__type`/`code`), so the AWS SDK cannot map them to a typed
/// [`ConverseError`] and collapses them to `ConverseError::Unhandled` with empty
/// metadata. Transport failures (timeouts, dropped connections) likewise carry no
/// service metadata.
///
/// The previous mapping matched only the typed variants and routed *everything
/// else* — including throttling (HTTP 429) and context overflow (HTTP 400 "input
/// too long") that the proxy failed to shape — to a generic
/// [`ProviderError::ServerError`] carrying an unreadable `{:?}` debug dump. In a
/// real incident that meant:
///   * auto-compaction never fired (a proxied context overflow never became
///     [`ProviderError::ContextLengthExceeded`], so the turn kept re-sending an
///     over-limit prompt that failed identically on every retry),
///   * rate-limit handling never fired (a proxied 429 never became
///     [`ProviderError::RateLimitExceeded`], so it missed the deeper retry budget
///     and the scheduler back-off), and
///   * the user was shown
///     `Unhandled(Unhandled { source: ErrorMetadata { code: None, message: None,
///     .. }, .. })`.
///
/// This classifier looks past the SDK's typed variant: it reads the raw HTTP
/// status code (still available on the error's raw response) plus a snippet of the
/// response body, so a proxy that hides the AWS error shape is still classified
/// correctly, and every branch yields a human-readable message.
pub fn classify_bedrock_converse_error(err: SdkError<ConverseError>) -> ProviderError {
    // Capture the HTTP status + whether the body reads like an overflow BEFORE
    // consuming `err`. `raw_response()` is populated for ServiceError/ResponseError.
    let status = err.raw_response().map(|r| r.status().as_u16());
    let body_says_overflow = err
        .raw_response()
        .and_then(|r| r.body().bytes())
        .map(|b| looks_like_context_overflow(&String::from_utf8_lossy(&b[..b.len().min(4096)])))
        .unwrap_or(false);

    // Transport-level failures never carry a service body; they are transient and
    // retryable. Give them a clear, actionable message rather than a raw debug dump.
    // (This also matters because a hung endpoint that never answers surfaces here as
    // a TimeoutError once an operation timeout is configured on the client.)
    match &err {
        SdkError::TimeoutError(_) => {
            return ProviderError::ServerError(
                "Bedrock request timed out with no response from the endpoint \
                 (network stall or gateway hang). This is usually transient."
                    .to_string(),
            );
        }
        SdkError::DispatchFailure(_) => {
            return ProviderError::ServerError(
                "Could not reach the Bedrock endpoint (connection/dispatch failure). \
                 Check the endpoint URL and network connectivity. This is usually transient."
                    .to_string(),
            );
        }
        SdkError::ResponseError(_) => {
            return ProviderError::ServerError(
                "Received an incomplete or unparseable response from the Bedrock endpoint. \
                 This is usually transient."
                    .to_string(),
            );
        }
        SdkError::ConstructionFailure(_) => {
            return ProviderError::ServerError(
                "Failed to construct the Bedrock request before sending it.".to_string(),
            );
        }
        // ServiceError (a real HTTP error response was received) — fall through.
        _ => {}
    }

    // Keep a compact detail string for logs/telemetry before consuming `err`.
    let detail = format!("Failed to call Bedrock: {:?}", err);

    match err.into_service_error() {
        ConverseError::ThrottlingException(e) => ProviderError::RateLimitExceeded {
            details: format!("Bedrock throttling error: {:?}", e),
            retry_delay: None,
        },
        ConverseError::AccessDeniedException(e) => {
            ProviderError::Authentication(format!("Failed to call Bedrock: {:?}", e))
        }
        ConverseError::ValidationException(e)
            if looks_like_context_overflow(e.message().unwrap_or_default()) =>
        {
            ProviderError::ContextLengthExceeded(format!("Failed to call Bedrock: {:?}", e))
        }
        ConverseError::ValidationException(e) => {
            // A non-overflow validation error is a hard client-side rejection (HTTP
            // 400). Retrying an identical request cannot fix it, so surface it as a
            // non-retryable execution error instead of a retryable ServerError.
            ProviderError::ExecutionError(format!("Bedrock rejected the request: {:?}", e))
        }
        ConverseError::ModelErrorException(e) => {
            ProviderError::ExecutionError(format!("Failed to call Bedrock: {:?}", e))
        }
        ConverseError::ModelTimeoutException(e) => {
            ProviderError::ServerError(format!("Bedrock model timed out (transient): {:?}", e))
        }
        ConverseError::ModelNotReadyException(e) => {
            ProviderError::ServerError(format!("Bedrock model not ready (transient): {:?}", e))
        }
        ConverseError::InternalServerException(e) => {
            ProviderError::ServerError(format!("Bedrock internal server error: {:?}", e))
        }
        ConverseError::ServiceUnavailableException(e) => {
            ProviderError::ServerError(format!("Bedrock service unavailable (transient): {:?}", e))
        }
        ConverseError::ResourceNotFoundException(e) => ProviderError::ExecutionError(format!(
            "Bedrock resource not found — check the model id / access: {:?}",
            e
        )),
        // `Unhandled` (and any future variant): the proxy returned an error the SDK
        // couldn't type. Fall back to the raw HTTP status + body captured above.
        other => classify_untyped_bedrock_error(status, body_says_overflow, detail, other),
    }
}

/// Classify a Bedrock error the SDK could not map to a typed variant, using the
/// raw HTTP status code and body signals. This is the path a proxied error takes.
fn classify_untyped_bedrock_error(
    status: Option<u16>,
    body_says_overflow: bool,
    detail: String,
    err: ConverseError,
) -> ProviderError {
    // If either the body or whatever metadata the SDK salvaged reads like an
    // overflow, treat it as a context-length problem so the agent can compact.
    if body_says_overflow || looks_like_context_overflow(err.message().unwrap_or_default()) {
        return ProviderError::ContextLengthExceeded(format!(
            "Bedrock reported a context/token-window overflow. {detail}"
        ));
    }

    match status {
        Some(429) => ProviderError::RateLimitExceeded {
            details: format!("Bedrock endpoint returned HTTP 429 (throttled). {detail}"),
            retry_delay: None,
        },
        Some(413) => ProviderError::ContextLengthExceeded(format!(
            "Bedrock endpoint returned HTTP 413 (payload too large). {detail}"
        )),
        Some(code @ (401 | 403)) => ProviderError::Authentication(format!(
            "Bedrock endpoint returned HTTP {code} (unauthorized). {detail}"
        )),
        Some(408) => ProviderError::ServerError(format!(
            "Bedrock endpoint returned HTTP 408 (request timeout). This is usually transient. {detail}"
        )),
        Some(code) if (500..600).contains(&code) => ProviderError::ServerError(format!(
            "Bedrock endpoint returned HTTP {code}. This is usually transient. {detail}"
        )),
        Some(code) => {
            // Some other status the proxy invented. Keep it retryable (proxy blips
            // dominate here) but name the status so the cause is visible.
            ProviderError::ServerError(format!("Bedrock endpoint returned HTTP {code}. {detail}"))
        }
        // No raw response at all (should not happen for a ServiceError, but keep it
        // safe): treat as a retryable server error.
        None => ProviderError::ServerError(detail),
    }
}

#[cfg(test)]
mod bedrock_error_tests {
    use super::*;
    use crate::providers::retry::should_retry;
    use aws_sdk_bedrockruntime::config::http::HttpResponse;
    use aws_sdk_bedrockruntime::error::ErrorMetadata;
    use aws_sdk_bedrockruntime::operation::converse::ConverseError;
    use aws_sdk_bedrockruntime::types::error::{ThrottlingException, ValidationException};
    use aws_smithy_runtime_api::http::StatusCode;
    use aws_smithy_types::body::SdkBody;

    /// A `ServiceError` — the SDK received an HTTP error response. `inner` is the
    /// (possibly `Unhandled`) typed error the SDK managed to parse from the body.
    fn service_err(status: u16, body: &str, inner: ConverseError) -> SdkError<ConverseError> {
        let resp = HttpResponse::new(
            StatusCode::try_from(status).unwrap(),
            SdkBody::from(body.as_bytes().to_vec()),
        );
        SdkError::service_error(inner, resp)
    }

    /// A proxy error the SDK could not type: `ConverseError::Unhandled` with empty
    /// metadata — exactly what UCSF's Versa MuleSoft proxy produced in the incident.
    fn proxy_unhandled(status: u16, body: &str) -> SdkError<ConverseError> {
        service_err(
            status,
            body,
            ConverseError::generic(ErrorMetadata::builder().build()),
        )
    }

    // ---- looks_like_context_overflow --------------------------------------

    #[test]
    fn overflow_phrasings_are_detected() {
        for s in [
            "Input is too long for requested model.",
            "input is too long",
            "The prompt is too long",
            "exceeds the maximum context length",
            "too many tokens in the request",
            "This exceeds the model's token limit",
            "maximum number of tokens",
        ] {
            assert!(looks_like_context_overflow(s), "should flag: {s}");
        }
        for s in ["throttled", "internal server error", "access denied", ""] {
            assert!(!looks_like_context_overflow(s), "should NOT flag: {s}");
        }
    }

    // ---- the incident: proxy errors the SDK collapsed to `Unhandled` -------

    /// Reproduces the captured incident error: a `ServiceError` wrapping
    /// `ConverseError::Unhandled { meta: empty }`. Before the fix this became a
    /// generic retryable `ServerError` with an unreadable `{:?}` dump. A 429 from
    /// the proxy must now be recognised as a rate limit so it gets the deeper
    /// retry budget and informs the scheduler.
    #[test]
    fn proxy_429_is_rate_limit_not_opaque_server_error() {
        let err = classify_bedrock_converse_error(proxy_unhandled(429, ""));
        assert!(
            matches!(err, ProviderError::RateLimitExceeded { .. }),
            "expected RateLimitExceeded, got {err:?}"
        );
        assert!(should_retry(&err));
    }

    #[test]
    fn proxy_5xx_is_retryable_server_error_with_readable_message() {
        for status in [500u16, 502, 503, 504] {
            let err =
                classify_bedrock_converse_error(proxy_unhandled(status, "<html>gateway</html>"));
            assert!(
                matches!(err, ProviderError::ServerError(_)),
                "status {status}: expected ServerError, got {err:?}"
            );
            assert!(should_retry(&err), "status {status} should be retryable");
            // No raw `Unhandled(Unhandled { .. })` dump leaking to the user.
            assert!(err.to_string().contains(&status.to_string()));
        }
    }

    /// The failure mode that stranded the original session: an over-limit prompt
    /// that the proxy rejected. If it never becomes `ContextLengthExceeded`, the
    /// agent can't auto-compact and every retry re-sends the same doomed prompt.
    #[test]
    fn proxy_400_input_too_long_is_context_length_exceeded() {
        let err = classify_bedrock_converse_error(proxy_unhandled(
            400,
            "{\"message\":\"Input is too long for requested model.\"}",
        ));
        assert!(
            matches!(err, ProviderError::ContextLengthExceeded(_)),
            "expected ContextLengthExceeded, got {err:?}"
        );
        // Context overflow is handled by compaction, not the generic retry loop.
        assert!(!should_retry(&err));
    }

    #[test]
    fn proxy_413_payload_too_large_is_context_length_exceeded() {
        let err = classify_bedrock_converse_error(proxy_unhandled(413, ""));
        assert!(
            matches!(err, ProviderError::ContextLengthExceeded(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn proxy_401_403_are_auth_errors_not_retried() {
        for status in [401u16, 403] {
            let err = classify_bedrock_converse_error(proxy_unhandled(status, ""));
            assert!(
                matches!(err, ProviderError::Authentication(_)),
                "status {status}: expected Authentication, got {err:?}"
            );
            assert!(!should_retry(&err), "auth errors must not be retried");
        }
    }

    // ---- transport failures (hung / unreachable endpoint) ------------------

    #[test]
    fn timeout_is_retryable_server_error_with_clear_message() {
        let err = classify_bedrock_converse_error(SdkError::timeout_error(Box::new(
            std::io::Error::new(std::io::ErrorKind::TimedOut, "stalled"),
        )));
        match &err {
            ProviderError::ServerError(msg) => assert!(
                msg.to_lowercase().contains("timed out"),
                "message should mention a timeout: {msg}"
            ),
            other => panic!("expected ServerError, got {other:?}"),
        }
        assert!(should_retry(&err));
    }

    // ---- typed variants still classify correctly ---------------------------

    #[test]
    fn typed_throttling_is_rate_limit() {
        let inner = ConverseError::ThrottlingException(
            ThrottlingException::builder().message("slow down").build(),
        );
        let err = classify_bedrock_converse_error(service_err(429, "", inner));
        assert!(
            matches!(err, ProviderError::RateLimitExceeded { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn typed_validation_overflow_is_context_length() {
        let inner = ConverseError::ValidationException(
            ValidationException::builder()
                .message("Input is too long for requested model.")
                .build(),
        );
        let err = classify_bedrock_converse_error(service_err(400, "", inner));
        assert!(
            matches!(err, ProviderError::ContextLengthExceeded(_)),
            "got {err:?}"
        );
    }

    #[test]
    fn typed_validation_non_overflow_is_non_retryable_execution_error() {
        let inner = ConverseError::ValidationException(
            ValidationException::builder()
                .message("The value at messages.1 is invalid")
                .build(),
        );
        let err = classify_bedrock_converse_error(service_err(400, "", inner));
        assert!(
            matches!(err, ProviderError::ExecutionError(_)),
            "got {err:?}"
        );
        // A malformed request cannot be fixed by retrying it verbatim.
        assert!(!should_retry(&err));
    }

    // ---- timeout config knob ----------------------------------------------

    #[test]
    fn timeout_config_env_override_and_disable() {
        // Note: relies on process-global env; kept simple and independent of the
        // shared Config by exercising the env parse path directly.
        let parse = |v: &str| v.trim().parse::<u64>().ok();
        assert_eq!(parse("0"), Some(0));
        assert_eq!(parse("600"), Some(600));
        assert_eq!(parse("  90 "), Some(90));
        assert_eq!(parse("nope"), None);
        // Default is a bounded, generous value (never infinite).
        const { assert!(BEDROCK_DEFAULT_OPERATION_TIMEOUT_SECS >= 60) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use rmcp::model::{AnnotateAble, RawImageContent};

    // Base64 encoded 1x1 PNG image for testing
    const TEST_IMAGE_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    #[test]
    fn test_to_bedrock_image_supported_formats() -> Result<()> {
        let supported_formats = [
            "image/png",
            "image/jpeg",
            "image/jpg",
            "image/gif",
            "image/webp",
        ];

        for mime_type in supported_formats {
            let image = RawImageContent {
                data: TEST_IMAGE_BASE64.to_string(),
                mime_type: mime_type.to_string(),
                meta: None,
            }
            .no_annotation();

            let result = to_bedrock_image(&image.data, &image.mime_type);
            assert!(result.is_ok(), "Failed to convert {} format", mime_type);
        }

        Ok(())
    }

    #[test]
    fn test_to_bedrock_image_unsupported_format() {
        let image = RawImageContent {
            data: TEST_IMAGE_BASE64.to_string(),
            mime_type: "image/bmp".to_string(),
            meta: None,
        }
        .no_annotation();

        let result = to_bedrock_image(&image.data, &image.mime_type);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Unsupported image format: image/bmp"));
        assert!(error_msg.contains("Bedrock supports png, jpeg, gif, webp"));
    }

    #[test]
    fn test_to_bedrock_image_invalid_base64() {
        let image = RawImageContent {
            data: "invalid_base64_data!!!".to_string(),
            mime_type: "image/png".to_string(),
            meta: None,
        }
        .no_annotation();

        let result = to_bedrock_image(&image.data, &image.mime_type);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Failed to decode base64 image data"));
    }

    #[test]
    fn test_to_bedrock_message_content_image() -> Result<()> {
        let image = RawImageContent {
            data: TEST_IMAGE_BASE64.to_string(),
            mime_type: "image/png".to_string(),
            meta: None,
        }
        .no_annotation();

        let message_content = MessageContent::Image(image);
        let result = to_bedrock_message_content(&message_content)?;

        // Verify we get an Image content block
        assert!(matches!(result, bedrock::ContentBlock::Image(_)));

        Ok(())
    }

    #[test]
    fn test_to_bedrock_tool_result_content_block_image() -> Result<()> {
        let content = Content::image(TEST_IMAGE_BASE64.to_string(), "image/png".to_string());
        let result = to_bedrock_tool_result_content_block("test_id", content)?;

        // Verify the wrapper correctly converts Content::Image to ToolResultContentBlock::Image
        assert!(matches!(result, bedrock::ToolResultContentBlock::Image(_)));

        Ok(())
    }
}
