//! The subscription-only child environment.
//!
//! Both coding-agent providers drive a vendor CLI that the **user** has already
//! signed in to. The whole point of the feature is that inference is billed to
//! that subscription, not to a metered API key — so the one thing that must
//! never happen is for a stray key in the daemon's environment to silently
//! reroute the run onto an API account.
//!
//! That failure is quiet, which is what makes it dangerous. Claude Code's
//! documented auth precedence has seven levels and `ANTHROPIC_API_KEY` outranks
//! the subscription OAuth token; in `-p` (non-interactive) mode the key is used
//! with no approval prompt. So a machine that merely *has* the variable exported
//! bills the API while the UI still reports "Max". Measured on a dev box: with
//! `ANTHROPIC_API_KEY` set to a bogus value the run reported
//! `apiKeySource: "ANTHROPIC_API_KEY"` and still succeeded — meaning the field
//! alone cannot be trusted as proof of the billing path. Hence *two* defences:
//! remove the possibility here, and assert the observed source at the call site
//! (see each provider's `assert_subscription_auth`).
//!
//! ## Why this is not `prepare_agent_child_command`
//!
//! [`crate::subprocess::prepare_agent_child_command`] strips the **daemon's own**
//! credentials (issue #57) and deliberately keeps everything else, because "the
//! user's environment and an extension's declared credential are not ours to
//! censor". That judgement is right for a shell tool and wrong here: for a
//! subscription-billed CLI the user's *own* `ANTHROPIC_API_KEY` is precisely
//! what has to go. Two different policies, so two different functions. Both are
//! applied, and the ordering matters — see [`configure_subscription_child`].

use tokio::process::Command;

/// Credentials that would divert a coding-agent CLI off the user's subscription.
///
/// Grouped by what each group reroutes *to*, because the list only makes sense
/// as "every way these two CLIs can be told to bill someone else". Adapted from
/// BioOKF Studio's `subscription_only_environment`, which solved the same problem
/// for the same two binaries.
///
/// This is deliberately **not** "every credential-looking variable". Over-
/// stripping has its own regression history in this repo (issue #24 was a
/// truncated `PATH` breaking every Homebrew binary), and an extension's own
/// credential is none of our business. Everything here is specifically an
/// inference-routing control for `claude` or `codex`.
pub const SUBSCRIPTION_DIVERTING_ENV_KEYS: &[&str] = &[
    // ---- Direct first-party API keys -------------------------------------
    // Claude Code: ANTHROPIC_API_KEY (precedence 3) and ANTHROPIC_AUTH_TOKEN
    // (precedence 2) both outrank the subscription OAuth token (precedence 5).
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    // Codex: an API key here flips `auth_mode` off "chatgpt".
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    // ---- Base-URL redirection -------------------------------------------
    // A rewritten base URL sends subscription-authenticated traffic to a third
    // party. Removing it also removes the header-rewriting proxy trick some
    // harnesses use to disguise themselves, which this provider must not do.
    "ANTHROPIC_BASE_URL",
    "OPENAI_BASE_URL",
    "OPENAI_API_BASE",
    // ---- Claude Code's alternate-backend switches ------------------------
    // Each of these makes Claude Code authenticate to a cloud instead of to
    // claude.ai, so the run is neither subscription-billed nor covered by the
    // consumer terms the user is operating under.
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    // ---- AWS Bedrock ------------------------------------------------------
    "AWS_BEARER_TOKEN_BEDROCK",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_SECURITY_TOKEN",
    "AWS_PROFILE",
    "AWS_DEFAULT_PROFILE",
    "AWS_WEB_IDENTITY_TOKEN_FILE",
    "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
    "AWS_CONTAINER_CREDENTIALS_FULL_URI",
    "AWS_REGION",
    "AWS_DEFAULT_REGION",
    // ---- Google Vertex ----------------------------------------------------
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GOOGLE_CLOUD_PROJECT",
    "GCLOUD_PROJECT",
    "CLOUD_ML_REGION",
    // ---- Microsoft Foundry / Azure ---------------------------------------
    "AZURE_OPENAI_API_KEY",
    "AZURE_OPENAI_ENDPOINT",
    "AZURE_CLIENT_ID",
    "AZURE_CLIENT_SECRET",
    "AZURE_TENANT_ID",
];

