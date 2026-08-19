//! The **Claude Agent** provider: drives the user's own `claude` CLI on the
//! user's own Claude subscription.
//!
//! # Why the CLI and not the Agent SDK
//!
//! This looks like the sort of thing that ought to use `@anthropic-ai/claude-agent-sdk`,
//! and it deliberately does not. Anthropic's own documentation settles it:
//!
//! * The headless page is titled *"Run Claude Code programmatically"* and states
//!   that it "covers using the Agent SDK via the CLI (`claude -p`)" — `-p` **is**
//!   the SDK's CLI form, not a lesser path around it.
//! * The SDK overview: "The SDK is available as a library for Python and
//!   TypeScript only. To drive the same agent loop from another language, run the
//!   CLI as a subprocess with the `-p` flag and `--output-format json`." Biorouter
//!   is Rust, so this is the documented route.
//! * The SDK would actively be worse here. Its overview directs third-party
//!   developers to *API key* authentication, its use is governed by Anthropic's
//!   Commercial Terms, and it is only a wrapper that spawns this same binary
//!   anyway — the npm package ships nothing but `bin/claude.exe`. Nothing about it
//!   unlocks subscription usage.
//!
//! Subscription billing is confirmed by Anthropic's own help centre, which names
//! `claude -p` as drawing from the plan's usage limits.
//!
//! # The flags that are not optional
//!
//! Four of the arguments below are load-bearing and were each established by
//! measurement, not preference:
//!
//! * `--setting-sources ""` — without it a `-p` session **executes the hooks in
//!   the working directory's `.claude/settings.json`**, because `-p` shows no
//!   workspace-trust dialog. Biorouter sets the child's cwd to the session's
//!   working directory, which may be any repository the user happens to have
//!   open, so this is arbitrary command execution triggered by cwd alone. Tested
//!   against a hostile fixture: without this flag the fixture's `SessionStart`
//!   hook ran.
//! * `--strict-mcp-config` — without it the child connects the MCP servers in the
//!   user's own configuration. Measured: a bare run showed the developer's
//!   personal clinical-database server as `connected` inside a child Biorouter
//!   thought it was fully isolating. `--tools ""` does *not* cover this; it
//!   suppresses built-ins only.
//! * `--tools ""` — the child's own Read/Edit/Bash are switched off. A tool the
//!   child runs itself is invisible to Biorouter's inspectors, permission modes,
//!   `.biorouterignore` and vault. Biorouter's own tools reach it over MCP
//!   instead, where Biorouter's dispatcher executes them and every existing gate
//!   still fires.
//! * `--system-prompt` — replaces Claude Code's default prompt with Biorouter's.
//!   Besides being correct, it is a 16x saving: the default prompt measured 25,022
//!   tokens per call, and Biorouter's measured 1,527.
//!
//! `--bare` must **never** be passed. It is documented as never reading OAuth
//! credentials or the system keychain, which is precisely the credential this
//! provider exists to use. It is also documented as becoming the default for `-p`
//! in a future release, which would silently break subscription billing — hence
//! [`assert_subscription_auth`], which fails loudly rather than quietly billing an
//! API account.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{Role, Tool};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::base::{
    ConfigKey, ModelInfo, Provider, ProviderMetadata, ProviderUsage, Usage,
};
use super::coding_agent::{
    self, discovery, env as agent_env, transcript, CodingAgentKind,
};
use super::errors::ProviderError;
use crate::config::search_path::SearchPaths;
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;

const KIND: CodingAgentKind = CodingAgentKind::ClaudeCode;

