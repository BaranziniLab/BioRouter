use crate::agents::extension::ExtensionConfig;
use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use crate::config::{extension_entry_is_persisted, get_extension_entry_by_name, ExtensionEntry};
use crate::privacy::ProviderTier;
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, ErrorCode, ErrorData, GetPromptResult, Implementation,
    InitializeResult, JsonObject, ListPromptsResult, ListResourcesResult, ListToolsResult,
    ProtocolVersion, ReadResourceResult, ServerCapabilities, ServerNotification, Tool,
    ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::error;

pub static EXTENSION_NAME: &str = "Extension Manager";
// pub static DISPLAY_NAME: &str = "Extension Manager";

#[derive(Debug, thiserror::Error)]
pub enum ExtensionManagerToolError {
    #[error("Unknown tool: {tool_name}")]
    UnknownTool { tool_name: String },

    #[error("Extension manager not available")]
    ManagerUnavailable,

    #[error("Missing required parameter: {param_name}")]
    MissingParameter { param_name: String },

    #[error("Invalid action: {action}. Must be 'enable' or 'disable'")]
    InvalidAction { action: String },

    #[error("Extension operation failed: {message}")]
    OperationFailed { message: String },

    #[error("Failed to deserialize parameters: {0}")]
    DeserializationError(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ManageExtensionAction {
    Enable,
    Disable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ManageExtensionsParams {
    pub action: ManageExtensionAction,
    pub extension_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReadResourceParams {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListResourcesParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_name: Option<String>,
}

pub const READ_RESOURCE_TOOL_NAME: &str = "read_resource";
pub const LIST_RESOURCES_TOOL_NAME: &str = "list_resources";
pub const SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME: &str = "search_available_extensions";
pub const MANAGE_EXTENSIONS_TOOL_NAME: &str = "manage_extensions";
pub const MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE: &str = "extensionmanager__manage_extensions";

pub struct ExtensionManagerClient {
    info: InitializeResult,
    #[allow(dead_code)]
    context: PlatformExtensionContext,
}

/// Gate for the `manage_extensions` enable path (#42): an extension whose
/// **persisted** config entry carries `enabled: false` was turned off by the
/// operator, and the agent must not silently re-enable it — that would defeat
/// the pinned tool environment the operator set up (benchmarking, safety).
/// The refusal tells the model to ask the user instead; `/ext:` and the
/// GUI/config remain the user-explicit escape hatches.
///
/// `persisted` is the provenance signal (`extension_entry_is_persisted`):
/// `get_extension_entry_by_name` reads the post-injection map, where an
/// absent platform extension is injected with its default — so a default-off
/// one (e.g. `chatrecall`) shows up as `enabled: false` without any operator
/// ever writing that. Only an entry actually present in the on-disk config
/// counts as operator-disabled; injected defaults stay agent-enableable.
///
/// `caller` is Gate F1 (issue #56), and it is a REQUIRED parameter rather than
/// a check bolted onto the caller because this predicate is the only part of
/// the enable path that is pure — testable with no global config, no registry
/// and no live extension. Enabling `ucsfomopagent` is not a tool call into a
/// private server, it is the call that SPAWNS one: it pulls that server's
/// `CLINICAL_RECORDS_*` secrets out of the keychain and opens a session to the
/// UCSF CDW. Gate C refusing the first tool call afterwards is already too
/// late.
fn check_enable_allowed(
    entry: Option<ExtensionEntry>,
    persisted: bool,
    extension_name: &str,
    caller: ProviderTier,
) -> Result<ExtensionConfig, ErrorData> {
    match entry {
        None => Err(ErrorData::new(
            ErrorCode::RESOURCE_NOT_FOUND,
            format!(
                "Extension '{}' not found. Please check the extension name and try again.",
                extension_name
            ),
            None,
        )),
        Some(entry) if !entry.enabled && persisted => Err(ErrorData::new(
            ErrorCode::INVALID_REQUEST,
            format!(
                "Extension '{}' is disabled in the Biorouter configuration (enabled: false). \
                 The operator turned it off deliberately, so do not enable it yourself. \
                 If it is needed for this task, ask the user to re-enable it — in the desktop \
                 app under Settings > Extensions, with `biorouter configure`, or by editing \
                 the extension's entry in config.yaml.",
                extension_name
            ),
            None,
        )),
        // Gate F1. Before the permit arm below, and stated on the NAME the
        // model asked for rather than on the config record: nothing local may
        // grant private (R11(i)), so the tier comes from the compiled-in
        // marketplace baseline the same way it does at every admission point.
        Some(_)
            if crate::privacy::classify_extension(extension_name).is_private()
                && caller == ProviderTier::Public =>
        {
            Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!(
                    "Extension '{extension_name}' is a private extension: the Biorouter \
                     marketplace marks it as reaching data held inside the institution, so only \
                     a private model may enable or call it. This session is running on a public \
                     model, so do not enable it. If it is needed for this task, ask the user to \
                     switch this chat to a private model first — in the desktop app under \
                     Settings > Models, or with the model chip in the composer."
                ),
                None,
            ))
        }
        Some(entry) => Ok(entry.config),
    }
}

impl ExtensionManagerClient {
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
                Extension Management

                Use these tools to discover, enable, and disable extensions, as well as review resources.

                Available tools:
                - search_available_extensions: Find extensions available to enable/disable
                - manage_extensions: Enable or disable extensions
                - list_resources: List resources from extensions
                - read_resource: Read specific resources from extensions

                When you lack the tools needed to complete a task, use search_available_extensions first
                to discover what extensions can help.

                Use manage_extensions to enable or disable specific extensions by name.
                Use list_resources and read_resource to work with extension data and resources.
            "#}.to_string()),
        };

        Ok(Self { info, context })
    }

