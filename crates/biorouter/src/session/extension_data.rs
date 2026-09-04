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
    /// Stuck on something outside the agent's control (an answer from the user,
    /// an external dependency). Without it the model encodes the state in the
    /// item's *text* instead — real items titled `BLOCKED: get user's answer`
    /// and `BLOCKED (2nd time): …` were observed in a live session, which is the
    /// schema being routed around.
    Blocked,
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
            "blocked" | "stuck" | "waiting" | "blocked_on" | "on_hold" | "needs_input"
            | "needs_answer" => Some(Self::Blocked),
            _ => None,
        }
    }

    /// Markdown-checkbox marker used in the compact MOIM rendering.
    fn marker(self) -> &'static str {
        match self {
            Self::Pending => "[ ]",
            Self::InProgress => "[~]",
            Self::Blocked => "[!]",
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
    /// The item this one was expanded out of, when [`TodoState::expand_item`]
    /// kept the coarse item as a grouping row. `None` for a top-level item.
    ///
    /// Additive and serde-defaulted on purpose: every `todo.v1` blob written
    /// before nesting existed still deserializes unchanged, so this needed no
    /// version bump and no migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

/// What an in-place expansion did, so the caller can report it rather than
/// leave the model to infer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    /// Ids assigned to the new steps, in order.
    pub ids: Vec<String>,
    /// Whether the original item survived as a grouping row.
    pub kept_parent: bool,
    /// Set when the expanded item was itself a child: the new steps became its
    /// siblings under this parent instead of a third level. Reported rather
    /// than applied silently — the caller asked for something the one-level
    /// shape cannot give it.
    pub nested_under: Option<String>,
}