/// Remove every credential that could divert this child off the subscription.
///
/// Returns the same `Command` so it can be chained. This does **not** call
/// [`crate::subprocess::prepare_agent_child_command`]; see
/// [`configure_subscription_child`] for the correct combination and ordering.
pub fn scrub_diverting_credentials(command: &mut Command) -> &mut Command {
    for key in SUBSCRIPTION_DIVERTING_ENV_KEYS {
        command.env_remove(key);
    }
    command
}

/// The one correct way to finish configuring a coding-agent child process.
///
/// ⚠ **Call this LAST**, after every `.env()`, `.envs()`, `.arg()` and
/// `.current_dir()` on the command. Both halves manipulate the same env map that
/// `.env()` writes, so a later `.env()` re-admits what was removed — and for
/// `prepare_agent_child_command` that specifically means re-admitting
/// `BIOROUTER_SERVER__SECRET_KEY`, which would hand a coding agent full
/// authenticated access to the daemon's REST API (issue #57).
///
/// The two scrubs are independent and both are required:
///
/// * [`scrub_diverting_credentials`] — the *user's* inference credentials, so
///   the run stays on the subscription.
/// * `prepare_agent_child_command` — the *daemon's* private credentials, so the
///   child cannot act as the daemon.
pub fn configure_subscription_child(command: &mut Command) -> &mut Command {
    scrub_diverting_credentials(command);
    crate::subprocess::prepare_agent_child_command(command);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key we claim to strip is actually stripped, and nothing else in the
    /// environment is touched. The negative half matters as much as the positive
    /// one: over-stripping is its own regression class in this repo, and an
    /// extension's declared credential is not ours to remove.
    #[test]
    fn strips_every_diverting_credential_and_nothing_else() {
        let mut cmd = Command::new("printenv");
        for key in SUBSCRIPTION_DIVERTING_ENV_KEYS {
            cmd.env(key, "would-divert-billing");
        }
        // Bystanders that must survive: an extension credential, the user's
        // shell, and PATH (issue #24 — a truncated PATH broke every Homebrew
        // binary, so PATH removal is a real regression, not a hypothetical).
        cmd.env("SPOKEAGENT_PASSCODE", "extension-credential")
            .env("PATH", "/usr/bin")
            .env("HOME", "/Users/someone");

        scrub_diverting_credentials(&mut cmd);

        let envs: Vec<(String, Option<String>)> = cmd
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();

        for key in SUBSCRIPTION_DIVERTING_ENV_KEYS {
            let entry = envs.iter().find(|(k, _)| k == key);
            assert_eq!(
                entry.map(|(_, v)| v.clone()),
                Some(None),
                "{key} must be explicitly removed from the child environment"
            );
        }
        for (name, expected) in [
            ("SPOKEAGENT_PASSCODE", "extension-credential"),
            ("PATH", "/usr/bin"),
            ("HOME", "/Users/someone"),
        ] {
            assert_eq!(
                envs.iter()
                    .find(|(k, _)| k == name)
                    .and_then(|(_, v)| v.clone())
                    .as_deref(),
                Some(expected),
                "{name} is not ours to censor and must survive the scrub"
            );
        }
    }

    /// The two API-key families that actually cause the silent-rebilling bug are
    /// present. Spelled out separately from the loop above so that deleting one
    /// from the list fails a test that names it, rather than passing vacuously.
    #[test]
    fn covers_both_vendors_primary_keys() {
        for required in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "OPENAI_API_KEY",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_USE_BEDROCK",
        ] {
            assert!(
                SUBSCRIPTION_DIVERTING_ENV_KEYS.contains(&required),
                "{required} must stay in the scrub list"
            );
        }
    }

    /// `configure_subscription_child` removes the daemon secret as well as the
    /// billing-diverting keys. This is the composition test: either scrub alone
    /// leaves a hole, and the ordering doc on the function is only meaningful if
    /// both halves are known to run.
    #[test]
    fn configure_subscription_child_also_drops_the_daemon_secret() {
        let mut cmd = Command::new("printenv");
        cmd.env("BIOROUTER_SERVER__SECRET_KEY", "daemon-private")
            .env("ANTHROPIC_API_KEY", "would-divert-billing")
            .env("PATH", "/usr/bin");

        configure_subscription_child(&mut cmd);

        let envs: Vec<(String, Option<String>)> = cmd
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        for removed in ["BIOROUTER_SERVER__SECRET_KEY", "ANTHROPIC_API_KEY"] {
            assert_eq!(
                envs.iter().find(|(k, _)| k == removed).map(|(_, v)| v.clone()),
                Some(None),
                "{removed} must not reach a coding-agent child"
            );
        }
    }
}
