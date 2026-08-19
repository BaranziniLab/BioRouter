//! Shared machinery for the two providers that drive a vendor coding-agent CLI
//! on the user's **own subscription**.
//!
//! # What these providers are
//!
//! `claude_code` and `codex` are unlike every other provider in this tree: there
//! is no base URL and no API key. Each spawns a CLI the *user* installed and
//! signed in to, and that CLI resolves its own credential and bills the user's
//! own plan. BioRouter never sees, stores, brokers, proxies or transmits the
//! credential — it only starts a process.
//!
//! That is not merely a convenient design, it is the compliance boundary. Both
//! vendors permit ordinary individual use of one's own subscription and forbid a
//! third party from *offering* their login or routing requests on behalf of its
//! users. Every rule below follows from staying firmly on the permitted side:
//!
//! * BioRouter never implements a vendor login flow. It surfaces the vendor's own
//!   command ([`CodingAgentKind::login_command`]) for the user to run.
//! * BioRouter never disguises itself as the vendor's first-party harness. No
//!   entrypoint spoofing, no header rewriting, no proxy — some harnesses do this
//!   and it is exactly what the terms target.
//! * The credential-diverting environment is scrubbed ([`env`]) so a run cannot
//!   silently land on a metered API account the user did not choose.
//!
//! # The privacy tier, which is the whole security argument
//!
//! Both providers are [`crate::privacy::ProviderTier::Public`] with
//! `runs_locally: false` and no affiliation — i.e. both leave the trait defaults
//! alone, which are already correct and are documented as fail-safe.
//!
//! The tempting mistake is to reason "the subprocess is local, so this is
//! Private". That is a category error and a dangerous one. `runs_locally` is
//! defined as whether *inference* happens on the user's machine, and here it does
//! not — it happens at Anthropic or OpenAI. `Public` is, per the privacy-tier
//! design, "everything hosted by an AI company or a large cloud".
//!
//! Getting this right is what makes the feature safe for a clinical-research
//! setting. A consumer subscription carries no BAA and no zero-data-retention
//! agreement, so PHI must never reach it. Because these providers are `Public`,
//! the bind gate refuses to attach them to any session already classified
//! `Private` — one that has touched an institutional clinical extension or a
//! private knowledge base. Declaring `Private` would forge that badge and delete
//! the protection.
//!
//! # What the child agent may and may not do
//!
//! Both CLIs are full agents with their own file and shell tools. Those are
//! switched **off** (`--tools ""`, `sandbox: read-only`), because a tool the child
//! runs itself is invisible to BioRouter's inspectors, permission modes,
//! `.biorouterignore` and vault. What the child gets instead is BioRouter's own
//! tools, over MCP, executed by BioRouter's dispatcher — so every existing gate
//! still fires on them. See [`super::coding_agent::bridge`] once that lands.

pub mod appserver;
pub mod discovery;
pub mod env;
pub mod transcript;

pub use discovery::{
    codex_home, configured_command, probe, probe_all, resolve_binary, AgentAvailability, AuthState,
    CodingAgentKind,
};

use super::errors::ProviderError;

/// Turn a missing-or-unusable CLI into the error the user should act on.
///
/// Separate from the generic error mapper because these are *setup* failures, and
/// the only useful response is a specific instruction. A coding-agent provider
/// that fails this way must not look like a transient server error, or the retry
/// layer will hide the one message that would fix it.
pub fn unavailable_error(kind: CodingAgentKind, availability: &AgentAvailability) -> ProviderError {
    match &availability.auth {
        AuthState::NotInstalled => ProviderError::ExecutionError(format!(
            "{} is not installed, or is not on a path Biorouter searches.\n\n\
             Install it with:\n    {}\n\n\
             If it is already installed somewhere unusual (nvm, volta, bun, asdf), set {} to its \
             full path in Settings instead.",
            kind.display_name(),
            kind.install_hint(),
            kind.command_config_key(),
        )),
        AuthState::SignedOut => ProviderError::Authentication(format!(
            "{} is installed but not signed in.\n\n\
             Run this yourself, in a terminal:\n    {}\n\n\
             Biorouter deliberately does not perform the sign-in: the credential stays between \
             you and the vendor, and never passes through Biorouter.",
            kind.display_name(),
            kind.login_command(),
        )),
        AuthState::SignedInWithApiKey => ProviderError::Authentication(format!(
            "{} is signed in with an API key rather than a subscription, so this provider cannot \
             run: it exists specifically to use your own plan, and it removes API credentials from \
             the environment it starts.\n\n\
             Either sign in with your subscription:\n    {}\n\n\
             or use the metered provider for this vendor instead.",
            kind.display_name(),
            kind.login_command(),
        )),
        AuthState::Indeterminate { detail } => ProviderError::ExecutionError(format!(
            "Could not determine whether {} is usable: {detail}",
            kind.display_name(),
        )),
        AuthState::SignedInSubscription { .. } => ProviderError::ExecutionError(format!(
            "{} reported a usable subscription but the run could not start.",
            kind.display_name(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn availability(kind: CodingAgentKind, auth: AuthState) -> AgentAvailability {
        AgentAvailability {
            kind,
            provider_id: kind.provider_id().to_string(),
            display_name: kind.display_name().to_string(),
            path: None,
            version: None,
            auth,
            login_command: kind.login_command().to_string(),
            install_hint: kind.install_hint().to_string(),
        }
    }

    /// A setup failure must be classified so the retry layer does not bury it.
    /// "Not signed in" is an auth error; "not installed" is not something a retry
    /// or a credential change fixes, so it stays an execution error.
    #[test]
    fn setup_failures_are_classified_for_the_retry_layer() {
        let signed_out = unavailable_error(
            CodingAgentKind::ClaudeCode,
            &availability(CodingAgentKind::ClaudeCode, AuthState::SignedOut),
        );
        assert!(matches!(signed_out, ProviderError::Authentication(_)));

        let missing = unavailable_error(
            CodingAgentKind::Codex,
            &availability(CodingAgentKind::Codex, AuthState::NotInstalled),
        );
        assert!(matches!(missing, ProviderError::ExecutionError(_)));
    }

    /// Every setup error names the exact command the user should run. The whole
    /// reason this function exists instead of a generic string is that the user
    /// cannot fix any of these states without being told the command.
    #[test]
    fn every_actionable_error_names_a_command() {
        for kind in CodingAgentKind::all() {
            for (auth, expected) in [
                (AuthState::NotInstalled, kind.install_hint()),
                (AuthState::SignedOut, kind.login_command()),
                (AuthState::SignedInWithApiKey, kind.login_command()),
            ] {
                let msg = unavailable_error(kind, &availability(kind, auth)).to_string();
                assert!(
                    msg.contains(expected),
                    "{kind:?} error should tell the user to run `{expected}`, got: {msg}"
                );
            }
        }
    }

    /// The signed-out message must state that Biorouter is not doing the login.
    /// That sentence is the user-visible half of the compliance posture, so it is
    /// pinned rather than left to future editing.
    #[test]
    fn signed_out_message_states_that_biorouter_does_not_broker_the_login() {
        let msg = unavailable_error(
            CodingAgentKind::ClaudeCode,
            &availability(CodingAgentKind::ClaudeCode, AuthState::SignedOut),
        )
        .to_string();
        assert!(msg.contains("never passes through Biorouter"));
    }
}
