//! Issue #56 Task 31 — the CLI's privacy surfaces (R10).
//!
//! Every repair affordance Phase 4 shipped before this is a GUI card, and the
//! CLI is not an optional surface: a user who never opens the desktop app must
//! still be able to see what tier a chat is on, understand why one refused to
//! start, and get out of it.
//!
//! Three things live here, and they are pure functions with the registry lookup
//! split off, so all of the wording is testable without a terminal:
//!
//! * [`tier_row`] — the `privacy` row of the session banner.
//! * [`terminal_refusal`] — the sentence a private chat gets when the provider
//!   resolved for it is public, whichever of the four precedence sources
//!   produced it, plus the repair.
//! * [`available_private_models`] — what the repair offers, read from the real
//!   provider registry so a fourth private provider appears with no edit here.
//!
//! ⚠ **The list is sorted, and the sort is load-bearing.** `providers()`
//! iterates a `HashMap`, so its order differs between runs; a refusal that named
//! whichever private model came out first would print a different "exact re-run
//! command" every time, which is worse than printing none.

use biorouter::privacy::{ProviderTier, SessionClassification};

/// A private model the user could move this chat onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateModel {
    /// The `--provider` value.
    pub name: String,
    /// What the settings grid calls it.
    pub display_name: String,
    /// Whether inference runs on this machine. Display only — it is what splits
    /// the private tier into "on this machine" and "inside the institution",
    /// and it is what [`sort_private_models`] orders on.
    pub runs_locally: bool,
}

