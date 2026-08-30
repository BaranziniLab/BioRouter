use crate::agents::extension::ExtensionConfig;
use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait, McpMeta};
use crate::config::{
    extension_entry_is_persisted, get_all_extensions, get_extension_entry_by_name, ExtensionEntry,
};
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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
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

/// Install a marketplace extension (#117).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallExtensionParams {
    /// The BAAM registry `id` of the extension to install, e.g.
    /// `playwright-agent`. Recorded as provenance so the privacy tier is
    /// re-derived from a stable id rather than from a renameable config name.
    pub registry_id: String,
    /// Enable the extension after installing it. Defaults to true.
    #[serde(default = "default_true")]
    pub enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchMarketplaceExtensionsParams {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteExtensionPackageParams {
    pub registry_id: String,
}

fn default_true() -> bool {
    true
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
pub const INSTALL_EXTENSION_TOOL_NAME: &str = "install_extension";
pub const BROWSE_MARKETPLACE_EXTENSIONS_TOOL_NAME: &str = "browse_marketplace_extensions";
pub const SEARCH_MARKETPLACE_EXTENSIONS_TOOL_NAME: &str = "search_marketplace_extensions";
pub const DELETE_EXTENSION_PACKAGE_TOOL_NAME: &str = "delete_extension_package";
pub const MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE: &str = "extensionmanager__manage_extensions";

const MARKETPLACE_APPROVAL_TTL: Duration = Duration::from_secs(5 * 60);

pub struct ExtensionManagerClient {
    info: InitializeResult,
    #[allow(dead_code)]
    context: PlatformExtensionContext,
}

#[derive(Clone, Copy)]
enum MarketplaceMutation {
    Install,
    DeletePackage,
}

impl MarketplaceMutation {
    fn tool_name(self) -> &'static str {
        match self {
            Self::Install => INSTALL_EXTENSION_TOOL_NAME,
            Self::DeletePackage => DELETE_EXTENSION_PACKAGE_TOOL_NAME,
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::DeletePackage => "permanently delete",
        }
    }

    fn risk(self) -> crate::permission::tool_risk::ToolRisk {
        match self {
            Self::Install => crate::permission::tool_risk::ToolRisk::Medium,
            Self::DeletePackage => crate::permission::tool_risk::ToolRisk::High,
        }
    }
}

fn marketplace_descriptor_json(
    descriptor: &crate::marketplace::MarketplaceExtensionDescriptor,
) -> Value {
    let affiliation = match &descriptor.affiliation {
        crate::privacy::ExtensionAffiliation::Any => Value::String("any".to_owned()),
        crate::privacy::ExtensionAffiliation::Institutions(ids) => Value::Array(
            ids.iter()
                .map(|id| Value::String(id.as_str().to_owned()))
                .collect(),
        ),
    };
    serde_json::json!({
        "registryId": descriptor.registry_id,
        "extensionName": descriptor.extension_name,
        "name": descriptor.name,
        "organization": descriptor.organization,
        "version": descriptor.version,
        "description": descriptor.description,
        "tags": descriptor.tags,
        "downloadUrl": descriptor.download_url,
        "filename": descriptor.filename,
        "license": descriptor.license,
        "privacy": descriptor.privacy,
        "affiliation": affiliation,
    })
}

fn marketplace_approval_request(
    mutation: MarketplaceMutation,
    descriptor: &crate::marketplace::MarketplaceExtensionDescriptor,
    package: Option<&ValidatedPackageInstall>,
) -> crate::pending_user_action::UserActionRequest {
    let mut arguments = marketplace_descriptor_json(descriptor)
        .as_object()
        .expect("marketplace descriptor is an object")
        .clone();
    arguments.insert(
        "action".to_owned(),
        Value::String(mutation.verb().to_owned()),
    );
    if let Some(package) = package {
        arguments.insert(
            "installedExtensionName".to_owned(),
            Value::String(package.extension_name.clone()),
        );
        arguments.insert(
            "installDirectory".to_owned(),
            Value::String(package.install_dir.display().to_string()),
        );
    }
    crate::pending_user_action::UserActionRequest::ToolApproval(
        crate::pending_user_action::ToolApprovalRequest {
            tool_name: mutation.tool_name().to_owned(),
            arguments,
            prompt: Some(format!(
                "Allow Biorouter to {} {} {} from the trusted BAAM registry?",
                mutation.verb(),
                descriptor.name,
                descriptor.version
            )),
            risk: Some(mutation.risk()),
            preview: None,
            requires_user_proof: true,
        },
    )
}

async fn await_marketplace_approval(
    actions: &Arc<crate::pending_user_action::PendingUserActions>,
    session_id: &str,
    request: crate::pending_user_action::UserActionRequest,
    ttl: Duration,
    cancel: Option<&CancellationToken>,
) -> Result<(), ExtensionManagerToolError> {
    if session_id.is_empty() {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Marketplace changes require a visible chat session for user approval"
                .to_owned(),
        });
    }
    let parked = actions.park(Some(session_id), None, request);
    match parked.wait(ttl, cancel).await {
        crate::pending_user_action::UserActionOutcome::Approved { .. } => Ok(()),
        outcome => Err(ExtensionManagerToolError::OperationFailed {
            message: format!(
                "The marketplace change was not made because the approval request {}.",
                outcome.refusal_detail()
            ),
        }),
    }
}

async fn trusted_marketplace_extension(
    registry_id: &str,
    caller: crate::privacy::ProviderTier,
) -> Result<crate::marketplace::MarketplaceExtensionDescriptor, ExtensionManagerToolError> {
    let loaded = crate::marketplace::load_marketplace_catalog()
        .await
        .map_err(|error| ExtensionManagerToolError::OperationFailed {
            message: error.to_string(),
        })?;
    resolve_marketplace_extension(&loaded.catalog, registry_id, caller)
}

fn resolve_marketplace_extension(
    catalog: &crate::marketplace::MarketplaceCatalog,
    registry_id: &str,
    caller: crate::privacy::ProviderTier,
) -> Result<crate::marketplace::MarketplaceExtensionDescriptor, ExtensionManagerToolError> {
    catalog
        .resolve_extension_for_install(registry_id, caller)
        .cloned()
        .map_err(|error| ExtensionManagerToolError::OperationFailed {
            message: error.to_string(),
        })
}

