//! Ingest documents, folders and URLs into a knowledge base from a chat.
//!
//! # Why this module exists
//!
//! The transactional ingest pipeline has always been there — `macros::ingest`
//! materializes the raw source, opens a git transaction branch, runs the bounded
//! KB sub-agent, validates what it wrote, commits or aborts, rebuilds the graph
//! cache and re-scans the committed tree. The desktop reaches it through
//! `POST /knowledge/bases/{id}/ingest`.
//!
//! A **chat** could not reach it at all. The knowledge extension exposes only
//! low-level primitives (`kb_add_raw_source`, `kb_write_page`, `kb_begin_txn`,
//! …), so a model asked to "ingest these PDFs" improvised: it extracted text by
//! hand, staged raw sources, assembled pages in large `execute_code` scripts and
//! wrote them one at a time. Every guarantee of the real pipeline — one
//! transaction per source, abort on failure, the tail verification that says
//! whether curated pages actually exist — was absent, and the observed run ended
//! with raw sources on disk and **no knowledge pages** (issue #108).
//!
//! This module is the chat-side entry to that same macro. It owns only what the
//! macro does not: turning what a person named into concrete sources, running a
//! batch, and reporting per-source truth.
//!
//! # What it deliberately does not own
//!
//! * **The transaction, the ratchet and the verification.** All three live in
//!   `macros::ingest`, which is called once per source. In particular the
//!   privacy ratchet (issue #56) is the macro's — this module adds **no** write
//!   choke point of its own, so there is no fifth place for a base's tier to be
//!   raised, and none for a read-only path to raise one by accident.
//! * **Provider selection.** The caller decides which provider runs the
//!   sub-agent and passes a completer factory built from it. Ordinary providers
//!   receive tools in the request; coding-agent providers receive the same
//!   dispatcher through their scoped bridge. A provider with neither mechanism
//!   is *reported* ([`tool_capability_refusal`]), never silently swapped for one
//!   that can — that would move the user's inference onto a different bill.

use std::path::PathBuf;

use biorouter_mcp::knowledge::{
    convert::SourceInput,
    macros::ingest::{ingest, IngestArgs, IngestCurationFailure, IngestFailurePhase},
    service::KnowledgeService,
    source_paths::{self, WarningLevel},
    subagent::{events::SubAgentEvent, loop_::SubAgentBounds},
};

use crate::providers::base::Provider;

// ---------------------------------------------------------------------------
// What the caller asked for
// ---------------------------------------------------------------------------

/// One thing the caller named, before expansion.
///
/// A [`Self::Path`] may be a file, a folder or an archive: it is expanded by
/// [`source_paths::expand_ingest_path`], the same expander the desktop dropzone
/// uses, so a folder of PDFs behaves identically from a chat and from the
/// Knowledge view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceSpec {
    Path(PathBuf),
    Url(String),
    Text { text: String, title: Option<String> },
}

impl SourceSpec {
    /// How this source is named back to the user. Never the content itself.
    pub fn label(&self) -> String {
        match self {
            Self::Path(p) => p.display().to_string(),
            Self::Url(u) => u.clone(),
            Self::Text { title, .. } => title.clone().unwrap_or_else(|| "pasted text".to_string()),
        }
    }
}

pub use super::provider_completer::CompleterFactory;

pub struct SourceIngestArgs {
    pub kb_id: String,
    /// The capability of the provider that will actually run the sub-agent
    /// (issue #56). Required and non-`Option` for the reason
    /// [`crate::knowledge::conversation_ingest::ConversationIngestArgs`] gives:
    /// an omission must be a compile error, not a quiet `false`.
    pub caller_capability: crate::privacy::ProviderTier,
    /// Whose agreements cover that same provider — DR-26's third axis. Off the
    /// same binding as the tier above, never a second lookup.
    pub caller_affiliation: Option<crate::privacy::affiliation::ModelAffiliation>,
    pub sources: Vec<SourceSpec>,
    pub completer: CompleterFactory,
    pub focus: Option<String>,
    pub bounds: SubAgentBounds,
    pub event_sink: Option<tokio::sync::mpsc::UnboundedSender<SubAgentEvent>>,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
    /// `provider/model`, carried into the report so the answer names the model
    /// the work ran on. An acceptance criterion of issue #108: provider choice
    /// and billing mode must be **visible**, not inferred.
    pub model_label: String,
}

// ---------------------------------------------------------------------------
// What came back
// ---------------------------------------------------------------------------

/// The macro's tail scan, reduced to what a chat answer can carry.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct VerificationSummary {
    /// True only when the scan RAN and found no errors — never a quiet `true`
    /// for a scan that could not run. See `macros::ingest::Verification`.
    pub ok: bool,
    pub errors: usize,
    pub warnings: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_error: Option<String>,
}

