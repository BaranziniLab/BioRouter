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
    /// One exact trusted BAAM registry id.
    pub registry_id: Option<String>,
    /// Several exact trusted BAAM registry ids. The whole batch is validated
    /// before any package is removed.
    #[serde(default)]
    #[schemars(length(max = 50))]
    pub registry_ids: Vec<String>,
}

fn preflight_delete_registry_ids(
    params: DeleteExtensionPackageParams,
) -> Result<Vec<String>, ExtensionManagerToolError> {
    let mut requested = params.registry_ids;
    if let Some(registry_id) = params.registry_id {
        requested.insert(0, registry_id);
    }
    if requested.is_empty() {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Give registry_id for one package or registry_ids for a batch".to_owned(),
        });
    }
    if requested.len() > 50 {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "An extension deletion batch may contain at most 50 packages".to_owned(),
        });
    }
    let mut seen = std::collections::BTreeSet::new();
    for registry_id in &requested {
        if registry_id.is_empty() {
            return Err(ExtensionManagerToolError::OperationFailed {
                message: "Marketplace registry ids cannot be empty".to_owned(),
            });
        }
        if !seen.insert(registry_id.clone()) {
            return Err(ExtensionManagerToolError::OperationFailed {
                message: format!("`{registry_id}` duplicates a package in this batch"),
            });
        }
    }
    Ok(requested)
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
}

impl MarketplaceMutation {
    fn tool_name(self) -> &'static str {
        match self {
            Self::Install => INSTALL_EXTENSION_TOOL_NAME,
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Self::Install => "install",
        }
    }

    fn risk(self) -> crate::permission::tool_risk::ToolRisk {
        match self {
            Self::Install => crate::permission::tool_risk::ToolRisk::Medium,
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

fn extension_enable_approval_request(
    extension_name: &str,
    entry: &ExtensionEntry,
) -> crate::pending_user_action::UserActionRequest {
    crate::pending_user_action::UserActionRequest::ToolApproval(
        crate::pending_user_action::ToolApprovalRequest {
            tool_name: MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE.to_owned(),
            arguments: serde_json::json!({
                "action": "enable",
                "extensionName": extension_name,
                "configKey": entry.config.key(),
                "scope": "currentChatOnly",
            })
            .as_object()
            .expect("extension enable approval is an object")
            .clone(),
            prompt: Some(format!(
                "Allow Biorouter to enable {extension_name} in this chat? Its persistent configuration remains disabled."
            )),
            risk: Some(crate::permission::tool_risk::ToolRisk::Medium),
            preview: None,
            requires_user_proof: true,
        },
    )
}

async fn await_extension_change_approval(
    actions: &Arc<crate::pending_user_action::PendingUserActions>,
    session_id: &str,
    request: crate::pending_user_action::UserActionRequest,
    ttl: Duration,
    cancel: Option<&CancellationToken>,
) -> Result<(), ExtensionManagerToolError> {
    if session_id.is_empty() {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Extension changes require a visible chat session for user approval"
                .to_owned(),
        });
    }
    let parked = actions.park(Some(session_id), None, request);
    match parked.wait(ttl, cancel).await {
        crate::pending_user_action::UserActionOutcome::Approved { .. } => {
            if cancel.is_some_and(CancellationToken::is_cancelled) {
                Err(ExtensionManagerToolError::OperationFailed {
                    message:
                        "The extension change was cancelled after approval; nothing was changed"
                            .to_owned(),
                })
            } else {
                Ok(())
            }
        }
        outcome => Err(ExtensionManagerToolError::OperationFailed {
            message: format!(
                "The extension change was not made because the approval request {}.",
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
    enabled: bool,
    config: ExtensionConfig,
    install_dir: PathBuf,
    extensions_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct ValidatedMarketplaceDeletion {
    descriptor: crate::marketplace::MarketplaceExtensionDescriptor,
    package: ValidatedPackageInstall,
}

async fn preflight_marketplace_deletions(
    registry_ids: &[String],
    caller: crate::privacy::ProviderTier,
) -> Result<Vec<ValidatedMarketplaceDeletion>, ExtensionManagerToolError> {
    let loaded = crate::marketplace::load_marketplace_catalog()
        .await
        .map_err(|error| ExtensionManagerToolError::OperationFailed {
            message: error.to_string(),
        })?;
    let mut plans = Vec::with_capacity(registry_ids.len());
    for registry_id in registry_ids {
        let descriptor = resolve_marketplace_extension(&loaded.catalog, registry_id, caller)?;
        let package = validated_marketplace_package(
            &descriptor,
            crate::privacy::provenance::marketplace_installs_for_registry_id(registry_id),
        )?;
        plans.push(ValidatedMarketplaceDeletion {
            descriptor,
            package,
        });
    }
    validate_unique_deletion_targets(&plans)?;
    Ok(plans)
}

fn validate_unique_deletion_targets(
    plans: &[ValidatedMarketplaceDeletion],
) -> Result<(), ExtensionManagerToolError> {
    let mut config_keys = std::collections::BTreeSet::new();
    let mut install_dirs = std::collections::BTreeSet::new();
    if plans.iter().any(|plan| {
        !config_keys.insert(plan.package.provenance.config_key.clone())
            || !install_dirs.insert(plan.package.install_dir.clone())
    }) {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "Two registry ids resolve to the same installed package; nothing was deleted"
                .to_owned(),
        });
    }
    Ok(())
}

fn marketplace_batch_delete_approval_request(
    plans: &[ValidatedMarketplaceDeletion],
) -> crate::pending_user_action::UserActionRequest {
    let arguments = serde_json::json!({
        "operation": "deleteExtensionPackage",
        "registryIds": plans.iter().map(|plan| plan.descriptor.registry_id.clone()).collect::<Vec<_>>(),
        "packages": plans.iter().map(|plan| serde_json::json!({
            "registryId": plan.descriptor.registry_id,
            "name": plan.descriptor.name,
            "version": plan.descriptor.version,
            "extensionName": plan.package.extension_name,
            "installDirectory": plan.package.install_dir,
        })).collect::<Vec<_>>(),
        "credentialsPreserved": true,
    })
    .as_object()
    .expect("batch deletion approval is an object")
    .clone();
    let preview = crate::conversation::tool_preview::ToolPreview::for_tool_call(
        DELETE_EXTENSION_PACKAGE_TOOL_NAME,
        &arguments,
    );
    crate::pending_user_action::UserActionRequest::ToolApproval(
        crate::pending_user_action::ToolApprovalRequest {
            tool_name: DELETE_EXTENSION_PACKAGE_TOOL_NAME.to_owned(),
            arguments,
            prompt: Some(format!(
                "Permanently delete {} installed BAAM extension package(s)?",
                plans.len()
            )),
            risk: Some(crate::permission::tool_risk::ToolRisk::High),
            preview,
            requires_user_proof: true,
        },
    )
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
        enabled: entry.enabled,
        config: entry.config,
        install_dir: canonical_install,
        extensions_root: canonical_root,
    })
}

async fn detach_marketplace_package_from_session(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    package: &ValidatedPackageInstall,
    cancel: &CancellationToken,
) -> Result<bool, ExtensionManagerToolError> {
    if cancel.is_cancelled() {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "The extension deletion was cancelled before detaching the package".to_owned(),
        });
    }
    let was_attached = manager
        .is_extension_enabled(&package.provenance.config_key)
        .await;
    if cancel.is_cancelled() {
        return Err(ExtensionManagerToolError::OperationFailed {
            message: "The extension deletion was cancelled before detaching the package".to_owned(),
        });
    }
    if was_attached {
        manager
            .remove_extension(&package.extension_name)
            .await
            .map_err(|error| ExtensionManagerToolError::OperationFailed {
                message: format!("Could not detach the package from this chat: {error}"),
            })?;
        if cancel.is_cancelled() {
            return Err(restore_detached_attachment(
                manager,
                package,
                true,
                "The extension deletion was cancelled before package files changed",
            )
            .await);
        }
    }
    Ok(was_attached)
}

