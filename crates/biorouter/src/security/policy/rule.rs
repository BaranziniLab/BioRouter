//! Declarative policy rule types + matching logic.
//!
//! Rules are plain data (deserialized from YAML) so they are auditable and
//! self-testable, replacing the inline `THREAT_PATTERNS` `const`. Each rule
//! carries embedded `tests` (Codex `execpolicy` style) that the engine runs at
//! load time and `cargo test` enforces.

use regex::Regex;
use serde::Deserialize;

use super::command::ParsedCommand;

/// The three first-class outcomes a rule can assert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

/// Rule authority tier (ascending). Higher tiers win ties; Slice 1 ships only
/// `Baseline` (embedded), but the ordering is defined now so the external-file
/// loader in a later slice slots in without a data-model change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    #[default]
    Baseline,
    User,
    Project,
    Admin,
}

impl Tier {
    /// Base of the effective-priority space; a rule's own `priority` is added on
    /// top, so an Admin rule always outranks any Baseline/User/Project rule.
    pub fn base_priority(&self) -> u32 {
        match self {
            Tier::Baseline => 0,
            Tier::User => 2000,
            Tier::Project => 3000,
            Tier::Admin => 4000,
        }
    }
}

/// The criteria that decide whether a rule applies to a tool call. A field left
/// unset (`None`) is a wildcard for that dimension. `binary` and `path_glob`
/// must be satisfied by the *same* pipeline segment, so an unrelated later
/// command cannot supply a missing token (`rm -rf ./x && cd /etc`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RuleMatch {
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub binary: Option<Vec<String>>,
    #[serde(default)]
    pub command_prefix: Option<Vec<String>>,
    #[serde(default)]
    pub arg_regex: Option<String>,
    #[serde(default)]
    pub path_glob: Option<Vec<String>>,
    #[serde(default)]
    pub pipes_to_shell: Option<bool>,
}

impl RuleMatch {
    /// A rule with no criteria at all would match every command — a footgun, so
    /// the loader rejects it.
    pub fn is_empty(&self) -> bool {
        self.tools.is_none()
            && self.binary.is_none()
            && self.command_prefix.is_none()
            && self.arg_regex.is_none()
            && self.path_glob.is_none()
            && self.pipes_to_shell.is_none()
    }
}

/// Embedded positive/negative examples, checked by `PolicyEngine::run_self_tests`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RuleTests {
    #[serde(default)]
    pub matches: Vec<String>,
    #[serde(default)]
    pub not_matches: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// A single declarative policy rule.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub justification: String,
    #[serde(rename = "match")]
    pub r#match: RuleMatch,
    pub decision: Decision,
    #[serde(default)]
    pub priority: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tier: Tier,
    #[serde(default)]
    pub tests: RuleTests,
}

impl Rule {
    /// Effective priority used to resolve competing rules across tiers.
    pub fn effective_priority(&self) -> u32 {
        self.tier.base_priority() + self.priority as u32
    }
}

/// The engine's decision for one tool call.
#[derive(Debug, Clone)]
pub struct PolicyVerdict {
    pub decision: Decision,
    /// Which rule fired (`None` = no rule matched -> default allow).
    pub rule_id: Option<String>,
    pub justification: String,
    /// `POL-<uuid>` (replaces the old `SEC-<uuid>` for command governance).
    pub finding_id: String,
}

/// A tiny glob matcher (`*`, `**`, `?`) compiled to a regex. `*` stops at `/`,
/// `**` (or a `/**` suffix) spans path separators, so `/etc/**` matches both
/// `/etc` and everything beneath it.
#[derive(Debug, Clone)]
pub struct Glob {
    re: Regex,
}

impl Glob {
    pub fn new(pattern: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            re: Regex::new(&glob_to_regex(pattern))?,
        })
    }

    pub fn matches(&self, text: &str) -> bool {
        self.re.is_match(text)
    }

    pub fn matches_path(&self, path: &std::path::Path) -> bool {
        let s = path.to_string_lossy().replace('\\', "/");
        self.re.is_match(&s)
    }
}

