//! The one home for "this chat's extension roster is now X" — the write into
//! `enabled_extensions.v0` on the session row, and the classifier that says
//! which model-facing catalog tools require it.
//!
//! Both used to be private items in [`crate::agents::agent`], which is a
//! `pub(crate) mod`: reachable from the reply loop and from nowhere else. The
//! extension handler needs the write (so an attach is durable *before* it
//! reports `"attached"`), and `biorouter-server`'s `/agent/call_tool` needs the
//! classifier (so a `manage_extensions` that arrives over HTTP is persisted at
//! all). Mirrors [`crate::agents::session_skills`], which is the same shape for
//! the other half of the catalog.

use anyhow::{anyhow, Result};

use crate::agents::extension_manager::ExtensionManager;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::session_manager::{SessionManager, SessionType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolCatalogMutation {
    pub persist_extension_state: bool,
}

/// Model-facing catalog tools that can change the callable surface during the
/// current turn. Read-only browse/search calls are deliberately absent.
pub fn tool_catalog_mutation(tool_name: &str) -> Option<ToolCatalogMutation> {
    let persist_extension_state = match tool_name {
        "extensionmanager__manage_extensions"
        | "extensionmanager__install_extension"
        | "extensionmanager__delete_extension_package" => true,
        "skills__installMarketplaceSkill"
        | "skills__importSkillPackage"
        | "skills__removeSkillPackage"
        | "skills__setSkillEnabled"
        // The retired pair still dispatches, so it still mutates the catalog.
        | "skills__hotLoadSkill"
        | "skills__hotUnloadSkill" => false,
        _ => return None,
    };
    Some(ToolCatalogMutation {
        persist_extension_state,
    })
}

/// Record the live manager's roster as this session's `enabled_extensions.v0`.
///
/// ⚠ The write closure is `move |_| Ok(value)` — a whole-key REPLACE, not a
/// merge. That is deliberate and load-bearing: a removal is expressed by the
/// key's *absence* from the live snapshot, so a `union(stored, live)` would
/// make disabling an extension unpersistable. `update_extension_state` still
/// does the read and the write inside one transaction, so a concurrent writer
/// of a *different* key of `extension_data` is not clobbered.
///
/// The snapshot goes through [`ExtensionManager::get_extension_configs`]
/// because the `!inprocess && origin != AutoInjected` filter lives inside it —
/// an auto-injected extension must never reach the session row.
pub async fn record(
    session_manager: &SessionManager,
    extension_manager: &ExtensionManager,
    session_id: &str,
) -> Result<()> {
    let session = session_manager.get_session(session_id, false).await?;
    if session.session_type == SessionType::SubAgent {
        return Err(anyhow!(
            "subagent extension grants are immutable runtime-profile authority"
        ));
    }
    let extensions_state =
        EnabledExtensionsState::new(extension_manager.get_extension_configs().await);
    let value = extensions_state
        .to_value()
        .map_err(|e| anyhow!("Extension state serialization failed: {}", e))?;

    let written = session_manager
        .update_extension_state(
            session_id,
            EnabledExtensionsState::EXTENSION_NAME,
            EnabledExtensionsState::VERSION,
            move |_| Ok(value),
        )
        .await?;
    if written.is_none() {
        return Err(anyhow!(
            "cannot record extension state: no session {session_id}"
        ));
    }
    Ok(())
}
