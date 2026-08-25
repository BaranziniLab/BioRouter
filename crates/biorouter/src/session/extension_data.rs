// Extension data management for sessions
// Provides a simple way to store extension-specific data with versioned keys

use crate::config::ExtensionConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;

/// Extension data containing all extension states
/// Keys are in format "extension_name.version" (e.g., "todo.v0")
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct ExtensionData {
    #[serde(flatten)]
    pub extension_states: HashMap<String, Value>,
}

impl ExtensionData {
    /// Create a new empty ExtensionData
    pub fn new() -> Self {
        Self {
            extension_states: HashMap::new(),
        }
    }

    /// Get extension state for a specific extension and version
    pub fn get_extension_state(&self, extension_name: &str, version: &str) -> Option<&Value> {
        let key = format!("{}.{}", extension_name, version);
        self.extension_states.get(&key)
    }

    /// Set extension state for a specific extension and version
    pub fn set_extension_state(&mut self, extension_name: &str, version: &str, state: Value) {
        let key = format!("{}.{}", extension_name, version);
        self.extension_states.insert(key, state);
    }

    /// Remove resolved connector credentials from the legacy extension snapshot
    /// before a session leaves the database through export or diagnostics.
    /// Malformed legacy state is dropped rather than copied verbatim.
    pub fn redact_resolved_auth_material_for_export(&mut self) {
        let key = format!(
            "{}.{}",
            EnabledExtensionsState::EXTENSION_NAME,
            EnabledExtensionsState::VERSION
        );
        let Some(value) = self.extension_states.remove(&key) else {
            return;
        };
        let Ok(mut state) = serde_json::from_value::<EnabledExtensionsState>(value) else {
            return;
        };
        state.extensions = state
            .extensions
            .iter()
            .map(ExtensionConfig::redacted_for_session_export)
            .collect();
        if let Ok(value) = serde_json::to_value(state) {
            self.extension_states.insert(key, value);
        }
    }
}

/// Helper trait for extension-specific state management
pub trait ExtensionState: Sized + Serialize + for<'de> Deserialize<'de> {
    /// The name of the extension
    const EXTENSION_NAME: &'static str;

    /// The version of the extension state format
    const VERSION: &'static str;

    /// Convert from JSON value
    fn from_value(value: &Value) -> Result<Self> {
        serde_json::from_value(value.clone()).map_err(|e| {
            anyhow::anyhow!(
                "Failed to deserialize {} state: {}",
                Self::EXTENSION_NAME,
                e
            )
        })
    }

    /// Convert to JSON value
    fn to_value(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|e| {
            anyhow::anyhow!("Failed to serialize {} state: {}", Self::EXTENSION_NAME, e)
        })
    }

    /// Get state from extension data
    fn from_extension_data(extension_data: &ExtensionData) -> Option<Self> {
        extension_data
            .get_extension_state(Self::EXTENSION_NAME, Self::VERSION)
            .and_then(|v| Self::from_value(v).ok())
    }

    /// Save state to extension data
    fn to_extension_data(&self, extension_data: &mut ExtensionData) -> Result<()> {
        let value = self.to_value()?;
        extension_data.set_extension_state(Self::EXTENSION_NAME, Self::VERSION, value);
        Ok(())
    }
}

/// Status of a single todo item (BR-60).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
}

impl TodoStatus {
    /// Lenient parse of a status word (accepts a few friendly aliases so the
    /// model isn't forced onto exact spelling).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().replace(['-', ' '], "_").as_str() {
            "pending" | "todo" | "open" | "not_started" | "unchecked" => Some(Self::Pending),
            "in_progress" | "inprogress" | "doing" | "active" | "started" | "wip" => {
                Some(Self::InProgress)
            }
            "completed" | "complete" | "done" | "finished" | "checked" => Some(Self::Completed),
            _ => None,
        }
    }

    /// Markdown-checkbox marker used in the compact MOIM rendering.
    fn marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::InProgress => "[~]",
            Self::Completed => "[x]",
        }
    }
}