/// Concrete model ids rather than the CLI's `sonnet`/`opus` aliases.
///
/// The aliases are nicer — they track the current model in each family, and a
/// pinned id rots (the previously pinned `claude-sonnet-4-20250514` was retired
/// while it sat in this file). They are still not what is advertised, for two
/// reasons that outweigh it:
///
/// 1. `tests/context_windows.rs` requires every advertised model to have its own
///    entry in `MODEL_CONTEXT_WINDOWS`, and an alias has none — so `sonnet` would
///    silently take the 128k default and the settings UI would display a wrong
///    window for a 1M model.
/// 2. Adding `"sonnet"` and `"opus"` to that table would put two very short
///    substrings into a globally-served pattern list, where any *future* model id
///    containing them but lacking its own exact entry would inherit this window.
///    That is safe only because `get_all_model_limits` happens to sort
///    longest-first, which nothing pins as an invariant.
///
/// `with_unlisted_models` still lets a user type `sonnet` by hand, and the CLI
/// accepts both spellings — verified against `claude` 2.1.235.
pub const CLAUDE_CODE_DEFAULT_MODEL: &str = "claude-sonnet-4-6";

pub const CLAUDE_CODE_DOC_URL: &str = "https://code.claude.com/docs/en/headless";

/// A turn's wall-clock ceiling. Generous because a real coding-agent turn can
/// legitimately run for minutes, but finite: none of the previous CLI-agent
/// providers had a timeout at all, so a wedged child held a session open forever
/// with no way to stop it.
const TURN_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Models advertised in the picker, with the context window each alias resolves
/// to today.
///
/// `ProviderMetadata::with_models` is used rather than `::new` on purpose: `::new`
/// derives each limit by looking the *name* up as a model id, and these are
/// aliases rather than ids, so every one of them would silently take the default
/// limit and the settings UI would display a wrong window.
fn known_models() -> Vec<ModelInfo> {
    // Each window must equal the one `MODEL_CONTEXT_WINDOWS` declares, because
    // `tests/context_windows.rs::provider_declared_windows_match_the_registry`
    // compares the two.
    vec![
        ModelInfo::new("claude-sonnet-4-6", 1_000_000),
        ModelInfo::new("claude-opus-5", 1_000_000),
        ModelInfo::new("claude-fable-5", 1_000_000),
        ModelInfo::new("claude-haiku-4-5", 200_000),
    ]
}

#[derive(Debug, serde::Serialize)]
pub struct ClaudeCodeProvider {
    command: PathBuf,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

impl ClaudeCodeProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        // Resolution only — a few stat calls. Nothing here may spawn a process:
        // `GET /config/providers` constructs every configured provider under a
        // 3-second timeout to sample tier and affiliation, so a probe here would
        // stall the whole settings page.
        let configured = discovery::configured_command(KIND);
        let command = discovery::resolve_binary(KIND, configured.as_deref()).ok_or_else(|| {
            anyhow::anyhow!(
                "could not find the `{}` command. Install it with:\n    {}\n\nIf it is installed \
                 somewhere Biorouter does not search (nvm, volta, bun, asdf), set {} to its full \
                 path.",
                KIND.default_command(),
                KIND.install_hint(),
                KIND.command_config_key(),
            )
        })?;

