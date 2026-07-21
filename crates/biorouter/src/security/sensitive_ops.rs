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
//! This gate inspects three argument shapes on a tool call:
//!   1. the **path arguments of file-editor-style tool calls** (`text_editor`
//!      and `*_file` / `mkdir` / … tools) — the original behaviour;
//!   2. **shell command lines** (`developer/shell` and any command-bearing tool)
//!      — a redirect (`>` / `>>`) into a sensitive target, or the path argument
//!      of a mutating binary (`cp`, `mv`, `tee`, `install`, `Set-Content`, …);
//!   3. the **`code` body of `code_execution/execute_code`** — the opaque JS
//!      blob the default code-execution mode wraps every file op in. Its
//!      *inner* tool calls are dispatched straight through the extension
//!      manager and never reach an agent-layer inspector, so a sensitive write
//!      hidden inside a script would otherwise run with no prompt (the R2-01
//!      regression: `echo … >> ~/.ssh/config` executed silently in Auto mode).
//!      The body's string literals are scanned as embedded shell commands and,
//!      when a mutating editor command is present, as sensitive path targets.
//!
//! Reads are never escalated: only redirect targets, mutating-binary targets,
//! and mutating editor writes count. A `cat /etc/hosts` or a `view` of a
//! sensitive path yields nothing.
//!
//! Known gap (documented, not silently ignored): a target *dynamically*
//! constructed inside a script (`shell({command: \`… >> ${dir}/config\`})` with
//! `dir` computed at runtime) cannot be resolved by static scanning and is not
//! escalated. Gating the code-execution extension's *inner* dispatch boundary
//! (`code_execution_extension::run_tool_handler`) against the same sensitivity
//! check — with a deny, since that layer cannot surface an interactive ask — is
//! the recommended deeper follow-up.

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::security::command_text_from;
use crate::security::policy::command::{redirect_targets, ParsedCommand};
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

/// True for the benign character devices that live under `/dev` (a system dir)
/// but are ubiquitous, harmless write targets: `/dev/null` is the universal
/// bit-bucket, and `/dev/std{out,err,in}`, `/dev/tty`, `/dev/fd/N`, and the
/// zero/random generators are standard redirect destinations. Writing to any of
/// them mutates nothing on disk. Without this exemption a routine
/// `… 2>/dev/null` (or a `/dev/null` literal inside code authored by an
/// `execute_code` write) trips the Auto-mode sensitive-write escalation and
/// parks the agent on a permission prompt it should never see.
fn is_benign_device(norm: &str) -> bool {
    let p = norm.to_ascii_lowercase();
    matches!(
        p.as_str(),
        "/dev/null"
            | "/dev/zero"
            | "/dev/full"
            | "/dev/stdin"
            | "/dev/stdout"
            | "/dev/stderr"
            | "/dev/tty"
            | "/dev/random"
            | "/dev/urandom"
    ) || p.starts_with("/dev/fd/")
}

