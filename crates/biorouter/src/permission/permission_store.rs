use crate::config::paths::Paths;
use crate::conversation::message::ToolRequest;
use anyhow::Result;
use blake3::Hasher;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use std::{fs::File, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolPermissionRecord {
    tool_name: String,
    allowed: bool,
    context_hash: String, // Hash of the tool's arguments/context to differentiate similar calls
    #[serde(skip_serializing_if = "Option::is_none")] // Don't serialize if None
    readable_context: Option<String>,
    timestamp: i64,
    expiry: Option<i64>, // Optional expiry timestamp
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolPermissionStore {
    permissions: HashMap<String, Vec<ToolPermissionRecord>>,
    version: u32, // For future schema migrations
    #[serde(skip)] // Don't serialize this field
    permissions_dir: PathBuf,
}

impl Default for ToolPermissionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPermissionStore {
    pub fn new() -> Self {
        Self {
            permissions: HashMap::new(),
            version: 1,
            permissions_dir: Paths::config_dir().join("permissions"),
        }
    }

    pub fn load() -> Result<Self> {
        let store = Self::new();
        let file_path = store.permissions_dir.join("tool_permissions.json");

        if !file_path.exists() {
            return Ok(store);
        }

        let file = File::open(file_path)?;
        let mut permissions: ToolPermissionStore = serde_json::from_reader(file)?;
        permissions.permissions_dir = store.permissions_dir;

        // Clean up expired entries on load
        permissions.cleanup_expired()?;

        Ok(permissions)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.permissions_dir)?;

        let path = self.permissions_dir.join("tool_permissions.json");
        let temp_path = path.with_extension("tmp");

        // Write complete content to temporary file
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&temp_path, &content)?;

        // Atomically rename temp file to target file
        std::fs::rename(temp_path, path)?;

        Ok(())
    }

    pub fn check_permission(&self, tool_request: &ToolRequest) -> Option<bool> {
        // A malformed request carries no recoverable permission key; degrade to
        // "no stored decision" (fail-closed: the caller will prompt) instead of
        // panicking and crashing the loop.
        let tool_call = match tool_request.tool_call.as_ref() {
            Ok(tool_call) => tool_call,
            Err(e) => {
                tracing::warn!("check_permission on malformed tool request: {e}");
                return None;
            }
        };
        let context_hash = self.hash_tool_context(tool_request);
        let key = format!("{}:{}", tool_call.name, context_hash);

        self.permissions.get(&key).and_then(|records| {
            records
                .iter()
                .rfind(|record| record.expiry.is_none_or(|exp| exp > Utc::now().timestamp()))
                .map(|record| record.allowed)
        })
    }

    pub fn record_permission(
        &mut self,
        tool_request: &ToolRequest,
        allowed: bool,
        expiry_duration: Option<Duration>,
    ) -> anyhow::Result<()> {
        // Refuse to persist a permission for a request we cannot read, rather
        // than panicking on the Err tool_call.
        let tool_call = match tool_request.tool_call.as_ref() {
            Ok(tool_call) => tool_call,
            Err(e) => {
                tracing::warn!("refusing to record permission for malformed tool request: {e}");
                anyhow::bail!("cannot record permission for a malformed tool request: {e}");
            }
        };
        let context_hash = self.hash_tool_context(tool_request);
        let key = format!("{}:{}", tool_call.name, context_hash);

        let record = ToolPermissionRecord {
            tool_name: tool_call.name.to_string().clone(),
            allowed,
            context_hash,
            readable_context: Some(tool_request.to_readable_string()),
            timestamp: Utc::now().timestamp(),
            expiry: expiry_duration.map(|d| Utc::now().timestamp() + d.as_secs() as i64),
        };

        self.permissions.entry(key).or_default().push(record);

        self.save()?;
        Ok(())
    }

    fn hash_tool_context(&self, tool_request: &ToolRequest) -> String {
        // Create a hash of the tool's arguments to differentiate similar calls
        // This helps identify when the same tool is being used in a different context
        // A malformed request (Err tool_call) hashes as empty rather than
        // panicking, so it degrades to a stable, argument-less key.
        let mut hasher = Hasher::new();
        let serialized = tool_request
            .tool_call
            .as_ref()
            .ok()
            .and_then(|tool_call| serde_json::to_string(&tool_call.arguments).ok())
            .unwrap_or_default();
        hasher.update(serialized.as_bytes());
        hasher.finalize().to_hex().to_string()
    }

    pub fn cleanup_expired(&mut self) -> anyhow::Result<()> {
        let now = Utc::now().timestamp();
        let mut changed = false;

        self.permissions.retain(|_, records| {
            records.retain(|record| record.expiry.is_none_or(|exp| exp > now));
            changed = changed || records.is_empty();
            !records.is_empty()
        });

        if changed {
            self.save()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{ErrorCode, ErrorData};

    /// A request whose `tool_call` is an `Err` (poisoned/missing payload).
    fn malformed_request() -> ToolRequest {
        ToolRequest {
            id: "bad".to_string(),
            tool_call: Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "poisoned tool_call payload".to_string(),
                None,
            )),
            metadata: None,
            tool_meta: None,
        }
    }

    #[test]
    fn check_permission_treats_malformed_request_as_absent() {
        let store = ToolPermissionStore::new();
        // Must degrade to "no stored permission" rather than panicking on Err.
        assert_eq!(store.check_permission(&malformed_request()), None);
    }

    #[test]
    fn record_permission_rejects_malformed_request() {
        let mut store = ToolPermissionStore::new();
        // Must fail closed (no persistence) instead of panicking on Err.
        let result = store.record_permission(&malformed_request(), true, None);
        assert!(result.is_err());
        assert!(store.permissions.is_empty());
    }

    #[test]
    fn hash_tool_context_is_stable_for_malformed_request() {
        let store = ToolPermissionStore::new();
        // Hashing a malformed request must not panic and stays deterministic.
        let a = store.hash_tool_context(&malformed_request());
        let b = store.hash_tool_context(&malformed_request());
        assert_eq!(a, b);
    }
}