/// One source's ending. `ok` says whether the complete ingest pipeline finished.
/// A failed outcome keeps enough phase and commit provenance in its fields and
/// error to distinguish rolled-back curation from a post-commit refresh failure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceOutcome {
    pub label: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// The commit that retained the raw source. This can be present even when
    /// `ok` is false and there is no curation commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_commit_sha: Option<String>,
    /// The commit containing curated pages. It can also be present on a failed
    /// outcome when curation committed but a post-commit refresh failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_phase: Option<IngestFailurePhase>,
    #[serde(default)]
    pub steps: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Something the path expander said about the drop. Reported verbatim rather
/// than re-worded: it already names the file and the remedy.
///
/// ⚠ A note is **not** the same as a skipped file. Most notes describe a file
/// that was left out, but `curation_warning` fires for a file that *was* staged.
/// The authoritative "what will be ingested" is the expander's file list, which
/// is what this module iterates; the notes explain the rest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExpansionNote {
    pub level: String,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceIngestReport {
    pub kb_id: String,
    pub model: String,
    pub outcomes: Vec<SourceOutcome>,
    #[serde(default)]
    pub notes: Vec<ExpansionNote>,
}

impl SourceIngestReport {
    pub fn succeeded(&self) -> usize {
        self.outcomes.iter().filter(|o| o.ok).count()
    }

    pub fn failed(&self) -> usize {
        self.outcomes.iter().filter(|o| !o.ok).count()
    }