/// The human-facing reason a target is sensitive, or `None` if it is ordinary.
fn sensitivity_reason(tp: &TargetPath, env: &EnvFacts) -> Option<&'static str> {
    match classify(tp, env) {
        Blast::Root | Blast::WildcardAtRoot => {
            return Some("the filesystem root or a whole volume")
        }
        Blast::HomeBare => return Some("your home directory itself"),
        Blast::SystemDir => {
            if !is_temp_path(&tp.norm) && !is_benign_device(&tp.norm) {
                return Some("a protected system directory");
            }
            // A temp path under a system dir (e.g. /var/folders) or a benign
            // pseudo-device (/dev/null, /dev/stderr, …) is ordinary.
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

/// Shell binaries whose path arguments are *write* targets. POSIX basenames,
/// PowerShell canonical cmdlets (matched case-insensitively — [`ParsedCommand`]
/// normalizes `ri` → `Remove-Item`), and `cmd.exe` verbs. Read-only tools
/// (`cat`, `less`, `grep`, `ls`) are deliberately absent, so reading a sensitive
/// path is never escalated.
#[rustfmt::skip]
const SHELL_MUTATING_BINARIES: &[&str] = &[
    // POSIX
    "cp", "mv", "rm", "rmdir", "unlink", "mkdir", "touch", "tee", "install", "ln", "dd",
    "truncate", "shred", "chmod", "chown", "chgrp", "mkfifo", "mknod", "patch", "rsync",
    // PowerShell cmdlets (canonicalized by the parser)
    "set-content", "add-content", "out-file", "new-item", "copy-item", "move-item",
    "remove-item", "clear-content", "rename-item",
    // cmd.exe
    "copy", "move", "del", "erase", "md", "rd", "ren", "rename", "xcopy", "robocopy",
];

/// Tokens whose presence in an `execute_code` body marks it as containing a
/// mutating editor call (`text_editor({command:"write"…})`), gating the
/// path-literal scan so a `view`-only script that merely *reads* a sensitive
/// path is not escalated.
#[rustfmt::skip]
const EDITOR_WRITE_INDICATORS: &[&str] =
    &["write", "create", "str_replace", "insert", "append", "delete"];

/// True for the code-execution `execute_code` tool, whose opaque JS body carries
/// the real (inner) tool calls and must be scanned for embedded sensitive writes.
fn is_execute_code(tool_name: &str) -> bool {
    tool_name.to_ascii_lowercase().contains("execute_code")
}

/// A sensitive **write** in a shell command line: any redirect target (`>` /
/// `>>`), or the path argument of a mutating binary. Returns the normalized
/// target path + reason, or `None` for a read.
fn command_writes_sensitively(command: &str, env: &EnvFacts) -> Option<(String, &'static str)> {
    // Redirects are unconditional writes, wherever in the line they appear.
    for rt in redirect_targets(command) {
        let tp = normalize_for(env.platform, &rt, env);
        if let Some(reason) = sensitivity_reason(&tp, env) {
            return Some((tp.norm, reason));
        }
    }
    // Mutating binaries: their (already classified) path/redirect targets.
    let parsed = ParsedCommand::parse_for(command, env.platform, env);
    for seg in &parsed.segments {
        if !SHELL_MUTATING_BINARIES.contains(&seg.binary.to_ascii_lowercase().as_str()) {
            continue;
        }
        for hit in &seg.targets {
            if let Some(reason) = sensitivity_reason(&hit.path, env) {
                return Some((hit.path.norm.clone(), reason));
            }
        }
    }
    None
}

/// Extract the raw inner text of every JS string / template literal in a script.
/// Escapes are kept verbatim (a `\n` stays two characters, never a real newline)
/// so an embedded path token is preserved exactly. Best-effort: interpolation
/// (`${…}`) and other dynamic construction are not resolved (the documented gap).
fn extract_string_literals(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            i += 1;
            let mut cur = String::new();
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\\' && i + 1 < chars.len() {
                    cur.push(ch);
                    cur.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if ch == quote {
                    i += 1;
                    break;
                }
                cur.push(ch);
                i += 1;
            }
            out.push(cur);
        } else {
            i += 1;
        }
    }
    out
}

/// Scan an `execute_code` JS body for a sensitive write: every string literal is
/// tried as an embedded shell command, and — when the script contains a mutating
/// editor command — as a sensitive path target for a `text_editor` write.
fn code_writes_sensitively(code: &str, env: &EnvFacts) -> Option<(String, &'static str)> {
    let literals = extract_string_literals(code);
    for lit in &literals {
        if let Some(hit) = command_writes_sensitively(lit, env) {
            return Some(hit);
        }
    }
    let lc = code.to_ascii_lowercase();
    if EDITOR_WRITE_INDICATORS.iter().any(|w| lc.contains(w)) {
        for lit in &literals {
            if lit.trim().is_empty() {
                continue;
            }
            let tp = normalize_for(env.platform, lit, env);
            if let Some(reason) = sensitivity_reason(&tp, env) {
                return Some((tp.norm, reason));
            }
        }
    }
    None
}