    async fn handle_search_available_extensions(
        &self,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        if let Some(weak_ref) = &self.context.extension_manager {
            if let Some(extension_manager) = weak_ref.upgrade() {
                match extension_manager.search_available_extensions().await {
                    Ok(content) => Ok(content),
                    Err(e) => Err(ExtensionManagerToolError::OperationFailed {
                        message: format!("Failed to search available extensions: {}", e.message),
                    }),
                }
            } else {
                Err(ExtensionManagerToolError::ManagerUnavailable)
            }
        } else {
            Err(ExtensionManagerToolError::ManagerUnavailable)
        }
    }

    /// `cap` is the capability THIS tool call was admitted on, taken straight
    /// off its `McpMeta` — never re-sampled here, for the reason
    /// [`crate::privacy::CallCapability`] exists: enabling an extension runs
    /// inside the driven future, an unbounded wall-clock gap past admission,
    /// and a fresh read there is what would let a Public-admitted call spawn a
    /// private server after the user switched models mid-turn.
    async fn handle_manage_extensions(
        &self,
        arguments: Option<JsonObject>,
        cap: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        let arguments = arguments.ok_or(ExtensionManagerToolError::MissingParameter {
            param_name: "arguments".to_string(),
        })?;

        let params: ManageExtensionsParams =
            serde_json::from_value(serde_json::Value::Object(arguments))?;

        match self
            .manage_extensions_impl(params.action, params.extension_name, cap)
            .await
        {
            Ok(content) => Ok(content),
            Err(error_data) => Err(ExtensionManagerToolError::OperationFailed {
                message: error_data.message.to_string(),
            }),
        }
    }

