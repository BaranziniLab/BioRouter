use crate::agents::tool_errors::{
    classify_error_data, kind_from_text, ToolErrorKind, ENVELOPE_KEY,
};
use crate::mcp_utils::ToolResult;
use rmcp::model::{CallToolRequestParams, ErrorCode, ErrorData, JsonObject};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

pub fn serialize<T, S>(value: &ToolResult<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    match value {
        Ok(val) => {
            let mut state = serializer.serialize_struct("ToolResult", 2)?;
            state.serialize_field("status", "success")?;
            state.serialize_field("value", val)?;
            state.end()
        }
        Err(err) => {
            // BR-51: the wire form of a failed tool call kept only its *text*.
            // The MCP error code and any structured payload the server attached
            // were dropped on the way to disk, so a reloaded conversation could
            // no longer tell a retryable blip from a hard failure. The taxonomy
            // rides along as two flat, optional fields — old readers ignore them,
            // and `error` still carries the whole human-readable message.
            let taxonomy = classify_error_data(err);
            let mut state = serializer.serialize_struct("ToolResult", 4)?;
            state.serialize_field("status", "error")?;
            state.serialize_field("error", &err.to_string())?;
            state.serialize_field("error_kind", taxonomy.kind.as_str())?;
            state.serialize_field("retryable", &taxonomy.retryable)?;
            state.end()
        }
    }
}

/// Rebuild the [`ErrorData`] for a persisted error, re-attaching the taxonomy as
/// its `data` payload so a reloaded result classifies exactly as the live one did
/// (rather than being re-guessed from prose that may have been truncated).
fn error_data_with_taxonomy(
    error: String,
    error_kind: Option<String>,
    retryable: Option<bool>,
) -> ErrorData {
    let kind = error_kind
        .as_deref()
        .and_then(ToolErrorKind::parse)
        .unwrap_or_else(|| kind_from_text(&error));
    let retryable = retryable.unwrap_or_else(|| kind.retryable());
    ErrorData {
        code: ErrorCode::INTERNAL_ERROR,
        message: Cow::from(error.clone()),
        data: Some(serde_json::json!({
            ENVELOPE_KEY: { "kind": kind.as_str(), "retryable": retryable, "message": error }
        })),
    }
}

#[derive(Deserialize)]
struct ToolCallWithValueArguments {
    name: String,
    arguments: serde_json::Value,
}

impl ToolCallWithValueArguments {
    fn into_call_tool_request_param(self) -> CallToolRequestParams {
        let arguments = match self.arguments {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            other => {
                let mut map = JsonObject::new();
                map.insert("value".to_string(), other);
                Some(map)
            }
        };
        CallToolRequestParams {
            task: None,
            name: Cow::Owned(self.name),
            arguments,
            meta: None,
        }
    }
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<ToolResult<CallToolRequestParams>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ResultFormat {
        SuccessWithCallToolRequestParam {
            status: String,
            value: CallToolRequestParams,
        },
        SuccessWithToolCallValueArguments {
            status: String,
            value: ToolCallWithValueArguments,
        },
        Error {
            status: String,
            error: String,
            // BR-51. Absent on anything written before the taxonomy existed, so
            // the kind is re-derived from the text in that case.
            #[serde(default)]
            error_kind: Option<String>,
            #[serde(default)]
            retryable: Option<bool>,
        },
    }

    let format = ResultFormat::deserialize(deserializer)?;

    match format {
        ResultFormat::SuccessWithCallToolRequestParam { status, value } => {
            if status == "success" {
                Ok(Ok(value))
            } else {
                Err(serde::de::Error::custom(format!(
                    "Expected status 'success', got '{}'",
                    status
                )))
            }
        }
        ResultFormat::SuccessWithToolCallValueArguments { status, value } => {
            if status == "success" {
                Ok(Ok(value.into_call_tool_request_param()))
            } else {
                Err(serde::de::Error::custom(format!(
                    "Expected status 'success', got '{}'",
                    status
                )))
            }
        }
        ResultFormat::Error {
            status,
            error,
            error_kind,
            retryable,
        } => {
            if status == "error" {
                Ok(Err(error_data_with_taxonomy(error, error_kind, retryable)))
            } else {
                Err(serde::de::Error::custom(format!(
                    "Expected status 'error', got '{}'",
                    status
                )))
            }
        }
    }
}

pub mod call_tool_result {
    use super::*;
    use rmcp::model::{CallToolResult, Content};