/// A single, individually-addressable todo item (BR-60). Replaces the former
/// full-overwrite `content: String` blob so per-item state can be updated
/// without rewriting (and truncating) the whole list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub status: TodoStatus,
}

/// TODO extension state: a structured per-item checklist plus an optional
/// living plan artifact the agent maintains as it works (BR-60). Legacy
/// `todo.v0` blobs are migrated to this shape on read (see [`TodoState::load`]).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TodoState {
    #[serde(default)]
    pub items: Vec<TodoItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

impl ExtensionState for TodoState {
    const EXTENSION_NAME: &'static str = "todo";
    // v1: structured items + plan. v0 was the `{"content": String}` blob.
    const VERSION: &'static str = "v1";
}

/// The legacy v0 blob, kept only so existing sessions can be migrated on read.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyTodoBlob {
    content: String,
}

impl ExtensionState for LegacyTodoBlob {
    const EXTENSION_NAME: &'static str = "todo";
    const VERSION: &'static str = "v0";
}

impl TodoState {
    /// Create an empty TODO state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the structured state, migrating a legacy `todo.v0` blob if that is
    /// all a session has. A blob that parses as a markdown checklist becomes
    /// structured items; freeform notes are preserved as the plan text so
    /// nothing is silently lost.
    pub fn load(extension_data: &ExtensionData) -> Option<Self> {
        if let Some(state) = Self::from_extension_data(extension_data) {
            return Some(state);
        }
        let legacy = LegacyTodoBlob::from_extension_data(extension_data)?;
        let items = parse_markdown_checklist(&legacy.content);
        let plan = if items.is_empty() {
            let trimmed = legacy.content.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        } else {
            None
        };
        Some(Self { items, plan })
    }

    /// Next sequential id (ids are stringified integers so they stay stable and
    /// compact in the MOIM rendering the model reads back).
    fn next_id(&self) -> u64 {
        self.items
            .iter()
            .filter_map(|i| i.id.parse::<u64>().ok())
            .max()
            .map_or(1, |m| m + 1)
    }

    /// Append new pending items; returns the ids assigned (skipping blanks).
    pub fn add_items<I: IntoIterator<Item = String>>(&mut self, texts: I) -> Vec<String> {
        let mut next = self.next_id();
        let mut ids = Vec::new();
        for text in texts {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let id = next.to_string();
            next += 1;
            self.items.push(TodoItem {
                id: id.clone(),
                text: text.to_string(),
                status: TodoStatus::Pending,
            });
            ids.push(id);
        }
        ids
    }

    /// Update one item's status and/or text. Returns false if the id is unknown.
    pub fn update_item(
        &mut self,
        id: &str,
        status: Option<TodoStatus>,
        text: Option<String>,
    ) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            if let Some(s) = status {
                item.status = s;
            }
            if let Some(t) = text {
                let t = t.trim();
                if !t.is_empty() {
                    item.text = t.to_string();
                }
            }
            true
        } else {
            false
        }
    }

    /// Replace the whole checklist from a markdown checklist (full write).
    pub fn set_from_markdown(&mut self, content: &str) {
        self.items = parse_markdown_checklist(content);
    }

    /// True when there is nothing worth re-injecting into context.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
            && self
                .plan
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
    }

    /// Compact, human-readable rendering for MOIM re-injection.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if let Some(plan) = self.plan.as_deref() {
            let plan = plan.trim();
            if !plan.is_empty() {
                out.push_str("Plan:\n");
                out.push_str(plan);
                out.push('\n');
            }
        }
        if !self.items.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("Todo checklist:\n");
            for item in &self.items {
                out.push_str(&format!(
                    "- {} (#{}) {}\n",
                    item.status.marker(),
                    item.id,
                    item.text
                ));
            }
        }
        out
    }
}

