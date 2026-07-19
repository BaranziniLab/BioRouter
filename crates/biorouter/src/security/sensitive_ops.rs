//! Fully-Automatic-mode sensitive-operation gate (owner Directive 2).
//!
//! # Policy (an explicit product decision — the single auditable list lives here)
//!
//! In [`BioRouterMode::Auto`] ("Fully Automatic") the agent and its extensions
//! may **create, read, edit and delete files anywhere on the user's machine
//! WITHOUT a prompt**. The one exception is a small, explicit set of *extremely
//! sensitive system operations*, which are routed through the **same** approval
//! flow every other prompt already uses — an [`InspectionAction::RequireApproval`]
//! that the agent surfaces with the confirmation TTL in
//! [`crate::agents::tool_execution`]. This is an **ask**, never a new hard block.
//!
//! What this gate does NOT change:
//!   * The always-on catastrophic-command denylist (BR-20) and the auditable
//!     command policy engine (BR-21) remain the floor — a `rm -rf /` is still a
//!     non-bypassable *deny*, not merely an ask, in every mode.
//!   * `.biorouterignore` / `SecretGuard` stays **absolute and mode-independent**:
//!     it denies reads/writes of user-declared secrets at the extension-manager
//!     dispatch boundary, and this gate never relaxes it.
//!   * Every mode other than `Auto`. The inspector is **inert** outside `Auto`
//!     (returns no results), because those modes already gate these operations:
//!     `Approve` prompts for everything, `SmartApprove` grades a write as
//!     destructive, and `Chat` runs no tools. Non-Auto behaviour is therefore
//!     provably unchanged — the whole gate is one early return away from a no-op.
//!
//! # What counts as "extremely sensitive" (the single list)
//!
//! A **mutating** file operation (create / write / edit / delete / move-onto —
//! never a read/`view`) whose target path is any of:
//!   1. the filesystem root or a whole volume ([`Blast::Root`] /
//!      [`Blast::WildcardAtRoot`]);
//!   2. the bare home directory itself ([`Blast::HomeBare`]);
//!   3. a protected system directory ([`Blast::SystemDir`]) — `/System`, `/usr`,
//!      `/bin`, `/sbin`, `/etc`, `/Library`, `/boot`, a Windows system root, …
//!      **except** the OS temp trees (`/tmp`, `/var/folders/**`, `$TMPDIR`),
//!      which live under a system dir but are ordinary scratch space;
//!   4. a credential / persistence location under `$HOME` that classifies as
//!      *ordinary* for the blast rule but is a known secret store — SSH keys,
//!      GPG/AWS/gcloud/kube/docker creds, macOS keychains, launchd agents, and
//!      browser credential stores (see [`SENSITIVE_HOME_SUBPATHS`]).
//!
//! Reads are intentionally out of scope here: secret-file reads are already
//! denied by `SecretGuard`. Only mutations escalate, so that ordinary Auto-mode
//! work — writing scratch files, editing the workspace, deleting build output —
//! keeps running with no prompt.
//!
//! # Boundary (documented for security review)
//!
//! This gate inspects the **path arguments of file-editor-style tool calls**
//! (`text_editor` and `*_file` / `mkdir` / … tools). It does **not** parse
//! arbitrary shell command lines — those are already governed by the command
//! policy engine (system-dir destructive shapes are *denied*) and by
//! `SecretGuard` (secret-file access is *denied*). A shell write to a non-secret
//! user-credential path (e.g. `cp key ~/.ssh/authorized_keys`) is the one shape
//! neither this gate nor the policy engine currently escalates; extending the
//! policy engine's `ask` rules to those destinations is the recommended
//! follow-up.

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::security::policy::target::{classify, normalize_for, Blast, EnvFacts, TargetPath};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

/// Object-argument keys whose string (or string-array) value is a filesystem
/// path we should classify. Also matched: any key ending in `_path`, and the
/// plural `paths`.
const PATH_ARG_KEYS: &[&str] = &[
    "path",
    "file_path",
    "filepath",
    "filename",
    "file",
    "target",
    "target_path",
    "dest",
    "destination",
    "output_path",
    "output",
    "to",
    "new_path",
];