    pub fn serialize<S>(
        value: &ToolResult<CallToolResult>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        super::serialize(value, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ToolResult<CallToolResult>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ResultFormat {
            SuccessWithCallToolResult {
                status: String,
                value: CallToolResult,
            },
            SuccessWithContentVec {
                status: String,
                value: Vec<Content>,
            },
            Error {
                status: String,
                error: String,
                // BR-51, as above: optional, so a pre-taxonomy record still reads.
                #[serde(default)]
                error_kind: Option<String>,
                #[serde(default)]
                retryable: Option<bool>,
            },
        }

        let original_value = serde_json::Value::deserialize(deserializer)?;

        let format = ResultFormat::deserialize(&original_value).map_err(|e| {
            tracing::debug!(
                "Failed to deserialize call_tool_result: {}. Original data: {}",
                e,
                serde_json::to_string(&original_value)
                    .unwrap_or_else(|_| "<invalid json>".to_string())
            );
            serde::de::Error::custom(e)
        })?;

        match format {
            ResultFormat::SuccessWithCallToolResult { status, value } => {
                if status == "success" {
                    Ok(Ok(value))
                } else {
                    Err(serde::de::Error::custom(format!(
                        "Expected status 'success', got '{}'",
                        status
                    )))
                }
            }
            ResultFormat::SuccessWithContentVec { status, value } => {
                if status == "success" {
                    Ok(Ok(CallToolResult::success(value)))
                } else {
                    Err(serde::de::Error::custom(format!(
                        "Expected status 'success', got '{}'",
                        status
                    )))
                }
            }
            ResultFormat::Error {
                status,
                error,
                error_kind,
                retryable,
            } => {
                if status == "error" {
                    Ok(Err(super::error_data_with_taxonomy(
                        error, error_kind, retryable,
                    )))
                } else {
                    Err(serde::de::Error::custom(format!(
                        "Expected status 'error', got '{}'",
                        status
                    )))
                }
            }
        }
    }

    pub fn validate(result: ToolResult<CallToolResult>) -> ToolResult<CallToolResult> {
        match &result {
            Ok(call_tool_result) => match serde_json::to_string(call_tool_result) {
                Ok(json_str) => match serde_json::from_str::<CallToolResult>(&json_str) {
                    Ok(_) => result,
                    Err(e) => {
                        tracing::error!("CallToolResult failed validation by deserialization: {}. Original data: {}", e, json_str);
                        Err(ErrorData {
                            code: ErrorCode::INTERNAL_ERROR,
                            message: Cow::from(format!("Tool result validation failed: {}", e)),
                            data: None,
                        })
                    }
                },
                Err(e) => {
                    tracing::error!("CallToolResult failed serialization: {}", e);
                    Err(ErrorData {
                        code: ErrorCode::INTERNAL_ERROR,
                        message: Cow::from(format!("Tool result serialization failed: {}", e)),
                        data: None,
                    })
                }
            },
            Err(_) => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolResult, Content, ErrorCode, ErrorData};
    use std::borrow::Cow;
    #[test]
    fn test_validate_accepts_valid_call_tool_result() {
        let valid_result = CallToolResult {
            content: vec![Content::text("test")],
            is_error: Some(false),
            structured_content: None,
            meta: None,
        };

        let tool_result: ToolResult<CallToolResult> = Ok(valid_result);
        let validated = call_tool_result::validate(tool_result);

        assert!(
            validated.is_ok(),
            "Expected validation to pass for valid CallToolResult"
        );
    }
    #[test]
    fn test_validate_returns_error_for_invalid_calltoolresult() {
        let valid_result = CallToolResult {
            content: vec![],
            is_error: Some(false),
            structured_content: None,
            meta: None,
        };

        let tool_result: ToolResult<CallToolResult> = Ok(valid_result);
        let validated = call_tool_result::validate(tool_result);

        assert!(validated.is_err());
        assert!(validated
            .unwrap_err()
            .message
            .contains("Tool result validation failed"))
    }

    #[test]
    fn test_validate_passes_through_errors() {
        let error_result: ToolResult<CallToolResult> = Err(ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from("test error"),
            data: None,
        });

        let validated = call_tool_result::validate(error_result.clone());

        assert!(validated.is_err());
        assert_eq!(validated.unwrap_err().message, "test error");
    }

    // ── BR-51: the taxonomy on the wire ───────────────────────────────────

    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        #[serde(with = "call_tool_result")]
        tool_result: ToolResult<CallToolResult>,
    }

    #[test]
    fn a_persisted_error_carries_its_kind_and_retryability() {
        let wrapper = Wrapper {
            tool_result: Err(ErrorData {
                code: ErrorCode::INTERNAL_ERROR,
                message: Cow::from("connection reset by peer"),
                data: None,
            }),
        };

        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&wrapper).unwrap()).unwrap();
        assert_eq!(json["tool_result"]["status"], "error");
        assert_eq!(json["tool_result"]["error_kind"], "transient");
        assert_eq!(json["tool_result"]["retryable"], true);
        // The human-readable message is untouched — the taxonomy augments it.
        // (`error` is `ErrorData`'s Display, i.e. "<code>: <message>", as before.)
        assert_eq!(
            json["tool_result"]["error"],
            "-32603: connection reset by peer"
        );

        let back: Wrapper = serde_json::from_value(json).unwrap();
        let error = crate::agents::tool_errors::classify(&back.tool_result).unwrap();
        assert_eq!(
            error.kind,
            crate::agents::tool_errors::ToolErrorKind::Transient
        );
        assert!(error.retryable);
    }

    /// A session written before BR-51 has neither field. It must still load, and
    /// the class is re-derived from the text rather than lost.
    #[test]
    fn a_pre_taxonomy_error_still_deserializes() {
        let json = serde_json::json!({
            "tool_result": { "status": "error", "error": "No such file or directory" }
        });

        let back: Wrapper = serde_json::from_value(json).unwrap();
        let error = crate::agents::tool_errors::classify(&back.tool_result).unwrap();
        assert_eq!(
            error.kind,
            crate::agents::tool_errors::ToolErrorKind::NotFound
        );
        assert!(!error.retryable);
        assert_eq!(
            back.tool_result.unwrap_err().message,
            "No such file or directory"
        );
    }

    #[test]
    fn a_successful_result_is_unchanged_on_the_wire() {
        let wrapper = Wrapper {
            tool_result: Ok(CallToolResult::success(vec![Content::text("fine")])),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&wrapper).unwrap()).unwrap();
        assert_eq!(json["tool_result"]["status"], "success");
        assert!(json["tool_result"].get("error_kind").is_none());
    }
}