/// Parse a markdown checklist into structured items. Recognises `- [ ]`,
/// `- [x]`/`- [X]`, and `- [~]`/`- [-]` (in-progress) with `-`, `*`, or `+`
/// bullets; indentation (sub-tasks) is flattened. Non-checkbox lines are
/// ignored. Ids are assigned sequentially.
fn parse_markdown_checklist(content: &str) -> Vec<TodoItem> {
    let mut items = Vec::new();
    let mut next: u64 = 1;
    for line in content.lines() {
        let trimmed = line.trim_start();
        let Some(body) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        else {
            continue;
        };
        let (status, text) = if let Some(rest) = body.strip_prefix("[ ] ") {
            (TodoStatus::Pending, rest)
        } else if let Some(rest) = body
            .strip_prefix("[x] ")
            .or_else(|| body.strip_prefix("[X] "))
        {
            (TodoStatus::Completed, rest)
        } else if let Some(rest) = body
            .strip_prefix("[~] ")
            .or_else(|| body.strip_prefix("[-] "))
        {
            (TodoStatus::InProgress, rest)
        } else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        items.push(TodoItem {
            id: next.to_string(),
            text: text.to_string(),
            status,
        });
        next += 1;
    }
    items
}

/// Enabled extensions state implementation for storing which extensions are active
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnabledExtensionsState {
    pub extensions: Vec<ExtensionConfig>,
}

impl ExtensionState for EnabledExtensionsState {
    const EXTENSION_NAME: &'static str = "enabled_extensions";
    const VERSION: &'static str = "v0";
}

