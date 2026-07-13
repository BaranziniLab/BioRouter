//! Auto post-edit diagnostics feedback loop (BR-47).
//!
//! `text_editor` writes never triggered a syntax check: the model learned a file
//! was broken only if it *chose* to run tests. The developer extension's own
//! tree-sitter analyzer can parse the just-written file and report its ERROR /
//! MISSING nodes essentially for free (see
//! [`biorouter_mcp::developer::analyze::diagnostics`]), but nothing wired that
//! capability into the edit path.
//!
//! This module supplies the wiring's *pure* parts — the config gate, the
//! write-detection + path resolution, and the corrective-context formatting.
//! The agent loop ([`crate::agents::agent`]) owns the effectful parts: running the
//! analyzer on the edited files after a successful `text_editor` write and
//! injecting the feedback as agent-visible context, bounded by a per-reply
//! reflection counter so a file that never parses clean cannot wedge the turn.
//!
//! It rides the BR-19 PostToolUse seam: the diagnostics are computed at the same
//! turn boundary where PostToolUse hooks inject their `additionalContext`, and
//! the reflection counter mirrors the `POST_TOOL_HOOK_BLOCK_CAP` bound so an
//! always-broken file behaves exactly like an always-blocking hook — capped, then
//! delivered as-is.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::Config;
use crate::conversation::message::ToolRequest;

/// Default cap on how many times per reply a broken edit is reflected back to the
/// model. Aider's `max_reflections` is 3; a file still broken after three
/// corrective nudges in one reply will not be fixed by a fourth, and the model can
/// still act on the tool result it can already see.
pub const DEFAULT_MAX_REFLECTIONS: u32 = 3;

/// BR-47 policy, resolved once per reply (config reads touch the filesystem).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostEditDiagnosticsConfig {
    /// Master switch. Off = the exact pre-BR-47 behaviour (no post-edit check).
    pub enabled: bool,
    /// Per-reply reflection cap. `0` also disables injection.
    pub max_reflections: u32,
}

impl Default for PostEditDiagnosticsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_reflections: DEFAULT_MAX_REFLECTIONS,
        }
    }
}

impl PostEditDiagnosticsConfig {
    /// Inert (pre-BR-47 behaviour).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_reflections: 0,
        }
    }

    /// Resolve from config. Keys:
    ///
    /// * `BIOROUTER_POST_EDIT_DIAGNOSTICS` (bool) — master switch.
    /// * `BIOROUTER_POST_EDIT_DIAGNOSTICS_MAX_REFLECTIONS` (u32) — the per-reply cap.
    pub fn from_config() -> Self {
        Self::from(Config::global())
    }

    pub fn from(config: &Config) -> Self {
        let defaults = Self::default();
        let enabled = config
            .get_param::<bool>("BIOROUTER_POST_EDIT_DIAGNOSTICS")
            .unwrap_or(defaults.enabled);
        let max_reflections = config
            .get_param::<u32>("BIOROUTER_POST_EDIT_DIAGNOSTICS_MAX_REFLECTIONS")
            .unwrap_or(defaults.max_reflections);
        Self {
            enabled,
            max_reflections,
        }
    }

    /// Whether any injection can happen at all.
    pub fn is_active(&self) -> bool {
        self.enabled && self.max_reflections > 0
    }
}

/// The file a successful `text_editor` write changed, resolved against
/// `working_dir`.
///
/// `None` for a non-write command (`view` / `undo_edit`), a tool that is not
/// `text_editor`, or a call with no usable path. Only `write` / `str_replace` /
/// `insert` change file *content* in a way a fresh parse can check.
pub fn edited_path_from_tool_call(
    tool_name: &str,
    arguments: &serde_json::Map<String, Value>,
    working_dir: &Path,
) -> Option<PathBuf> {
    if !is_text_editor(tool_name) {
        return None;
    }
    let command = arguments.get("command").and_then(Value::as_str)?;
    if !matches!(command, "write" | "str_replace" | "insert") {
        return None;
    }
    // `file_path` is the developer tool's documented alias for `path`.
    let raw = arguments
        .get("path")
        .or_else(|| arguments.get("file_path"))
        .and_then(Value::as_str)?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(resolve_path(raw, working_dir))
}