        Ok(Self {
            command,
            model,
            name: KIND.provider_id().to_string(),
        })
    }

    /// The arguments shared by every invocation.
    ///
    /// `output_format` is the only axis that varies: `json` for a single blocking
    /// result, `stream-json` for the streaming path.
    fn base_args(&self, model_config: &ModelConfig, system: &str, output_format: &str) -> Vec<String> {
        let mut args: Vec<String> = vec!["-p".into()];

        // Isolation. See the module header — both of these are security-relevant
        // and neither is a substitute for the other.
        args.push("--setting-sources".into());
        args.push(String::new());
        args.push("--strict-mcp-config".into());

        // The child's own agentic tools are off. Biorouter's tools arrive over
        // MCP, where Biorouter executes them behind its own gates.
        args.push("--tools".into());
        args.push(String::new());

        // Biorouter's system prompt replaces Claude Code's, rather than being
        // appended to it.
        args.push("--system-prompt".into());
        args.push(system.to_string());

        // Sessions are Biorouter's to persist. Letting the CLI also write them
        // would leave a second, divergent transcript on disk that no Biorouter
        // control governs.
        args.push("--no-session-persistence".into());

        args.push("--output-format".into());
        args.push(output_format.into());

        let model = model_config.model_name.trim();
        if !model.is_empty() {
            args.push("--model".into());
            args.push(model.to_string());
        }

        args
    }

    /// Build the child process. Returns it unspawned so both paths share exactly
    /// one construction site.
    fn command_for(
        &self,
        model_config: &ModelConfig,
        system: &str,
        output_format: &str,
    ) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(self.base_args(model_config, system, output_format));

        // The child shells out to git, ripgrep and node on its own account, so
        // handing it the resolved absolute path is not enough — it needs the
        // augmented PATH too. Two of the four deleted CLI providers did this and
        // two did not; the two that did not were the ones that failed under the
        // GUI's truncated PATH.
        if let Ok(path) = SearchPaths::builder().with_npm().path() {
            cmd.env("PATH", path);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // LAST, after every arg and env. Both scrubs write the same env map that
        // `.env()` writes, so a later `.env()` would re-admit what they removed —
        // for the daemon scrub that means `BIOROUTER_SERVER__SECRET_KEY`, which
        // would make this child a fully authenticated client of Biorouter's own
        // REST API (issue #57).
        agent_env::configure_subscription_child(&mut cmd);
        cmd
    }

    /// Refuse to continue if the run is not actually on the subscription.
    ///
    /// `system/init` reports `apiKeySource`, which is `"none"` under subscription
    /// auth. The scrub in [`agent_env`] should already have made anything else
    /// impossible, so reaching this is a real defect — a settings-file
    /// `apiKeyHelper` (which outranks the OAuth token), or a future release
    /// flipping `-p` to `--bare`. Either way the correct response is to stop,
    /// because the alternative is billing an account the user did not choose.
    fn assert_subscription_auth(source: Option<&str>) -> Result<(), ProviderError> {
        match source {
            None | Some("none") => Ok(()),
            Some(other) => Err(ProviderError::Authentication(format!(
                "This run would have been billed to {other} rather than to your Claude \
                 subscription, so it was stopped.\n\nBiorouter removes API credentials from the \
                 environment it starts `claude` in, so something outside the environment is \
                 supplying one — most likely an `apiKeyHelper` in a Claude Code settings file, \
                 which outranks subscription sign-in."
            ))),
        }
    }

    /// Map a `system/api_retry` error category, or a nonzero exit, onto a typed
    /// provider error.
    ///
    /// Typing this matters: every one of the deleted CLI providers collapsed all
    /// failures into `RequestFailed`, so the retry layer could not tell a
    /// credential problem it must not retry from a blip it should.
    fn classify(category: Option<&str>, detail: String) -> ProviderError {
        match category {
            Some("authentication_failed") | Some("oauth_org_not_allowed") => {
                ProviderError::Authentication(detail)
            }
            Some("rate_limit") => ProviderError::RateLimitExceeded {
                details: detail,
                retry_delay: None,
            },
            Some("billing_error") => ProviderError::Authentication(detail),
            Some("overloaded") | Some("server_error") => ProviderError::ServerError(detail),
            Some("max_output_tokens") => ProviderError::ContextLengthExceeded(detail),
            Some("model_not_found") | Some("invalid_request") => {
                ProviderError::RequestFailed(detail)
            }
            _ => ProviderError::RequestFailed(detail),
        }
    }

    /// Run one turn and return every stdout line, plus stderr.
    ///
    /// stderr is drained **concurrently** with stdout. All four deleted providers
    /// piped stderr and never read it, which threw away the CLI's own diagnostic
    /// on every failure and could deadlock a child that wrote more than the pipe
    /// buffer to it.
    async fn run(
        &self,
        mut cmd: tokio::process::Command,
        prompt: &str,
    ) -> Result<(Vec<String>, String, std::process::ExitStatus), ProviderError> {
        let mut child = cmd.spawn().map_err(|e| {
            ProviderError::ExecutionError(format!(
                "could not start `{}`: {e}",
                self.command.display()
            ))
        })?;

        // The prompt goes on stdin, never in argv. A flattened conversation can
        // be far larger than the platform's argv limit, and the deleted provider
        // put the whole thing in a single `-p` argument.
        if let Some(mut stdin) = child.stdin.take() {
            let bytes = prompt.as_bytes().to_vec();
            tokio::spawn(async move {
                let _ = stdin.write_all(&bytes).await;
                let _ = stdin.shutdown().await;
            });
        }

        let stdout = child.stdout.take().ok_or_else(|| {
            ProviderError::ExecutionError("could not capture claude stdout".into())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ProviderError::ExecutionError("could not capture claude stderr".into())
        })?;

        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut out = Vec::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    out.push(line);
                }
            }
            out
        });
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut out = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                out.push_str(&line);
                out.push('\n');
            }
            out
        });

        let waited = tokio::time::timeout(TURN_TIMEOUT, child.wait()).await;
        let status = match waited {
            Ok(Ok(status)) => status,
            Ok(Err(e)) => {
                return Err(ProviderError::ExecutionError(format!(
                    "waiting for `claude` failed: {e}"
                )))
            }
            Err(_) => {
                let _ = child.start_kill();
                return Err(ProviderError::ExecutionError(format!(
                    "`claude` did not finish within {}s and was stopped",
                    TURN_TIMEOUT.as_secs()
                )));
            }
        };

        let lines = stdout_task.await.unwrap_or_default();
        let errors = stderr_task.await.unwrap_or_default();
        Ok((lines, errors, status))
    }

    /// Parse `--output-format json`: one object carrying the final text and the
    /// authoritative usage.
    fn parse_result_object(
        &self,
        model: &str,
        lines: &[String],
        stderr: &str,
        status: std::process::ExitStatus,
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        // `system/init` may precede the result object when the CLI emits startup
        // frames, so scan rather than assuming a single line.
        let mut result: Option<Value> = None;
        let mut api_key_source: Option<String> = None;
        let mut retry_category: Option<String> = None;

        for line in lines {
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            match v.get("type").and_then(Value::as_str) {
                Some("system") => match v.get("subtype").and_then(Value::as_str) {
                    Some("init") => {
                        api_key_source = v
                            .get("apiKeySource")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                    }
                    Some("api_retry") => {
                        retry_category =
                            v.get("error").and_then(Value::as_str).map(str::to_string);
                    }
                    _ => {}
                },
                Some("result") => result = Some(v),
                _ => {}
            }
        }

        Self::assert_subscription_auth(api_key_source.as_deref())?;

        let Some(result) = result else {
            let detail = if stderr.trim().is_empty() {
                format!("`claude` produced no result (exit {:?})", status.code())
            } else {
                format!(
                    "`claude` produced no result (exit {:?}): {}",
                    status.code(),
                    stderr.trim()
                )
            };
            return Err(Self::classify(retry_category.as_deref(), detail));
        };

        if result.get("is_error").and_then(Value::as_bool) == Some(true) {
            let detail = result
                .get("result")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| Some(stderr.trim().to_string()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "`claude` reported an error".into());
            let category = result
                .get("subtype")
                .and_then(Value::as_str)
                .or(retry_category.as_deref());
            return Err(Self::classify(category, detail));
        }

        let text = result
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if text.trim().is_empty() {
            return Err(ProviderError::RequestFailed(
                "`claude` returned an empty response".into(),
            ));
        }

        let usage = parse_usage(result.get("usage"));
        let message = Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp(),
            vec![MessageContent::text(text)],
        );

        // The provider field must be set, or the usage row is attributed by model
        // name and `canonical_model_pricing` invents a per-token price for a run
        // that billed a subscription — the exact failure
        // `pricing::blocks_fallback_pricing` exists to prevent, reached by a
        // different route.
        let mut provider_usage = ProviderUsage::new(model.to_string(), usage);
        provider_usage.provider = Some(KIND.provider_id().to_string());
        Ok((message, provider_usage))
    }
}