/// Why [`TodoState::expand_item`] refused. The wording is the tool layer's job;
/// this only says which refusal it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandError {
    /// No item carries that id.
    UnknownId,
    /// Expanding finished work would silently discard the record of it.
    AlreadyCompleted,
    /// The item already has children; replacing it would orphan or delete them.
    AlreadyExpanded { children: Vec<String> },
    /// Every proposed step was blank.
    NoItems,
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

    /// Build (but do not insert) pending items for `texts`, assigning fresh
    /// sequential ids and the given parent. Blank texts are skipped.
    fn new_items<I: IntoIterator<Item = String>>(
        &self,
        texts: I,
        parent: Option<&str>,
    ) -> Vec<TodoItem> {
        let mut next = self.next_id();
        let mut built = Vec::new();
        for text in texts {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            built.push(TodoItem {
                id: next.to_string(),
                text: text.to_string(),
                status: TodoStatus::Pending,
                parent: parent.map(str::to_string),
            });
            next += 1;
        }
        built
    }

    /// Index just past `index` and any children that follow it, so an insertion
    /// there lands after the whole block instead of between a parent and its
    /// own first child.
    fn block_end(&self, index: usize) -> usize {
        let id = self.items[index].id.clone();
        let mut end = index + 1;
        while self
            .items
            .get(end)
            .is_some_and(|item| item.parent.as_deref() == Some(id.as_str()))
        {
            end += 1;
        }
        end
    }

    /// Ids of the items nested under `id`.
    fn children_of(&self, id: &str) -> Vec<String> {
        self.items
            .iter()
            .filter(|item| item.parent.as_deref() == Some(id))
            .map(|item| item.id.clone())
            .collect()
    }

    /// Append new pending items; returns the ids assigned (skipping blanks).
    pub fn add_items<I: IntoIterator<Item = String>>(&mut self, texts: I) -> Vec<String> {
        let new = self.new_items(texts, None);
        let ids = new.iter().map(|item| item.id.clone()).collect();
        self.items.extend(new);
        ids
    }

    /// Insert new pending items directly after item `after`, so ordinary
    /// mid-list insertion stops requiring a whole-list rewrite. Every existing
    /// id is left alone. When the anchor is itself a child the new items become
    /// its siblings under the same parent, so `after` never has to be top level.
    ///
    /// Returns `None` when `after` names no item.
    pub fn add_items_after<I: IntoIterator<Item = String>>(
        &mut self,
        texts: I,
        after: &str,
    ) -> Option<Vec<String>> {
        let anchor = self.items.iter().position(|item| item.id == after)?;
        let parent = self.items[anchor].parent.clone();
        let at = self.block_end(anchor);
        let new = self.new_items(texts, parent.as_deref());
        let ids = new.iter().map(|item| item.id.clone()).collect();
        self.items.splice(at..at, new);
        Some(ids)
    }

    /// Replace item `id` with `texts`, **in place**.
    ///
    /// This is the operation a plan actually needs when a coarse item turns out
    /// to be several steps. The new rows take the old item's slot in `items`
    /// order, so the checklist keeps telling the truth about sequence, and no
    /// surrounding id is renumbered — ids are how [`Self::update_item`]
    /// addresses an item, so they must stay stable. The alternative routes both
    /// lie: appending puts the steps after work they come before, and a full
    /// rewrite renumbers everything and keeps a completed status only if the
    /// caller faithfully re-emits it.
    ///
    /// With `keep_parent` the original survives as a grouping row and the new
    /// rows nest under it. Nesting is deliberately **one level deep**: a tree
    /// invites a taxonomy instead of the work, so expanding an item that is
    /// already a child produces siblings under the same parent (reported by
    /// [`Expanded::nested_under`]) rather than a third level.
    pub fn expand_item<I: IntoIterator<Item = String>>(
        &mut self,
        id: &str,
        texts: I,
        keep_parent: bool,
    ) -> Result<Expanded, ExpandError> {
        let index = self
            .items
            .iter()
            .position(|item| item.id == id)
            .ok_or(ExpandError::UnknownId)?;
        if self.items[index].status == TodoStatus::Completed {
            return Err(ExpandError::AlreadyCompleted);
        }
        // Never destroy tracked state: re-expanding would either orphan the
        // existing children or delete their statuses with them.
        let existing = self.children_of(id);
        if !existing.is_empty() {
            return Err(ExpandError::AlreadyExpanded { children: existing });
        }

        let grandparent = self.items[index].parent.clone();
        let keeping = keep_parent && grandparent.is_none();
        let child_parent = if keeping {
            Some(id.to_string())
        } else {
            grandparent.clone()
        };

        let new = self.new_items(texts, child_parent.as_deref());
        if new.is_empty() {
            return Err(ExpandError::NoItems);
        }
        let ids: Vec<String> = new.iter().map(|item| item.id.clone()).collect();

        if keeping {
            let at = index + 1;
            self.items.splice(at..at, new);
        } else {
            self.items.splice(index..index + 1, new);
        }
        Ok(Expanded {
            ids,
            kept_parent: keeping,
            nested_under: if keeping { None } else { grandparent },
        })
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
                // Children are indented so a grouping row reads as one, and
                // deliberately no deeper: nesting is one level (see
                // `expand_item`).
                let indent = if item.parent.is_some() { "  " } else { "" };
                out.push_str(&format!(
                    "{}- {} (#{}) {}\n",
                    indent,
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
/// `- [x]`/`- [X]`, `- [~]`/`- [-]` (in-progress) and `- [!]` (blocked) with
/// `-`, `*`, or `+` bullets; indentation (sub-tasks) is flattened, so a full
/// rewrite drops nesting as well as ids. Non-checkbox lines are ignored. Ids
/// are assigned sequentially.
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
        } else if let Some(rest) = body.strip_prefix("[!] ") {
            (TodoStatus::Blocked, rest)
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
            parent: None,
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
    fn test_todo_expand_replaces_in_place_and_keeps_surrounding_ids() {
        let mut todo = TodoState::new();
        todo.add_items([
            "survey the inputs".to_string(),
            "set up the workspace".to_string(),
            "do the actual work".to_string(),
            "test and verify".to_string(),
        ]);
        // Work already finished elsewhere in the list must survive untouched.
        todo.update_item("1", Some(TodoStatus::Completed), None);
        todo.update_item("2", Some(TodoStatus::Completed), None);

        let expanded = todo
            .expand_item(
                "3",
                ["write it".to_string(), "wire it up".to_string()],
                true,
            )
            .unwrap();
        assert_eq!(expanded.ids, vec!["5", "6"]);
        assert!(expanded.kept_parent);

        // Order stays truthful: the new steps sit where #3 was, BEFORE #4.
        let order: Vec<&str> = todo.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, vec!["1", "2", "3", "5", "6", "4"]);

        // No surrounding id is renumbered, and no status is lost.
        assert_eq!(todo.items[0].status, TodoStatus::Completed);
        assert_eq!(todo.items[1].status, TodoStatus::Completed);
        assert_eq!(todo.items[5].id, "4");
        assert_eq!(todo.items[5].text, "test and verify");
        assert_eq!(todo.items[5].status, TodoStatus::Pending);
        assert_eq!(todo.items[5].parent, None);

        // The steps nest under the item they came from; the parent stays flat.
        assert_eq!(todo.items[3].parent.as_deref(), Some("3"));
        assert_eq!(todo.items[4].parent.as_deref(), Some("3"));
        assert_eq!(todo.items[2].parent, None);
    }

    #[test]
    fn test_todo_expand_without_keep_parent_takes_the_parents_slot() {
        let mut todo = TodoState::new();
        todo.add_items(["one".to_string(), "two".to_string(), "three".to_string()]);
        let expanded = todo
            .expand_item("2", ["two a".to_string(), "two b".to_string()], false)
            .unwrap();

        assert!(!expanded.kept_parent);
        assert_eq!(expanded.nested_under, None);
        let order: Vec<&str> = todo.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, vec!["1", "4", "5", "3"]);
        // Replacing rather than grouping leaves a flat list.
        assert!(todo.items.iter().all(|i| i.parent.is_none()));
    }

    #[test]
    fn test_todo_expand_stays_one_level_deep() {
        let mut todo = TodoState::new();
        todo.add_items(["build".to_string()]);
        todo.expand_item("1", ["compile".to_string(), "link".to_string()], true)
            .unwrap();

        // Expanding a CHILD produces siblings under the original parent, never a
        // third level — and says so rather than doing it silently.
        let expanded = todo
            .expand_item(
                "2",
                ["cargo build".to_string(), "check warnings".to_string()],
                true,
            )
            .unwrap();
        assert!(!expanded.kept_parent);
        assert_eq!(expanded.nested_under.as_deref(), Some("1"));
        assert!(todo
            .items
            .iter()
            .all(|i| i.parent.is_none() || i.parent.as_deref() == Some("1")));
        let order: Vec<&str> = todo.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, vec!["1", "4", "5", "3"]);
    }

    #[test]
    fn test_todo_expand_refuses_completed_and_already_expanded_items() {
        let mut todo = TodoState::new();
        todo.add_items(["done already".to_string(), "coarse".to_string()]);
        todo.update_item("1", Some(TodoStatus::Completed), None);

        assert_eq!(
            todo.expand_item("1", ["a".to_string()], true),
            Err(ExpandError::AlreadyCompleted)
        );
        assert_eq!(
            todo.expand_item("99", ["a".to_string()], true),
            Err(ExpandError::UnknownId)
        );
        assert_eq!(
            todo.expand_item("2", ["   ".to_string()], true),
            Err(ExpandError::NoItems)
        );
        // Every refusal leaves the list exactly as it was.
        assert_eq!(todo.items.len(), 2);

        todo.expand_item("2", ["step".to_string()], true).unwrap();
        // Re-expanding would orphan or delete the children (and their statuses).
        assert_eq!(
            todo.expand_item("2", ["other".to_string()], true),
            Err(ExpandError::AlreadyExpanded {
                children: vec!["3".to_string()]
            })
        );
    }

    #[test]
    fn test_todo_add_items_after_inserts_at_the_right_index() {
        let mut todo = TodoState::new();
        todo.add_items(["one".to_string(), "two".to_string(), "three".to_string()]);

        let ids = todo
            .add_items_after(["one and a half".to_string()], "1")
            .unwrap();
        assert_eq!(ids, vec!["4"]);
        let order: Vec<&str> = todo.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(order, vec!["1", "4", "2", "3"]);
        assert!(todo.items.iter().all(|i| i.parent.is_none()));

        // An unknown anchor is refused, not appended.
        assert!(todo.add_items_after(["nope".to_string()], "99").is_none());
        assert_eq!(todo.items.len(), 4);
    }

    #[test]
    fn test_todo_add_after_a_parent_clears_its_block_and_after_a_child_is_a_sibling() {
        let mut todo = TodoState::new();
        todo.add_items(["group".to_string(), "later".to_string()]);
        todo.expand_item("1", ["a".to_string(), "b".to_string()], true)
            .unwrap();
        assert_eq!(
            todo.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["1", "3", "4", "2"]
        );

        // After the parent means after its whole block, not between it and its
        // own first child.
        todo.add_items_after(["sibling of group".to_string()], "1")
            .unwrap();
        assert_eq!(
            todo.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["1", "3", "4", "5", "2"]
        );
        assert_eq!(todo.items[3].parent, None);

        // After a child means a sibling under the same parent.
        todo.add_items_after(["between a and b".to_string()], "3")
            .unwrap();
        assert_eq!(
            todo.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            vec!["1", "3", "6", "4", "5", "2"]
        );
        assert_eq!(todo.items[2].parent.as_deref(), Some("1"));
    }

    #[test]
    fn test_todo_v1_blob_without_parent_still_loads() {
        // Back-compat: `parent` is additive and serde-defaulted, so every blob
        // written before nesting existed must load unchanged — no version bump,
        // no migration.
        let mut extension_data = ExtensionData::new();
        extension_data.set_extension_state(
            "todo",
            "v1",
            json!({
                "items": [
                    {"id": "1", "text": "legacy one", "status": "completed"},
                    {"id": "2", "text": "legacy two", "status": "in_progress"}
                ],
                "plan": "old plan"
            }),
        );

        let loaded = TodoState::load(&extension_data).unwrap();
        assert_eq!(loaded.items.len(), 2);
        assert_eq!(loaded.items[0].status, TodoStatus::Completed);
        assert_eq!(loaded.items[0].parent, None);
        assert_eq!(loaded.items[1].parent, None);
        assert_eq!(loaded.plan.as_deref(), Some("old plan"));

        // And a top-level item still serializes without the key at all, so an
        // older reader sees the shape it expects.
        let round_tripped = serde_json::to_value(&loaded).unwrap();
        assert!(round_tripped["items"][0].get("parent").is_none());
    }

    #[test]
    fn test_todo_blocked_round_trips_through_parse_serialize_and_render() {
        for alias in ["blocked", "stuck", "waiting", "needs input", "needs-input"] {
            assert_eq!(
                TodoStatus::parse(alias),
                Some(TodoStatus::Blocked),
                "{alias}"
            );
        }

        let mut todo = TodoState::new();
        todo.add_items(["get the user's answer".to_string()]);
        todo.update_item("1", Some(TodoStatus::Blocked), None);

        // Serialize -> deserialize.
        let value = serde_json::to_value(&todo).unwrap();
        assert_eq!(value["items"][0]["status"], "blocked");
        let back: TodoState = serde_json::from_value(value).unwrap();
        assert_eq!(back.items[0].status, TodoStatus::Blocked);

        // Render -> re-parse: the rendered marker is one `todo_write` accepts.
        let rendered = back.render();
        assert!(
            rendered.contains("- [!] (#1) get the user's answer"),
            "{rendered}"
        );
        let mut reparsed = TodoState::new();
        reparsed.set_from_markdown(&rendered);
        assert_eq!(reparsed.items.len(), 1);
        assert_eq!(reparsed.items[0].status, TodoStatus::Blocked);
    }

    #[test]
    fn test_todo_render_indents_children_under_their_parent() {
        let mut todo = TodoState::new();
        todo.add_items(["do the work".to_string(), "verify".to_string()]);
        todo.expand_item("1", ["write it".to_string()], true)
            .unwrap();

        let rendered = todo.render();
        assert!(rendered.contains("- [ ] (#1) do the work"), "{rendered}");
        assert!(rendered.contains("  - [ ] (#3) write it"), "{rendered}");
        // Top-level rows stay flush, so an existing reader is unaffected.
        assert!(rendered.contains("\n- [ ] (#2) verify"), "{rendered}");
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