fn ensure_descriptor_unchanged(
    approved: &crate::marketplace::MarketplaceExtensionDescriptor,
    current: &crate::marketplace::MarketplaceExtensionDescriptor,
) -> Result<(), ExtensionManagerToolError> {
    if approved == current {
        Ok(())
    } else {
        Err(ExtensionManagerToolError::OperationFailed {
            message: format!(
                "Marketplace entry `{}` changed after approval; nothing was changed. Review and approve the current entry instead.",
                approved.registry_id
            ),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ValidatedPackageInstall {
    provenance: crate::privacy::provenance::MarketplaceInstallProvenance,
    extension_name: String,
    config: ExtensionConfig,
    install_dir: PathBuf,
    extensions_root: PathBuf,
}

fn validated_marketplace_package(
    descriptor: &crate::marketplace::MarketplaceExtensionDescriptor,
    candidates: Vec<crate::privacy::provenance::MarketplaceInstallProvenance>,
) -> Result<ValidatedPackageInstall, ExtensionManagerToolError> {
    validated_marketplace_package_at(
        descriptor,
        candidates,
        crate::extension_install::brxt::extensions_root(),
        get_all_extensions(),
    )
}

fn validated_marketplace_package_at(
    descriptor: &crate::marketplace::MarketplaceExtensionDescriptor,
    candidates: Vec<crate::privacy::provenance::MarketplaceInstallProvenance>,
    root: PathBuf,
    configured: Vec<ExtensionEntry>,
) -> Result<ValidatedPackageInstall, ExtensionManagerToolError> {
    let [provenance] = candidates.as_slice() else {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: match candidates.len() {
                0 => format!(
                    "No validated marketplace package is installed for `{}`",
                    descriptor.registry_id
                ),
                count => format!(
                    "Found {count} installed packages for `{}`; refusing an ambiguous deletion",
                    descriptor.registry_id
                ),
            },
        });
    };
    if provenance.registry_id != descriptor.registry_id {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Marketplace provenance no longer matches the selected registry entry"
                .to_owned(),
        });
    }

    let source = url::Url::parse(&provenance.source_url).map_err(|_| {
        ExtensionManagerToolError::OperationFailed {
            message: "Marketplace install provenance has an invalid source URL".to_owned(),
        }
    })?;
    if source.scheme() != "https"
        || !source.username().is_empty()
        || source.password().is_some()
        || source.port().is_some()
        || source.query().is_some()
        || source.fragment().is_some()
        || source.host_str() != descriptor.download_url.host_str()
        || !source.path().ends_with(".brxt")
    {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Marketplace install provenance is not a trusted .brxt source".to_owned(),
        });
    }

    let install_dir = PathBuf::from(&provenance.install_dir);
    if !install_dir.is_absolute() {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Marketplace install provenance does not name an absolute package directory"
                .to_owned(),
        });
    }
    let canonical_root = std::fs::canonicalize(&root).map_err(|error| {
        ExtensionManagerToolError::OperationFailed {
            message: format!("Could not validate the extensions directory: {error}"),
        }
    })?;
    let canonical_install = std::fs::canonicalize(&install_dir).map_err(|error| {
        ExtensionManagerToolError::OperationFailed {
            message: format!("Could not validate the installed package: {error}"),
        }
    })?;
    if !canonical_install.is_dir()
        || canonical_install.parent() != Some(canonical_root.as_path())
        || canonical_install
            .file_name()
            .and_then(|name| name.to_str())
            .map(crate::config::extensions::name_to_key)
            .as_deref()
            != Some(provenance.config_key.as_str())
    {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Marketplace package is not one direct child of the extensions directory"
                .to_owned(),
        });
    }

    let entry = configured
        .into_iter()
        .find(|entry| entry.config.key() == provenance.config_key)
        .ok_or_else(|| ExtensionManagerToolError::OperationFailed {
            message: "Marketplace package no longer has a matching extension configuration"
                .to_owned(),
        })?;
    let references_recorded_dir = match &entry.config {
        ExtensionConfig::Stdio { args, .. } => args
            .iter()
            .any(|argument| argument == &provenance.install_dir),
        _ => false,
    };
    if !references_recorded_dir {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Marketplace package configuration does not reference its recorded directory"
                .to_owned(),
        });
    }

    Ok(ValidatedPackageInstall {
        provenance: provenance.clone(),
        extension_name: entry.config.name(),
        config: entry.config,
        install_dir: canonical_install,
        extensions_root: canonical_root,
    })
}

async fn detach_marketplace_package_from_session(
    context: &PlatformExtensionContext,
    package: &ValidatedPackageInstall,
) -> Result<
    (
        Arc<crate::agents::extension_manager::ExtensionManager>,
        bool,
    ),
    ExtensionManagerToolError,
> {
    let manager = context
        .extension_manager
        .as_ref()
        .and_then(|weak| weak.upgrade())
        .ok_or(ExtensionManagerToolError::ManagerUnavailable)?;
    let was_attached = manager
        .is_extension_enabled(&package.provenance.config_key)
        .await;
    if was_attached {
        manager
            .remove_extension(&package.extension_name)
            .await
            .map_err(|error| ExtensionManagerToolError::OperationFailed {
                message: format!("Could not detach the package from this chat: {error}"),
            })?;
    }
    Ok((manager, was_attached))
}

async fn restore_staged_marketplace_package(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    package: &ValidatedPackageInstall,
    quarantine: &std::path::Path,
    was_attached: bool,
) -> bool {
    let restored = std::fs::rename(quarantine, &package.install_dir).is_ok();
    if was_attached {
        let _ = manager.add_extension(package.config.clone()).await;
    }
    restored
}

fn provenance_removal_error(
    result: std::io::Result<bool>,
    restored: bool,
) -> ExtensionManagerToolError {
    let message = match result {
        Ok(false) if restored => {
            "Marketplace provenance changed before deletion; the staged package was restored"
                .to_owned()
        }
        Ok(false) => "Marketplace provenance changed before deletion, and the staged package could not be restored"
            .to_owned(),
        Err(error) if restored => format!(
            "Could not update marketplace provenance; the staged package was restored: {error}"
        ),
        Err(error) => format!(
            "Could not update marketplace provenance, and the staged package could not be restored: {error}"
        ),
        Ok(true) => unreachable!("successful provenance removal has no error"),
    };
    ExtensionManagerToolError::OperationFailed { message }
}