    /// The answer a chat reads.
    ///
    /// ⚠ **It never claims completion when only raw sources exist.** A source
    /// whose sub-agent wrote no pages comes back as an `Err` from the macro,
    /// which has already aborted the curation transaction while retaining the
    /// raw source — so it is counted a failure and both outcomes are stated.
    /// Reporting "ingested 3 sources" for three staged PDFs and zero curated
    /// pages is exactly the outcome issue #108 is about.
    pub fn summary(&self) -> String {
        let curated = self
            .outcomes
            .iter()
            .filter(|outcome| outcome.commit_sha.is_some())
            .count();
        let total = self.outcomes.len();
        let outcome_uncertain = self
            .outcomes
            .iter()
            .any(|outcome| outcome.failure_phase == Some(IngestFailurePhase::OutcomeUncertain));
        let all_failures_rolled_back = self
            .outcomes
            .iter()
            .filter(|outcome| !outcome.ok)
            .all(|outcome| outcome.failure_phase == Some(IngestFailurePhase::RolledBack));
        let mut out = if total == 0 {
            format!(
                "No sources were available to curate into knowledge base '{}' on {}.",
                self.kb_id, self.model
            )
        } else if outcome_uncertain {
            format!(
                "Confirmed curation commits for {curated} of {total} source(s) in knowledge base \
                 '{}' on {}; at least one outcome is uncertain. Refresh the knowledge base and \
                 history before retrying.",
                self.kb_id, self.model
            )
        } else if curated == 0 && all_failures_rolled_back {
            format!(
                "Curated 0 of {total} source(s) into knowledge base '{}' on {}. Curation rolled \
                 back for each failure; raw sources were retained in the base and its history.",
                self.kb_id, self.model
            )
        } else if curated == 0 {
            format!(
                "Curated 0 of {total} source(s) into knowledge base '{}' on {}. See each source's \
                 status for whether curation started and which raw source was retained.",
                self.kb_id, self.model
            )
        } else {
            format!(
                "Curated {curated} of {total} source(s) into knowledge base '{}' on {}.",
                self.kb_id, self.model
            )
        };
        for outcome in &self.outcomes {
            out.push_str("\n  ");
            if outcome.ok {
                let sha: String = outcome
                    .commit_sha
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .take(8)
                    .collect();
                let raw = outcome.raw_commit_sha.as_deref().map_or_else(
                    || "raw source already retained".to_string(),
                    |raw_sha| format!("raw commit {}", raw_sha.chars().take(8).collect::<String>()),
                );
                out.push_str(&format!(
                    "✓ {} — source {}, {raw}, curation commit {sha}, {} sub-agent step(s)",
                    outcome.label,
                    outcome.source_id.as_deref().unwrap_or("?"),
                    outcome.steps
                ));
                if let Some(v) = &outcome.verification {
                    if let Some(err) = &v.scan_error {
                        out.push_str(&format!("; the post-commit check could not run: {err}"));
                    } else if v.errors > 0 || v.warnings > 0 {
                        out.push_str(&format!(
                            "; verification: {} error(s), {} warning(s)",
                            v.errors, v.warnings
                        ));
                    } else {
                        out.push_str("; verified clean");
                    }
                }
            } else {
                out.push_str(&format!(
                    "✗ {} — {}",
                    outcome.label,
                    outcome.error.as_deref().unwrap_or("failed")
                ));
            }
        }
        for note in &self.notes {
            out.push_str(&format!(
                "\n  [{}] {}: {}",
                note.level, note.title, note.message
            ));
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Provider capability
// ---------------------------------------------------------------------------

/// `Some(refusal)` when this provider cannot drive the ingest sub-agent, in the
/// sentence the user reads; `None` when it can.
///
/// Ordinary providers drive the macro through request tool calls. Coding-agent
/// providers drive it through `ProviderCompleter::complete_with_dispatch`,
/// which installs a scoped MCP bridge to the macro's dispatcher. Providers with
/// neither mechanism must be rejected before the source is touched; otherwise a
/// tool-less completion is indistinguishable from a model that simply had
/// nothing more to do.
///
/// ⚠ **A capability question, never a name check, and never a substitution.**
/// The refusal names the model, says why, and hands back the two choices that
/// are the user's to make. Quietly re-routing the work to an API provider would
/// change which account the inference is billed to — issue #108 asks for a
/// reported mismatch precisely so that cannot happen silently.
pub fn tool_capability_refusal(provider: &dyn Provider) -> Option<String> {
    if provider.supports_tool_calls() {
        return None;
    }
    Some(format!(
        "Knowledge ingestion cannot run on `{}` because that provider cannot drive the bounded \
         Knowledge tool loop. Nothing was ingested and nothing was billed. Either switch this \
         chat to a model that can use tools, or pass `model` to run this one ingest on such a \
         model while the chat stays where it is.",
        provider.get_name()
    ))
}

// ---------------------------------------------------------------------------
// The batch
// ---------------------------------------------------------------------------

/// A second `SubAgentBounds` with the same three values.
///
/// The type is neither `Clone` nor `Copy` in `biorouter-mcp` and every field is,
/// so a batch — which needs one per macro call — rebuilds rather than reaching
/// across the crate boundary to add a derive for one caller's convenience.
fn same_bounds(b: &SubAgentBounds) -> SubAgentBounds {
    SubAgentBounds {
        max_steps: b.max_steps,
        max_wall: b.max_wall,
        max_tokens: b.max_tokens,
    }
}

/// Expand what the caller named into the concrete sources the macro will see.
///
/// Local paths go through [`source_paths::expand_ingest_path`] — the desktop
/// dropzone's expander — so folders, archives, size caps and unreadable-binary
/// detection behave identically on both surfaces rather than being re-derived
/// here.
fn expand(spec: &SourceSpec) -> (Vec<(String, SourceInput)>, Vec<ExpansionNote>) {
    match spec {
        SourceSpec::Url(url) => (vec![(url.clone(), SourceInput::Url(url.clone()))], vec![]),
        SourceSpec::Text { text, title } => (
            vec![(
                spec.label(),
                SourceInput::Text {
                    text: text.clone(),
                    title: title.clone(),
                },
            )],
            vec![],
        ),
        SourceSpec::Path(path) => match source_paths::expand_ingest_path(path) {
            Err(e) => (
                vec![],
                vec![ExpansionNote {
                    level: "error".into(),
                    title: format!("{} could not be read", path.display()),
                    message: format!("{e:#}"),
                }],
            ),
            Ok(set) => {
                let notes = set
                    .warnings
                    .into_iter()
                    .map(|w| ExpansionNote {
                        level: match w.level {
                            WarningLevel::Error => "error".into(),
                            WarningLevel::Warning => "warning".into(),
                        },
                        title: w.title,
                        message: w.message,
                    })
                    .collect();
                let files = set
                    .files
                    .into_iter()
                    .map(|f| (f.relative_path, SourceInput::Path(f.path)))
                    .collect();
                (files, notes)
            }
        },
    }
}

type ExpandedSources = (Vec<(String, SourceInput)>, Vec<ExpansionNote>);

fn expand_sources(sources: &[SourceSpec]) -> anyhow::Result<ExpandedSources> {
    if sources.is_empty() {
        anyhow::bail!(
            "no sources given: pass `sources` (a list of file paths, folders or URLs), or one of \
             `path` / `url` / `text`"
        );
    }

    let mut items = Vec::new();
    let mut notes = Vec::new();
    for spec in sources {
        let (found, spec_notes) = expand(spec);
        notes.extend(spec_notes);
        items.extend(found);
    }

    if items.is_empty() {
        let detail = if notes.is_empty() {
            String::new()
        } else {
            format!(
                " {}",
                notes
                    .iter()
                    .map(|n| format!("{}: {}", n.title, n.message))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        anyhow::bail!(
            "nothing to ingest: none of the named sources yielded readable material.{detail}"
        );
    }

    Ok((items, notes))
}

fn cancelled_outcome(label: String) -> SourceOutcome {
    SourceOutcome {
        label,
        ok: false,
        source_id: None,
        raw_commit_sha: None,
        commit_sha: None,
        failure_phase: Some(IngestFailurePhase::NotStarted),
        steps: 0,
        verification: None,
        error: Some("cancelled before this source started".to_string()),
    }
}

async fn ingest_source(
    svc: &KnowledgeService,
    args: &SourceIngestArgs,
    label: String,
    source: SourceInput,
) -> SourceOutcome {
    let outcome = ingest(
        svc,
        IngestArgs {
            kb_id: args.kb_id.clone(),
            // Issue #56: the ProviderTier -> bool crossing, the same one
            // `conversation_ingest` makes, because `IngestArgs` lives in
            // `biorouter-mcp`, which cannot name `ProviderTier`.
            caller_is_private: args.caller_capability.is_private(),
            caller_affiliation: crate::privacy::affiliation::caller_affiliation(
                args.caller_affiliation,
            ),
            source,
            completer: (args.completer)(),
            focus: args.focus.clone(),
            bounds: same_bounds(&args.bounds),
            event_sink: args.event_sink.clone(),
            cancel: args.cancel.clone(),
        },
    )
    .await;

    match outcome {
        Ok(r) => SourceOutcome {
            label,
            ok: true,
            source_id: Some(r.source_id),
            raw_commit_sha: r.raw_commit_sha,
            commit_sha: Some(r.commit_sha),
            failure_phase: None,
            steps: r.steps,
            verification: Some(VerificationSummary {
                ok: r.verification.ok,
                errors: r.verification.errors,
                warnings: r.verification.warnings,
                scan_error: r.verification.scan_error,
            }),
            error: None,
        },
        Err(e) => {
            let retained = e.downcast_ref::<IngestCurationFailure>();
            SourceOutcome {
                label,
                ok: false,
                source_id: retained.map(|failure| failure.source_id.clone()),
                raw_commit_sha: retained.and_then(|failure| failure.raw_commit_sha.clone()),
                commit_sha: retained.and_then(|failure| failure.curation_commit_sha.clone()),
                failure_phase: retained.map(|failure| failure.phase),
                steps: 0,
                verification: None,
                // The macro's own words. It already says whether the run wrote
                // nothing, how many steps it took and which tool calls failed;
                // a second wording here would be a second answer to drift from.
                error: Some(format!("{e:#}")),
            }
        }
    }
}

/// Ingest every named source into one knowledge base, one transaction each.
///
/// **One macro call per source, and that is the unit of atomicity.** A source
/// that fails aborts its own transaction and leaves the others alone, which is
/// what makes per-source status meaningful and a retry safe. The alternative —
/// one transaction over the batch — would make one bad PDF discard four good
/// digests.
///
/// A cancelled batch stops at the current source; the ones not reached are
/// reported as such rather than silently dropped.
pub async fn ingest_sources(
    svc: &KnowledgeService,
    args: SourceIngestArgs,
) -> anyhow::Result<SourceIngestReport> {
    let (items, notes) = expand_sources(&args.sources)?;
    let mut report = SourceIngestReport {
        kb_id: args.kb_id.clone(),
        model: args.model_label.clone(),
        outcomes: Vec::with_capacity(items.len()),
        notes,
    };

    for (label, source) in items {
        let outcome = if args
            .cancel
            .as_ref()
            .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
        {
            cancelled_outcome(label)
        } else {
            ingest_source(svc, &args, label, source).await
        };
        report.outcomes.push(outcome);
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter_mcp::knowledge::subagent::loop_::Completer;

    struct ToollessProvider;

    #[async_trait::async_trait]
    impl Provider for ToollessProvider {
        fn metadata() -> crate::providers::base::ProviderMetadata {
            crate::providers::base::ProviderMetadata::empty()
        }

        fn get_name(&self) -> &str {
            "tool-less-fixture"
        }

        fn get_model_config(&self) -> crate::model::ModelConfig {
            crate::model::ModelConfig::new_or_fail("tool-less-model")
        }

        fn supports_tool_calls(&self) -> bool {
            false
        }

        async fn complete_with_model(
            &self,
            _model_config: &crate::model::ModelConfig,
            _system: &str,
            _messages: &[crate::conversation::message::Message],
            _tools: &[rmcp::model::Tool],
        ) -> Result<
            (
                crate::conversation::message::Message,
                crate::providers::base::ProviderUsage,
            ),
            crate::providers::errors::ProviderError,
        > {
            unreachable!("the capability check never calls the provider")
        }
    }

    fn outcome(label: &str, ok: bool) -> SourceOutcome {
        SourceOutcome {
            label: label.into(),
            ok,
            source_id: ok.then(|| "src-1".to_string()),
            raw_commit_sha: Some(if ok {
                "rawabc1234567890".to_string()
            } else {
                "rawdef1234567890".to_string()
            }),
            commit_sha: ok.then(|| "abcdef1234567890".to_string()),
            failure_phase: (!ok).then_some(IngestFailurePhase::RolledBack),
            steps: if ok { 7 } else { 0 },
            verification: ok.then(VerificationSummary::default),
            error: (!ok).then(|| {
                "ingest wrote no knowledge pages for source src-2 (3 step(s), ended: \
                 NoMoreToolCalls). Curation rolled back; raw source src-2 was retained in commit \
                 rawdef1234567890."
                    .to_string()
            }),
        }
    }

    fn report(outcomes: Vec<SourceOutcome>) -> SourceIngestReport {
        SourceIngestReport {
            kb_id: "ms-papers".into(),
            model: "anthropic/claude-opus-5".into(),
            outcomes,
            notes: vec![],
        }
    }

    /// The headline defect of issue #108: three PDFs staged, no curated pages,
    /// and a run that reported success. A report where nothing curated must say
    /// both that curation rolled back and that the raw inputs remain.
    #[test]
    fn a_run_that_curated_nothing_never_reads_as_ingested() {
        let text = report(vec![outcome("a.pdf", false), outcome("b.pdf", false)]).summary();
        assert!(
            text.starts_with(
                "Curated 0 of 2 source(s) into knowledge base 'ms-papers' on \
                 anthropic/claude-opus-5."
            ),
            "got: {text}"
        );
        assert!(text.contains("Curation rolled back"), "got: {text}");
        assert!(text.contains("raw sources were retained"), "got: {text}");
        assert!(!text.contains("base is unchanged"), "got: {text}");
    }

    /// Per-source status, and a partial batch that is honest about both halves.
    #[test]
    fn a_partial_batch_reports_each_source_separately() {
        let text = report(vec![
            outcome("papers/a.pdf", true),
            outcome("papers/b.pdf", false),
        ])
        .summary();
        assert!(text.starts_with("Curated 1 of 2 source(s)"), "got: {text}");
        assert!(text.contains(
            "✓ papers/a.pdf — source src-1, raw commit rawabc12, curation commit abcdef12"
        ));
        assert!(text.contains("✗ papers/b.pdf — ingest wrote no knowledge pages"));
    }

    /// The model the work ran on is named, always — issue #108 asks for the
    /// provider choice to be visible rather than inferred.
    #[test]
    fn the_summary_always_names_the_model_it_ran_on() {
        for outcomes in [vec![outcome("a.pdf", true)], vec![outcome("a.pdf", false)]] {
            let text = report(outcomes).summary();
            assert!(
                text.contains("anthropic/claude-opus-5"),
                "the model must be named in every summary: {text}"
            );
        }
    }

    /// An unverified scan is not a clean one — the distinction
    /// `macros::ingest::Verification` exists to make, carried through.
    #[test]
    fn a_scan_that_could_not_run_is_reported_as_such() {
        let mut o = outcome("a.pdf", true);
        o.verification = Some(VerificationSummary {
            ok: false,
            scan_error: Some("could not read the tree".into()),
            ..Default::default()
        });
        let text = report(vec![o]).summary();
        assert!(
            text.contains("the post-commit check could not run"),
            "got: {text}"
        );
        assert!(!text.contains("verified clean"), "got: {text}");
    }

    /// Both request-tool and bridge-tool providers are accepted. A provider
    /// with neither mechanism is refused before any source is touched and is
    /// never silently replaced.
    #[test]
    fn providers_with_a_real_tool_path_are_accepted_and_others_are_refused() {
        use crate::model::ModelConfig;
        use crate::providers::claude_code::ClaudeCodeProvider;
        use crate::providers::ollama::OllamaProvider;

        // A real provider, built the way a declarative JSON file builds one —
        // `from_custom_config` rather than `from_env`, which is async and reads
        // the user's config. Not a stub: production `supports_tool_calls` is the
        // fact under test.
        let config = crate::config::declarative_providers::DeclarativeProviderConfig {
            name: "ingest-fixture".to_string(),
            engine: crate::config::declarative_providers::ProviderEngine::Ollama,
            display_name: "Ingest fixture".to_string(),
            description: None,
            api_key_env: "NOT_USED".to_string(),
            base_url: "http://localhost:11434".to_string(),
            models: vec![],
            headers: None,
            timeout_seconds: None,
            supports_streaming: None,
        };
        let ok = OllamaProvider::from_custom_config(ModelConfig::new_or_fail("qwen3"), config)
            .expect("a declarative ollama provider must construct");
        assert!(
            tool_capability_refusal(&ok).is_none(),
            "an ordinary API provider drives the sub-agent fine"
        );

        let coding_agent = ClaudeCodeProvider::for_tests(
            std::path::PathBuf::from("/usr/bin/claude"),
            "claude-sonnet-4-6",
        );
        assert!(
            tool_capability_refusal(&coding_agent).is_none(),
            "the provider-driven bridge makes macro tools reachable"
        );

        let unsupported = ToollessProvider;
        let refusal = tool_capability_refusal(&unsupported)
            .expect("a provider with no tool path must be refused");
        assert!(
            refusal.contains(unsupported.get_name()),
            "the refusal must name the model it refused: {refusal}"
        );
        assert!(
            refusal.contains("Nothing was ingested"),
            "the refusal must say nothing happened: {refusal}"
        );
        // Both remedies, because they are the user's choice to make and the
        // tool's job is to state them rather than pick one.
        assert!(refusal.contains("switch this chat"), "got: {refusal}");
        assert!(refusal.contains("`model`"), "got: {refusal}");
    }

    #[test]
    fn a_url_and_a_text_source_expand_to_themselves() {
        let (items, notes) = expand(&SourceSpec::Url("https://example.org/p.pdf".into()));
        assert_eq!(items.len(), 1);
        assert!(notes.is_empty());
        assert!(matches!(items[0].1, SourceInput::Url(_)));

        let (items, _) = expand(&SourceSpec::Text {
            text: "hello".into(),
            title: Some("Note".into()),
        });
        assert_eq!(items[0].0, "Note");
    }

    // --- end to end, against the real macro --------------------------------

    use biorouter_mcp::knowledge::page_fixtures::valid_page;
    use biorouter_mcp::knowledge::paths;
    use biorouter_mcp::knowledge::subagent::loop_::{LlmMessage, LlmReply, LlmToolCall};
    use rmcp::model::Tool;

    /// Pops canned replies, like the ingest macro's own fixture. A mock rather
    /// than `test_mode::TestModeCompleter`, whose behaviour is switched by a
    /// process-wide environment variable another test in this binary could have
    /// set — this one answers only to its own queue.
    struct MockCompleter {
        replies: tokio::sync::Mutex<Vec<LlmReply>>,
    }

    #[async_trait::async_trait]
    impl Completer for MockCompleter {
        async fn complete(
            &self,
            _system: &str,
            _messages: &[LlmMessage],
            _tools: &[Tool],
        ) -> anyhow::Result<LlmReply> {
            let mut q = self.replies.lock().await;
            if q.is_empty() {
                // A run that outlives its script is a broken fixture, not a
                // finished agent: returning "done" here would let a test pass by
                // accident.
                panic!("the mock completer ran out of canned replies");
            }
            Ok(q.remove(0))
        }
    }

    fn fresh_svc() -> (tempfile::TempDir, KnowledgeService) {
        let dir = tempfile::tempdir().unwrap();
        let svc = KnowledgeService::new(dir.path().to_path_buf());
        svc.create_base("k", "K", None).unwrap();
        (dir, svc)
    }

    fn file(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    /// A real one-page PDF with an extractable text layer.
    ///
    /// The acceptance criterion of issue #108 is stated in PDFs, and a markdown
    /// fixture would route around the whole `convert::pdf` branch — the layer
    /// the reported run actually fought with. Built the same way that
    /// converter's own tests build theirs, so there is no binary blob in the
    /// repository and no second idea of what a valid fixture is.
    fn pdf(dir: &std::path::Path, name: &str, text: &str) -> PathBuf {
        use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str};

        let mut doc = Pdf::new();
        let (catalog, tree, page_id, font, content_id) = (
            Ref::new(1),
            Ref::new(2),
            Ref::new(3),
            Ref::new(4),
            Ref::new(5),
        );
        doc.catalog(catalog).pages(tree);
        doc.pages(tree).kids([page_id]).count(1);
        let mut page = doc.page(page_id);
        page.parent(tree)
            .media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
            .resources()
            .fonts()
            .pair(Name(b"F1"), font);
        page.contents(content_id);
        page.finish();
        doc.type1_font(font).base_font(Name(b"Helvetica"));
        let mut content = Content::new();
        content
            .begin_text()
            .set_font(Name(b"F1"), 12.0)
            .next_line(72.0, 770.0)
            .show(Str(text.as_bytes()))
            .end_text();
        doc.stream(content_id, &content.finish());

        let p = dir.join(name);
        std::fs::write(&p, doc.finish()).unwrap();
        p
    }

    /// One page write per run, at a path that differs per run.
    ///
    /// ⚠ The differing path is load-bearing, not cosmetic: the macro commits
    /// only when the `knowledge/` tree actually CHANGED, so two runs writing
    /// identical content to one path would leave the second tree identical and
    /// the second source would (correctly) abort. Sharing a path here would test
    /// the opposite of what this test claims.
    fn writing_factory() -> CompleterFactory {
        let n = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        Box::new(move || {
            let i = n.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::new(MockCompleter {
                replies: tokio::sync::Mutex::new(vec![
                    LlmReply {
                        text: String::new(),
                        tool_calls: vec![LlmToolCall {
                            id: "req-1".into(),
                            name: "kb_write_page".into(),
                            args: serde_json::json!({
                                "path": format!("knowledge/sources/doc-{i}.md"),
                                "content": valid_page(
                                    "source",
                                    &format!("Doc {i}"),
                                    &format!("# Doc {i}\n\nCurated from the staged source."),
                                ),
                                "commit_message": "digest",
                            }),
                        }],
                    },
                    LlmReply {
                        text: "done".into(),
                        tool_calls: vec![],
                    },
                ]),
            })
        })
    }

    /// Every run answers with nothing at all — the shape a provider request that
    /// failed or was cut short leaves behind, and the one the macro must read as
    /// "wrote no pages" rather than "had nothing more to do".
    fn silent_factory() -> CompleterFactory {
        Box::new(|| {
            Box::new(MockCompleter {
                replies: tokio::sync::Mutex::new(vec![LlmReply {
                    text: String::new(),
                    tool_calls: vec![],
                }]),
            })
        })
    }

    fn args_for(sources: Vec<SourceSpec>, completer: CompleterFactory) -> SourceIngestArgs {
        SourceIngestArgs {
            kb_id: "k".into(),
            caller_capability: crate::privacy::ProviderTier::Public,
            caller_affiliation: None,
            sources,
            completer,
            focus: None,
            bounds: SubAgentBounds::default(),
            event_sink: None,
            cancel: None,
            model_label: "mock/mock-1".into(),
        }
    }

    fn curated_pages(svc: &KnowledgeService) -> Vec<String> {
        let dir = paths::kb_root(svc.root(), "k").join("knowledge/sources");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return vec![];
        };
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn raw_sources(svc: &KnowledgeService) -> Vec<String> {
        let dir = paths::kb_root(svc.root(), "k").join("raw");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return vec![];
        };
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    #[tokio::test]
    async fn a_cancelled_batch_does_not_stage_later_sources() {
        let (_dir, svc) = fresh_svc();
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        let completer: CompleterFactory = Box::new(|| {
            panic!("a cancelled source must not construct a completer");
        });
        let mut args = args_for(
            vec![
                SourceSpec::Text {
                    text: "alpha".into(),
                    title: Some("alpha".into()),
                },
                SourceSpec::Text {
                    text: "beta".into(),
                    title: Some("beta".into()),
                },
            ],
            completer,
        );
        args.cancel = Some(cancel);

        let report = ingest_sources(&svc, args).await.unwrap();

        assert!(raw_sources(&svc).is_empty());
        assert_eq!(report.outcomes.len(), 2);
        assert!(report.outcomes.iter().all(|outcome| {
            !outcome.ok
                && outcome.failure_phase == Some(IngestFailurePhase::NotStarted)
                && outcome.error.as_deref() == Some("cancelled before this source started")
        }));
    }

    /// The acceptance criterion, end to end: a batch of local documents produces
    /// raw sources **and** curated pages, each committed on its own, and the
    /// report says the pages exist. A real PDF beside a markdown file, because
    /// the reported failure was about PDFs and the converter branch they take is
    /// part of what is being claimed to work.
    #[tokio::test]
    async fn a_batch_of_local_documents_produces_curated_pages_and_commits() {
        let (dir, svc) = fresh_svc();
        let a = pdf(
            dir.path(),
            "alpha.pdf",
            "Alpha: a finding about alpha, long enough to carry a real text layer.",
        );
        let b = file(dir.path(), "b.md", "# Beta\n\nA finding about beta.");

        let report = ingest_sources(
            &svc,
            args_for(
                vec![SourceSpec::Path(a), SourceSpec::Path(b)],
                writing_factory(),
            ),
        )
        .await
        .expect("the batch itself must not fail");

        assert_eq!(report.succeeded(), 2, "{}", report.summary());
        assert_eq!(report.failed(), 0, "{}", report.summary());
        for outcome in &report.outcomes {
            assert!(
                outcome
                    .raw_commit_sha
                    .as_deref()
                    .is_some_and(|s| !s.is_empty()),
                "every new raw source reports its durable commit"
            );
            assert!(outcome.commit_sha.as_deref().is_some_and(|s| !s.is_empty()));
            assert!(
                outcome.verification.is_some(),
                "every committed source reports its post-commit scan"
            );
        }
        // Curated pages on disk, not merely a claim in the answer — the exact
        // distinction issue #108 turns on.
        let mut pages = curated_pages(&svc);
        pages.sort();
        assert_eq!(pages, vec!["doc-0.md", "doc-1.md"], "{}", report.summary());

        // ⚠ The mock completer writes its page whatever the source said, so
        // "curated pages exist" alone would pass even if the PDF had converted
        // to nothing. Read the staged raw markdown back and require the PDF's
        // own words in it: that is what proves the converter branch ran.
        let raw = raw_sources(&svc);
        assert_eq!(raw.len(), 2);
        let staged: String = raw
            .iter()
            .map(|id| {
                std::fs::read_to_string(
                    paths::kb_root(svc.root(), "k")
                        .join("raw")
                        .join(id)
                        .join("source.md"),
                )
                .unwrap_or_default()
            })
            .collect();
        assert!(
            staged.contains("a finding about alpha"),
            "the PDF's text layer must reach raw/<source>/source.md; got: {staged}"
        );
        assert!(
            staged.contains("A finding about beta"),
            "the markdown source must reach raw/<source>/source.md; got: {staged}"
        );
    }

    /// The failure half. A run that curates nothing must abort its transaction,
    /// leave no curated page behind, retain its raw source and raw-source commit,
    /// and be reported as a failure — never as an ingest that happened to be
    /// quiet or as a base that did not change.
    #[tokio::test]
    async fn a_source_that_curates_nothing_aborts_and_is_reported_as_a_failure() {
        let (dir, svc) = fresh_svc();
        let a = file(dir.path(), "a.md", "# Alpha\n\nA finding about alpha.");

        let report = ingest_sources(&svc, args_for(vec![SourceSpec::Path(a)], silent_factory()))
            .await
            .expect("a per-source failure is reported, not raised");

        assert_eq!(report.succeeded(), 0);
        assert_eq!(report.failed(), 1);
        assert!(
            curated_pages(&svc).is_empty(),
            "the transaction was aborted"
        );
        let outcome = &report.outcomes[0];
        let source_id = outcome
            .source_id
            .as_deref()
            .expect("the failed outcome reports its retained raw source");
        let raw_commit = outcome
            .raw_commit_sha
            .as_deref()
            .expect("the failed outcome reports its raw-source commit");
        assert_eq!(raw_sources(&svc), vec![source_id]);
        let history = svc.list_history("k", 10).unwrap();
        assert!(
            history.iter().any(|entry| entry.commit_sha == raw_commit),
            "the reported raw commit remains in history: {history:?}"
        );
        assert!(
            !history.iter().any(|entry| entry
                .delta
                .as_deref()
                .is_some_and(|delta| delta.contains("steps"))),
            "the rolled-back curation must not leave a curation commit: {history:?}"
        );
        let text = report.summary();
        assert!(text.starts_with("Curated 0 of 1 source(s)"), "got: {text}");
        assert!(text.contains("Curation rolled back"), "got: {text}");
        assert!(
            text.contains("raw source") && text.contains(source_id),
            "got: {text}"
        );
        assert!(text.contains(raw_commit), "got: {text}");
        assert!(!text.contains("base is unchanged"), "got: {text}");
    }

    /// A partial batch: one good document beside one that curates nothing. Each
    /// source is its own transaction, so the good one commits and the bad one
    /// does not take it down with it.
    #[tokio::test]
    async fn one_failing_source_does_not_discard_the_others() {
        let (dir, svc) = fresh_svc();
        let good = file(dir.path(), "good.md", "# Good\n\nWorth keeping.");
        let bad = file(dir.path(), "bad.md", "# Bad\n\nAlso worth keeping.");

        // Writes for the first run, silence for the second.
        let n = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let writing = writing_factory();
        let factory: CompleterFactory = Box::new(move || {
            if n.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                writing()
            } else {
                Box::new(MockCompleter {
                    replies: tokio::sync::Mutex::new(vec![LlmReply {
                        text: String::new(),
                        tool_calls: vec![],
                    }]),
                })
            }
        });

        let report = ingest_sources(
            &svc,
            args_for(vec![SourceSpec::Path(good), SourceSpec::Path(bad)], factory),
        )
        .await
        .unwrap();

        assert_eq!(report.succeeded(), 1, "{}", report.summary());
        assert_eq!(report.failed(), 1, "{}", report.summary());
        assert_eq!(curated_pages(&svc), vec!["doc-0.md"]);
        let text = report.summary();
        assert!(text.starts_with("Curated 1 of 2 source(s)"), "got: {text}");
    }

    /// A path that does not exist yields no sources and a note that names it,
    /// rather than an empty success.
    #[test]
    fn a_missing_path_expands_to_a_note_and_no_source() {
        let (items, notes) = expand(&SourceSpec::Path(std::path::PathBuf::from(
            "/nonexistent/definitely-not-here.pdf",
        )));
        assert!(items.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].level, "error");
        assert!(notes[0].title.contains("definitely-not-here.pdf"));
    }
}