/// `text_editor`-style `command` values that MUTATE the target (everything but
/// `view`). An editor call with no explicit command fails safe → treated as a
/// mutation (so it is checked, not silently allowed).
const MUTATING_EDITOR_COMMANDS: &[&str] = &[
    "write",
    "create",
    "str_replace",
    "insert",
    "undo_edit",
    "diff",
    "delete",
    "append",
];

/// Tool-name substrings that mark a non-editor tool as a file mutation. Only
/// consulted together with a sensitive target path, so an over-broad match here
/// cannot escalate an ordinary-path call.
const MUTATING_NAME_HINTS: &[&str] = &[
    "write",
    "create",
    "edit",
    "delete",
    "remove",
    "append",
    "save",
    "move",
    "rename",
    "mkdir",
    "put",
    "unlink",
    "rmdir",
    "overwrite",
    "patch",
    "copy",
];

/// Tool-name substrings that mark a tool as read-only, vetoing a mutating-hint
/// match (belt-and-suspenders: the hint sets do not overlap).
const READONLY_NAME_HINTS: &[&str] = &["view", "read", "list", "search", "preview", "get_"];

/// Credential / persistence directories under `$HOME`. Stored lowercased and
/// `/`-separated (relative to home); a mutation at or beneath any of these is
/// sensitive even though it lives under `$HOME` (so it is [`Blast::Ordinary`]).
const SENSITIVE_HOME_SUBPATHS: &[&str] = &[
    ".ssh",           // SSH private keys, authorized_keys, config
    ".gnupg",         // GPG keyring
    ".aws",           // AWS credentials/config
    ".config/gcloud", // Google Cloud creds
    ".kube",          // Kubernetes creds
    ".docker",        // Docker registry auth
    // macOS
    "library/keychains",                         // user keychain
    "library/launchagents",                      // per-user launchd persistence
    "library/preferences/com.apple.loginwindow", // login hooks
    "library/application support/google/chrome",
    "library/application support/chromium",
    "library/application support/bravesoftware",
    "library/application support/firefox",
    "library/application support/microsoft edge",
    // Linux
    ".config/google-chrome",
    ".config/chromium",
    ".config/bravesoftware",
    ".mozilla",
    // Windows (%LOCALAPPDATA% / %APPDATA% expand under home for the classifier)
    "appdata/local/google/chrome",
    "appdata/roaming/mozilla",
];

/// True for a normalized path inside an OS temp tree. macOS `$TMPDIR` lives
/// under `/var/folders/**` (a system dir), and `/tmp` is `/private/tmp`; agents
/// write scratch files there constantly, so a temp target is never sensitive.
fn is_temp_path(norm: &str) -> bool {
    let p = norm.to_ascii_lowercase();
    if p == "/tmp" || p == "/private/tmp" || p == "/var/tmp" || p == "/private/var/tmp" {
        return true;
    }
    const TEMP_PREFIXES: &[&str] = &[
        "/tmp/",
        "/private/tmp/",
        "/var/folders/",
        "/private/var/folders/",
        "/var/tmp/",
        "/private/var/tmp/",
    ];
    if TEMP_PREFIXES.iter().any(|t| p.starts_with(t)) {
        return true;
    }
    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        let t = tmpdir.trim_end_matches('/').to_ascii_lowercase();
        if !t.is_empty() && (p == t || p.starts_with(&format!("{t}/"))) {
            return true;
        }
    }
    false
}