async fn restore_staged_marketplace_package(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    package: &ValidatedPackageInstall,
    quarantine: &std::path::Path,
    was_attached: bool,
) -> Result<(), String> {
    std::fs::rename(quarantine, &package.install_dir)
        .map_err(|error| format!("the staged package could not be restored: {error}"))?;
    if was_attached {
        manager.add_extension(package.config.clone()).await.map_err(|error| {
            format!(
                "the package files were restored, but the chat attachment could not be restored: {error}"
            )
        })?;
    }
    Ok(())
}

fn provenance_removal_error(
    result: std::io::Result<bool>,
    restoration: Result<(), String>,
) -> ExtensionManagerToolError {
    let cause = match result {
        Ok(false) => "Marketplace provenance changed before deletion".to_owned(),
        Err(error) => format!("Could not update marketplace provenance: {error}"),
        Ok(true) => unreachable!("successful provenance removal has no error"),
    };
    let message = match restoration {
        Ok(()) => format!("{cause}; the staged package was restored"),
        Err(error) => format!("{cause}; {error}"),
    };
    ExtensionManagerToolError::OperationFailed { message }
}

async fn restore_detached_attachment(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    package: &ValidatedPackageInstall,
    was_attached: bool,
    reason: &str,
) -> ExtensionManagerToolError {
    let message = if !was_attached {
        reason.to_owned()
    } else {
        match manager.add_extension(package.config.clone()).await {
            Ok(()) => format!("{reason}; the chat attachment was restored"),
            Err(error) => {
                format!("{reason}; the chat attachment could not be restored: {error}")
            }
        }
    };
    ExtensionManagerToolError::OperationFailed { message }
}

async fn remove_staged_marketplace_config(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    package: &ValidatedPackageInstall,
    quarantine: &std::path::Path,
    was_attached: bool,
) -> Result<ExtensionEntry, ExtensionManagerToolError> {
    let expected_entry = ExtensionEntry {
        enabled: package.enabled,
        config: package.config.clone(),
    };
    let config_removed = match crate::config::extensions::remove_extension_if_matches(
        &package.provenance.config_key,
        &expected_entry,
    ) {
        Ok(removed) => removed,
        Err(error) => {
            let restoration =
                restore_staged_marketplace_package(manager, package, quarantine, was_attached)
                    .await;
            return Err(ExtensionManagerToolError::OperationFailed {
                message: match restoration {
                    Ok(()) => format!(
                        "Could not update the extension configuration; the staged package was restored: {error}"
                    ),
                    Err(restoration) => format!(
                        "Could not update the extension configuration: {error}; {restoration}"
                    ),
                },
            });
        }
    };
    if !config_removed {
        let restoration =
            restore_staged_marketplace_package(manager, package, quarantine, was_attached).await;
        return Err(ExtensionManagerToolError::OperationFailed {
            message: match restoration {
                Ok(()) => "The extension configuration changed before deletion; the staged package was restored"
                    .to_owned(),
                Err(error) => {
                    format!("The extension configuration changed before deletion; {error}")
                }
            },
        });
    }
    Ok(expected_entry)
}

async fn remove_staged_marketplace_provenance(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    package: &ValidatedPackageInstall,
    quarantine: &std::path::Path,
    was_attached: bool,
    expected_entry: ExtensionEntry,
) -> Result<(), ExtensionManagerToolError> {
    let provenance_result =
        crate::privacy::provenance::remove_marketplace_install_provenance(&package.provenance);
    if matches!(&provenance_result, Ok(true)) {
        return Ok(());
    }

    let config_restored = crate::config::extensions::restore_extension_if_absent(expected_entry)
        .map_err(|error| error.to_string())
        .and_then(|restored| {
            restored
                .then_some(())
                .ok_or_else(|| "a concurrent configuration replacement was preserved".to_owned())
        });
    let package_restored =
        restore_staged_marketplace_package(manager, package, quarantine, was_attached).await;
    let restoration = match (config_restored, package_restored) {
        (Ok(()), Ok(())) => Ok(()),
        (config, package) => Err(format!(
            "rollback was incomplete (config: {}; package: {})",
            config.err().unwrap_or_else(|| "restored".to_owned()),
            package.err().unwrap_or_else(|| "restored".to_owned())
        )),
    };
    Err(provenance_removal_error(provenance_result, restoration))
}

