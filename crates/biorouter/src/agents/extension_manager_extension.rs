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
/// `cap` is Gate F1 (issue #56), and it is a REQUIRED parameter rather than
/// a check bolted onto the caller because this predicate is the only part of
/// the enable path that is pure — testable with no global config, no registry
/// and no live extension. Enabling `ucsfomopagent` is not a tool call into a
/// private server, it is the call that SPAWNS one: it pulls that server's
/// `CLINICAL_RECORDS_*` secrets out of the keychain and opens a session to the
/// UCSF CDW. Gate C refusing the first tool call afterwards is already too
/// late.
///
/// ⚠ **The capability, not a bare tier.** DR-15's master opt-out reaches this
/// gate through [`CallCapability::enforced`], and it is taken here rather than
/// at the call site so that the whole of Gate F1 — the tier axis and the toggle
/// axis — is one function, testable in both toggle positions with no global
/// mutation, and so that Task 30's structural inventory can name a single
/// `(file, fn)` pair for this row. `handle_manage_extensions` used to perform
/// the collapse itself, which left the toggle half of the decision with no
/// subject a test could hold.
fn check_enable_allowed(
    entry: Option<ExtensionEntry>,
    persisted: bool,
    extension_name: &str,
    cap: crate::privacy::CallCapability,
) -> Result<ExtensionConfig, ErrorData> {
    // DR-15's master opt-out, read off the SAME sample as the tier so the two
    // can never be observed at different instants. With tiers switched off the
    // caller is treated as private, which silences the Gate F1 arm below and
    // nothing else — the alternative, a second flag inside this predicate, is
    // exactly the second read `CallCapability` exists to prevent.
    let caller = if cap.enforced() {
        cap.tier()
    } else {
        ProviderTier::Private
    };
    // ⚠ **ONE resolution, both axes, for the whole of this function** — and the
    // reason this is a chain of early returns rather than the `match` it used to
    // be. Nothing local may GRANT private (R11(i)), so the tier still comes from
    // the compiled-in marketplace baseline; the entry is passed alongside the
    // name the model asked for because Task 43 (DR-23) resolves a renamed entry
    // through the install directory in its arguments, which the name no longer
    // carries. Passing the config can only raise the answer, never lower it.
    //
    // Task 48 (DR-26) added the affiliation arm below, and it first asked the
    // registry a SECOND time from a `match` guard — the exact "two lookups let
    // the two axes disagree about one entry" pattern `resolve_extension` exists
    // to prevent, and which that same task removed at `get_client_for_tool` and
    // at `/agent/add_extension`. A guard cannot hand its arm a value, so the
    // `match` was the shape that made two calls the path of least resistance.
    // Resolved once, here; both gates below read fields off that one value.
    let class = crate::privacy::resolve_extension(
        extension_name,
        entry.as_ref().map(|entry| &entry.config),
    );

    // Gate F1, and it is FIRST — above the not-found branch and above #42's
    // operator pin, which is a change finding 13 forced.
    //
    // ⚠ **Because both of those branches are install-state oracles.** "Extension
    // 'ucsfomopagent' not found" and "…is disabled in the Biorouter
    // configuration" each tell a public model something about what this machine
    // has installed and how the operator configured it — which is precisely the
    // secret the sibling finding closed at the catalogue, where the same names
    // were being printed outright. Answering the tier question first collapses
    // every private name a public caller can ask about onto one refusal, whether
    // it is installed, configured-off, or absent entirely.
    //
    // Nothing is lost in the other direction: a PRIVATE caller reaches the
    // not-found and operator-pin branches exactly as before, and so does a
    // public caller asking about a public extension — which is every case those
    // two branches were written for. The refusal below is also strictly the
    // safer thing to say, since it names no local state at all: it reports what
    // the compiled-in marketplace baseline says about a name the model itself
    // chose.
    if class.tier.is_private() && caller == ProviderTier::Public {
        return Err(ErrorData::new(
            ErrorCode::INVALID_REQUEST,
            format!(
                "Extension '{extension_name}' is a private extension: the Biorouter \
                 marketplace marks it as reaching data held inside the institution, so only \
                 a private model may enable or call it. This session is running on a public \
                 model, so do not enable it. If it is needed for this task, {}",
                // DR-16 (Task 18A) turned this sentence into a shared constant:
                // the same words now reach the model through the HTTP channels
                // too, and two copies would drift.
                crate::privacy::refusal::ASK_THE_USER_TO_SWITCH
            ),
            None,
        ));
    }

    let Some(entry) = entry else {
        return Err(ErrorData::new(
            ErrorCode::RESOURCE_NOT_FOUND,
            format!(
                "Extension '{}' not found. Please check the extension name and try again.",
                extension_name
            ),
            None,
        ));
    };
    if !entry.enabled && persisted {
        return Err(ErrorData::new(
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
        ));
    }

    // Task 48 / DR-26's third axis, and the LAST check before the permit
    // because it is the narrowest: every gate above refuses a class of
    // extension outright, where this refuses one PAIRING of an extension with
    // one bound model.
    //
    // ⚠ **The agent is refused, not warned, and that is not an inconsistency
    // with the bind surface.** DR-26's asymmetry: a user who insists may
    // proceed past a warning; an agent never clears one automatically — it
    // escalates to the user or the call does not happen.
    // `extensionmanager__manage_extensions` is the agent's path, and there is
    // no user in it. The user's own enable path is `/agent/add_extension`,
    // which warns and proceeds.
    //
    // Like Gate F1 above, this fires BEFORE the server is spawned. Enabling a
    // clinical connector pulls its credentials out of the keychain and opens
    // the session; a refusal at the first tool call is already too late. The
    // master opt-out reaches this through the capability, not through a second
    // read of the global.
    //
    // ⚠ **Task 49's grant is deliberately NOT consulted here.** A grant is the
    // user's acceptance of a *data flow* through a connector this chat already
    // has; it is not permission for the model to attach a connector the chat did
    // not have. Reading it here would let an agent turn one accepted flow into
    // the authority to open the very server that flow runs over — the enable is
    // what pulls credentials and starts the process — which is the "an agent
    // never clears a mismatch automatically" half of DR-26 undone by a lookup.
    // The route out is unchanged and is a user's: enable it from Settings
    // (`/agent/add_extension`, which warns and proceeds), then accept the flow.
    //
    // Task 57: `None` follows from that. An accept control here would let the
    // model turn "the user accepted this flow" into "the user asked for this
    // server to be started", which is the sentence above undone by a button.
    if let Some(warning) = cap.cross_affiliation_warning(extension_name, &class) {
        return Err(crate::privacy::refusal::cross_affiliation_refusal(
            &warning, None,
        ));
    }
    Ok(entry.config)
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

    /// `admitted` is the capability THIS tool call was admitted on, taken
    /// straight off its `McpMeta` and threaded into Gate E's catalogue filter
    /// (issue #56, finding 13). The manager must not sample its own, for the
    /// reason every other handler in this file gives: the read would happen
    /// inside the driven future, an unbounded wall-clock gap past admission, and
    /// a fresh one there is what would let a Public-admitted call read a private
    /// connector's name and marketplace description out of the catalogue after
    /// the user switched models mid-turn.
    async fn handle_search_available_extensions(
        &self,
        admitted: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        if let Some(weak_ref) = &self.context.extension_manager {
            if let Some(extension_manager) = weak_ref.upgrade() {
                match extension_manager
                    .search_available_extensions(admitted)
                    .await
                {
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
            // Issue #56 Gate F1, the DISABLE half (finding 14). This branch used
            // to return before any privacy decision existed on the path: `cap`
            // was in scope, and `remove_extension` ran without consulting it. A
            // chat on a public model could therefore unload the clinical
            // connector — a server Gate E keeps out of that model's tool list
            // entirely, refuses every call into, and (since finding 13) will not
            // even name in the catalogue. Being unable to see a connector while
            // being able to unload it is the disagreement this closes.
            //
            // The gate is `assert_extension_manageable`, which is
            // `assert_extension_reachable` verbatim, so discovery and management
            // answer with one function rather than two rules — including the
            // inverted unknown-name default that keeps this refusal from being
            // the existence oracle the leak next door was.
            extension_manager
                .assert_extension_manageable(&extension_name, cap)
                .await?;
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
        // The capability is handed to Gate F1 WHOLE — both axes, one sample.
        // The collapse DR-15's opt-out performs lives inside
        // `check_enable_allowed`, so the toggle half of this decision has a
        // subject a test can hold; doing it here left it with none.
        let config = check_enable_allowed(entry, persisted, &extension_name, cap)?;

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
            // Issue #56 Gate E, finding 13: the CATALOGUE is a discovery
            // surface, so it carries the admitted capability exactly as the
            // three below do. It reaches no server, which is why Task 15 left it
            // out of Gate C's siblings — but naming a private connector and its
            // marketplace blurb to a public model is the disclosure Gate E
            // exists to prevent, arriving through a different door.
            SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME => {
                self.handle_search_available_extensions(meta.capability)
                    .await
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
    use crate::privacy::CallCapability;

    /// The capability a public caller carries with the feature ON — the pair
    /// every pre-#56 test in this module was implicitly written against.
    fn public_enforcing() -> CallCapability {
        CallCapability::for_test(ProviderTier::Public, true)
    }

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
        let err = check_enable_allowed(None, false, "ghost", public_enforcing()).unwrap_err();
        assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
        assert!(err.message.contains("not found"), "{}", err.message);
    }

    #[test]
    fn enable_of_operator_disabled_extension_is_refused_with_guidance() {
        // #42: `enabled: false` written into config.yaml must be a dependable
        // pin — the agent may not silently re-enable what the operator turned
        // off. `persisted: true` = the entry exists in the on-disk config.
        let err = check_enable_allowed(Some(entry(false)), true, "developer", public_enforcing())
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
        let config =
            check_enable_allowed(Some(entry(false)), false, "chatrecall", public_enforcing())
                .expect("allowed");
        assert_eq!(config.name(), "developer");
    }

    #[test]
    fn enable_of_config_enabled_extension_passes_the_config_through() {
        let config = check_enable_allowed(Some(entry(true)), true, "developer", public_enforcing())
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
            CallCapability::for_test(Public, true),
        )
        .unwrap_err();
        assert!(e.message.contains("marketplace"), "{}", e.message);
        assert!(e.message.contains("private"), "{}", e.message);
        check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            CallCapability::for_test(Private, true),
        )
        .expect("a private caller may enable it");
        check_enable_allowed(
            Some(entry_for("developer")),
            false,
            "developer",
            CallCapability::for_test(Public, true),
        )
        .expect("public extensions are unaffected");
    }

    /// Task 30's matrix ROW 11, asserted where the gate lives.
    ///
    /// ⚠ **Why this row is here and not in `crates/biorouter/tests/privacy_toggle.rs`
    /// with the other seventeen.** Gate F1 is an ON-THE-TOOL-CALL-PATH gate: the
    /// master toggle reaches it through the [`CallCapability`] that
    /// `Agent::dispatch_tool_call` already sampled, never through a second read
    /// of the global. So the OFF column of this row is *by construction* a
    /// capability whose `enforced` is false — and the only constructor for one
    /// is `CallCapability::for_test`, which is `#[cfg(test)] pub(crate)` and
    /// therefore invisible to an integration binary. Driving it end-to-end
    /// instead would mean writing `ucsfomopagent` into the developer's real
    /// `config.yaml`, because `get_extension_entry_by_name` reads the global
    /// config: the ON column would otherwise stop at "not found" and never
    /// reach the arm this row is about.
    ///
    /// That `CallCapability::sample` itself reads the master toggle is asserted
    /// end-to-end by the matrix's rows 3, 4/5 and 7, which flip the global and
    /// watch a real dispatch change its answer. This row is the other half: the
    /// gate honours what the sample handed it.
    #[test]
    fn the_master_toggle_silences_gate_f1() {
        use ProviderTier::Public;
        // ON — the shipped default: a public model may not spawn the clinical
        // connector's process.
        assert!(check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            CallCapability::for_test(Public, true),
        )
        .is_err());
        // OFF — DR-15: nothing is refused. The SAME public caller may enable it.
        let config = check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            CallCapability::for_test(Public, false),
        )
        .expect("with privacy tiers off, nothing is refused");
        assert_eq!(config.name(), "ucsfomopagent");
        // …and the toggle silences GATE F1 ONLY. #42's operator pin is not a
        // privacy control and must survive the master switch being off, or
        // turning privacy tiers off would quietly hand the agent the power to
        // re-enable everything the operator disabled.
        let err = check_enable_allowed(
            Some(entry(false)),
            true,
            "developer",
            CallCapability::for_test(Public, false),
        )
        .unwrap_err();
        assert!(err.message.contains("operator"), "{}", err.message);
    }

    /// **Task 43 / DR-23's gate, Step 3.1 — the Gate F1 arm.**
    ///
    /// F1 fires BEFORE the extension is spawned, off the name the model asked
    /// for. Enabling a private connector is not a call into it, it is the call
    /// that SPAWNS it — pulling its credentials out of the keychain and opening
    /// the session — so a rename that got past this gate would already have
    /// disclosed something by the time Gate C looked.
    ///
    /// Both the name asked for and the entry's own `name` are the renamed one;
    /// the `--directory` argument is the only surviving link to the install.
    #[test]
    fn a_renamed_private_extension_is_still_refused_by_gate_f1() {
        let install_dir = "/home/researcher/.config/biorouter/extensions/UCSFOMOPAgent";
        crate::privacy::provenance::insert_test_record_at(
            "f1-omop-as-installed",
            "ucsfomopagent",
            Some(install_dir),
        );
        let renamed = ExtensionEntry {
            enabled: true,
            config: ExtensionConfig::Stdio {
                name: "f1-mystuff".to_string(),
                description: "renamed by hand in config.yaml".to_string(),
                cmd: "uv".to_string(),
                args: vec![
                    "run".to_string(),
                    "--directory".to_string(),
                    install_dir.to_string(),
                    "server.py".to_string(),
                ],
                envs: crate::agents::extension::Envs::default(),
                env_keys: vec![],
                timeout: Some(300),
                bundled: None,
                available_tools: vec![],
            },
        };
        assert_eq!(
            crate::privacy::classify_extension("f1-mystuff"),
            ProviderTier::Public,
            "the fixture only discriminates if the NAME alone reads public"
        );

        let err = check_enable_allowed(Some(renamed), false, "f1-mystuff", public_enforcing())
            .expect_err("renaming the config entry must not let a public model spawn it");
        assert!(err.message.contains("private extension"), "{}", err.message);
    }

    /// The capability a session bound to `institution`'s private model carries.
    fn bound_to(institution: &str) -> CallCapability {
        CallCapability::for_test_affiliated(
            ProviderTier::Private,
            true,
            Some(crate::privacy::affiliation::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new(institution),
            )),
        )
    }

    /// **Task 48, surface 5 — extension enablement.** Bind and enablement are
    /// the same mismatch found from opposite ends; this is the end where the
    /// model is already bound and the extension arrives.
    ///
    /// ⚠ **The agent is refused rather than warned, and that is DR-26's
    /// user/agent asymmetry rather than an inconsistency with the bind
    /// surface.** A user who insists may proceed past a warning; an agent never
    /// clears one automatically — it escalates to the user or the call does not
    /// happen. `extensionmanager__manage_extensions` is the agent's enable path,
    /// so the call does not happen. The USER's enable path is
    /// `/agent/add_extension`, which warns and proceeds.
    ///
    /// Enabling is also the earliest point at which this can be caught:
    /// spawning `ucsfomopagent` pulls its `CLINICAL_RECORDS_*` secrets out of
    /// the keychain and opens a session to the UCSF CDW, which is a disclosure
    /// no later refusal takes back.
    #[test]
    fn an_agent_may_not_enable_an_extension_its_model_is_affiliation_incompatible_with() {
        let err = check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            bound_to("stanford"),
        )
        .expect_err("a Stanford-covered model may not spawn a UCSF connector");
        let expected = bound_to("stanford")
            .cross_affiliation_warning(
                "ucsfomopagent",
                &crate::privacy::resolve_extension("ucsfomopagent", None),
            )
            .expect("the fixture only discriminates if this pair really mismatches");
        assert!(err.message.contains(&expected), "{}", err.message);
    }

    /// The same call on a LOCAL private model is allowed — `Local` is DR-26's
    /// most permissive affiliation, not a peer of the institutions.
    #[test]
    fn a_local_model_may_enable_every_private_extension() {
        let config = check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            CallCapability::for_test_affiliated(
                ProviderTier::Private,
                true,
                Some(crate::privacy::affiliation::ModelAffiliation::Local),
            ),
        )
        .expect("a local model discloses nothing, so nothing needs papering");
        assert_eq!(config.name(), "ucsfomopagent");
    }

    /// **Finding 13's ordering consequence, on the enable path.**
    ///
    /// Gate F1 now runs above the not-found branch. A public caller asking to
    /// enable a private connector this machine does not have gets the tier
    /// refusal, not `not found` — so the two answers no longer tell it which
    /// private connectors are installed here. That is the same secret the
    /// catalogue fix stopped printing outright one function away.
    #[test]
    fn a_public_caller_cannot_tell_an_absent_private_extension_from_an_installed_one() {
        let absent =
            check_enable_allowed(None, false, "ucsfomopagent", public_enforcing()).unwrap_err();
        let installed = check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            public_enforcing(),
        )
        .unwrap_err();
        assert_eq!(absent.code, installed.code);
        assert_eq!(absent.message, installed.message);
        assert!(
            absent.message.contains("private extension"),
            "{}",
            absent.message
        );
        assert_ne!(
            absent.code,
            ErrorCode::RESOURCE_NOT_FOUND,
            "`not found` for a private name is an install-state oracle"
        );

        // The operator pin is the other install-state answer, and it is behind
        // the same gate now. #42's refusal must still be what a PUBLIC extension
        // gets, or the reorder swallowed it.
        let pinned =
            check_enable_allowed(Some(entry(false)), true, "developer", public_enforcing())
                .unwrap_err();
        assert!(pinned.message.contains("operator"), "{}", pinned.message);
        // …and a name that is public and absent still says so, which is what
        // keeps `enable_of_unknown_extension_is_not_found` a live assertion
        // rather than one the reorder made unreachable.
        assert_eq!(
            check_enable_allowed(None, false, "ghost", public_enforcing())
                .unwrap_err()
                .code,
            ErrorCode::RESOURCE_NOT_FOUND
        );
    }

    /// A PRIVATE caller reaches every branch the reorder moved past, unchanged.
    #[test]
    fn the_reorder_does_not_touch_a_private_callers_answers() {
        use ProviderTier::Private;
        let cap = CallCapability::for_test(Private, true);
        assert_eq!(
            check_enable_allowed(None, false, "ucsfomopagent", cap)
                .unwrap_err()
                .code,
            ErrorCode::RESOURCE_NOT_FOUND,
            "a private caller is still told when a private extension is absent"
        );
        let pinned = check_enable_allowed(Some(entry(false)), true, "developer", cap).unwrap_err();
        assert!(pinned.message.contains("operator"), "{}", pinned.message);
    }

    /// DR-15's master opt-out reaches the affiliation arm through the same
    /// capability, not through a second read of the global.
    #[test]
    fn the_master_toggle_silences_the_affiliation_arm_of_the_enable_gate() {
        let cap = CallCapability::for_test_affiliated(
            ProviderTier::Private,
            false,
            Some(crate::privacy::affiliation::ModelAffiliation::institution(
                crate::privacy::affiliation::InstitutionId::new("stanford"),
            )),
        );
        let config = check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            cap,
        )
        .expect("with privacy tiers off, nothing is refused");
        assert_eq!(config.name(), "ucsfomopagent");
    }

    // ----------------------------------------------------------------------
    // Issue #56 findings 13 and 14, WIRING. Nine guards in this campaign shipped
    // correct, tested and called by nothing, so each new gate is asserted twice:
    // once through the real `call_tool` entry point (below), and once
    // structurally against this file's production text
    // (`both_new_gates_have_production_callers`), because a behavioural test can
    // be satisfied by a helper the tool dispatch does not actually reach.
    // ----------------------------------------------------------------------

    /// A live `ExtensionManagerClient` over a real `ExtensionManager`, reached
    /// exactly as `configure_agent` builds it.
    fn a_live_tool_client() -> (
        tempfile::TempDir,
        std::sync::Arc<crate::agents::extension_manager::ExtensionManager>,
        ExtensionManagerClient,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let em = std::sync::Arc::new(
            crate::agents::extension_manager::ExtensionManager::new_without_provider(
                dir.path().to_path_buf(),
            ),
        );
        let context = PlatformExtensionContext {
            extension_manager: Some(std::sync::Arc::downgrade(&em)),
            session_manager: em.get_context().session_manager.clone(),
        };
        let client = ExtensionManagerClient::new(context).expect("the platform client builds");
        (dir, em, client)
    }

    async fn disable(client: &ExtensionManagerClient, name: &str, cap: CallCapability) -> String {
        let result = client
            .call_tool(
                MANAGE_EXTENSIONS_TOOL_NAME,
                Some(
                    serde_json::json!({ "action": "disable", "extension_name": name })
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
                McpMeta::new("session-under-test", cap),
                CancellationToken::default(),
            )
            .await
            .expect("the platform client always answers, error or not");
        // The handler reports refusals as `is_error` content rather than a
        // transport error, so the text is where the verdict is.
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// **Finding 14, through the production tool call.** `manage_extensions
    /// {disable}` returned before any privacy decision existed on the path:
    /// `cap` was already a parameter, and this branch simply never read it.
    #[tokio::test]
    async fn a_public_chat_may_not_disable_a_private_extension_through_the_tool() {
        let (_dir, em, client) = a_live_tool_client();

        let refused = disable(
            &client,
            "ucsfomopagent",
            CallCapability::for_test(ProviderTier::Public, true),
        )
        .await;
        assert!(
            refused.contains("private extension"),
            "a public chat dropped the clinical connector: {refused}"
        );

        // The SAME call on a private model succeeds, or the test is passing
        // against a tool that refuses everything.
        let allowed = disable(
            &client,
            "ucsfomopagent",
            CallCapability::for_test(ProviderTier::Private, true),
        )
        .await;
        assert!(allowed.contains("disabled successfully"), "{allowed}");

        // …and a public chat keeps every public extension it could disable
        // before, or the gate is a blanket refusal wearing a tier's clothes.
        //
        // `developer` has to be really installed for this half to mean
        // anything: the gate inherits `assert_extension_reachable`'s inverted
        // unknown-name default, so an absent name refuses whatever its tier
        // would have been, and a fixture that skipped the load would assert the
        // oracle rather than the permit.
        let target = crate::agents::extension_manager::resolve_bundled_extension("developer")
            .expect("`developer` is a bundled extension");
        em.add_extension(target.into_config("under test".to_string()))
            .await
            .expect("the developer builtin loads");
        let public_ext = disable(
            &client,
            "developer",
            CallCapability::for_test(ProviderTier::Public, true),
        )
        .await;
        assert!(public_ext.contains("disabled successfully"), "{public_ext}");
    }

    /// DR-15's master opt-out reaches the disable gate through the capability.
    #[tokio::test]
    async fn the_master_toggle_silences_the_disable_gate() {
        let (_dir, _em, client) = a_live_tool_client();
        let text = disable(
            &client,
            "ucsfomopagent",
            CallCapability::for_test(ProviderTier::Public, false),
        )
        .await;
        assert!(
            text.contains("disabled successfully"),
            "with privacy tiers off nothing is refused: {text}"
        );
    }

    /// ⚠ **A guard with no caller is the failure mode this campaign has shipped
    /// nine times.** Both gates added for findings 13 and 14 are asserted here
    /// against this file's PRODUCTION text — the tool-dispatch `match` arm and
    /// the disable branch — so a later refactor that keeps the gates compiling
    /// while routing around them fails loudly.
    ///
    /// The behavioural tests above cannot substitute: they call the gate, and a
    /// gate can be called by a test while the dispatch arm no longer reaches it.
    #[test]
    fn both_new_gates_have_production_callers() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/agents/extension_manager_extension.rs");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the audit could not read {}: {e}", path.display()));
        assert!(
            src.len() > 10_000,
            "the audit read {} bytes — a truncated read reports the same absence \
             as a removed call site",
            src.len()
        );
        // Production only. `mod tests` in this file sits below an unindented
        // `#[cfg(test)]`, and the assertions in this very function would
        // otherwise satisfy themselves.
        let production = src
            .split("\n#[cfg(test)]")
            .next()
            .expect("split always yields a first element");
        assert!(
            production.len() < src.len(),
            "the production/test split found no `#[cfg(test)]` boundary, so this \
             function is asserting against its own source"
        );
        let calls = |needle: &str| {
            production
                .lines()
                .any(|l| !l.trim_start().starts_with("//") && l.contains(needle))
        };
        assert!(
            calls("self.handle_search_available_extensions(meta.capability)"),
            "finding 13's catalogue filter has no production caller: the \
             `search_available_extensions` dispatch arm no longer threads the \
             admitted capability"
        );
        assert!(
            calls(".assert_extension_manageable(&extension_name, cap)"),
            "finding 14's disable gate has no production caller: \
             `manage_extensions {{disable}}` no longer asks it"
        );
    }
}