/// The human-facing reason a target is sensitive, or `None` if it is ordinary.
fn sensitivity_reason(tp: &TargetPath, env: &EnvFacts) -> Option<&'static str> {
    match classify(tp, env) {
        Blast::Root | Blast::WildcardAtRoot => {
            return Some("the filesystem root or a whole volume")
        }
        Blast::HomeBare => return Some("your home directory itself"),
        Blast::SystemDir => {
            if !is_temp_path(&tp.norm) {
                return Some("a protected system directory");
            }
            // A temp path under a system dir (e.g. /var/folders) is ordinary.
        }
        Blast::Ordinary => {}
    }

    // Credential / persistence locations under $HOME (Blast::Ordinary above).
    if !env.home.is_empty() {
        let p = tp.norm.to_ascii_lowercase();
        let home = normalize_for(env.platform, &env.home, env)
            .norm
            .to_ascii_lowercase();
        if !home.is_empty() {
            if let Some(rel) = p.strip_prefix(&format!("{home}/")) {
                for sub in SENSITIVE_HOME_SUBPATHS {
                    if rel == *sub || rel.starts_with(&format!("{sub}/")) {
                        return Some("a credential or persistence path in your home directory");
                    }
                }
            }
        }
    }
    None
}

/// Whether this tool call is a file **mutation** (create/write/edit/delete/…),
/// as opposed to a read/`view`. Editor tools carry the operation in `command`;
/// other file tools are inferred from the tool name.
fn operation_is_mutating(tool_name: &str, args: &Map<String, Value>) -> bool {
    let name = tool_name.to_ascii_lowercase();
    let is_editor = name.contains("text_editor") || name.contains("str_replace");
    if is_editor {
        return match args.get("command").and_then(Value::as_str) {
            Some(cmd) => {
                MUTATING_EDITOR_COMMANDS.contains(&cmd.trim().to_ascii_lowercase().as_str())
            }
            None => true, // no explicit command → fail safe (check it)
        };
    }
    if READONLY_NAME_HINTS.iter().any(|h| name.contains(h)) {
        return false;
    }
    MUTATING_NAME_HINTS.iter().any(|h| name.contains(h))
}

