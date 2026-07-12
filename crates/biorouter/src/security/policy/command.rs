//! Argv parsing + path canonicalization for the command policy engine.
//!
//! The old regex scanner matched patterns against the raw `Tool: … / args`
//! blob, so it was trivially evadable (`/usr/bin/env rm -rf /`, quote-splice,
//! `curl | bash` inside a subshell). `ParsedCommand` instead tokenizes argv,
//! unwraps `sudo`/`env`/`sh -c` wrappers, splits pipelines/subshells, and
//! canonicalizes path arguments against the session cwd, so a policy rule sees
//! the *resolved* binary and target rather than a string shape.

use std::path::{Component, Path, PathBuf};

/// Wrappers whose leading occurrence is peeled away to reveal the real binary,
/// e.g. `sudo env FOO=1 rm …` -> `rm …`. Kept deliberately small.
const WRAPPER_BINARIES: &[&str] = &[
    "sudo", "env", "command", "exec", "nice", "nohup", "time", "stdbuf", "setsid", "doas", "xargs",
];

/// Interpreters treated as "a shell" for `pipes_to_shell` / `sh -c` unwrapping.
const SHELL_BINARIES: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "csh", "tcsh", "ksh", "ash",
];

/// Depth cap so a maliciously nested `sh -c "sh -c …"` cannot recurse forever.
const MAX_UNWRAP_DEPTH: u8 = 4;

/// One pipeline stage / subshell command after tokenization + wrapper unwrap.
#[derive(Debug, Clone)]
pub struct Segment {
    /// Resolved `argv[0]` basename (`/usr/bin/rm` -> `rm`).
    pub binary: String,
    /// Full token list (post wrapper-unwrap), for `arg_regex`/prefix matching.
    pub argv: Vec<String>,
    /// Canonicalized path arguments (relative paths resolved against cwd).
    pub paths: Vec<PathBuf>,
}

impl Segment {
    /// A normalized `binary arg1 arg2 …` line for `command_prefix` matching.
    pub fn command_line(&self) -> String {
        self.argv.join(" ")
    }
}

/// A parsed command: the raw text plus every decomposed segment.
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub raw: String,
    pub segments: Vec<Segment>,
    /// A stage pipes into (or is) an interpreter shell — the `curl | bash` shape.
    pub reads_shell: bool,
}

impl ParsedCommand {
    /// Tokenize + `sudo`/`env`/`sh -c` unwrap + pipeline/subshell split + path
    /// canonicalization against `cwd`. Best-effort: an un-tokenizable stage
    /// falls back to whitespace splitting, and an empty parse yields a single
    /// raw segment so `arg_regex` deny rules still see the text.
    pub fn parse(command: &str, cwd: &Path) -> ParsedCommand {
        let mut segments = Vec::new();
        let mut reads_shell = false;
        parse_recursive(command, cwd, 0, &mut segments, &mut reads_shell);

        if segments.is_empty() {
            segments.push(Segment {
                binary: String::new(),
                argv: command.split_whitespace().map(String::from).collect(),
                paths: Vec::new(),
            });
        }

        ParsedCommand {
            raw: command.to_string(),
            segments,
            reads_shell,
        }
    }
}

fn is_shell_binary(name: &str) -> bool {
    SHELL_BINARIES.contains(&name)
}

/// Top-level operator following a segment (quote-aware split classifies these).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sep {
    Pipe,
    Other,
}

/// Split a command string into `(segment_text, following_sep)` respecting single
/// and double quotes. `&&`, `||`, `;`, `&`, newlines are command separators;
/// a lone `|` is a pipe (stays inside the same command).
fn split_operators(text: &str) -> Vec<(String, Sep)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_single {
            cur.push(c);
            if c == '\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            cur.push(c);
            if c == '"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                in_single = true;
                cur.push(c);
            }
            '"' => {
                in_double = true;
                cur.push(c);
            }
            '|' => {
                if chars.get(i + 1) == Some(&'|') {
                    out.push((std::mem::take(&mut cur), Sep::Other));
                    i += 1;
                } else {
                    out.push((std::mem::take(&mut cur), Sep::Pipe));
                }
            }
            '&' => {
                out.push((std::mem::take(&mut cur), Sep::Other));
                if chars.get(i + 1) == Some(&'&') {
                    i += 1;
                }
            }
            ';' | '\n' | '\r' => {
                out.push((std::mem::take(&mut cur), Sep::Other));
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    out.push((cur, Sep::Other));
    out
}