/// Institutional first, then by name.
///
/// Both halves are deliberate. **Institutional first** because the chat being
/// repaired is usually private *because* it reached institutional data, and a
/// local model cannot pick that work back up — offering `llamacpp` as the first
/// answer to "my OMOP chat will not start" is a suggestion that does not work.
/// **Then by name**, because the first element becomes an exact command printed
/// to a user, and an unstable one is a command that is wrong half the time.
pub fn sort_private_models(models: &mut [PrivateModel]) {
    models.sort_by(|a, b| {
        a.runs_locally
            .cmp(&b.runs_locally)
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Every provider this install publishes as Private, sorted.
///
/// The registry's declared tier, not an instance's: nothing is bound here, and
/// building one per candidate would need every private provider's credentials
/// just to print a list. The consequence is stated where it matters — an
/// `ollama` re-pointed off the machine is listed, and Gate A refuses it on the
/// bind, which reads the instance.
pub async fn available_private_models() -> Vec<PrivateModel> {
    let mut models: Vec<PrivateModel> = biorouter::providers::providers()
        .await
        .into_iter()
        .filter(|(metadata, _)| metadata.tier == ProviderTier::Private)
        .map(|(metadata, _)| PrivateModel {
            name: metadata.name,
            display_name: metadata.display_name,
            runs_locally: metadata.runs_locally,
        })
        .collect();
    sort_private_models(&mut models);
    models
}

/// The `privacy` row of the session banner.
///
/// It says the tier and, for a private chat, the one consequence a user needs to
/// hold in their head: this chat will refuse a public model. The public row is
/// deliberately terse — the full DR-17 disclosure is printed by `biorouter
/// configure` when the provider is chosen, and repeating it on every session
/// start is how a disclosure becomes wallpaper.
pub fn tier_row(classification: SessionClassification) -> String {
    match classification {
        SessionClassification::Private => {
            "Private — only a model hosted inside the institution may run here".to_string()
        }
        SessionClassification::Public => "Public".to_string(),
    }
}

/// Where the provider that is about to be refused came from.
///
/// `biorouter-cli/src/session/builder.rs` resolves it as `--provider` flag →
/// saved session provider → the workflow's `biorouter_provider` → global
/// default. Three of those four are things the user did not type just now, so
/// "why is this chat refusing to start" has four different answers and only one
/// of them is obvious.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSource {
    /// `--provider` on this very command line.
    CliFlag,
    /// The provider stored on the session row being resumed.
    SavedSession,
    /// `settings.biorouter_provider` in the workflow being run.
    Workflow,
    /// `BIOROUTER_PROVIDER` / the configured default.
    GlobalDefault,
}

impl ProviderSource {
    /// The clause that says where the model came from. Present tense, no blame,
    /// and different for each source — a refusal that says the same thing for
    /// all four is a refusal that explains none of them.
    fn because(self, provider: &str) -> String {
        match self {
            Self::CliFlag => format!("`--provider {provider}` names a public model"),
            Self::SavedSession => {
                format!("this chat is still set to `{provider}`, which is a public model")
            }
            Self::Workflow => format!("this workflow pins `{provider}`, which is a public model"),
            Self::GlobalDefault => {
                format!("your default model is `{provider}`, which is a public model")
            }
        }
    }
}

/// The whole terminal refusal: why, what is available, and the exact commands
/// that fix it.
///
/// §14.4: it names the session **id**, the provider and the tier. The id is the
/// argument to both printed commands, so it has to be here — and it is the one
/// piece of a session the user already typed. The chat's name, its working
/// directory and its contents are not, and no caller can supply them: the
/// signature has nowhere to put them.
///
/// An empty `private_models` is not an impossible state — a stripped install can
/// have none — and it must not print `--provider ` with nothing after it.
pub fn terminal_refusal(
    session_id: &str,
    provider: &str,
    source: ProviderSource,
    private_models: &[PrivateModel],
) -> String {
    format!(
        "This chat is private, so only a model hosted inside the institution may run in it, and \
         {}. Nothing has been sent and the chat is unchanged.\n{}",
        source.because(provider),
        repair_block(session_id, private_models)
    )
}

/// The half of [`terminal_refusal`] that says what to DO — split out so the
/// workflow module's own load-time sentence (which explains a *shared artefact*,
/// not a precedence slot, and is therefore worded there) can be followed by the
/// same repair rather than by a second copy of it.
pub fn repair_block(session_id: &str, private_models: &[PrivateModel]) -> String {
    let mut out = String::new();
    match private_models.first() {
        Some(first) => {
            let list = private_models
                .iter()
                .map(|m| format!("{} ({})", m.display_name, m.name))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("\nPrivate models on this install: {list}.\n"));
            out.push_str(&format!(
                "\nRe-run this chat on one of them:\n\n    biorouter session --resume --id \
                 {session_id} --provider {}\n",
                first.name
            ));
        }
        None => {
            out.push_str(
                "\nThis install publishes no private models. Configure one with `biorouter \
                 configure` before resuming this chat.\n",
            );
        }
    }
    out.push_str(&format!(
        "\nOr, if this chat's contents are not private after all, declassify it:\n\n    \
         biorouter session declassify {session_id}\n"
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::privacy::SessionClassification;

    fn versa() -> PrivateModel {
        PrivateModel {
            name: "versa_azure".to_string(),
            display_name: "Versa API Azure".to_string(),
            runs_locally: false,
        }
    }

    fn llamacpp() -> PrivateModel {
        PrivateModel {
            name: "llamacpp".to_string(),
            display_name: "Llama Server".to_string(),
            runs_locally: true,
        }
    }

    #[test]
    fn the_cli_prints_the_tier_and_the_exact_re_run_command() {
        // (a) the tier, printed at session start.
        assert!(tier_row(SessionClassification::Private).contains("Private"));
        assert!(tier_row(SessionClassification::Public).contains("Public"));

        // (b) the refusal names the available private models and the exact
        //     command that re-runs THIS chat on one of them.
        let refusal = terminal_refusal(
            "20260801_7",
            "anthropic",
            ProviderSource::GlobalDefault,
            &[versa(), llamacpp()],
        );
        assert!(
            refusal.contains("--provider versa_azure"),
            "the refusal must carry the exact re-run command: {refusal}"
        );
        assert!(
            refusal.contains("Versa"),
            "the refusal must list the available private models: {refusal}"
        );
        assert!(refusal.contains("Llama Server"), "{refusal}");
        assert!(
            refusal.contains("biorouter session declassify 20260801_7"),
            "the other way out is the escape hatch: {refusal}"
        );
    }

    /// The named provider is the institutional one, and it is chosen rather than
    /// happened upon: `providers()` iterates a `HashMap`, so an unsorted list
    /// would name a different model on every run and the "exact re-run command"
    /// would be a lie half the time.
    #[test]
    fn the_named_model_is_deterministic_and_institutional_first() {
        let mut a = vec![llamacpp(), versa()];
        sort_private_models(&mut a);
        let mut b = vec![versa(), llamacpp()];
        sort_private_models(&mut b);
        assert_eq!(a[0].name, "versa_azure");
        assert_eq!(b[0].name, "versa_azure");
    }

    /// §14.4: a refusal names the tier and the model and nothing about the
    /// conversation.
    #[test]
    fn the_refusal_carries_no_session_content() {
        let refusal = terminal_refusal(
            "20260801_7",
            "anthropic",
            ProviderSource::SavedSession,
            &[versa()],
        );
        for content in ["Patient MRN 4471 workup", "phi/cohort-3"] {
            assert!(!refusal.contains(content), "{refusal}");
        }
    }

    /// Each of the four precedence sources says where the public model came
    /// from, because "why is my private chat refusing to start" has four
    /// different answers and three of them are not the one the user expects.
    #[test]
    fn every_precedence_source_explains_itself() {
        let mut seen: Vec<String> = vec![];
        for source in [
            ProviderSource::CliFlag,
            ProviderSource::SavedSession,
            ProviderSource::Workflow,
            ProviderSource::GlobalDefault,
        ] {
            let text = terminal_refusal("20260801_7", "anthropic", source, &[versa()]);
            assert!(text.contains("anthropic"), "{text}");
            assert!(!seen.contains(&text), "two sources gave the same sentence");
            seen.push(text);
        }
        assert_eq!(seen.len(), 4);
    }

    /// A stripped install with no private provider at all must not print
    /// `--provider ` with nothing after it, which reads as a command to run.
    #[test]
    fn with_no_private_model_the_refusal_offers_no_empty_command() {
        // The REPAIR, not the whole refusal: `ProviderSource::CliFlag`'s own
        // clause legitimately quotes `--provider anthropic` back at the user,
        // and an assertion over the whole string cannot tell that apart from the
        // dangling command this is about.
        let repair = repair_block("20260801_7", &[]);
        assert!(!repair.contains("--provider"), "{repair}");
        assert!(repair.contains("biorouter configure"), "{repair}");
        // The escape hatch survives: it needs no private model to exist.
        assert!(
            repair.contains("biorouter session declassify 20260801_7"),
            "{repair}"
        );
        // …and with one, the command is complete.
        assert!(repair_block("20260801_7", &[versa()]).contains("--provider versa_azure"));
    }

    /// The list comes from the real registry, so a fourth private provider
    /// appears here with no edit in this file — and `versa_azure` really is in
    /// it, which is what keeps the fixture-driven tests above honest.
    #[tokio::test]
    async fn the_private_models_offered_come_from_the_registry() {
        let models = available_private_models().await;
        assert!(models.iter().any(|m| m.name == "versa_azure"), "{models:?}");
        assert!(models.iter().any(|m| m.name == "llamacpp"), "{models:?}");
        assert!(
            !models.iter().any(|m| m.name == "anthropic"),
            "a public model must never be offered as the repair: {models:?}"
        );
        assert_eq!(
            models[0].name, "versa_azure",
            "the registry list must arrive sorted, or the re-run command is nondeterministic"
        );
    }
}