/// Every path-like value in the argument map (string or array-of-strings).
fn path_values(args: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for (key, value) in args {
        let k = key.to_ascii_lowercase();
        let is_path_key =
            PATH_ARG_KEYS.iter().any(|pk| k == *pk) || k.ends_with("_path") || k == "paths";
        if !is_path_key {
            continue;
        }
        match value {
            Value::String(s) => out.push(s.clone()),
            Value::Array(items) => {
                for item in items {
                    if let Some(s) = item.as_str() {
                        out.push(s.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Classify a tool call: `Some(reason)` when it is an extremely sensitive
/// system operation that must be approved even in Auto mode; `None` otherwise.
///
/// Pure over its inputs (path canonicalization never touches the filesystem —
/// see [`normalize_for`]), so it is unit-testable without a live agent.
pub fn sensitive_file_operation(
    tool_name: &str,
    args: &Map<String, Value>,
    working_dir: &Path,
) -> Option<String> {
    if !operation_is_mutating(tool_name, args) {
        return None;
    }
    let env = EnvFacts::host(&working_dir.to_string_lossy());
    for raw in path_values(args) {
        if raw.trim().is_empty() {
            continue;
        }
        let tp = normalize_for(env.platform, &raw, &env);
        if let Some(reason) = sensitivity_reason(&tp, &env) {
            return Some(format!("writes to {} ({reason})", tp.norm));
        }
    }
    None
}

/// Inspector that, **in Auto mode only**, escalates the sensitive-operation set
/// to the standard approval flow. See the module docs for the policy.
pub struct SensitiveOpsInspector;

#[async_trait]
impl ToolInspector for SensitiveOpsInspector {
    fn name(&self) -> &'static str {
        "sensitive_ops"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        biorouter_mode: BioRouterMode,
        session: &crate::session::Session,
    ) -> Result<Vec<InspectionResult>> {
        // Inert outside Auto — every other mode already gates these operations,
        // so this keeps non-Auto behaviour provably unchanged (one early return).
        if biorouter_mode != BioRouterMode::Auto {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        for request in tool_requests {
            let Ok(tool_call) = &request.tool_call else {
                continue;
            };
            let Some(args) = tool_call.arguments.as_ref() else {
                continue;
            };
            if let Some(reason) =
                sensitive_file_operation(&tool_call.name, args, &session.working_dir)
            {
                tracing::warn!(
                    counter.biorouter.sensitive_op_escalated = 1,
                    tool_name = %tool_call.name,
                    tool_request_id = %request.id,
                    "Sensitive file operation escalated to approval in Auto mode"
                );
                results.push(InspectionResult {
                    tool_request_id: request.id.clone(),
                    action: InspectionAction::RequireApproval(Some(format!(
                        "🔒 Sensitive system operation in Fully-Automatic mode.\n\
                         This tool call {reason}.\n\
                         Approve it to continue, or deny it. Ordinary file changes \
                         run without a prompt in this mode."
                    ))),
                    reason: format!("Sensitive file operation ({reason})"),
                    confidence: 1.0,
                    inspector_name: self.name().to_string(),
                    finding_id: Some(format!("SENS-{}", Uuid::new_v4().simple())),
                });
            }
        }
        Ok(results)
    }

    // `is_enabled` uses the trait default (always registered): the mode gate
    // lives in `inspect`, so a mid-session mode change is honoured without
    // re-plumbing the inspector list.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::policy::target::Platform;
    use serde_json::json;

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn nix_env() -> EnvFacts {
        EnvFacts::for_platform(Platform::Linux, "/home/me/proj", "/home/me")
    }

    // --- path classification (pure) ---------------------------------------

    #[test]
    fn system_dirs_are_sensitive() {
        let env = nix_env();
        for raw in ["/etc/passwd", "/usr/local/bin/x", "/boot/grub", "/"] {
            let tp = normalize_for(Platform::Linux, raw, &env);
            assert!(
                sensitivity_reason(&tp, &env).is_some(),
                "{raw} must be sensitive"
            );
        }
    }

    #[test]
    fn macos_system_and_credential_dirs_are_sensitive() {
        let env = EnvFacts::for_platform(Platform::Macos, "/Users/me/proj", "/Users/me");
        for raw in [
            "/Library/LaunchDaemons/evil.plist",
            "/System/Library/x",
            "~/.ssh/authorized_keys",
            "~/Library/Keychains/login.keychain-db",
            "~/Library/Application Support/Google/Chrome/Default/Login Data",
            "~/.aws/credentials",
        ] {
            let tp = normalize_for(Platform::Macos, raw, &env);
            assert!(
                sensitivity_reason(&tp, &env).is_some(),
                "{raw} must be sensitive (norm={})",
                tp.norm
            );
        }
    }

    #[test]
    fn ordinary_and_temp_targets_are_not_sensitive() {
        let env = nix_env();
        for raw in [
            "/home/me/proj/out.csv",
            "~/Downloads/report.pdf",
            "/home/me/Documents/notes.txt",
            "./build/index.html",
            "/tmp/scratch.txt",
            "/var/folders/ab/T/tmp123/x.bin", // macOS TMPDIR shape, still ordinary
        ] {
            let tp = normalize_for(Platform::Linux, raw, &env);
            assert!(
                sensitivity_reason(&tp, &env).is_none(),
                "{raw} must be ordinary (norm={})",
                tp.norm
            );
        }
    }

    // --- mutation detection -----------------------------------------------

    #[test]
    fn editor_view_is_not_a_mutation_but_writes_are() {
        assert!(!operation_is_mutating(
            "developer__text_editor",
            &args(json!({"command": "view", "path": "/etc/hosts"}))
        ));
        for cmd in ["write", "str_replace", "insert", "undo_edit"] {
            assert!(
                operation_is_mutating(
                    "developer__text_editor",
                    &args(json!({"command": cmd, "path": "/etc/hosts"}))
                ),
                "{cmd} is a mutation"
            );
        }
    }

    #[test]
    fn file_tool_names_classify_by_verb() {
        assert!(operation_is_mutating(
            "files__write_file",
            &args(json!({"path": "x"}))
        ));
        assert!(operation_is_mutating(
            "files__delete_file",
            &args(json!({"path": "x"}))
        ));
        assert!(!operation_is_mutating(
            "files__read_file",
            &args(json!({"path": "x"}))
        ));
        assert!(!operation_is_mutating(
            "files__list_dir",
            &args(json!({"path": "x"}))
        ));
    }

    // --- the public entry point -------------------------------------------

    #[test]
    fn editor_write_to_system_dir_is_flagged() {
        let finding = sensitive_file_operation(
            "developer__text_editor",
            &args(json!({"command": "write", "path": "/etc/cron.d/backdoor"})),
            Path::new("/home/me/proj"),
        );
        assert!(finding.is_some(), "write to /etc must be flagged");
    }

    #[test]
    fn editor_view_of_system_dir_is_not_flagged() {
        let finding = sensitive_file_operation(
            "developer__text_editor",
            &args(json!({"command": "view", "path": "/etc/hosts"})),
            Path::new("/home/me/proj"),
        );
        assert!(finding.is_none(), "reading /etc must NOT be flagged");
    }

    #[test]
    fn editor_write_to_ordinary_path_is_not_flagged() {
        let finding = sensitive_file_operation(
            "developer__text_editor",
            &args(json!({"command": "write", "path": "/tmp/qa/hi.txt"})),
            Path::new("/home/me/proj"),
        );
        assert!(
            finding.is_none(),
            "an ordinary tmp write must not be flagged"
        );
    }

    // --- the inspector, end to end (the directive's gates) -----------------

    use crate::conversation::message::ToolRequest;
    use rmcp::model::CallToolRequestParams;

    fn write_request(id: &str, path: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams {
                task: None,
                name: "developer__text_editor".into(),
                arguments: Some(args(json!({ "command": "write", "path": path }))),
                meta: None,
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    /// Gate (b): in Auto mode a sensitive-path write is routed to approval.
    #[tokio::test]
    async fn auto_mode_routes_sensitive_write_to_approval() {
        let inspector = SensitiveOpsInspector;
        let requests = vec![write_request("req_sys", "/etc/cron.d/backdoor")];
        let results = inspector
            .inspect(
                &requests,
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();
        let r = results
            .iter()
            .find(|r| r.tool_request_id == "req_sys")
            .expect("sensitive write should produce a result");
        assert!(matches!(r.action, InspectionAction::RequireApproval(_)));
        assert_eq!(r.inspector_name, "sensitive_ops");
        assert!(r.finding_id.as_deref().unwrap_or("").starts_with("SENS-"));
    }

    /// Gate (a): in Auto mode an ordinary out-of-workspace write yields NO
    /// escalation, so the permission inspector's Auto `Allow` stands (no prompt).
    #[tokio::test]
    async fn auto_mode_leaves_ordinary_write_unescalated() {
        let inspector = SensitiveOpsInspector;
        // Absolute, outside the session working dir, but ordinary.
        let requests = vec![write_request("req_ok", "/tmp/qa-r1b/hi.txt")];
        let results = inspector
            .inspect(
                &requests,
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "an ordinary write must not be escalated, got {results:?}"
        );
    }

    /// Gate (c): every non-Auto mode is inert — even a sensitive write yields no
    /// result, so those modes' existing behaviour is provably unchanged.
    #[tokio::test]
    async fn non_auto_modes_are_inert() {
        let inspector = SensitiveOpsInspector;
        let requests = vec![write_request("req_sys", "/etc/cron.d/backdoor")];
        for mode in [
            BioRouterMode::Approve,
            BioRouterMode::SmartApprove,
            BioRouterMode::Chat,
        ] {
            let results = inspector
                .inspect(&requests, &[], mode, &crate::session::Session::default())
                .await
                .unwrap();
            assert!(
                results.is_empty(),
                "{mode:?} must be inert (no sensitive-ops result), got {results:?}"
            );
        }
    }
}