/// Pull Biorouter's four disjoint token buckets out of the CLI's `usage` object.
///
/// Claude Code reports the same shape the Anthropic API does, where `input_tokens`
/// already **excludes** both cache buckets — which is exactly the invariant
/// [`Usage`] documents, so no subtraction is needed here.
fn parse_usage(usage: Option<&Value>) -> Usage {
    let Some(u) = usage else {
        return Usage::default();
    };
    let get = |k: &str| u.get(k).and_then(Value::as_i64).map(|v| v as i32);

    let input = get("input_tokens");
    let output = get("output_tokens");
    let cache_read = get("cache_read_input_tokens");
    let cache_creation = get("cache_creation_input_tokens");

    Usage {
        input_tokens: input,
        output_tokens: output,
        // Context occupancy for the live gauge, which for a cache-aware provider
        // includes the cache buckets. Deliberately not the billed total.
        total_tokens: match (input, output) {
            (None, None) => None,
            _ => Some(
                input.unwrap_or(0)
                    + output.unwrap_or(0)
                    + cache_read.unwrap_or(0)
                    + cache_creation.unwrap_or(0),
            ),
        },
        cache_read_input_tokens: cache_read,
        cache_creation_input_tokens: cache_creation,
        ..Default::default()
    }
}

#[async_trait]
impl Provider for ClaudeCodeProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::with_models(
            KIND.provider_id(),
            KIND.display_name(),
            "Uses the Claude subscription you are already signed in to, through your own \
             installed `claude` command. No API key. Requires Claude Code to be installed and \
             signed in.",
            CLAUDE_CODE_DEFAULT_MODEL,
            known_models(),
            CLAUDE_CODE_DOC_URL,
            vec![ConfigKey::new(
                KIND.command_config_key(),
                true,
                false,
                Some(KIND.default_command()),
            )],
        )
        .with_unlisted_models()
        // Public, and NOT `runs_locally`. The subprocess is local; the inference
        // is Anthropic's. Getting this wrong would forge a private badge and let
        // the bind gate attach this provider to a session holding clinical data —
        // which a consumer subscription has no BAA to receive. Both values are
        // the trait defaults, restated here so the decision is visible.
        .with_tier(crate::privacy::ProviderTier::Public)
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn complete_with_model(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        // Tools are accepted and not forwarded yet. They cannot simply be dropped
        // once the MCP bridge lands, so this is the one seam that changes then.
        let prompt = transcript::flatten(messages).ok_or_else(|| {
            ProviderError::RequestFailed(
                "there is no user message for `claude` to answer".to_string(),
            )
        })?;

        let cmd = self.command_for(model_config, system, "json");
        let (lines, stderr, status) = self.run(cmd, &prompt).await?;
        self.parse_result_object(&model_config.model_name, &lines, &stderr, status)
    }

    /// Ask the CLI's credential store whether this provider can run at all.
    ///
    /// Spawns, so it must never be called from `from_env` — see
    /// [`discovery`]'s module header.
    async fn fetch_supported_models(&self) -> Result<Option<Vec<String>>, ProviderError> {
        let availability = discovery::probe(KIND).await;
        if !availability.auth.is_subscription() {
            return Err(coding_agent::unavailable_error(KIND, &availability));
        }
        Ok(Some(
            known_models().into_iter().map(|m| m.name).collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ClaudeCodeProvider {
        ClaudeCodeProvider {
            command: PathBuf::from("/usr/bin/claude"),
            model: ModelConfig::new("claude-sonnet-4-6").unwrap(),
            name: KIND.provider_id().to_string(),
        }
    }

    /// The isolation flags are the security boundary, so their presence is pinned
    /// rather than left to review. Without `--setting-sources ""` a `-p` run
    /// executes the working directory's `.claude/settings.json` hooks; without
    /// `--strict-mcp-config` it connects the user's own MCP servers.
    #[test]
    fn the_isolation_flags_are_always_present() {
        let args = provider().base_args(&ModelConfig::new("claude-sonnet-4-6").unwrap(), "SYS", "json");

        let i = args.iter().position(|a| a == "--setting-sources").expect(
            "--setting-sources is required: without it a -p run executes the cwd's hooks",
        );
        assert_eq!(
            args[i + 1], "",
            "--setting-sources must be given an empty value to load no sources"
        );
        assert!(
            args.iter().any(|a| a == "--strict-mcp-config"),
            "--strict-mcp-config is required: without it the child loads the user's own MCP servers"
        );

        let t = args
            .iter()
            .position(|a| a == "--tools")
            .expect("--tools is required so the child's own Read/Edit/Bash stay off");
        assert_eq!(args[t + 1], "", "--tools must be empty to disable all built-ins");
    }

    /// `--bare` never reads OAuth credentials or the keychain, so passing it would
    /// silently defeat the entire provider.
    #[test]
    fn bare_mode_is_never_requested() {
        let args = provider().base_args(&ModelConfig::new("claude-sonnet-4-6").unwrap(), "SYS", "json");
        assert!(
            !args.iter().any(|a| a == "--bare"),
            "--bare never reads OAuth credentials and must not be passed"
        );
    }

    /// Biorouter's prompt replaces Claude Code's rather than being appended, which
    /// is both correct and a 16x token saving.
    #[test]
    fn the_system_prompt_replaces_rather_than_appends() {
        let args = provider().base_args(&ModelConfig::new("claude-sonnet-4-6").unwrap(), "SYS", "json");
        let i = args.iter().position(|a| a == "--system-prompt").unwrap();
        assert_eq!(args[i + 1], "SYS");
        assert!(!args.iter().any(|a| a == "--append-system-prompt"));
    }

    #[test]
    fn the_output_format_is_the_only_axis_that_varies() {
        let p = provider();
        let m = ModelConfig::new("claude-sonnet-4-6").unwrap();
        let json = p.base_args(&m, "SYS", "json");
        let stream = p.base_args(&m, "SYS", "stream-json");
        assert_eq!(json.len(), stream.len());
        let differences: Vec<_> = json
            .iter()
            .zip(stream.iter())
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(differences.len(), 1, "only --output-format's value may differ");
    }

    /// An `apiKeySource` other than "none" means the run would be billed to a
    /// metered account, which is the one outcome this provider exists to prevent.
    #[test]
    fn a_non_subscription_auth_source_is_refused() {
        assert!(ClaudeCodeProvider::assert_subscription_auth(Some("none")).is_ok());
        assert!(ClaudeCodeProvider::assert_subscription_auth(None).is_ok());

        let err = ClaudeCodeProvider::assert_subscription_auth(Some("ANTHROPIC_API_KEY"))
            .expect_err("a key-sourced run must be refused");
        assert!(matches!(err, ProviderError::Authentication(_)));
        assert!(err.to_string().contains("apiKeyHelper"));
    }

    /// Failures must be typed, so the retry layer does not retry a credential
    /// problem or give up on a blip.
    #[test]
    fn error_categories_map_to_typed_errors() {
        for (category, matches_auth) in [
            ("authentication_failed", true),
            ("oauth_org_not_allowed", true),
            ("billing_error", true),
        ] {
            let e = ClaudeCodeProvider::classify(Some(category), "d".into());
            assert_eq!(
                matches!(e, ProviderError::Authentication(_)),
                matches_auth,
                "{category} should be an authentication error"
            );
        }
        assert!(matches!(
            ClaudeCodeProvider::classify(Some("rate_limit"), "d".into()),
            ProviderError::RateLimitExceeded { .. }
        ));
        assert!(matches!(
            ClaudeCodeProvider::classify(Some("overloaded"), "d".into()),
            ProviderError::ServerError(_)
        ));
        assert!(matches!(
            ClaudeCodeProvider::classify(Some("max_output_tokens"), "d".into()),
            ProviderError::ContextLengthExceeded(_)
        ));
        // An unknown category must not be silently treated as success.
        assert!(matches!(
            ClaudeCodeProvider::classify(Some("something_new"), "d".into()),
            ProviderError::RequestFailed(_)
        ));
    }

    /// The four token buckets are disjoint, which is the invariant `Usage`
    /// documents and what makes `billed_total` reconcile with a vendor bill.
    #[test]
    fn usage_buckets_stay_disjoint() {
        let v = serde_json::json!({
            "input_tokens": 10,
            "output_tokens": 45,
            "cache_read_input_tokens": 5232,
            "cache_creation_input_tokens": 907
        });
        let u = parse_usage(Some(&v));
        assert_eq!(u.input_tokens, Some(10));
        assert_eq!(u.output_tokens, Some(45));
        assert_eq!(u.cache_read_input_tokens, Some(5232));
        assert_eq!(u.cache_creation_input_tokens, Some(907));
        assert_eq!(u.total_tokens, Some(10 + 45 + 5232 + 907));
    }

    #[test]
    fn absent_usage_is_not_invented() {
        assert_eq!(parse_usage(None).input_tokens, None);
        assert_eq!(parse_usage(None).total_tokens, None);
    }

    /// A real captured `result` frame parses, and the usage row is attributed to
    /// this provider rather than left for the model name to decide.
    #[test]
    fn a_captured_result_frame_parses_and_is_attributed() {
        let lines = vec![
            r#"{"type":"system","subtype":"init","apiKeySource":"none","model":"claude-haiku-4-5-20251001"}"#.to_string(),
            r#"{"type":"result","subtype":"success","is_error":false,"result":"PROOF_OK","usage":{"input_tokens":10,"output_tokens":45,"cache_read_input_tokens":0,"cache_creation_input_tokens":25022}}"#.to_string(),
        ];
        let (message, usage) = provider()
            .parse_result_object("claude-sonnet-4-6", &lines, "", exit_ok())
            .expect("a success frame must parse");

        assert_eq!(message.as_concat_text(), "PROOF_OK");
        assert_eq!(message.role, Role::Assistant);
        assert_eq!(
            usage.provider.as_deref(),
            Some("claude_code"),
            "an unattributed usage row gets a fabricated per-token price"
        );
        assert_eq!(usage.usage.cache_creation_input_tokens, Some(25022));
    }

    /// A run that reports a key source is refused even though the frame says
    /// success — the billing path is wrong, and that is not something to recover
    /// from silently.
    #[test]
    fn a_successful_frame_with_a_key_source_is_still_refused() {
        let lines = vec![
            r#"{"type":"system","subtype":"init","apiKeySource":"ANTHROPIC_API_KEY"}"#.to_string(),
            r#"{"type":"result","subtype":"success","is_error":false,"result":"hi","usage":{}}"#
                .to_string(),
        ];
        let err = provider()
            .parse_result_object("claude-sonnet-4-6", &lines, "", exit_ok())
            .expect_err("a key-sourced run must be refused even on success");
        assert!(matches!(err, ProviderError::Authentication(_)));
    }

    /// When the CLI produces nothing usable, its stderr is what tells the user
    /// why — all four deleted providers discarded it.
    #[test]
    fn stderr_reaches_the_error_when_there_is_no_result() {
        let err = provider()
            .parse_result_object("claude-sonnet-4-6", &[], "claude: command failed spectacularly", exit_ok())
            .expect_err("no result frame is a failure");
        assert!(
            err.to_string().contains("command failed spectacularly"),
            "the CLI's own diagnostic must survive into the error: {err}"
        );
    }

    #[test]
    fn metadata_is_public_and_not_locally_computed() {
        let m = ClaudeCodeProvider::metadata();
        assert_eq!(m.name, "claude_code", "the underscore is what pricing keys on");
        assert_eq!(m.display_name, "Claude Agent");
        assert_eq!(m.tier, crate::privacy::ProviderTier::Public);
        assert!(
            !m.runs_locally,
            "the subprocess is local but the inference is not"
        );
        // One required key with a default, which is what makes a keyless provider
        // report as configured at all.
        assert_eq!(m.config_keys.len(), 1);
        assert_eq!(m.config_keys[0].name, "CLAUDE_CODE_COMMAND");
        assert!(m.config_keys[0].required);
        assert!(!m.config_keys[0].secret);
        assert_eq!(m.config_keys[0].default.as_deref(), Some("claude"));
    }

    fn exit_ok() -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(0)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(0)
        }
    }
}