    async fn manage_extensions_impl(
        &self,
        action: ManageExtensionAction,
        extension_name: String,
        cap: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ErrorData> {
        let extension_manager = self
            .context
            .extension_manager
            .as_ref()
            .and_then(|weak| weak.upgrade())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Extension manager is no longer available".to_string(),
                    None,
                )
            })?;

        if action == ManageExtensionAction::Disable {
            return extension_manager
                .remove_extension(&extension_name)
                .await
                .map(|_| {
                    vec![Content::text(format!(
                        "The extension '{}' has been disabled successfully",
                        extension_name
                    ))]
                })
                .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None));
        }

        let entry = get_extension_entry_by_name(&extension_name);
        let persisted = entry
            .as_ref()
            .is_some_and(|entry| extension_entry_is_persisted(&entry.config.name()));
        // DR-15's master opt-out, read off the SAME sample as the tier so the
        // two can never be observed at different instants. With tiers switched
        // off the caller is handed to the gate as private, which silences Gate
        // F1 and nothing else — the alternative, a second flag inside the pure
        // predicate, is exactly the second read this type exists to prevent.
        let caller = if cap.enforced() {
            cap.tier()
        } else {
            ProviderTier::Private
        };
        let config = check_enable_allowed(entry, persisted, &extension_name, caller)?;

        extension_manager
            .add_extension(config)
            .await
            .map(|_| {
                vec![Content::text(format!(
                    "The extension '{}' has been installed successfully",
                    extension_name
                ))]
            })
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))
    }

    /// `admitted` is the capability THIS tool call was admitted on, taken
    /// straight off its `McpMeta` and threaded into Gate C's sibling guard
    /// (issue #56). The manager must not sample its own: this runs inside the
    /// driven future, an unbounded wall-clock gap past admission, and a fresh
    /// read there is what would let a Public-admitted call list a private
    /// extension's resources after the user switched models mid-turn.
    async fn handle_list_resources(
        &self,
        arguments: Option<JsonObject>,
        admitted: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        if let Some(weak_ref) = &self.context.extension_manager {
            if let Some(extension_manager) = weak_ref.upgrade() {
                let params = arguments
                    .map(serde_json::Value::Object)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                match extension_manager
                    .list_resources(
                        params,
                        Some(admitted),
                        tokio_util::sync::CancellationToken::default(),
                    )
                    .await
                {
                    Ok(content) => Ok(content),
                    Err(e) => Err(ExtensionManagerToolError::OperationFailed {
                        message: format!("Failed to list resources: {}", e.message),
                    }),
                }
            } else {
                Err(ExtensionManagerToolError::ManagerUnavailable)
            }
        } else {
            Err(ExtensionManagerToolError::ManagerUnavailable)
        }
    }

    /// `admitted`: see [`Self::handle_list_resources`]. `read_resource` with no
    /// `extension_name` fans out over every installed extension, so the value
    /// threaded here decides which servers this call is allowed to probe.
    async fn handle_read_resource(
        &self,
        arguments: Option<JsonObject>,
        admitted: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        if let Some(weak_ref) = &self.context.extension_manager {
            if let Some(extension_manager) = weak_ref.upgrade() {
                let params = arguments
                    .map(serde_json::Value::Object)
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                match extension_manager
                    .read_resource_tool(
                        params,
                        Some(admitted),
                        tokio_util::sync::CancellationToken::default(),
                    )
                    .await
                {
                    Ok(content) => Ok(content),
                    Err(e) => Err(ExtensionManagerToolError::OperationFailed {
                        message: format!("Failed to read resource: {}", e.message),
                    }),
                }
            } else {
                Err(ExtensionManagerToolError::ManagerUnavailable)
            }
        } else {
            Err(ExtensionManagerToolError::ManagerUnavailable)
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn get_tools(&self) -> Vec<Tool> {
        let mut tools = vec![
            Tool::new(
                SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME.to_string(),
                "Searches for additional extensions available to help complete tasks.
        Use this tool when you're unable to find a specific feature or functionality you need to complete your task, or when standard approaches aren't working.
        These extensions might provide the exact tools needed to solve your problem.
        If you find a relevant one, consider using your tools to enable it.".to_string(),
                Arc::new(
                    serde_json::json!({
                        "type": "object",
                        "required": [],
                        "properties": {}
                    })
                    .as_object()
                    .expect("Schema must be an object")
                    .clone()
                ),
            ).annotate(ToolAnnotations {
                title: Some("Discover extensions".to_string()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(false),
                open_world_hint: Some(false),
            }),
            Tool::new(
                MANAGE_EXTENSIONS_TOOL_NAME.to_string(),
                "Tool to manage extensions and tools in biorouter context.
            Enable or disable extensions to help complete tasks.
            Enable or disable an extension by providing the extension name.
            ".to_string(),
                Arc::new(
                    serde_json::to_value(schema_for!(ManageExtensionsParams))
                        .expect("Failed to serialize schema")
                        .as_object()
                        .expect("Schema must be an object")
                        .clone()
                ),
            ).annotate(ToolAnnotations {
                title: Some("Enable or disable an extension".to_string()),
                read_only_hint: Some(false),
                destructive_hint: Some(false),
                idempotent_hint: Some(false),
                open_world_hint: Some(false),
            }),
        ];

        // Only add resource tools if extension manager supports resources
        if let Some(weak_ref) = &self.context.extension_manager {
            if let Some(extension_manager) = weak_ref.upgrade() {
                if extension_manager.supports_resources().await {
                    tools.extend([
                        Tool::new(
                            LIST_RESOURCES_TOOL_NAME.to_string(),
                            indoc! {r#"
            List resources from an extension(s).

            Resources allow extensions to share data that provide context to LLMs, such as
            files, database schemas, or application-specific information. This tool lists resources
            in the provided extension, and returns a list for the user to browse. If no extension
            is provided, the tool will search all extensions for the resource.
        "#}.to_string(),
                            Arc::new(
                                serde_json::to_value(schema_for!(ListResourcesParams))
                                    .expect("Failed to serialize schema")
                                    .as_object()
                                    .expect("Schema must be an object")
                                    .clone()
                            ),
                        ).annotate(ToolAnnotations {
                            title: Some("List resources".to_string()),
                            read_only_hint: Some(true),
                            destructive_hint: Some(false),
                            idempotent_hint: Some(false),
                            open_world_hint: Some(false),
                        }),
                        Tool::new(
                            READ_RESOURCE_TOOL_NAME.to_string(),
                            indoc! {r#"
            Read a resource from an extension.

            Resources allow extensions to share data that provide context to LLMs, such as
            files, database schemas, or application-specific information. This tool searches for the
            resource URI in the provided extension, and reads in the resource content. If no extension
            is provided, the tool will search all extensions for the resource.
        "#}.to_string(),
                            Arc::new(
                                serde_json::to_value(schema_for!(ReadResourceParams))
                                    .expect("Failed to serialize schema")
                                    .as_object()
                                    .expect("Schema must be an object")
                                    .clone()
                            ),
                        ).annotate(ToolAnnotations {
                            title: Some("Read a resource".to_string()),
                            read_only_hint: Some(true),
                            destructive_hint: Some(false),
                            idempotent_hint: Some(false),
                            open_world_hint: Some(false),
                        }),
                    ]);
                }
            }
        }

        tools
    }
}

#[async_trait]
impl McpClientTrait for ExtensionManagerClient {
    async fn list_resources(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn read_resource(
        &self,
        _uri: &str,
        _cancellation_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        // Extension manager doesn't expose resources directly
        Err(Error::TransportClosed)
    }

    async fn list_tools(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: self.get_tools().await,
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
        let result = match name {
            SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME => {
                self.handle_search_available_extensions().await
            }
            // Issue #56 Gate F1: enabling an extension SPAWNS its server, so it
            // carries the admitted capability for the same reason the two reads
            // below do.
            MANAGE_EXTENSIONS_TOOL_NAME => {
                self.handle_manage_extensions(arguments, meta.capability)
                    .await
            }
            // Issue #56: these two reach an MCP server, so they carry the
            // capability this call was ADMITTED on into Gate C's sibling guard
            // rather than letting the manager sample a newer one.
            LIST_RESOURCES_TOOL_NAME => {
                self.handle_list_resources(arguments, meta.capability).await
            }
            READ_RESOURCE_TOOL_NAME => self.handle_read_resource(arguments, meta.capability).await,
            _ => Err(ExtensionManagerToolError::UnknownTool {
                tool_name: name.to_string(),
            }),
        };

        match result {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => {
                // Log the error for debugging
                error!("Extension manager tool '{}' failed: {}", name, error);

                // Return proper error result with is_error flag set
                Ok(CallToolResult {
                    content: vec![Content::text(error.to_string())],
                    is_error: Some(true), // ✅ Properly mark as error
                    structured_content: None,
                    meta: None,
                })
            }
        }
    }

    async fn list_prompts(
        &self,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListPromptsResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn get_prompt(
        &self,
        _name: &str,
        _arguments: Value,
        _cancellation_token: CancellationToken,
    ) -> Result<GetPromptResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        mpsc::channel(1).1
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(enabled: bool) -> ExtensionEntry {
        ExtensionEntry {
            enabled,
            config: ExtensionConfig::Builtin {
                name: "developer".to_string(),
                display_name: Some("Developer".to_string()),
                description: "shell and file tools".to_string(),
                timeout: None,
                bundled: Some(true),
                available_tools: vec![],
            },
        }
    }

    #[test]
    fn enable_of_unknown_extension_is_not_found() {
        let err = check_enable_allowed(None, false, "ghost", ProviderTier::Public).unwrap_err();
        assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
        assert!(err.message.contains("not found"), "{}", err.message);
    }

    #[test]
    fn enable_of_operator_disabled_extension_is_refused_with_guidance() {
        // #42: `enabled: false` written into config.yaml must be a dependable
        // pin — the agent may not silently re-enable what the operator turned
        // off. `persisted: true` = the entry exists in the on-disk config.
        let err = check_enable_allowed(Some(entry(false)), true, "developer", ProviderTier::Public)
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_REQUEST);
        assert!(err.message.contains("disabled"), "{}", err.message);
        assert!(
            err.message.contains("operator"),
            "must say who disabled it: {}",
            err.message
        );
        assert!(
            err.message.contains("ask the user"),
            "must direct the model to the user: {}",
            err.message
        );
    }

    #[test]
    fn enable_of_default_off_injected_extension_is_allowed() {
        // #42 provenance: an absent platform extension is injected into the
        // extensions map with its default — a default-off one (chatrecall)
        // carries `enabled: false` without any operator writing it. That is
        // not an operator pin, so the agent enable flow must stay open.
        let config = check_enable_allowed(
            Some(entry(false)),
            false,
            "chatrecall",
            ProviderTier::Public,
        )
        .expect("allowed");
        assert_eq!(config.name(), "developer");
    }

    #[test]
    fn enable_of_config_enabled_extension_passes_the_config_through() {
        let config =
            check_enable_allowed(Some(entry(true)), true, "developer", ProviderTier::Public)
                .expect("allowed");
        assert_eq!(config.name(), "developer");
    }

    /// The same entry shape as [`entry`], but under an arbitrary name and
    /// always enabled — the tier is resolved from the NAME the model asked
    /// for, never from the config record, so this fixture only has to carry a
    /// name that is not itself a refusal for some other reason.
    fn entry_for(name: &str) -> ExtensionEntry {
        ExtensionEntry {
            enabled: true,
            config: ExtensionConfig::Builtin {
                name: name.to_string(),
                display_name: None,
                description: "fixture".to_string(),
                timeout: None,
                bundled: Some(true),
                available_tools: vec![],
            },
        }
    }

    /// Gate F1 (#56): `extensionmanager__manage_extensions` is not a tool call
    /// into a private server — it is the call that SPAWNS one. Enabling
    /// `ucsfomopagent` pulls its `CLINICAL_RECORDS_*` secrets out of the
    /// keychain and opens a session to the UCSF CDW; being refused afterwards,
    /// at the first tool call, is already too late.
    #[test]
    fn a_public_caller_may_not_enable_a_private_extension() {
        use ProviderTier::{Private, Public};
        // Pure, like its four siblings above, so it needs no global config.
        let e = check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            Public,
        )
        .unwrap_err();
        assert!(e.message.contains("marketplace"), "{}", e.message);
        assert!(e.message.contains("private"), "{}", e.message);
        check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            Private,
        )
        .expect("a private caller may enable it");
        check_enable_allowed(Some(entry_for("developer")), false, "developer", Public)
            .expect("public extensions are unaffected");
    }
}