async fn delete_staged_marketplace_package(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    plan: &ValidatedMarketplaceDeletion,
    was_attached: bool,
    cancel: &CancellationToken,
    caller: crate::privacy::ProviderTier,
) -> Result<(), ExtensionManagerToolError> {
    let package = &plan.package;
    if cancel.is_cancelled() {
        return Err(restore_detached_attachment(
            manager,
            package,
            was_attached,
            "The extension deletion was cancelled before package files changed",
        )
        .await);
    }
    if let Err(error) = revalidate_approved_marketplace_deletion(plan, caller).await {
        return Err(restore_detached_attachment(
            manager,
            package,
            was_attached,
            &format!("The installed package changed immediately before deletion: {error}"),
        )
        .await);
    }
    let quarantine = package
        .extensions_root
        .join(format!(".delete-{}", uuid::Uuid::new_v4()));
    if let Err(error) = std::fs::rename(&package.install_dir, &quarantine) {
        return Err(restore_detached_attachment(
            manager,
            package,
            was_attached,
            &format!("Could not stage the package for deletion: {error}"),
        )
        .await);
    }
    if cancel.is_cancelled() {
        let restoration =
            restore_staged_marketplace_package(manager, package, &quarantine, was_attached).await;
        return Err(ExtensionManagerToolError::OperationFailed {
            message: match restoration {
                Ok(()) => "The extension deletion was cancelled; the staged package was restored"
                    .to_owned(),
                Err(error) => format!("The extension deletion was cancelled; {error}"),
            },
        });
    }

    let expected_entry =
        remove_staged_marketplace_config(manager, package, &quarantine, was_attached).await?;
    remove_staged_marketplace_provenance(
        manager,
        package,
        &quarantine,
        was_attached,
        expected_entry,
    )
    .await?;

    std::fs::remove_dir_all(&quarantine).map_err(|error| {
        ExtensionManagerToolError::OperationFailed {
            message: format!(
                "The extension was detached and unregistered, but its quarantined package could not be removed: {error}"
            ),
        }
    })
}

async fn revalidate_approved_marketplace_deletion(
    approved: &ValidatedMarketplaceDeletion,
    caller: crate::privacy::ProviderTier,
) -> Result<(), ExtensionManagerToolError> {
    let registry_id = approved.descriptor.registry_id.clone();
    let current = preflight_marketplace_deletions(&[registry_id], caller).await?;
    if current.first() == Some(approved) {
        Ok(())
    } else {
        Err(ExtensionManagerToolError::OperationFailed {
            message: "The marketplace entry or installed package changed after approval".to_owned(),
        })
    }
}

fn untouched_deletion_result(
    plan: &ValidatedMarketplaceDeletion,
    status: &str,
    reason: &str,
) -> Value {
    serde_json::json!({
        "registryId": plan.descriptor.registry_id,
        "extensionName": plan.package.extension_name,
        "status": status,
        "error": reason,
        "untouched": true,
        "credentialsPreserved": true,
    })
}

fn remaining_deletion_results(
    plans: &[ValidatedMarketplaceDeletion],
    status: &str,
    reason: &str,
) -> Vec<Value> {
    plans
        .iter()
        .map(|plan| untouched_deletion_result(plan, status, reason))
        .collect()
}

async fn delete_one_marketplace_package(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    plan: &ValidatedMarketplaceDeletion,
    cancel: &CancellationToken,
    caller: crate::privacy::ProviderTier,
) -> (bool, Value) {
    let result = match detach_marketplace_package_from_session(manager, &plan.package, cancel).await
    {
        Ok(was_attached) => {
            match delete_staged_marketplace_package(manager, plan, was_attached, cancel, caller)
                .await
            {
                Ok(()) => {
                    return (
                        true,
                        serde_json::json!({
                            "registryId": plan.descriptor.registry_id,
                            "extensionName": plan.package.extension_name,
                            "status": "deleted",
                            "detachedFromCurrentSession": was_attached,
                            "credentialsPreserved": true,
                        }),
                    );
                }
                Err(error) => error,
            }
        }
        Err(error) => error,
    };
    (
        false,
        serde_json::json!({
            "registryId": plan.descriptor.registry_id,
            "extensionName": plan.package.extension_name,
            "status": "error",
            "error": result.to_string(),
            "credentialsPreserved": true,
        }),
    )
}

async fn execute_marketplace_deletion_batch(
    manager: &Arc<crate::agents::extension_manager::ExtensionManager>,
    plans: &[ValidatedMarketplaceDeletion],
    cancel: &CancellationToken,
    caller: crate::privacy::ProviderTier,
) -> (bool, Vec<Value>) {
    let mut all_deleted = true;
    let mut results = Vec::with_capacity(plans.len());
    for (index, plan) in plans.iter().enumerate() {
        if cancel.is_cancelled() {
            results.extend(remaining_deletion_results(
                &plans[index..],
                "cancelled",
                "The deletion batch was cancelled before this package was changed",
            ));
            return (false, results);
        }
        if let Err(error) = revalidate_approved_marketplace_deletion(plan, caller).await {
            results.extend(remaining_deletion_results(
                &plans[index..],
                "notDeleted",
                &format!(
                    "The approved batch changed before this package could be deleted: {error}"
                ),
            ));
            return (false, results);
        }
        if cancel.is_cancelled() {
            results.extend(remaining_deletion_results(
                &plans[index..],
                "cancelled",
                "The deletion batch was cancelled before this package was changed",
            ));
            return (false, results);
        }

        let (deleted, result) = delete_one_marketplace_package(manager, plan, cancel, caller).await;
        all_deleted &= deleted;
        results.push(result);
    }
    (all_deleted, results)
}