/// Convenience for the agent loop: pull the edited path straight off a
/// [`ToolRequest`]. `None` unless the request is a well-formed `text_editor`
/// write.
pub fn edited_path_from_request(request: &ToolRequest, working_dir: &Path) -> Option<PathBuf> {
    let tool_call = request.tool_call.as_ref().ok()?;
    let arguments = tool_call.arguments.as_ref()?;
    edited_path_from_tool_call(tool_call.name.as_ref(), arguments, working_dir)
}

fn is_text_editor(tool_name: &str) -> bool {
    // Extension tools are namespaced `<server>__<tool>` (e.g. `developer__text_editor`).
    tool_name == "text_editor" || tool_name.ends_with("__text_editor")
}

fn resolve_path(raw: &str, working_dir: &Path) -> PathBuf {
    let expanded = shellexpand::tilde(raw);
    let path = Path::new(expanded.as_ref());
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path)
    }
}

/// What to do with a batch of freshly-checked edits, given the running
/// reflection count. Keeps the increment / reset / cap policy out of the reply
/// loop so it can be unit-tested on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflectionOutcome {
    /// At least one edited file is still broken and the budget is not spent:
    /// inject the diagnostics and advance the counter to `reflections + 1`.
    Inject { next: u32 },
    /// Every edited file parsed clean (or there was nothing dirty): a genuine fix
    /// restores the budget, so the counter resets to 0.
    Reset,
    /// A file is still broken but the per-reply cap is reached: deliver the result
    /// as-is and leave the counter where it is, so the turn is never wedged.
    Capped,
}

/// Resolve the reflection policy for one batch of post-edit checks.
///
/// `any_dirty` is whether any edited file produced diagnostics this batch;
/// `reflections` is how many diagnostics injections have already happened this
/// reply; `cap` is [`PostEditDiagnosticsConfig::max_reflections`].
pub fn next_reflection(any_dirty: bool, reflections: u32, cap: u32) -> ReflectionOutcome {
    if !any_dirty {
        ReflectionOutcome::Reset
    } else if reflections < cap {
        ReflectionOutcome::Inject {
            next: reflections + 1,
        }
    } else {
        ReflectionOutcome::Capped
    }
}

/// One file's diagnostics, ready to render into the corrective message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiagnostics {
    /// Path as it should read to the model (usually the raw argument the model sent).
    pub path: String,
    /// One rendered `line L:C: message` per diagnostic.
    pub lines: Vec<String>,
}