async fn delete_staged_marketplace_package(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    package: &ValidatedPackageInstall,
    was_attached: bool,
) -> Result<(), ExtensionManagerToolError> {
    let quarantine = package
        .extensions_root
        .join(format!(".delete-{}", uuid::Uuid::new_v4()));
    if let Err(error) = std::fs::rename(&package.install_dir, &quarantine) {
        if was_attached {
            let _ = manager.add_extension(package.config.clone()).await;
        }
        return Err(ExtensionManagerToolError::OperationFailed {
            message: format!("Could not stage the package for deletion: {error}"),
        });
    }

    let provenance_result =
        crate::privacy::provenance::remove_marketplace_install_provenance(&package.provenance);
    if !matches!(&provenance_result, Ok(true)) {
        let restored =
            restore_staged_marketplace_package(manager, package, &quarantine, was_attached).await;
        return Err(provenance_removal_error(provenance_result, restored));
    }

    crate::config::extensions::remove_extension(&package.provenance.config_key);
    std::fs::remove_dir_all(&quarantine).map_err(|error| {
        ExtensionManagerToolError::OperationFailed {
            message: format!(
                "The extension was detached and unregistered, but its quarantined package could not be removed: {error}"
            ),
        }
    })
}

/// The `manage_extensions` enable door: ask the shared enable gate, then resolve
/// the config to load.
///
/// ⚠ **Every refusal this function can give comes from
/// [`refusal::extension_enable_refusal`], which is the WHOLE of Gate F1 plus
/// #42's operator pin, in one clause order.** It used to hand-write the tier arm
/// here — `class.tier.is_private() && caller == ProviderTier::Public`, with its
/// own sentence — while the workspace's two enable doors expressed the same rule
/// through `refusal::privacy_refusal` and asked the operator pin *first*. Two
/// spellings, two orders; the workspace order reopened at `workspace_open
/// {new:{extensions}}` the exact install-state oracle finding 13 had just closed
/// here. There is now one function, called from all three doors. If a fourth
/// enable door appears, give it this one — do not write a fifth arm here.
///
/// `persisted` is #42's provenance signal (`extension_entry_is_persisted`):
/// `get_extension_entry_by_name` reads the post-injection map, where an
/// absent platform extension is injected with its default — so a default-off
/// one (e.g. `chatrecall`) shows up as `enabled: false` without any operator
/// ever writing that. Only an entry actually present in the on-disk config
/// counts as operator-disabled; injected defaults stay agent-enableable. It is a
/// parameter rather than a lookup so this whole path stays pure: testable with no
/// global config, no registry and no live extension.
///
/// `cap` is handed on WHOLE — both axes and DR-15's master opt-out off one
/// sample — rather than collapsed at the call site, so the toggle half of the
/// decision has a subject a test can hold and Task 30's structural inventory can
/// name a single `(file, fn)` pair for this row.
///
/// ⚠ **The not-found branch is BELOW the gate, and that is finding 13.**
/// "Extension 'ucsfomopagent' not found" tells a public model what this machine
/// has installed — the same secret the sibling finding stopped the catalogue
/// printing outright. Asking the gate first collapses every private name a public
/// caller can ask about onto one refusal, whether it is installed,
/// configured-off, or absent entirely. Nothing is lost in the other direction: a
/// PRIVATE caller reaches this branch exactly as before, and so does a public
/// caller asking about a public extension — which is every case it was written
/// for.
///
/// [`refusal::extension_enable_refusal`]: crate::privacy::refusal::extension_enable_refusal
fn check_enable_allowed(
    entry: Option<ExtensionEntry>,
    persisted: bool,
    extension_name: &str,
    cap: crate::privacy::CallCapability,
) -> Result<ExtensionConfig, ErrorData> {
    if crate::agents::extension_manager::resolve_bundled_extension(extension_name).is_some() {
        return Err(crate::agents::extension_manager::capability_management_error(extension_name));
    }
    if let Some(refusal) = entry.as_ref().and_then(|entry| {
        crate::agents::extension_manager::capability_management_refusal(&entry.config)
    }) {
        return Err(refusal);
    }
    if let Some(err) = crate::privacy::refusal::extension_enable_refusal(
        cap,
        extension_name,
        entry.as_ref(),
        persisted,
    ) {
        return Err(err);
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
                - browse_marketplace_extensions: Browse trusted BAAM entries visible to this model
                - search_marketplace_extensions: Search trusted BAAM entries
                - install_extension: Install an exact trusted registry id after user approval
                - delete_extension_package: Permanently delete a validated marketplace package after user approval
                - list_resources/read_resource: Resource tools, when they are advertised for the current session

                When you lack the tools needed to complete a task, use search_available_extensions first
                to discover what extensions can help.

                Use manage_extensions to enable or disable third-party extensions by name.
                Built-in and platform capabilities are managed separately and this tool refuses them.
                Use browse/search to obtain an exact registry id, then install_extension when the
                extension is not installed at all. Never provide a download URL or install
                one by running shell commands, and NEVER ask the user to type an API key,
                password or token into the chat — install_extension opens Biorouter's own
                approval and credential dialogs, and a credential in a chat message cannot configure anything.
                delete_extension_package removes package files but deliberately preserves shared credentials.
                Use list_resources and read_resource only when they appear in the current tool catalog;
                they are omitted when no loaded extension supports resources.
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

    async fn handle_browse_marketplace_extensions(
        &self,
        cap: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        self.marketplace_extensions(None, cap).await
    }

    async fn handle_search_marketplace_extensions(
        &self,
        arguments: Option<JsonObject>,
        cap: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        let arguments = arguments.ok_or(ExtensionManagerToolError::MissingParameter {
            param_name: "arguments".to_owned(),
        })?;
        let params: SearchMarketplaceExtensionsParams =
            serde_json::from_value(Value::Object(arguments))?;
        self.marketplace_extensions(Some(&params.query), cap).await
    }

    async fn marketplace_extensions(
        &self,
        query: Option<&str>,
        cap: crate::privacy::CallCapability,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        let loaded = crate::marketplace::load_marketplace_catalog()
            .await
            .map_err(|error| ExtensionManagerToolError::OperationFailed {
                message: error.to_string(),
            })?;
        let entries = match query {
            Some(query) => loaded.catalog.search_extensions(cap.tier(), query),
            None => loaded.catalog.browse_extensions(cap.tier()),
        };
        let source = match loaded.source {
            crate::marketplace::MarketplaceCatalogSource::Live => "live",
            crate::marketplace::MarketplaceCatalogSource::LastGood => "lastGood",
            crate::marketplace::MarketplaceCatalogSource::Embedded => "embedded",
        };
        let body = serde_json::json!({
            "source": source,
            "stale": loaded.is_stale(),
            "extensions": entries
                .into_iter()
                .map(marketplace_descriptor_json)
                .collect::<Vec<_>>(),
        });
        Ok(vec![Content::text(
            serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_owned()),
        )])
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

    /// Install a marketplace extension, asking the *user* for any credentials.
    ///
    /// ⚠ **This tool exists so that the model never has to ask for a secret.**
    /// Before it, an agent told to "install the Playwright extension" had three
    /// options and all of them were wrong: tell the user to paste a token into
    /// the chat, run a shell command with the token in `ps` and the shell
    /// history, or install without it and report success for something that
    /// cannot authenticate. The install parks on a credential card the user
    /// answers in Biorouter's own dialog, and this tool learns only which key
    /// NAMES were configured.
    ///
    /// The report it returns is the same [`InstallReport`] the CLI prints, and
    /// its shape has nowhere a value can sit — deliberately, because this one is
    /// serialised straight into a tool result.
    ///
    /// [`InstallReport`]: crate::extension_install::InstallReport
    async fn handle_install_extension(
        &self,
        arguments: Option<JsonObject>,
        session_id: String,
        cap: crate::privacy::CallCapability,
        cancel: CancellationToken,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        use crate::extension_install::{
            CredentialPolicy, ExtensionInstallTransaction, InstallSource, InstallState,
            DEFAULT_CREDENTIAL_TTL,
        };

        let arguments = arguments.ok_or(ExtensionManagerToolError::MissingParameter {
            param_name: "arguments".to_string(),
        })?;
        let params: InstallExtensionParams =
            serde_json::from_value(serde_json::Value::Object(arguments))?;

        // This preflight deliberately ignores the diagnostic master switch.
        // Public model capability never authorizes a private marketplace entry.
        let approved = trusted_marketplace_extension(&params.registry_id, cap.tier()).await?;
        let approval = marketplace_approval_request(MarketplaceMutation::Install, &approved, None);
        await_marketplace_approval(
            crate::pending_user_action::PendingUserActions::global(),
            &session_id,
            approval,
            MARKETPLACE_APPROVAL_TTL,
            Some(&cancel),
        )
        .await?;

        // Approval binds the exact daemon-owned descriptor, not merely its id.
        // A registry update between the card and the click gets a new card.
        let current = trusted_marketplace_extension(&params.registry_id, cap.tier()).await?;
        ensure_descriptor_unchanged(&approved, &current)?;

        let attach_refusal = crate::privacy::refusal::extension_enable_refusal(
            cap,
            &current.extension_name,
            None,
            false,
        )
        .map(|error| error.message.to_string());
        let mut transaction = ExtensionInstallTransaction::new(InstallSource::Marketplace {
            registry_id: current.registry_id.clone(),
            url: current.download_url.as_str().to_owned(),
        })
        .enabled(params.enable);
        if attach_refusal.is_none() {
            if let Some(weak) = &self.context.extension_manager {
                transaction = transaction.attach_to(weak.clone());
            }
        }

        let report = transaction
            .run(
                CredentialPolicy::Ask {
                    session_id: Some(session_id),
                    owner: None,
                    ttl: DEFAULT_CREDENTIAL_TTL,
                },
                Some(&cancel),
            )
            .await;

        let json = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string());
        match &report.state {
            InstallState::NeedsCredentials { .. } | InstallState::Cancelled => {
                // Not an error: a person declined or could not be asked. Say so
                // plainly, and — critically — do NOT suggest asking them for the
                // value in chat.
                Ok(vec![Content::text(format!(
                    "{json}\n\nThe extension was not registered. Do not ask the user to type any \
                     credential into this chat: a value in a chat message cannot configure \
                     anything and would expose it. Tell them the Biorouter dialog is waiting, or \
                     that they can run `biorouter extension configure <name>` at a terminal."
                ))])
            }
            InstallState::Failed { reason } => Err(ExtensionManagerToolError::OperationFailed {
                message: reason.clone(),
            }),
            _ => Ok(vec![Content::text(match attach_refusal {
                Some(refusal) if params.enable => format!(
                    "{json}\n\nThe package is installed but was not attached to this chat: {refusal}"
                ),
                _ => json,
            })]),
        }
    }

    async fn handle_delete_extension_package(
        &self,
        arguments: Option<JsonObject>,
        session_id: String,
        cap: crate::privacy::CallCapability,
        cancel: CancellationToken,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        let arguments = arguments.ok_or(ExtensionManagerToolError::MissingParameter {
            param_name: "arguments".to_owned(),
        })?;
        let params: DeleteExtensionPackageParams =
            serde_json::from_value(Value::Object(arguments))?;

        let approved_descriptor =
            trusted_marketplace_extension(&params.registry_id, cap.tier()).await?;
        let approved_package = validated_marketplace_package(
            &approved_descriptor,
            crate::privacy::provenance::marketplace_installs_for_registry_id(&params.registry_id),
        )?;
        let approval = marketplace_approval_request(
            MarketplaceMutation::DeletePackage,
            &approved_descriptor,
            Some(&approved_package),
        );
        await_marketplace_approval(
            crate::pending_user_action::PendingUserActions::global(),
            &session_id,
            approval,
            MARKETPLACE_APPROVAL_TTL,
            Some(&cancel),
        )
        .await?;

        let current_descriptor =
            trusted_marketplace_extension(&params.registry_id, cap.tier()).await?;
        ensure_descriptor_unchanged(&approved_descriptor, &current_descriptor)?;
        let current_package = validated_marketplace_package(
            &current_descriptor,
            crate::privacy::provenance::marketplace_installs_for_registry_id(&params.registry_id),
        )?;
        if current_package != approved_package {
            return Err(ExtensionManagerToolError::OperationFailed {
                message: "The installed package changed after approval; nothing was deleted"
                    .to_owned(),
            });
        }

        let (manager, was_attached) =
            detach_marketplace_package_from_session(&self.context, &current_package).await?;
        delete_staged_marketplace_package(&manager, &current_package, was_attached).await?;

        let report = serde_json::json!({
            "state": "deleted",
            "registryId": current_descriptor.registry_id,
            "extensionName": current_package.extension_name,
            "detachedFromCurrentSession": was_attached,
            "credentialsPreserved": true,
        });
        Ok(vec![Content::text(
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_owned()),
        )])
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
                    vec![Content::text(
                        serde_json::json!({
                            "extensionName": extension_name,
                            "sessionState": "detached",
                            "persistentConfigurationChanged": false,
                        })
                        .to_string(),
                    )]
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
                vec![Content::text(
                    serde_json::json!({
                        "extensionName": extension_name,
                        "sessionState": "attached",
                        "persistentConfigurationChanged": false,
                    })
                    .to_string(),
                )]
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

        tools.extend([
            Tool::new(
                BROWSE_MARKETPLACE_EXTENSIONS_TOOL_NAME.to_owned(),
                "Browse BAAM marketplace extensions visible to this model. Returns trusted registry ids and install metadata; private entries are hidden from public models."
                    .to_owned(),
                Arc::new(
                    serde_json::json!({
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {}
                    })
                    .as_object()
                    .expect("Schema must be an object")
                    .clone(),
                ),
            )
            .annotate(ToolAnnotations {
                title: Some("Browse BAAM marketplace".to_owned()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(true),
            }),
            Tool::new(
                SEARCH_MARKETPLACE_EXTENSIONS_TOOL_NAME.to_owned(),
                "Search trusted BAAM marketplace entries by id, name, organization, description, or tag. Private entries are hidden from public models."
                    .to_owned(),
                Arc::new(
                    serde_json::to_value(schema_for!(SearchMarketplaceExtensionsParams))
                        .expect("Failed to serialize schema")
                        .as_object()
                        .expect("Schema must be an object")
                        .clone(),
                ),
            )
            .annotate(ToolAnnotations {
                title: Some("Search BAAM marketplace".to_owned()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(true),
            }),
            Tool::new(
                INSTALL_EXTENSION_TOOL_NAME.to_owned(),
                "Install a BAAM extension by its exact trusted registry id. Biorouter resolves the download URL itself and requires the user's proof-backed approval before any package installation."
                    .to_owned(),
                Arc::new(
                    serde_json::to_value(schema_for!(InstallExtensionParams))
                        .expect("Failed to serialize schema")
                        .as_object()
                        .expect("Schema must be an object")
                        .clone(),
                ),
            )
            .annotate(ToolAnnotations {
                title: Some("Install a BAAM extension".to_owned()),
                read_only_hint: Some(false),
                destructive_hint: Some(false),
                idempotent_hint: Some(false),
                open_world_hint: Some(true),
            }),
            Tool::new(
                DELETE_EXTENSION_PACKAGE_TOOL_NAME.to_owned(),
                "Permanently delete one validated marketplace-installed .brxt package by exact registry id. Requires the user's proof-backed approval and preserves credentials."
                    .to_owned(),
                Arc::new(
                    serde_json::to_value(schema_for!(DeleteExtensionPackageParams))
                        .expect("Failed to serialize schema")
                        .as_object()
                        .expect("Schema must be an object")
                        .clone(),
                ),
            )
            .annotate(ToolAnnotations {
                title: Some("Delete an installed BAAM package".to_owned()),
                read_only_hint: Some(false),
                destructive_hint: Some(true),
                idempotent_hint: Some(false),
                open_world_hint: Some(false),
            }),
        ]);

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
            BROWSE_MARKETPLACE_EXTENSIONS_TOOL_NAME => {
                self.handle_browse_marketplace_extensions(meta.capability)
                    .await
            }
            SEARCH_MARKETPLACE_EXTENSIONS_TOOL_NAME => {
                self.handle_search_marketplace_extensions(arguments, meta.capability)
                    .await
            }
            // Issue #56 Gate F1: enabling an extension SPAWNS its server, so it
            // carries the admitted capability for the same reason the two reads
            // below do.
            MANAGE_EXTENSIONS_TOOL_NAME => {
                self.handle_manage_extensions(arguments, meta.capability)
                    .await
            }
            // Issue #117. Carries the session id because the credential card is
            // published to that session's queue — a card with no session is a
            // dialog nobody's chat renders.
            INSTALL_EXTENSION_TOOL_NAME => {
                self.handle_install_extension(
                    arguments,
                    meta.session_id.clone(),
                    meta.capability,
                    _cancellation_token.clone(),
                )
                .await
            }
            DELETE_EXTENSION_PACKAGE_TOOL_NAME => {
                self.handle_delete_extension_package(
                    arguments,
                    meta.session_id.clone(),
                    meta.capability,
                    _cancellation_token.clone(),
                )
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
    // The production half of this file no longer names a tier: every arm of the
    // enable decision moved into `refusal::extension_enable_refusal`, so the
    // import lives with the tests that still state one.
    use crate::privacy::ProviderTier;

    /// The capability a public caller carries with the feature ON — the pair
    /// every pre-#56 test in this module was implicitly written against.
    fn public_enforcing() -> CallCapability {
        CallCapability::for_test(ProviderTier::Public, true)
    }

    fn entry(enabled: bool) -> ExtensionEntry {
        ExtensionEntry {
            enabled,
            config: ExtensionConfig::stdio(
                "publicfixture",
                "fixture-command",
                "shell and file tools",
                30_u64,
            ),
        }
    }

    fn marketplace_catalog() -> crate::marketplace::MarketplaceCatalog {
        crate::marketplace::MarketplaceCatalog::from_bytes(
            &serde_json::to_vec(&serde_json::json!({
                "version": 2,
                "source": "https://biorouter.ucsf.edu/baam",
                "institutions": { "ucsf": "UCSF" },
                "extensions": [
                    {
                        "id": "manager-public-fixture",
                        "name": "Manager Public Fixture",
                        "organization": "Example",
                        "version": "v1.0.0",
                        "description": "Public fixture",
                        "tags": ["fixture"],
                        "github": "https://github.com/example/manager-public-fixture",
                        "download": "https://github.com/example/manager-public-fixture/releases/download/v1.0.0/manager-public-fixture.brxt",
                        "filename": "manager-public-fixture.brxt",
                        "license": "Apache-2.0",
                        "privacy": "public"
                    },
                    {
                        "id": "manager-private-fixture",
                        "name": "Manager Private Fixture",
                        "organization": "Example",
                        "version": "v1.0.0",
                        "description": "Private fixture",
                        "tags": ["fixture"],
                        "github": "https://github.com/example/manager-private-fixture",
                        "download": "https://github.com/example/manager-private-fixture/releases/download/v1.0.0/manager-private-fixture.brxt",
                        "filename": "manager-private-fixture.brxt",
                        "license": "Apache-2.0",
                        "privacy": "private",
                        "extension_name": "manager-private-fixture",
                        "affiliation": ["ucsf"]
                    }
                ],
                "skills": []
            }))
            .unwrap(),
        )
        .unwrap()
    }

    fn marketplace_descriptor() -> crate::marketplace::MarketplaceExtensionDescriptor {
        resolve_marketplace_extension(
            &marketplace_catalog(),
            "manager-public-fixture",
            ProviderTier::Private,
        )
        .unwrap()
    }

    #[test]
    fn install_schema_accepts_only_a_registry_id_and_enable_flag() {
        let schema = serde_json::to_value(schema_for!(InstallExtensionParams)).unwrap();
        let properties = schema
            .pointer("/properties")
            .and_then(Value::as_object)
            .expect("install schema properties");
        assert!(properties.contains_key("registry_id"));
        assert!(properties.contains_key("enable"));
        assert!(!properties.contains_key("url"));
        assert!(
            serde_json::from_value::<InstallExtensionParams>(serde_json::json!({
                "registry_id": "manager-public-fixture",
                "url": "https://attacker.invalid/payload.brxt"
            }))
            .is_err()
        );
    }

    #[test]
    fn public_to_private_install_preflight_is_absolute_before_mutation() {
        let catalog = marketplace_catalog();
        let public_with_master_switch_off = CallCapability::for_test(ProviderTier::Public, false);
        let denied = resolve_marketplace_extension(
            &catalog,
            "manager-private-fixture",
            public_with_master_switch_off.tier(),
        )
        .expect_err("the master switch must not authorize marketplace installation");
        assert!(denied.to_string().contains("unavailable"));

        assert!(resolve_marketplace_extension(
            &catalog,
            "manager-private-fixture",
            ProviderTier::Private,
        )
        .is_ok());
        assert!(resolve_marketplace_extension(
            &catalog,
            "manager-public-fixture",
            ProviderTier::Public,
        )
        .is_ok());
    }

    #[test]
    fn marketplace_mutations_construct_proof_required_approval_cards() {
        for mutation in [
            MarketplaceMutation::Install,
            MarketplaceMutation::DeletePackage,
        ] {
            let request = marketplace_approval_request(mutation, &marketplace_descriptor(), None);
            let crate::pending_user_action::UserActionRequest::ToolApproval(request) = request
            else {
                panic!("marketplace mutation did not create an approval")
            };
            assert!(request.requires_user_proof);
            assert_eq!(request.tool_name, mutation.tool_name());
            assert_eq!(request.risk, Some(mutation.risk()));
            assert!(request.arguments.contains_key("registryId"));
            assert!(request.arguments.contains_key("downloadUrl"));
        }
    }

    #[tokio::test]
    async fn marketplace_approval_cancellation_and_timeout_stop_the_mutation() {
        let descriptor = marketplace_descriptor();

        let cancelled_actions = Arc::new(crate::pending_user_action::PendingUserActions::default());
        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = await_marketplace_approval(
            &cancelled_actions,
            "manager-cancel-fixture",
            marketplace_approval_request(MarketplaceMutation::Install, &descriptor, None),
            Duration::from_secs(5),
            Some(&cancel),
        )
        .await
        .unwrap_err();
        assert!(cancelled.to_string().contains("cancelled"));

        let timed_out_actions = Arc::new(crate::pending_user_action::PendingUserActions::default());
        let timed_out = await_marketplace_approval(
            &timed_out_actions,
            "manager-timeout-fixture",
            marketplace_approval_request(MarketplaceMutation::Install, &descriptor, None),
            Duration::ZERO,
            None,
        )
        .await
        .unwrap_err();
        assert!(timed_out.to_string().contains("expired"));
    }

    #[test]
    fn descriptor_changes_invalidate_an_existing_approval() {
        let approved = marketplace_descriptor();
        let mut changed = approved.clone();
        changed.version = "v2.0.0".to_owned();
        changed.download_url =
            url::Url::parse("https://github.com/example/v2/manager-public-fixture.brxt").unwrap();
        assert!(ensure_descriptor_unchanged(&approved, &approved).is_ok());
        assert!(ensure_descriptor_unchanged(&approved, &changed).is_err());
    }

    #[tokio::test]
    async fn marketplace_install_and_delete_are_advertised_tools() {
        let (_dir, _manager, client) = a_live_tool_client();
        let names = client
            .get_tools()
            .await
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        for expected in [
            BROWSE_MARKETPLACE_EXTENSIONS_TOOL_NAME,
            SEARCH_MARKETPLACE_EXTENSIONS_TOOL_NAME,
            INSTALL_EXTENSION_TOOL_NAME,
            DELETE_EXTENSION_PACKAGE_TOOL_NAME,
        ] {
            assert!(names.iter().any(|name| name == expected), "{expected}");
        }
        for resource_tool in [LIST_RESOURCES_TOOL_NAME, READ_RESOURCE_TOOL_NAME] {
            assert!(
                !names.iter().any(|name| name == resource_tool),
                "{resource_tool} must not be advertised without resource support"
            );
        }
        let instructions = client
            .get_info()
            .and_then(|info| info.instructions.as_deref())
            .expect("Extension Manager instructions");
        assert!(instructions.contains("only when they appear in the current tool catalog"));
        assert!(instructions.contains("omitted when no loaded extension supports resources"));
    }

    fn package_entry(name: &str, install_dir: &std::path::Path) -> ExtensionEntry {
        ExtensionEntry {
            enabled: true,
            config: ExtensionConfig::Stdio {
                name: name.to_owned(),
                description: "marketplace package fixture".to_owned(),
                cmd: "uv".to_owned(),
                args: vec![
                    "run".to_owned(),
                    "--directory".to_owned(),
                    install_dir.display().to_string(),
                    "server.py".to_owned(),
                ],
                envs: crate::agents::extension::Envs::default(),
                env_keys: vec!["SHARED_CREDENTIAL".to_owned()],
                timeout: Some(300),
                bundled: None,
                available_tools: Vec::new(),
            },
        }
    }

    fn package_provenance(
        registry_id: &str,
        config_key: &str,
        install_dir: &std::path::Path,
    ) -> crate::privacy::provenance::MarketplaceInstallProvenance {
        crate::privacy::provenance::MarketplaceInstallProvenance {
            config_key: config_key.to_owned(),
            registry_id: registry_id.to_owned(),
            install_dir: install_dir.display().to_string(),
            source_url: format!(
                "https://github.com/example/{registry_id}/releases/download/v1.0.0/{registry_id}.brxt"
            ),
        }
    }

    #[test]
    fn package_deletion_accepts_only_one_direct_validated_marketplace_child() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("extensions");
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let install_dir = root.join("ManagerPublicFixture");
        std::fs::create_dir(&install_dir).unwrap();
        let descriptor = marketplace_descriptor();
        let provenance = package_provenance(
            &descriptor.registry_id,
            "managerpublicfixture",
            &install_dir,
        );

        let validated = validated_marketplace_package_at(
            &descriptor,
            vec![provenance.clone()],
            root.clone(),
            vec![package_entry("ManagerPublicFixture", &install_dir)],
        )
        .unwrap();
        assert_eq!(validated.install_dir, install_dir);
        assert_eq!(validated.provenance, provenance);
        assert!(matches!(
            &validated.config,
            ExtensionConfig::Stdio { env_keys, .. }
                if env_keys.contains(&"SHARED_CREDENTIAL".to_owned())
        ));

        let ambiguous = validated_marketplace_package_at(
            &descriptor,
            vec![provenance.clone(), provenance],
            root,
            vec![package_entry("ManagerPublicFixture", &install_dir)],
        )
        .unwrap_err();
        assert!(ambiguous.to_string().contains("ambiguous"));
    }

    #[test]
    fn package_deletion_rejects_paths_outside_the_extensions_root() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("extensions");
        let outside = temp.path().join("other-package");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let outside = std::fs::canonicalize(outside).unwrap();
        let descriptor = marketplace_descriptor();
        let error = validated_marketplace_package_at(
            &descriptor,
            vec![package_provenance(
                &descriptor.registry_id,
                "other-package",
                &outside,
            )],
            root,
            vec![package_entry("other-package", &outside)],
        )
        .unwrap_err();
        assert!(error.to_string().contains("direct child"));
        assert!(outside.is_dir(), "validation must not mutate the target");
    }

    #[test]
    fn package_deletion_path_never_revokes_or_removes_credentials() {
        let source = include_str!("extension_manager_extension.rs");
        let delete_body = source
            .split("async fn handle_delete_extension_package")
            .nth(1)
            .and_then(|tail| tail.split("async fn manage_extensions_impl").next())
            .expect("delete handler boundaries");
        for forbidden in [
            "revoke(",
            "remove_secret",
            "delete_secret",
            "env_keys.clear",
        ] {
            assert!(
                !delete_body.contains(forbidden),
                "package deletion must preserve possibly shared credentials: {forbidden}"
            );
        }
        assert!(delete_body.contains("credentialsPreserved"));
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
        let err = check_enable_allowed(
            Some(entry(false)),
            true,
            "publicfixture",
            public_enforcing(),
        )
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
        // An injected non-capability entry can carry `enabled: false` without
        // an operator writing it. That is not an operator pin.
        let config = check_enable_allowed(
            Some(entry(false)),
            false,
            "publicfixture",
            public_enforcing(),
        )
        .expect("allowed");
        assert_eq!(config.name(), "publicfixture");
    }

    #[test]
    fn enable_of_config_enabled_extension_passes_the_config_through() {
        let config =
            check_enable_allowed(Some(entry(true)), true, "publicfixture", public_enforcing())
                .expect("allowed");
        assert_eq!(config.name(), "publicfixture");
    }

    /// The same entry shape as [`entry`], but under an arbitrary name and
    /// always enabled — the tier is resolved from the NAME the model asked
    /// for, never from the config record, so this fixture only has to carry a
    /// name that is not itself a refusal for some other reason.
    fn entry_for(name: &str) -> ExtensionEntry {
        ExtensionEntry {
            enabled: true,
            config: ExtensionConfig::stdio(name, "fixture-command", "fixture", 30_u64),
        }
    }

    fn capability_entries() -> [ExtensionEntry; 2] {
        [
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::Builtin {
                    name: "developer".to_owned(),
                    display_name: Some("Developer".to_owned()),
                    description: "built-in fixture".to_owned(),
                    timeout: Some(30),
                    bundled: Some(true),
                    available_tools: Vec::new(),
                },
            },
            ExtensionEntry {
                enabled: true,
                config: ExtensionConfig::Platform {
                    name: "Extension Manager".to_owned(),
                    description: "platform fixture".to_owned(),
                    bundled: Some(true),
                    available_tools: Vec::new(),
                },
            },
        ]
    }

    #[test]
    fn builtin_and_platform_capabilities_cannot_be_enabled_as_extensions() {
        for entry in capability_entries() {
            let name = entry.config.name();
            let error = check_enable_allowed(
                Some(entry),
                true,
                &name,
                CallCapability::for_test(ProviderTier::Private, true),
            )
            .expect_err("capabilities are managed outside Extension Manager");
            assert!(error.message.contains("capability"), "{}", error.message);
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
            Some(entry_for("publicfixture")),
            false,
            "publicfixture",
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
            "publicfixture",
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
        let pinned = check_enable_allowed(
            Some(entry(false)),
            true,
            "publicfixture",
            public_enforcing(),
        )
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

    /// **The oracle, asserted at this door against EVERY install state — the
    /// gate for the seam finding 4's fix left between the two enable doors.**
    ///
    /// The test above compares two states (absent, installed). The third is the
    /// one the workspace's copy of this gate got wrong: an extension that is
    /// installed *and pinned off by the operator*. That copy asked #42's pin
    /// first, so `workspace_open {new:{extensions}}` answered "…is disabled in
    /// the Biorouter configuration (enabled: false)" to a public caller who may
    /// not have the connector at all — an install-state oracle, reopened one
    /// function away from where finding 13 had just closed it. Both doors now
    /// call `refusal::extension_enable_refusal`, so this asserts the property at
    /// the shared gate through this door and its twin asserts it through the
    /// other two.
    ///
    /// The last two assertions are what stop the fix being "refuse everything":
    /// a caller who MAY have the extension still meets the pin, and a public
    /// caller still meets it for a public extension.
    #[test]
    fn no_install_state_reaches_a_caller_who_may_not_enable_the_extension() {
        const NAME: &str = "ucsfomopagent";
        let pinned_off = || {
            let mut e = entry_for(NAME);
            e.enabled = false;
            e
        };

        let absent = check_enable_allowed(None, false, NAME, public_enforcing()).unwrap_err();
        let installed =
            check_enable_allowed(Some(entry_for(NAME)), false, NAME, public_enforcing())
                .unwrap_err();
        let pinned =
            check_enable_allowed(Some(pinned_off()), true, NAME, public_enforcing()).unwrap_err();

        for (state, err) in [
            ("installed and enabled", &installed),
            ("installed and pinned off by the operator", &pinned),
        ] {
            assert_eq!(
                (absent.code, absent.message.to_string()),
                (err.code, err.message.to_string()),
                "the refusal tells a public caller that the private connector is {state}"
            );
        }
        assert!(
            absent.message.contains("private extension"),
            "{}",
            absent.message
        );
        for leak in ["enabled: false", "not found", "operator"] {
            assert!(
                !absent.message.contains(leak),
                "the refusal a caller who may not see this connector gets names local \
                 state (`{leak}`): {}",
                absent.message
            );
        }

        // …and the pin is not swallowed. A caller ENTITLED to the connector still
        // meets #42, which is the half a reorder breaks silently.
        let entitled = check_enable_allowed(
            Some(pinned_off()),
            true,
            NAME,
            CallCapability::for_test(ProviderTier::Private, true),
        )
        .unwrap_err();
        assert!(
            entitled.message.contains("operator"),
            "{}",
            entitled.message
        );
        // Same for a public caller and a PUBLIC extension — every case #42 was
        // written for.
        let public_pin = check_enable_allowed(
            Some(entry(false)),
            true,
            "publicfixture",
            public_enforcing(),
        )
        .unwrap_err();
        assert!(
            public_pin.message.contains("operator"),
            "{}",
            public_pin.message
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
        let pinned =
            check_enable_allowed(Some(entry(false)), true, "publicfixture", cap).unwrap_err();
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
        assert!(
            allowed.contains(r#""sessionState":"detached""#),
            "{allowed}"
        );

        assert!(
            !em.is_extension_enabled("ucsfomopagent").await,
            "the entitled call must have actually detached the extension"
        );
    }

    #[tokio::test]
    async fn builtin_and_platform_capabilities_cannot_be_disabled_as_extensions() {
        let (_dir, em, client) = a_live_tool_client();
        for entry in capability_entries() {
            let name = entry.config.name();
            em.add_extension(entry.config)
                .await
                .unwrap_or_else(|error| panic!("the {name} capability loads: {error}"));
            let refusal = disable(
                &client,
                &name,
                CallCapability::for_test(ProviderTier::Private, true),
            )
            .await;
            assert!(refusal.contains("capability"), "{name}: {refusal}");
            assert!(
                em.is_extension_enabled(&crate::config::extensions::name_to_key(&name))
                    .await,
                "a rejected disable detached {name}"
            );
        }
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
            text.contains(r#""sessionState":"detached""#),
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
            "the audit read {} bytes; a truncated read reports the same absence \
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

        // The seam: this door's enable gate is the SHARED one, and this file
        // holds no second spelling of it. A behavioural test cannot see the
        // difference — a local re-derivation that happens to agree today passes
        // every assertion above — and agreeing-today is exactly what the two
        // copies did until one of them was reordered.
        assert!(
            calls("crate::privacy::refusal::extension_enable_refusal("),
            "the `manage_extensions` enable door no longer asks the shared enable \
             gate. Its arms (tier, affiliation, operator pin) and their ORDER are \
             the workspace doors' too; a copy here is how the two drifted apart \
             the first time"
        );
        // `extension_entry_is_persisted` is deliberately NOT in this list: the
        // gate takes `persisted` as an argument precisely so it stays pure, and
        // asking that helper is this door's job. What must not come back is an
        // arm of the DECISION.
        //
        // ⚠ **The list is shared, and that is the fix.** This scan used to carry
        // its own literal naming only the idiom THIS file's history produced
        // (`class.tier.is_private()` / `ASK_THE_USER_TO_SWITCH`), while
        // `workspace_extension.rs`'s scan carried a different literal naming
        // only ITS idiom (`resolve_extension(` / `privacy_refusal(` / the
        // affiliation helpers). Two files, two lists, one rule — so a
        // re-derivation written in the other file's vocabulary satisfied both
        // scans and neither reported anything. The guard against duplicating a
        // rule had been duplicated. One const, read by both, in
        // `workspace_extension::tests::ENABLE_GATE_RESPELLINGS`.
        crate::agents::workspace_extension::tests::asserts_no_respellings(
            production,
            "extension_manager_extension.rs",
        );
    }
}