fn marketplace_deletion_report(
    registry_ids: Vec<String>,
    results: Vec<Value>,
    all_deleted: bool,
) -> Value {
    let mut report = serde_json::json!({
        "state": if all_deleted { "deleted" } else { "partial" },
        "registryIds": registry_ids,
        "results": results,
        "credentialsPreserved": true,
    });
    let single = report
        .get("results")
        .and_then(Value::as_array)
        .filter(|results| results.len() == 1)
        .and_then(|results| results.first())
        .cloned();
    if let (Some(report), Some(single)) = (
        report.as_object_mut(),
        single.as_ref().and_then(Value::as_object),
    ) {
        for key in ["registryId", "extensionName", "detachedFromCurrentSession"] {
            if let Some(value) = single.get(key) {
                report.insert(key.to_owned(), value.clone());
            }
        }
    }
    report
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
    check_enable_allowed_impl(entry, persisted, extension_name, cap, false)
}

fn check_enable_allowed_with_user_grant(
    entry: Option<ExtensionEntry>,
    persisted: bool,
    extension_name: &str,
    cap: crate::privacy::CallCapability,
) -> Result<ExtensionConfig, ErrorData> {
    check_enable_allowed_impl(entry, persisted, extension_name, cap, true)
}

fn check_enable_allowed_impl(
    entry: Option<ExtensionEntry>,
    persisted: bool,
    extension_name: &str,
    cap: crate::privacy::CallCapability,
    user_granted: bool,
) -> Result<ExtensionConfig, ErrorData> {
    if crate::agents::extension_manager::resolve_bundled_extension(extension_name).is_some() {
        return Err(crate::agents::extension_manager::capability_management_error(extension_name));
    }
    if let Some(refusal) = entry.as_ref().and_then(|entry| {
        crate::agents::extension_manager::capability_management_refusal(&entry.config)
    }) {
        return Err(refusal);
    }
    if let Some(err) = crate::privacy::refusal::extension_manager_enable_refusal(
        cap,
        extension_name,
        entry.as_ref(),
        persisted,
        user_granted,
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
                - delete_extension_package: Permanently delete one or several validated marketplace packages after one user approval
                - list_resources/read_resource: Resource tools, when they are advertised for the current session

                When you lack the tools needed to complete a task, use search_available_extensions first
                to discover what extensions can help.

                Use manage_extensions to enable or disable third-party extensions by name.
                Built-in and platform capabilities are managed separately and this tool refuses them.
                A successful change applies immediately in the current turn. Its response names the
                exact availableTools or removedTools; call an available tool directly by that name,
                and never call a removed tool unless the extension is attached again.
                Use browse/search to obtain an exact registry id, then install_extension when the
                extension is not installed at all. Never provide a download URL or install
                one by running shell commands, and NEVER ask the user to type an API key,
                password or token into the chat — install_extension opens Biorouter's own
                approval and credential dialogs, and a credential in a chat message cannot configure anything.
                delete_extension_package validates the entire bounded batch before removing any package,
                reports each result, and deliberately preserves shared credentials.
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
        session_id: String,
        cap: crate::privacy::CallCapability,
        cancel: CancellationToken,
    ) -> Result<Vec<Content>, ExtensionManagerToolError> {
        let arguments = arguments.ok_or(ExtensionManagerToolError::MissingParameter {
            param_name: "arguments".to_string(),
        })?;

        let params: ManageExtensionsParams =
            serde_json::from_value(serde_json::Value::Object(arguments))?;

        let action = params.action;
        let extension_name = params.extension_name;
        let first = self
            .manage_extensions_impl(action.clone(), extension_name.clone(), cap, false)
            .await
            .map_err(|error| ExtensionManagerToolError::OperationFailed {
                message: error.message.to_string(),
            });
        let result = if action == ManageExtensionAction::Enable && first.is_err() {
            let approved_entry = get_extension_entry_by_name(&extension_name);
            let persisted = approved_entry
                .as_ref()
                .is_some_and(|entry| extension_entry_is_persisted(&entry.config.name()));
            let can_grant = approved_entry.as_ref().is_some_and(|entry| {
                !entry.enabled
                    && persisted
                    && check_enable_allowed_with_user_grant(
                        Some(entry.clone()),
                        true,
                        &extension_name,
                        cap,
                    )
                    .is_ok()
            });
            if can_grant {
                let approved_entry = approved_entry.expect("grant candidate has an entry");
                await_extension_change_approval(
                    crate::pending_user_action::PendingUserActions::global(),
                    &session_id,
                    extension_enable_approval_request(&extension_name, &approved_entry),
                    MARKETPLACE_APPROVAL_TTL,
                    Some(&cancel),
                )
                .await?;

                let current_entry = get_extension_entry_by_name(&extension_name);
                let unchanged = current_entry.as_ref().is_some_and(|current| {
                    extension_entry_is_persisted(&current.config.name())
                        && current.enabled == approved_entry.enabled
                        && current.config == approved_entry.config
                });
                if !unchanged {
                    return Err(ExtensionManagerToolError::OperationFailed {
                        message: "The extension configuration changed after approval; nothing was enabled"
                            .to_owned(),
                    });
                }
                let current_entry = current_entry.expect("unchanged entry exists");
                let config = check_enable_allowed_with_user_grant(
                    Some(current_entry),
                    true,
                    &extension_name,
                    cap,
                )
                .map_err(|error| ExtensionManagerToolError::OperationFailed {
                    message: error.message.to_string(),
                })?;
                self.attach_extension_to_session(extension_name, config, cap)
                    .await
                    .map_err(|error| ExtensionManagerToolError::OperationFailed {
                        message: error.message.to_string(),
                    })
            } else {
                first
            }
        } else {
            first
        };

        result
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
        await_extension_change_approval(
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

        let registry_ids = preflight_delete_registry_ids(params)?;
        let approved = preflight_marketplace_deletions(&registry_ids, cap.tier()).await?;
        await_extension_change_approval(
            crate::pending_user_action::PendingUserActions::global(),
            &session_id,
            marketplace_batch_delete_approval_request(&approved),
            MARKETPLACE_APPROVAL_TTL,
            Some(&cancel),
        )
        .await?;
        if cancel.is_cancelled() {
            return Err(ExtensionManagerToolError::OperationFailed {
                message: "The extension deletion was cancelled; nothing was deleted".to_owned(),
            });
        }
        let current = preflight_marketplace_deletions(&registry_ids, cap.tier()).await?;
        if current != approved {
            return Err(ExtensionManagerToolError::OperationFailed {
                message: "A marketplace entry or installed package changed after approval; nothing was deleted"
                    .to_owned(),
            });
        }

        let manager = self
            .context
            .extension_manager
            .as_ref()
            .and_then(|weak| weak.upgrade())
            .ok_or(ExtensionManagerToolError::ManagerUnavailable)?;
        let (all_deleted, results) =
            execute_marketplace_deletion_batch(&manager, &current, &cancel, cap.tier()).await;
        let report = marketplace_deletion_report(registry_ids, results, all_deleted);
        Ok(vec![Content::text(
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_owned()),
        )])
    }

    async fn manage_extensions_impl(
        &self,
        action: ManageExtensionAction,
        extension_name: String,
        cap: crate::privacy::CallCapability,
        user_granted: bool,
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
            let extension_key = crate::config::extensions::name_to_key(&extension_name);
            let mut removed_tools = extension_manager
                .get_prefixed_tools_for_extension_and_capability(&extension_key, cap)
                .await
                .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
                .into_iter()
                .map(|tool| tool.name.to_string())
                .collect::<Vec<_>>();
            removed_tools.sort();
            return extension_manager
                .remove_extension(&extension_name)
                .await
                .map(|_| {
                    vec![Content::text(
                        serde_json::json!({
                            "extensionName": extension_name,
                            "sessionState": "detached",
                            "persistentConfigurationChanged": false,
                            "removedTools": removed_tools,
                            "toolAvailability": "revokedImmediately",
                            "guidance": "The removedTools are unavailable now. Do not call them unless the extension is attached again.",
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
        let config = if user_granted {
            check_enable_allowed_with_user_grant(entry, persisted, &extension_name, cap)?
        } else {
            check_enable_allowed(entry, persisted, &extension_name, cap)?
        };

        self.attach_extension_to_session(extension_name, config, cap)
            .await
    }

    async fn attach_extension_to_session(
        &self,
        extension_name: String,
        config: ExtensionConfig,
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
                    "Extension manager is no longer available".to_owned(),
                    None,
                )
            })?;
        let extension_key = config.key();
        extension_manager
            .add_extension(config)
            .await
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?;
        let mut available_tools = extension_manager
            .get_prefixed_tools_for_extension_and_capability(&extension_key, cap)
            .await
            .map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None))?
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        available_tools.sort();
        Ok(vec![Content::text(
            serde_json::json!({
                "extensionName": extension_name,
                "sessionState": "attached",
                "persistentConfigurationChanged": false,
                "availableTools": available_tools,
                "toolAvailability": "immediate",
                "guidance": "The availableTools are callable now in this turn. Use the exact tool name needed for the user's task.",
            })
            .to_string(),
        )])
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
            Changes apply immediately in the current turn. The result lists exact availableTools
            after attach or removedTools after detach; use or stop using those names accordingly.
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
                "Permanently delete one or up to 50 validated marketplace-installed .brxt packages by exact registry id. The whole batch is preflighted before one proof-backed approval, every result is reported, and credentials are preserved."
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
                self.handle_manage_extensions(
                    arguments,
                    meta.session_id.clone(),
                    meta.capability,
                    _cancellation_token.clone(),
                )
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
        let mutation = MarketplaceMutation::Install;
        let request = marketplace_approval_request(mutation, &marketplace_descriptor(), None);
        let crate::pending_user_action::UserActionRequest::ToolApproval(request) = request else {
            panic!("marketplace mutation did not create an approval")
        };
        assert!(request.requires_user_proof);
        assert_eq!(request.tool_name, mutation.tool_name());
        assert_eq!(request.risk, Some(mutation.risk()));
        assert!(request.arguments.contains_key("registryId"));
        assert!(request.arguments.contains_key("downloadUrl"));
    }

    #[test]
    fn operator_disabled_enable_constructs_a_proof_bound_session_only_card() {
        let request = extension_enable_approval_request("publicfixture", &entry(false));
        let crate::pending_user_action::UserActionRequest::ToolApproval(request) = request else {
            panic!("extension enable did not create an approval")
        };
        assert!(request.requires_user_proof);
        assert_eq!(request.tool_name, MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE);
        assert_eq!(
            request.arguments.get("scope").and_then(Value::as_str),
            Some("currentChatOnly")
        );
        assert!(!request.arguments.contains_key("persistedEntry"));
        let encoded = serde_json::to_string(&request.arguments).unwrap();
        for secret_config_field in ["envs", "env_keys", "args", "cmd"] {
            assert!(!encoded.contains(secret_config_field), "{encoded}");
        }
    }

    #[tokio::test]
    async fn marketplace_approval_cancellation_and_timeout_stop_the_mutation() {
        let descriptor = marketplace_descriptor();

        let cancelled_actions = Arc::new(crate::pending_user_action::PendingUserActions::default());
        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = await_extension_change_approval(
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
        let timed_out = await_extension_change_approval(
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
            install_id: Some(format!("test-install-{registry_id}")),
            registry_id: registry_id.to_owned(),
            install_dir: install_dir.display().to_string(),
            source_url: format!(
                "https://github.com/example/{registry_id}/releases/download/v1.0.0/{registry_id}.brxt"
            ),
        }
    }

    struct DeletionFixture {
        registry_id: String,
        config_key: String,
        extension_name: String,
        install_dir: PathBuf,
        entry: ExtensionEntry,
    }

    impl DeletionFixture {
        fn assert_provenance_present(&self) {
            assert!(
                crate::privacy::provenance::marketplace_installs_for_registry_id(&self.registry_id)
                    .iter()
                    .any(|provenance| provenance.config_key == self.config_key),
                "the marketplace provenance for {} must still be current",
                self.registry_id
            );
        }

        fn assert_provenance_removed(&self) {
            assert!(
                crate::privacy::provenance::marketplace_installs_for_registry_id(&self.registry_id)
                    .iter()
                    .all(|provenance| provenance.config_key != self.config_key),
                "the marketplace provenance for {} survived deletion",
                self.registry_id
            );
        }

        fn remove_persisted_artifacts(&self) {
            crate::config::extensions::remove_extension(&self.config_key);
            if let Some(provenance) =
                crate::privacy::provenance::marketplace_installs_for_registry_id(&self.registry_id)
                    .into_iter()
                    .find(|provenance| provenance.config_key == self.config_key)
            {
                let _ =
                    crate::privacy::provenance::remove_marketplace_install_provenance(&provenance);
            }
            if self.install_dir.exists() {
                let _ = std::fs::remove_dir_all(&self.install_dir);
            }
        }
    }

    impl Drop for DeletionFixture {
        fn drop(&mut self) {
            self.remove_persisted_artifacts();
        }
    }

    fn pinned_path_root() -> env_lock::EnvGuard<'static> {
        let current = std::env::var("BIOROUTER_PATH_ROOT").ok();
        env_lock::lock_env([("BIOROUTER_PATH_ROOT", current.as_deref())])
    }

    async fn install_deletion_fixture(registry_id: &str, label: &str) -> DeletionFixture {
        let descriptor = trusted_marketplace_extension(registry_id, ProviderTier::Public)
            .await
            .unwrap_or_else(|error| panic!("the shipped {registry_id} descriptor loads: {error}"));
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let extension_name = format!("ManagerDelete{label}{suffix}");
        let config_key = crate::config::extensions::name_to_key(&extension_name);
        let install_dir = crate::extension_install::brxt::extensions_root().join(&extension_name);
        std::fs::create_dir_all(&install_dir).unwrap();
        let entry = package_entry(&extension_name, &install_dir);
        crate::config::extensions::set_extension(entry.clone());
        crate::privacy::provenance::record(
            &extension_name,
            crate::privacy::provenance::ExtensionProvenance {
                install_id: Some(format!("delete-fixture-{suffix}")),
                registry_id: registry_id.to_owned(),
                install_dir: Some(install_dir.display().to_string()),
                source_url: Some(descriptor.download_url.as_str().to_owned()),
                bundle_sha256: None,
                recorded_at: None,
            },
        )
        .unwrap();
        let fixture = DeletionFixture {
            registry_id: registry_id.to_owned(),
            config_key,
            extension_name,
            install_dir,
            entry,
        };
        fixture.assert_provenance_present();
        fixture
    }

    fn tool_result_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content| content.as_text().map(|text| text.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn wait_for_delete_card(session_id: &str) -> String {
        tokio::time::timeout(
            Duration::from_secs(30),
            crate::action_required_manager::ActionRequiredManager::global()
                .request_arrived(session_id),
        )
        .await
        .expect("the deletion call must publish its approval card");
        let messages = crate::action_required_manager::ActionRequiredManager::global()
            .drain_requests(session_id);
        let approval_id = messages
            .iter()
            .flat_map(|message| &message.content)
            .find_map(|content| {
                let crate::conversation::message::MessageContent::ActionRequired(action) = content
                else {
                    return None;
                };
                let crate::conversation::message::ActionRequiredData::ToolConfirmation {
                    id,
                    tool_name,
                    ..
                } = &action.data
                else {
                    return None;
                };
                (tool_name == DELETE_EXTENSION_PACKAGE_TOOL_NAME).then(|| id.clone())
            })
            .expect("the deletion call must publish a tool-confirmation card");
        assert!(crate::pending_user_action::PendingUserActions::global()
            .requires_user_proof_in_session(session_id, &approval_id));
        approval_id
    }

    async fn approve_delete_card(session_id: &str) {
        let approval_id = wait_for_delete_card(session_id).await;
        assert_eq!(
            crate::pending_user_action::PendingUserActions::global().resolve_in_session(
                session_id,
                &approval_id,
                crate::pending_user_action::UserActionOutcome::Approved {
                    permission: crate::permission::Permission::AllowOnce,
                },
            ),
            crate::pending_user_action::ResolveOutcome::Delivered
        );
    }

    async fn run_approved_delete(
        client: Arc<ExtensionManagerClient>,
        session_id: String,
        arguments: Value,
    ) -> CallToolResult {
        let running = tokio::spawn({
            let session_id = session_id.clone();
            async move {
                client
                    .call_tool(
                        DELETE_EXTENSION_PACKAGE_TOOL_NAME,
                        Some(arguments.as_object().unwrap().clone()),
                        McpMeta::new(
                            session_id,
                            CallCapability::for_test(ProviderTier::Public, true),
                        ),
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        approve_delete_card(&session_id).await;
        tokio::time::timeout(Duration::from_secs(30), running)
            .await
            .expect("the approved deletion must finish promptly")
            .expect("the deletion task must not panic")
            .expect("the extension manager must return a tool result")
    }

    #[tokio::test]
    async fn approved_single_package_deletion_removes_package_config_provenance_and_session_state()
    {
        let _path_root = pinned_path_root();
        let fixture = install_deletion_fixture("playwrightagent", "Single").await;
        let (_manager_root, manager, client) = a_live_tool_client();
        assert!(!manager.is_extension_enabled(&fixture.config_key).await);
        let session_id = format!("delete-single-{}", uuid::Uuid::new_v4());

        let result = run_approved_delete(
            Arc::new(client),
            session_id,
            serde_json::json!({ "registry_id": fixture.registry_id.clone() }),
        )
        .await;

        assert_ne!(result.is_error, Some(true), "{}", tool_result_text(&result));
        let report: Value = serde_json::from_str(&tool_result_text(&result)).unwrap();
        assert_eq!(report["state"], "deleted");
        assert_eq!(report["results"][0]["status"], "deleted");
        assert!(!fixture.install_dir.exists());
        assert!(get_extension_entry_by_name(&fixture.extension_name).is_none());
        fixture.assert_provenance_removed();
        assert!(!manager.is_extension_enabled(&fixture.config_key).await);
    }

    #[tokio::test]
    async fn approved_batch_deletion_removes_every_package_config_provenance_and_session_state() {
        let _path_root = pinned_path_root();
        let first = install_deletion_fixture("codegraphagent", "BatchOne").await;
        let second = install_deletion_fixture("bioroffice", "BatchTwo").await;
        let (_manager_root, manager, client) = a_live_tool_client();
        let session_id = format!("delete-batch-{}", uuid::Uuid::new_v4());

        let result = run_approved_delete(
            Arc::new(client),
            session_id,
            serde_json::json!({
                "registry_ids": [first.registry_id.clone(), second.registry_id.clone()]
            }),
        )
        .await;

        assert_ne!(result.is_error, Some(true), "{}", tool_result_text(&result));
        let report: Value = serde_json::from_str(&tool_result_text(&result)).unwrap();
        assert_eq!(report["state"], "deleted");
        assert_eq!(report["results"].as_array().map(Vec::len), Some(2));
        for fixture in [&first, &second] {
            assert!(!fixture.install_dir.exists());
            assert!(get_extension_entry_by_name(&fixture.extension_name).is_none());
            fixture.assert_provenance_removed();
            assert!(!manager.is_extension_enabled(&fixture.config_key).await);
        }
    }

    #[tokio::test]
    async fn post_approval_config_revalidation_leaves_the_replacement_and_package_untouched() {
        let _path_root = pinned_path_root();
        let fixture = install_deletion_fixture("opennotebookagent", "Revalidate").await;
        let (_manager_root, manager, client) = a_live_tool_client();
        let client = Arc::new(client);
        let session_id = format!("delete-revalidate-{}", uuid::Uuid::new_v4());
        let running = tokio::spawn({
            let client = Arc::clone(&client);
            let session_id = session_id.clone();
            let registry_id = fixture.registry_id.clone();
            async move {
                client
                    .call_tool(
                        DELETE_EXTENSION_PACKAGE_TOOL_NAME,
                        Some(
                            serde_json::json!({ "registry_id": registry_id })
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                        McpMeta::new(
                            session_id,
                            CallCapability::for_test(ProviderTier::Public, true),
                        ),
                        CancellationToken::new(),
                    )
                    .await
            }
        });
        let approval_id = wait_for_delete_card(&session_id).await;
        let mut replacement = fixture.entry.clone();
        replacement.enabled = false;
        crate::config::extensions::set_extension(replacement.clone());
        assert_eq!(
            crate::pending_user_action::PendingUserActions::global().resolve_in_session(
                &session_id,
                &approval_id,
                crate::pending_user_action::UserActionOutcome::Approved {
                    permission: crate::permission::Permission::AllowOnce,
                },
            ),
            crate::pending_user_action::ResolveOutcome::Delivered
        );

        let result = tokio::time::timeout(Duration::from_secs(30), running)
            .await
            .expect("the stale approved deletion must finish promptly")
            .expect("the deletion task must not panic")
            .expect("the extension manager must return a tool result");
        assert_eq!(result.is_error, Some(true), "{}", tool_result_text(&result));
        assert!(tool_result_text(&result).contains("changed after approval"));
        assert!(fixture.install_dir.is_dir());
        let current = get_extension_entry_by_name(&fixture.extension_name)
            .expect("the replacement config must remain registered");
        assert!(!current.enabled);
        assert_eq!(current.config, replacement.config);
        fixture.assert_provenance_present();
        assert!(!manager.is_extension_enabled(&fixture.config_key).await);
        fixture.remove_persisted_artifacts();
    }

    #[tokio::test]
    async fn package_deletion_accepts_only_one_direct_validated_marketplace_child() {
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
        let plan = ValidatedMarketplaceDeletion {
            descriptor: descriptor.clone(),
            package: validated.clone(),
        };
        let approval = marketplace_batch_delete_approval_request(std::slice::from_ref(&plan));
        let crate::pending_user_action::UserActionRequest::ToolApproval(approval) = approval else {
            panic!("batch deletion did not create a tool approval")
        };
        assert!(approval.requires_user_proof);
        assert_eq!(
            approval.risk,
            Some(crate::permission::tool_risk::ToolRisk::High)
        );
        assert_eq!(
            approval
                .arguments
                .get("registryIds")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert!(approval.preview.is_some());

        let (_manager_root, manager, _client) = a_live_tool_client();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let cancelled = delete_staged_marketplace_package(
            &manager,
            &plan,
            false,
            &cancel,
            ProviderTier::Private,
        )
        .await
        .expect_err("cancellation before staging must stop package deletion");
        assert!(cancelled.to_string().contains("cancelled"));
        assert!(
            validated.install_dir.is_dir(),
            "a pre-staging cancellation must leave the approved package untouched"
        );

        let mut alias_descriptor = descriptor.clone();
        alias_descriptor.registry_id = "different-registry-id".to_owned();
        let alias = validate_unique_deletion_targets(&[
            ValidatedMarketplaceDeletion {
                descriptor: descriptor.clone(),
                package: validated.clone(),
            },
            ValidatedMarketplaceDeletion {
                descriptor: alias_descriptor,
                package: validated.clone(),
            },
        ])
        .expect_err("two registry ids may not alias one package");
        assert!(alias.to_string().contains("same installed package"));

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
    fn extension_package_batch_preflight_is_bounded_ordered_and_unique() {
        let ids = preflight_delete_registry_ids(DeleteExtensionPackageParams {
            registry_id: Some("first".to_owned()),
            registry_ids: vec!["second".to_owned()],
        })
        .expect("valid batch");
        assert_eq!(ids, vec!["first", "second"]);

        assert!(preflight_delete_registry_ids(DeleteExtensionPackageParams {
            registry_id: Some("same".to_owned()),
            registry_ids: vec!["same".to_owned()],
        })
        .unwrap_err()
        .to_string()
        .contains("duplicates"));
        assert!(preflight_delete_registry_ids(DeleteExtensionPackageParams {
            registry_id: None,
            registry_ids: Vec::new(),
        })
        .is_err());
        assert!(preflight_delete_registry_ids(DeleteExtensionPackageParams {
            registry_id: None,
            registry_ids: (0..51).map(|index| format!("pkg-{index}")).collect(),
        })
        .unwrap_err()
        .to_string()
        .contains("50"));

        let schema = serde_json::to_value(schema_for!(DeleteExtensionPackageParams)).unwrap();
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .expect("deletion schema properties");
        assert!(properties.contains_key("registry_id"));
        assert!(properties.contains_key("registry_ids"));
        assert_eq!(
            properties
                .get("registry_ids")
                .and_then(|value| value.get("maxItems"))
                .and_then(Value::as_u64),
            Some(50)
        );
        assert!(
            serde_json::from_value::<DeleteExtensionPackageParams>(serde_json::json!({
                "registry_id": "one",
                "unexpected": true
            }))
            .is_err()
        );
    }

    #[test]
    fn package_deletion_path_never_revokes_or_removes_credentials() {
        let source = include_str!("extension_manager_extension.rs");
        let delete_handler = source
            .split("async fn handle_delete_extension_package")
            .nth(1)
            .and_then(|tail| tail.split("async fn manage_extensions_impl").next())
            .expect("delete handler boundaries");
        let delete_report = source
            .split("fn marketplace_deletion_report")
            .nth(1)
            .and_then(|tail| tail.split("/// The `manage_extensions` enable door").next())
            .expect("delete report boundaries");
        let delete_body = format!("{delete_handler}\n{delete_report}");
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
    fn explicit_user_grant_can_attach_a_public_operator_disabled_extension() {
        let config = check_enable_allowed_with_user_grant(
            Some(entry(false)),
            true,
            "publicfixture",
            public_enforcing(),
        )
        .expect("proof-backed user approval may override the operator pin for this chat");
        assert_eq!(config.name(), "publicfixture");

        let private = check_enable_allowed_with_user_grant(
            Some(ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::stdio(
                    "ucsfomopagent",
                    "fixture-command",
                    "private fixture",
                    30_u64,
                ),
            }),
            true,
            "ucsfomopagent",
            public_enforcing(),
        )
        .expect_err("user approval cannot cross the public-to-private boundary");
        assert!(
            private.message.contains("private extension"),
            "{}",
            private.message
        );

        let pinned_private = check_enable_allowed_with_user_grant(
            Some(ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::stdio(
                    "ucsfomopagent",
                    "fixture-command",
                    "private fixture",
                    30_u64,
                ),
            }),
            true,
            "ucsfomopagent",
            CallCapability::for_test(ProviderTier::Private, true),
        )
        .expect_err("explicit grant never overrides the operator pin for a private extension");
        assert!(
            pinned_private.message.contains("operator"),
            "{}",
            pinned_private.message
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
    fn a_public_model_never_enables_a_private_extension_through_the_manager() {
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
        // The manager's product contract is stricter than the diagnostic
        // privacy toggle: a public model may never install or spawn a private
        // extension through this agent-controlled door.
        let off = check_enable_allowed(
            Some(entry_for("ucsfomopagent")),
            false,
            "ucsfomopagent",
            CallCapability::for_test(Public, false),
        )
        .expect_err("the manager never grants a private extension to a public model");
        assert!(off.message.contains("private extension"), "{}", off.message);
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

    #[tokio::test]
    async fn attach_result_names_every_tool_that_is_immediately_callable() {
        let (_dir, em, client) = a_live_tool_client();
        let config = ExtensionConfig::Platform {
            name: "extensionmanager".to_owned(),
            description: "Extension Manager".to_owned(),
            bundled: Some(true),
            available_tools: Vec::new(),
        };

        let content = client
            .attach_extension_to_session(
                "Extension Manager".to_owned(),
                config,
                CallCapability::for_test(ProviderTier::Public, true),
            )
            .await
            .expect("the platform extension attaches");
        let text = content[0].as_text().expect("JSON text response");
        let payload: serde_json::Value = serde_json::from_str(&text.text).unwrap();
        let reported = payload["availableTools"]
            .as_array()
            .expect("attach reports an exact tool roster")
            .iter()
            .map(|name| name.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let mut actual = em
            .get_prefixed_tools_for_extension_and_capability(
                "extensionmanager",
                CallCapability::for_test(ProviderTier::Public, true),
            )
            .await
            .unwrap()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        actual.sort();

        assert_eq!(reported, actual);
        assert!(!reported.is_empty(), "the fixture must expose real tools");
        assert_eq!(payload["toolAvailability"], "immediate");
        assert!(payload["guidance"]
            .as_str()
            .unwrap()
            .contains("callable now in this turn"));
    }

    #[tokio::test]
    async fn detach_result_names_every_tool_that_is_revoked_immediately() {
        let (_dir, em, client) = a_live_tool_client();
        let context = PlatformExtensionContext {
            extension_manager: Some(std::sync::Arc::downgrade(&em)),
            session_manager: em.get_context().session_manager.clone(),
        };
        let config = ExtensionConfig::stdio(
            "publicfixture",
            "unused-fixture-command",
            "ordinary public extension fixture",
            30_u64,
        );
        em.add_client(
            config.key(),
            config,
            std::sync::Arc::new(
                ExtensionManagerClient::new(context).expect("fixture client builds"),
            ),
            None,
            None,
        )
        .await;
        let cap = CallCapability::for_test(ProviderTier::Public, true);
        let mut before = em
            .get_prefixed_tools_for_extension_and_capability("publicfixture", cap)
            .await
            .unwrap()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        before.sort();
        assert!(!before.is_empty(), "the fixture must expose real tools");

        let content = client
            .manage_extensions_impl(
                ManageExtensionAction::Disable,
                "publicfixture".to_owned(),
                cap,
                false,
            )
            .await
            .expect("the ordinary public extension detaches");
        let text = content[0].as_text().expect("JSON text response");
        let payload: serde_json::Value = serde_json::from_str(&text.text).unwrap();
        let reported = payload["removedTools"]
            .as_array()
            .expect("detach reports an exact revoked roster")
            .iter()
            .map(|name| name.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();

        assert_eq!(reported, before);
        assert_eq!(payload["toolAvailability"], "revokedImmediately");
        assert!(!em.is_extension_enabled("publicfixture").await);
        assert!(em
            .get_prefixed_tools_for_extension_and_capability("publicfixture", cap)
            .await
            .unwrap()
            .is_empty());
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