/// Frame a batch of post-edit diagnostics as agent-visible corrective context.
///
/// The wrapper tag mirrors the hook-context framing so the model treats it as a
/// self-contained signal, and it is explicit that this is a *syntax* check (not a
/// type/lint pass) and that a suspected false positive should be noted and passed,
/// not looped on — the reflection cap is the hard backstop, this is the soft one.
pub fn frame_post_edit_diagnostics(files: &[FileDiagnostics]) -> String {
    let mut body = String::new();
    for file in files {
        body.push_str(&format!(
            "`{}` — {} syntax issue(s) after your edit:\n",
            file.path,
            file.lines.len()
        ));
        for line in &file.lines {
            body.push_str(&format!("  - {line}\n"));
        }
    }
    format!(
        "<post-edit-diagnostics>\n\
         An automatic syntax check ran on the file(s) you just edited and found problems. \
         This is a tree-sitter parse check \u{2014} it catches broken syntax (unbalanced \
         brackets, missing tokens), not type, import, or logic errors.\n\
         {body}\
         Fix these before continuing. If a flagged line is actually valid syntax (a false \
         positive), say so in one line and move on \u{2014} do not loop on it.\n\
         </post-edit-diagnostics>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(v: Value) -> serde_json::Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn write_command_yields_absolute_path() {
        let a = args(json!({"command": "write", "path": "/repo/src/main.rs", "file_text": "x"}));
        let p = edited_path_from_tool_call("developer__text_editor", &a, Path::new("/work"));
        assert_eq!(p, Some(PathBuf::from("/repo/src/main.rs")));
    }

    #[test]
    fn relative_path_resolves_against_working_dir() {
        let a = args(json!({"command": "str_replace", "path": "src/main.rs"}));
        let p = edited_path_from_tool_call("text_editor", &a, Path::new("/work"));
        assert_eq!(p, Some(PathBuf::from("/work/src/main.rs")));
    }

    #[test]
    fn file_path_alias_is_accepted() {
        let a = args(json!({"command": "insert", "file_path": "/repo/a.py"}));
        let p = edited_path_from_tool_call("developer__text_editor", &a, Path::new("/work"));
        assert_eq!(p, Some(PathBuf::from("/repo/a.py")));
    }

    #[test]
    fn view_and_undo_are_ignored() {
        for cmd in ["view", "undo_edit"] {
            let a = args(json!({"command": cmd, "path": "/repo/a.rs"}));
            assert_eq!(
                edited_path_from_tool_call("developer__text_editor", &a, Path::new("/work")),
                None,
                "command {cmd} should not trigger diagnostics"
            );
        }
    }

    #[test]
    fn non_text_editor_tool_is_ignored() {
        let a = args(json!({"command": "write", "path": "/repo/a.rs"}));
        assert_eq!(
            edited_path_from_tool_call("developer__shell", &a, Path::new("/work")),
            None
        );
    }

    #[test]
    fn empty_or_missing_path_is_ignored() {
        let a = args(json!({"command": "write", "path": "   "}));
        assert_eq!(
            edited_path_from_tool_call("text_editor", &a, Path::new("/work")),
            None
        );
        let a = args(json!({"command": "write"}));
        assert_eq!(
            edited_path_from_tool_call("text_editor", &a, Path::new("/work")),
            None
        );
    }

    #[test]
    fn disabled_config_is_inert() {
        let c = PostEditDiagnosticsConfig::disabled();
        assert!(!c.is_active());
        // Default is active.
        assert!(PostEditDiagnosticsConfig::default().is_active());
        // Zero reflections disables even when enabled.
        assert!(!PostEditDiagnosticsConfig {
            enabled: true,
            max_reflections: 0
        }
        .is_active());
    }

    #[test]
    fn reflection_policy_injects_resets_and_caps() {
        // Under the cap with a dirty batch -> inject and advance.
        assert_eq!(
            next_reflection(true, 0, 3),
            ReflectionOutcome::Inject { next: 1 }
        );
        assert_eq!(
            next_reflection(true, 2, 3),
            ReflectionOutcome::Inject { next: 3 }
        );
        // A clean batch resets, regardless of the running count.
        assert_eq!(next_reflection(false, 2, 3), ReflectionOutcome::Reset);
        // At/over the cap with a still-broken file -> capped, never wedged.
        assert_eq!(next_reflection(true, 3, 3), ReflectionOutcome::Capped);
        assert_eq!(next_reflection(true, 9, 3), ReflectionOutcome::Capped);
        // A zero cap never injects.
        assert_eq!(next_reflection(true, 0, 0), ReflectionOutcome::Capped);
    }

    #[test]
    fn framed_context_names_files_and_lines() {
        let framed = frame_post_edit_diagnostics(&[FileDiagnostics {
            path: "src/main.rs".to_string(),
            lines: vec!["line 3:5: missing `}`".to_string()],
        }]);
        assert!(framed.contains("<post-edit-diagnostics>"));
        assert!(framed.contains("`src/main.rs`"));
        assert!(framed.contains("line 3:5: missing `}`"));
        assert!(framed.contains("false positive"));
    }
}