/// Classify a tool call: `Some(reason)` when it is an extremely sensitive
/// system operation that must be approved even in Auto mode; `None` otherwise.
///
/// Inspects three shapes (see the module boundary docs): file-editor path
/// arguments, shell command lines, and the `execute_code` JS body. Pure over its
/// inputs except for reading the host environment (`$HOME`, cwd) via
/// [`EnvFacts::host`]; path canonicalization never touches the filesystem.
pub fn sensitive_file_operation(
    tool_name: &str,
    args: &Map<String, Value>,
    working_dir: &Path,
) -> Option<String> {
    let env = EnvFacts::host(&working_dir.to_string_lossy());

    // 1. File-editor / file-tool path arguments (mutations only).
    if operation_is_mutating(tool_name, args) {
        for raw in path_values(args) {
            if raw.trim().is_empty() {
                continue;
            }
            let tp = normalize_for(env.platform, &raw, &env);
            if let Some(reason) = sensitivity_reason(&tp, &env) {
                return Some(format!("writes to {} ({reason})", tp.norm));
            }
        }
    }

    // 2. Shell command lines (developer/shell and any command-bearing tool).
    if let Some(command) = command_text_from(tool_name, args) {
        if let Some((path, reason)) = command_writes_sensitively(&command, &env) {
            return Some(format!("writes to {path} ({reason})"));
        }
    }

    // 3. code_execution/execute_code JS body — its inner tool calls bypass every
    //    agent-layer inspector, so scan the script itself.
    if is_execute_code(tool_name) {
        if let Some(code) = args.get("code").and_then(Value::as_str) {
            if let Some((path, reason)) = code_writes_sensitively(code, &env) {
                return Some(format!("writes to {path} ({reason})"));
            }
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

    // --- shell command lines (R2-01) --------------------------------------

    #[test]
    fn shell_redirect_into_ssh_config_is_flagged() {
        let env = nix_env();
        // The exact demonstrated exploit: a silent append to ~/.ssh/config.
        let hit = command_writes_sensitively("echo 'Host x' >> ~/.ssh/config", &env);
        assert!(
            hit.is_some(),
            "append-redirect into ~/.ssh/config must be flagged"
        );
    }

    #[test]
    fn shell_mutating_binary_into_sensitive_target_is_flagged() {
        let env = nix_env();
        for cmd in [
            "cp ./key /etc/cron.d/backdoor",
            "tee /etc/hosts",
            "install -m 600 k ~/.ssh/authorized_keys",
            "mv ./x ~/.aws/credentials",
        ] {
            assert!(
                command_writes_sensitively(cmd, &env).is_some(),
                "{cmd} must be flagged"
            );
        }
    }

    #[test]
    fn shell_reads_of_sensitive_paths_are_not_flagged() {
        let env = nix_env();
        for cmd in [
            "cat /etc/hosts",
            "less ~/.ssh/config",
            "grep x /etc/passwd",
            "ls -la /etc",
        ] {
            assert!(
                command_writes_sensitively(cmd, &env).is_none(),
                "{cmd} is a read, must not be flagged"
            );
        }
    }

    #[test]
    fn shell_ordinary_writes_are_not_flagged() {
        let env = nix_env();
        for cmd in [
            "echo hi > /tmp/x",
            "cp a /home/me/Downloads/b",
            "tee /home/me/proj/out.txt",
        ] {
            assert!(
                command_writes_sensitively(cmd, &env).is_none(),
                "{cmd} must not be flagged"
            );
        }
    }

    /// Redirects to `/dev/null` (and the other benign character devices) are the
    /// universal bit-bucket — never a sensitive write. Regression: a routine
    /// `… 2>/dev/null` used to trip the `/dev` system-dir escalation and park the
    /// agent on an approval prompt even in Auto mode.
    #[test]
    fn shell_redirect_to_dev_null_is_not_flagged() {
        let env = nix_env();
        for cmd in [
            "find landing -type f 2>/dev/null | sort",
            "git status --short 2>/dev/null",
            "grep -r foo . >/dev/null 2>&1",
            "make >/dev/null",
            "cat huge.log > /dev/null",
        ] {
            assert!(
                command_writes_sensitively(cmd, &env).is_none(),
                "{cmd} redirects to /dev/null and must NOT be flagged"
            );
        }
    }

    #[test]
    fn dev_null_and_std_streams_are_not_sensitive_editor_targets() {
        for path in [
            "/dev/null",
            "/dev/stdout",
            "/dev/stderr",
            "/dev/tty",
            "/dev/fd/2",
        ] {
            let finding = sensitive_file_operation(
                "developer__text_editor",
                &args(json!({ "command": "write", "path": path })),
                Path::new("/home/me/proj"),
            );
            assert!(
                finding.is_none(),
                "a write target of {path} (a benign device) must NOT be flagged"
            );
        }
    }

    /// A real `/etc` write must still be caught — the exemption is scoped to the
    /// benign devices only, not all of `/dev` or the rest of the system tree.
    #[test]
    fn dev_null_exemption_does_not_leak_to_other_system_paths() {
        let env = nix_env();
        assert!(
            command_writes_sensitively("echo x > /dev/sda", &env).is_some(),
            "a write to a real block device must still be flagged"
        );
        assert!(
            command_writes_sensitively("echo x > /etc/passwd", &env).is_some(),
            "a write to /etc must still be flagged"
        );
    }

    /// The observed drive stall: an `execute_code` body whose only sensitive-looking
    /// token is a `2>/dev/null` redirect (either live or embedded in authored code)
    /// must run without an approval prompt.
    #[test]
    fn execute_code_with_dev_null_redirect_is_not_flagged() {
        let env = nix_env();
        let code = r#"import { shell, text_editor } from "developer";
const repo = "/home/me/proj";
const status = shell({ command: `cd ${repo} && git status --short && find landing -type f -print 2>/dev/null | sort` });
record_result({ status });"#;
        assert!(
            code_writes_sensitively(code, &env).is_none(),
            "an execute_code body whose only /dev reference is 2>/dev/null must NOT be flagged"
        );
    }

    // --- execute_code bodies (R2-01) --------------------------------------

    #[test]
    fn execute_code_embedded_ssh_write_is_flagged() {
        let env = nix_env();
        let code = r#"import { shell } from "developer";
shell({ command: `printf 'Host evil\n' >> ~/.ssh/config` });
record_result("done");"#;
        assert!(
            code_writes_sensitively(code, &env).is_some(),
            "an embedded shell redirect into ~/.ssh/config must be flagged"
        );
    }

    #[test]
    fn execute_code_editor_write_to_sensitive_path_is_flagged() {
        let env = nix_env();
        let code = r#"import { text_editor } from "developer";
text_editor({ command: "write", path: "~/.ssh/authorized_keys", file_text: "ssh-rsa AAAA" });"#;
        assert!(
            code_writes_sensitively(code, &env).is_some(),
            "an embedded text_editor write to ~/.ssh must be flagged"
        );
    }

    #[test]
    fn execute_code_view_of_sensitive_path_is_not_flagged() {
        let env = nix_env();
        let code = r#"import { text_editor } from "developer";
const c = text_editor({ command: "view", path: "/etc/hosts" });
record_result(c);"#;
        assert!(
            code_writes_sensitively(code, &env).is_none(),
            "a view of /etc/hosts must not be flagged"
        );
    }

    #[test]
    fn execute_code_markdown_angle_bracket_placeholder_is_not_flagged() {
        // Regression (BIOOKF-I1-DEFECT-1): a text_editor write whose file_text is a
        // markdown doc containing an angle-bracket placeholder line like
        // `knowledge/<type>/<slug>.md` must NOT be read as a shell redirect to `/`.
        // The `>` of `<type>` was captured by redirect_targets, yielding target `/`
        // → Blast::Root → a spurious "writes to / (the filesystem root or a whole
        // volume)" approval prompt on an ordinary /tmp write.
        let env = nix_env();
        let code = r#"import { text_editor } from "developer";
text_editor({ command: "write", path: "/tmp/biookf-rebuild/SPEC.md", file_text: `# BioOKF SPEC

Directory layout:

  raw/
  knowledge/
    <type>/
      <slug>.md
  index.md
  log.md

Every concept doc's type is one of the 28.
` });
record_result("done");"#;
        assert!(
            code_writes_sensitively(code, &env).is_none(),
            "markdown prose with <type>/<slug> placeholders must not be flagged as a root write"
        );
    }

    #[test]
    fn execute_code_ordinary_work_is_not_flagged() {
        let env = nix_env();
        let code = r#"import { shell, text_editor } from "developer";
const files = shell({ command: "ls -la /tmp" });
text_editor({ command: "write", path: "/tmp/out.txt", file_text: files });
record_result("ok");"#;
        assert!(
            code_writes_sensitively(code, &env).is_none(),
            "ordinary /tmp scratch work must not be flagged"
        );
    }

    /// End-to-end at the inspector: a `developer/shell` sensitive write in Auto
    /// mode is routed to approval (uses `/etc`, a host system dir, so the fixture
    /// is deterministic regardless of `$HOME`).
    #[tokio::test]
    async fn auto_mode_routes_shell_sensitive_write_to_approval() {
        let inspector = SensitiveOpsInspector;
        let req = ToolRequest {
            id: "req_shell".to_string(),
            tool_call: Ok(CallToolRequestParams {
                task: None,
                name: "developer__shell".into(),
                arguments: Some(args(json!({ "command": "cp ./k /etc/cron.d/backdoor" }))),
                meta: None,
            }),
            metadata: None,
            tool_meta: None,
        };
        let results = inspector
            .inspect(
                &[req],
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();
        assert!(
            results.iter().any(|r| r.tool_request_id == "req_shell"
                && matches!(r.action, InspectionAction::RequireApproval(_))),
            "shell write to /etc must be routed to approval, got {results:?}"
        );
    }

    /// End-to-end: an `execute_code` script that hides a sensitive shell write in
    /// its body is routed to approval in Auto mode (the R2-01 gate).
    #[tokio::test]
    async fn auto_mode_routes_execute_code_sensitive_write_to_approval() {
        let inspector = SensitiveOpsInspector;
        let req = ToolRequest {
            id: "req_code".to_string(),
            tool_call: Ok(CallToolRequestParams {
                task: None,
                name: "code_execution__execute_code".into(),
                arguments: Some(args(json!({
                    "code": "import { shell } from \"developer\";\nshell({ command: \"echo pwn >> /etc/cron.d/x\" });"
                }))),
                meta: None,
            }),
            metadata: None,
            tool_meta: None,
        };
        let results = inspector
            .inspect(
                &[req],
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();
        assert!(
            results.iter().any(|r| r.tool_request_id == "req_code"
                && matches!(r.action, InspectionAction::RequireApproval(_))),
            "execute_code hiding a sensitive write must be routed to approval, got {results:?}"
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