fn glob_to_regex(glob: &str) -> String {
    // Sentinels keep the multi-char `/**` / `**` cases out of the char loop.
    let prepared = glob.replace("/**", "\u{1}").replace("**", "\u{2}");
    let mut re = String::from("^");
    for c in prepared.chars() {
        match c {
            '\u{1}' => re.push_str("(/.*)?"),
            '\u{2}' => re.push_str(".*"),
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                re.push('\\');
                re.push(c);
            }
            other => re.push(other),
        }
    }
    re.push('$');
    re
}

/// A rule with its regex/globs pre-compiled, ready for hot-path evaluation.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub rule: Rule,
    arg_re: Option<Regex>,
    tool_globs: Option<Vec<Glob>>,
    path_globs: Option<Vec<Glob>>,
}

impl CompiledRule {
    /// Compile a rule, returning an error string (for logging) on a bad
    /// regex/glob or an empty match — a broken rule must not silently pass.
    pub fn compile(rule: Rule) -> Result<Self, String> {
        if rule.r#match.is_empty() {
            return Err(format!("rule '{}' has no match criteria", rule.id));
        }

        let arg_re = match &rule.r#match.arg_regex {
            Some(src) => Some(
                Regex::new(src).map_err(|e| format!("rule '{}' bad arg_regex: {e}", rule.id))?,
            ),
            None => None,
        };
        let tool_globs = compile_globs(&rule.id, "tools", &rule.r#match.tools)?;
        let path_globs = compile_globs(&rule.id, "path_glob", &rule.r#match.path_glob)?;

        Ok(Self {
            rule,
            arg_re,
            tool_globs,
            path_globs,
        })
    }

    /// Whether this rule applies to `(tool, cmd)`. All specified dimensions must
    /// hold (AND); `binary` and `path_glob` must co-occur in one segment.
    pub fn matches(&self, tool: &str, cmd: &ParsedCommand) -> bool {
        if let Some(globs) = &self.tool_globs {
            if !globs.iter().any(|g| g.matches(tool)) {
                return false;
            }
        }
        if let Some(re) = &self.arg_re {
            if !re.is_match(&cmd.raw) {
                return false;
            }
        }
        if let Some(expected) = self.rule.r#match.pipes_to_shell {
            if cmd.reads_shell != expected {
                return false;
            }
        }
        if let Some(prefixes) = &self.rule.r#match.command_prefix {
            let hit = cmd.segments.iter().any(|seg| {
                let line = seg.command_line();
                prefixes.iter().any(|p| line.starts_with(p.trim()))
            });
            if !hit {
                return false;
            }
        }

        let binaries = &self.rule.r#match.binary;
        if binaries.is_some() || self.path_globs.is_some() {
            let ok = cmd.segments.iter().any(|seg| {
                let bin_ok = binaries
                    .as_ref()
                    .is_none_or(|bins| bins.iter().any(|b| b.eq_ignore_ascii_case(&seg.binary)));
                let path_ok = self.path_globs.as_ref().is_none_or(|globs| {
                    seg.paths
                        .iter()
                        .any(|p| globs.iter().any(|g| g.matches_path(p)))
                });
                bin_ok && path_ok
            });
            if !ok {
                return false;
            }
        }

        true
    }
}

fn compile_globs(
    rule_id: &str,
    field: &str,
    patterns: &Option<Vec<String>>,
) -> Result<Option<Vec<Glob>>, String> {
    match patterns {
        None => Ok(None),
        Some(pats) => {
            let mut globs = Vec::with_capacity(pats.len());
            for p in pats {
                globs.push(
                    Glob::new(p).map_err(|e| format!("rule '{rule_id}' bad {field} '{p}': {e}"))?,
                );
            }
            Ok(Some(globs))
        }
    }
}