/// Group operator-split segments into commands, each command being its list of
/// pipeline stage strings.
fn split_into_commands(text: &str) -> Vec<Vec<String>> {
    let mut commands: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for (seg, sep) in split_operators(text) {
        if !seg.trim().is_empty() {
            cur.push(seg);
        }
        if sep == Sep::Other && !cur.is_empty() {
            commands.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        commands.push(cur);
    }
    commands
}

fn parse_recursive(
    text: &str,
    cwd: &Path,
    depth: u8,
    segments: &mut Vec<Segment>,
    reads_shell: &mut bool,
) {
    for stages in split_into_commands(text) {
        if stages.len() >= 2 {
            for stage in stages.iter().skip(1) {
                if stage_binary_is_shell(stage) {
                    *reads_shell = true;
                }
            }
        }
        for stage in stages {
            parse_stage(&stage, cwd, depth, segments, reads_shell);
        }
    }
}

/// Tokenize a stage, peel wrapper binaries, and return the effective argv.
fn tokenize_stage(stage: &str) -> Vec<String> {
    let tokens =
        shlex::split(stage).unwrap_or_else(|| stage.split_whitespace().map(String::from).collect());
    unwrap_wrappers(tokens)
}

fn stage_binary_is_shell(stage: &str) -> bool {
    let tokens = tokenize_stage(stage);
    tokens
        .first()
        .map(|t| is_shell_binary(&basename(t)))
        .unwrap_or(false)
}

fn parse_stage(
    stage: &str,
    cwd: &Path,
    depth: u8,
    segments: &mut Vec<Segment>,
    reads_shell: &mut bool,
) {
    let tokens = tokenize_stage(stage);
    let Some(first) = tokens.first() else {
        return;
    };
    let binary = basename(first);

    let paths: Vec<PathBuf> = tokens
        .iter()
        .skip(1)
        .filter(|t| is_path_arg(t))
        .map(|t| normalize_path(cwd, t))
        .collect();

    segments.push(Segment {
        binary: binary.clone(),
        argv: tokens.clone(),
        paths,
    });

    // `sh -c "<script>"` / `bash -lc "…"`: recurse into the inline script so its
    // real binary/target is seen by the same rule as the direct form.
    if depth < MAX_UNWRAP_DEPTH && is_shell_binary(&binary) {
        if let Some(script) = extract_dash_c_script(&tokens) {
            parse_recursive(&script, cwd, depth + 1, segments, reads_shell);
        }
    }
}

/// Drop leading wrapper binaries (`sudo`, `env`, …) plus their flags and
/// `NAME=value` assignments, returning the argv of the real command.
fn unwrap_wrappers(tokens: Vec<String>) -> Vec<String> {
    let mut idx = 0;
    let mut guard = 0;
    while idx < tokens.len() && guard < WRAPPER_BINARIES.len() + 4 {
        guard += 1;
        let bin = basename(&tokens[idx]);
        if !WRAPPER_BINARIES.contains(&bin.as_str()) {
            break;
        }
        idx += 1;
        // Skip this wrapper's own flags / assignments to reach the real argv[0].
        while idx < tokens.len() {
            let t = &tokens[idx];
            if t.starts_with('-') {
                // `sudo -u user` / `env -u NAME` consume a following value.
                idx += 1;
                if (t == "-u" || t == "--user") && idx < tokens.len() {
                    idx += 1;
                }
            } else if is_assignment(t) {
                idx += 1;
            } else {
                break;
            }
        }
    }
    if idx >= tokens.len() {
        tokens
    } else {
        tokens[idx..].to_vec()
    }
}

fn extract_dash_c_script(tokens: &[String]) -> Option<String> {
    for (i, t) in tokens.iter().enumerate() {
        // `-c`, `-lc`, `-xc`, … : a combined short-flag cluster ending in `c`.
        if t.starts_with('-') && !t.starts_with("--") && t.ends_with('c') && t.len() >= 2 {
            return tokens.get(i + 1).cloned();
        }
        if t == "--command" {
            return tokens.get(i + 1).cloned();
        }
    }
    None
}

fn basename(token: &str) -> String {
    Path::new(token)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| token.to_string())
}

fn is_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether a token should be treated as a filesystem-path argument (so it gets
/// canonicalized and offered to `path_glob`). Flags and `NAME=value` are not.
fn is_path_arg(token: &str) -> bool {
    !token.is_empty() && !token.starts_with('-') && !is_assignment(token)
}

/// Expand a leading `~` and lexically normalize a path against `cwd` WITHOUT
/// touching the filesystem (no symlink resolution, no existence requirement) —
/// a security check must not depend on FS state and must fold `..` traversal.
fn normalize_path(cwd: &Path, raw: &str) -> PathBuf {
    let expanded = expand_tilde(raw);
    let joined: PathBuf = if Path::new(&expanded).is_absolute() {
        PathBuf::from(&expanded)
    } else {
        cwd.join(&expanded)
    };

    let mut stack: Vec<Component> = Vec::new();
    for comp in joined.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(stack.last(), Some(Component::Normal(_))) {
                    stack.pop();
                }
            }
            other => stack.push(other),
        }
    }
    if stack.is_empty() {
        return PathBuf::from(".");
    }
    stack.iter().collect()
}

fn expand_tilde(raw: &str) -> String {
    if raw == "~" {
        return home_dir().unwrap_or_else(|| "~".to_string());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return format!("{home}/{rest}");
        }
    }
    raw.to_string()
}

fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var("USERPROFILE").ok().filter(|h| !h.is_empty()))
}