impl EnabledExtensionsState {
    pub fn new(extensions: Vec<ExtensionConfig>) -> Self {
        Self { extensions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extension_data_basic_operations() {
        let mut extension_data = ExtensionData::new();

        // Test setting and getting extension state
        let todo_state = json!({"content": "- Task 1\n- Task 2"});
        extension_data.set_extension_state("todo", "v0", todo_state.clone());

        assert_eq!(
            extension_data.get_extension_state("todo", "v0"),
            Some(&todo_state)
        );
        assert_eq!(extension_data.get_extension_state("todo", "v1"), None);
    }

    #[test]
    fn test_multiple_extension_states() {
        let mut extension_data = ExtensionData::new();

        // Add multiple extension states
        extension_data.set_extension_state("todo", "v0", json!("TODO content"));
        extension_data.set_extension_state("memory", "v1", json!({"items": ["item1", "item2"]}));
        extension_data.set_extension_state("config", "v2", json!({"setting": true}));

        // Check all states exist
        assert_eq!(extension_data.extension_states.len(), 3);
        assert!(extension_data.get_extension_state("todo", "v0").is_some());
        assert!(extension_data.get_extension_state("memory", "v1").is_some());
        assert!(extension_data.get_extension_state("config", "v2").is_some());
    }

    #[test]
    fn test_todo_state_trait() {
        let mut extension_data = ExtensionData::new();

        // Create and save a structured TODO state
        let mut todo = TodoState::new();
        todo.add_items(["Task 1".to_string(), "Task 2".to_string()]);
        todo.to_extension_data(&mut extension_data).unwrap();

        // Retrieve TODO state (persisted under todo.v1)
        let retrieved = TodoState::load(&extension_data).unwrap();
        assert_eq!(retrieved.items.len(), 2);
        assert_eq!(retrieved.items[0].id, "1");
        assert_eq!(retrieved.items[0].text, "Task 1");
        assert_eq!(retrieved.items[0].status, TodoStatus::Pending);
    }

    #[test]
    fn test_todo_add_assigns_sequential_ids() {
        let mut todo = TodoState::new();
        let first = todo.add_items(["a".to_string(), "".to_string(), "b".to_string()]);
        // Blank items are skipped.
        assert_eq!(first, vec!["1", "2"]);
        let second = todo.add_items(["c".to_string()]);
        assert_eq!(second, vec!["3"]);
        assert_eq!(todo.items.len(), 3);
    }

    #[test]
    fn test_todo_update_item_status_and_text() {
        let mut todo = TodoState::new();
        todo.add_items(["draft report".to_string()]);
        assert!(todo.update_item("1", Some(TodoStatus::InProgress), None));
        assert_eq!(todo.items[0].status, TodoStatus::InProgress);
        assert!(todo.update_item(
            "1",
            Some(TodoStatus::Completed),
            Some("final report".to_string())
        ));
        assert_eq!(todo.items[0].status, TodoStatus::Completed);
        assert_eq!(todo.items[0].text, "final report");
        // Unknown id is a no-op returning false.
        assert!(!todo.update_item("99", Some(TodoStatus::Completed), None));
    }

    #[test]
    fn test_todo_set_from_markdown_parses_statuses() {
        let mut todo = TodoState::new();
        todo.set_from_markdown(
            "- [x] done one\n- [ ] pending two\n  - [~] nested in progress\n* [X] done three\nnot a checkbox line\n",
        );
        assert_eq!(todo.items.len(), 4);
        assert_eq!(todo.items[0].status, TodoStatus::Completed);
        assert_eq!(todo.items[1].status, TodoStatus::Pending);
        assert_eq!(todo.items[2].status, TodoStatus::InProgress);
        assert_eq!(todo.items[2].text, "nested in progress");
        assert_eq!(todo.items[3].status, TodoStatus::Completed);
    }

    #[test]
    fn test_todo_status_lenient_parse() {
        assert_eq!(TodoStatus::parse("done"), Some(TodoStatus::Completed));
        assert_eq!(
            TodoStatus::parse("in-progress"),
            Some(TodoStatus::InProgress)
        );
        assert_eq!(TodoStatus::parse("  Pending "), Some(TodoStatus::Pending));
        assert_eq!(TodoStatus::parse("bogus"), None);
    }

    #[test]
    fn test_todo_migrates_legacy_v0_checklist() {
        let mut extension_data = ExtensionData::new();
        // Simulate a legacy todo.v0 blob.
        extension_data.set_extension_state(
            "todo",
            "v0",
            json!({"content": "- [x] shipped\n- [ ] follow up"}),
        );
        let migrated = TodoState::load(&extension_data).unwrap();
        assert_eq!(migrated.items.len(), 2);
        assert_eq!(migrated.items[0].status, TodoStatus::Completed);
        assert_eq!(migrated.items[1].text, "follow up");
        assert!(migrated.plan.is_none());
    }

    #[test]
    fn test_todo_migrates_legacy_v0_freeform_to_plan() {
        let mut extension_data = ExtensionData::new();
        extension_data.set_extension_state(
            "todo",
            "v0",
            json!({"content": "remember to email the PI about results"}),
        );
        let migrated = TodoState::load(&extension_data).unwrap();
        assert!(migrated.items.is_empty());
        assert_eq!(
            migrated.plan.as_deref(),
            Some("remember to email the PI about results")
        );
    }

    #[test]
    fn test_todo_render_and_is_empty() {
        let mut todo = TodoState::new();
        assert!(todo.is_empty());
        todo.plan = Some("1. gather data\n2. analyse".to_string());
        todo.add_items(["gather".to_string()]);
        todo.update_item("1", Some(TodoStatus::InProgress), None);
        let rendered = todo.render();
        assert!(rendered.contains("Plan:"));
        assert!(rendered.contains("Todo checklist:"));
        assert!(rendered.contains("[~] (#1) gather"));
        assert!(!todo.is_empty());
    }

    #[test]
    fn test_extension_data_serialization() {
        let mut extension_data = ExtensionData::new();
        extension_data.set_extension_state("todo", "v0", json!("TODO content"));
        extension_data.set_extension_state("memory", "v1", json!({"key": "value"}));

        // Serialize to JSON
        let json = serde_json::to_value(&extension_data).unwrap();

        // Check the structure
        assert!(json.is_object());
        assert_eq!(json.get("todo.v0"), Some(&json!("TODO content")));
        assert_eq!(json.get("memory.v1"), Some(&json!({"key": "value"})));

        // Deserialize back
        let deserialized: ExtensionData = serde_json::from_value(json).unwrap();
        assert_eq!(
            deserialized.get_extension_state("todo", "v0"),
            Some(&json!("TODO content"))
        );
        assert_eq!(
            deserialized.get_extension_state("memory", "v1"),
            Some(&json!({"key": "value"}))
        );
    }
}
