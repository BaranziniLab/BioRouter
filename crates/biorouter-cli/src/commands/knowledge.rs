//! `biorouter knowledge` subcommands — manage personal knowledge bases from the
//! CLI with the same service the desktop GUI drives over HTTP: list bases, show
//! or set the primary base, create a base, and run the ingest / lint / query
//! macros (each backed by a bounded knowledge sub-agent).
//!
//! The CLI has no session concept, so every command here is machine-wide: the
//! primary it reads and writes is the one in `.active-kb`, not a chat's.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use biorouter::config::Config;
use biorouter::knowledge::convert::SourceInput;
use biorouter::knowledge::macros::{
    ingest as ingest_macro, lint as lint_macro, query as query_macro,
};
use biorouter::knowledge::service::{KnowledgeService, PrimaryUpdate};
use biorouter::knowledge::subagent::loop_::{Completer, SubAgentBounds};
use biorouter::knowledge::ProviderCompleter;
use biorouter::model::ModelConfig;
use console::{style, Color};

/// Brand warm tan-brown accent (xterm-256 137 ≈ #af875f), Biorouter's light cream palette
const ACCENT: Color = Color::Color256(137);

fn service() -> Result<KnowledgeService> {
    KnowledgeService::new_default().map_err(|e| anyhow!("Failed to open knowledge store: {}", e))
}

/// Resolve the base a command operates on: the explicit `--kb` flag, else the
/// primary. Returns the id and, when it was resolved rather than given, a
/// notice the caller must print *before* doing any work — a KB-less write
/// must never be silent about which base it landed in.
fn resolve_kb(
    svc: &KnowledgeService,
    explicit: Option<String>,
) -> Result<(String, Option<String>)> {
    if let Some(id) = explicit {
        return Ok((id, None));
    }
    if let Some(id) = svc.primary_for_session(None)? {
        let notice = format!(
            "  {} using primary knowledge base {}",
            style("·").dim(),
            style(&id).fg(ACCENT).bold()
        );
        return Ok((id, Some(notice)));
    }
    let ids = svc.session_kb_ids(None)?;
    if ids.is_empty() {
        bail!("No knowledge bases yet. Create one with `biorouter knowledge create <id> --name <name>`.");
    }
    bail!(
        "No primary knowledge base. Pass --kb <id> (one of: {}), or set one with \
         `biorouter knowledge active --set <id>`.",
        ids.join(", ")
    )
}

/// Build an LLM completer from the configured (or overridden) provider/model,
/// mirroring how the server builds one for the HTTP macros.
async fn build_completer(
    provider: Option<String>,
    model: Option<String>,
) -> Result<Box<dyn Completer>> {
    // Honour the same offline test-mode switch the server uses, so CLI knowledge
    // macros (ingest / query / ingest-conversation) can be exercised without a
    // reachable LLM provider.
    if biorouter::knowledge::test_mode::env_enabled() {
        return Ok(Box::new(biorouter::knowledge::test_mode::TestModeCompleter));
    }
    let config = Config::global();
    let provider = provider
        .or_else(|| config.get_biorouter_provider().ok())
        .ok_or_else(|| anyhow!("No provider configured. Run `biorouter configure` first."))?;
    let model = model
        .or_else(|| config.get_biorouter_model().ok())
        .ok_or_else(|| anyhow!("No model configured. Run `biorouter configure` first."))?;

    let model_config = ModelConfig::new(&model)?;
    let provider = biorouter::providers::create(&provider, model_config).await?;
    Ok(Box::new(ProviderCompleter::new(provider)))
}

/// First 10 characters of a commit sha, for compact display.
fn short_sha(sha: &str) -> String {
    sha.chars().take(10).collect()
}

fn ingest_bounds() -> SubAgentBounds {
    SubAgentBounds {
        max_steps: 60,
        max_wall: Duration::from_secs(900),
        max_tokens: 200_000,
    }
}

fn section(title: &str) {
    println!("  {} {}", style("▌").fg(ACCENT), style(title).bold());
}

// ──────────────────────────────────────────────────────────────────────────────
// list
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_list(format: &str) -> Result<()> {
    let svc = service()?;
    // `render_list` builds a block with one newline per row; `println!` supplies
    // the last one, so trim or every listing ends in a blank line.
    println!("{}", render_list(&svc, format)?.trim_end());
    Ok(())
}

/// Render the base list. Visible bases are the set the agent uses; exactly one
/// of them may be the **primary** — the base a `--kb`-less write lands in — and
/// it is marked, which `cli.rs` has promised since the focus/discovery split.
fn render_list(svc: &KnowledgeService, format: &str) -> Result<String> {
    let bases = svc.list_bases()?;
    // One locked snapshot rather than a hidden read and a primary read that can
    // disagree — and, crucially, no `unwrap_or_default()`: swallowing a failed
    // read of `.hidden-kbs` rendered every base as visible to the agent while
    // the agent's own resolver errored on the same file. This listing is the
    // answer to "what can the agent see?", so it must refuse rather than guess.
    let selection = svc.selection(None)?;
    let hidden = selection.hidden_kbs;
    let primary = selection.primary_kb;

    if format == "json" {
        return Ok(serde_json::json!({
            "primary_kb": primary,
            "bases": bases.iter().map(|b| serde_json::json!({
                "id": b.id, "name": b.name, "color": b.color,
                "hidden": hidden.contains(&b.id),
                "primary": primary.as_deref() == Some(b.id.as_str()),
            })).collect::<Vec<_>>(),
        })
        .to_string());
    }

    let mut out = String::new();
    out.push_str(&format!(
        "  {} {}\n",
        style("▌").fg(ACCENT),
        style("Knowledge bases").bold()
    ));
    if bases.is_empty() {
        out.push_str(&format!(
            "    {}",
            style("none yet — create one with `biorouter knowledge create <id> --name <name>`")
                .dim()
        ));
        return Ok(out);
    }
    out.push_str(&format!(
        "    {}\n",
        style(
            "Visible bases are available to the agent; the primary is where a --kb-less ingest writes."
        )
        .dim()
    ));
    let width = bases.iter().map(|b| b.id.len()).max().unwrap_or(0);
    for base in &bases {
        let is_hidden = hidden.contains(&base.id);
        // A filled accent dot = visible to the agent; a dim ring = hidden.
        let marker = if is_hidden {
            style("○").dim().to_string()
        } else {
            style("●").fg(ACCENT).to_string()
        };
        let suffix = if is_hidden {
            style("  (hidden)").dim().to_string()
        } else if primary.as_deref() == Some(base.id.as_str()) {
            style("  (primary)").fg(ACCENT).to_string()
        } else {
            String::new()
        };
        out.push_str(&format!(
            "    {} {:<width$}  {}{}\n",
            marker,
            style(&base.id).bold(),
            style(&base.name).dim(),
            suffix,
            width = width
        ));
    }
    Ok(out)
}

// ──────────────────────────────────────────────────────────────────────────────
// hide / unhide — control which bases the agent can see
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_hide(id: String) -> Result<()> {
    let svc = service()?;
    println!("{}", hide_command(&svc, &id)?);
    Ok(())
}

pub async fn handle_unhide(id: String) -> Result<()> {
    let svc = service()?;
    println!("{}", unhide_command(&svc, &id)?);
    Ok(())
}

/// Naming a base that does not exist is a typo, not a no-op: report it before
/// claiming success. The locked operation below re-checks under the lock and
/// remains the authority — this pre-check exists only to say `knowledge list`.
fn require_base(svc: &KnowledgeService, id: &str) -> Result<()> {
    if !svc.list_bases()?.iter().any(|b| b.id == id) {
        bail!(
            "No knowledge base with id '{}'. Run `biorouter knowledge list` to see them.",
            id
        );
    }
    Ok(())
}

fn hide_command(svc: &KnowledgeService, id: &str) -> Result<String> {
    require_base(svc, id)?;
    svc.hide_kb(None, id, PrimaryUpdate::Unchanged)?;
    Ok(format!(
        "  {} {} is now hidden from the agent",
        style("✓").green(),
        style(id).fg(ACCENT).bold()
    ))
}

fn unhide_command(svc: &KnowledgeService, id: &str) -> Result<String> {
    require_base(svc, id)?;
    svc.include_kb(None, id, PrimaryUpdate::Unchanged)?;
    Ok(format!(
        "  {} {} is now visible to the agent",
        style("✓").green(),
        style(id).fg(ACCENT).bold()
    ))
}

// ──────────────────────────────────────────────────────────────────────────────
// active
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_active(set: Option<String>, clear: bool) -> Result<()> {
    let svc = service()?;
    println!("{}", active_command(&svc, set, clear)?);
    Ok(())
}

/// Show, set or clear the **primary** knowledge base — the base a `--kb`-less
/// ingest/query/lint writes to. Setting one validates membership: a base the
/// CLI hides from the agent can never be the primary.
fn active_command(svc: &KnowledgeService, set: Option<String>, clear: bool) -> Result<String> {
    if clear {
        svc.set_selection(None, None, PrimaryUpdate::Clear)?;
        return Ok(format!(
            "  {} primary knowledge base cleared",
            style("✓").green()
        ));
    }

    if let Some(id) = set {
        svc.set_selection(None, None, PrimaryUpdate::Set(&id))
            .map_err(|e| anyhow!("{e} Run `biorouter knowledge list` to see them."))?;
        return Ok(format!(
            "  {} primary knowledge base set to {}",
            style("✓").green(),
            style(&id).fg(ACCENT).bold()
        ));
    }

    Ok(match svc.primary_for_session(None)? {
        Some(id) => format!(
            "  {} {}",
            style("primary:").dim(),
            style(id).fg(ACCENT).bold()
        ),
        None => format!(
            "  {}",
            style("no primary knowledge base (use --set <id>)").dim()
        ),
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// create
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_create(id: String, name: Option<String>, color: Option<String>) -> Result<()> {
    let svc = service()?;
    println!("{}", create_command(&svc, id, name, color)?);
    Ok(())
}

/// Create a base — and only create it.
///
/// This used to pin the new base as the primary whenever none was set, so the
/// next `--kb`-less ingest "just worked". That is exactly the invention the
/// model forbids: the primary is where a `--kb`-less write *commits*, and a
/// pointer the user never chose sends an ingest into a base by accident, as a
/// git commit in that base's history that is easy to miss. The first base is
/// not a special case — one candidate is still a candidate, not a choice. With
/// no primary, a KB-less command fails and lists the candidates, so instead of
/// guessing we say how to choose.
fn create_command(
    svc: &KnowledgeService,
    id: String,
    name: Option<String>,
    color: Option<String>,
) -> Result<String> {
    let name = name.unwrap_or_else(|| id.clone());
    let manifest = svc
        .create_base(&id, &name, color.as_deref())
        .map_err(|e| anyhow!("Failed to create knowledge base: {}", e))?;

    let mut out = format!(
        "  {} created knowledge base {} {}",
        style("✓").green(),
        style(&manifest.id).fg(ACCENT).bold(),
        style(format!("({})", manifest.name)).dim()
    );

    if svc.primary_for_session(None)?.is_none() {
        out.push_str(&format!(
            "\n  {} {}",
            style("·").dim(),
            style(format!(
                "no primary knowledge base yet — set one with `biorouter knowledge active --set {}`",
                manifest.id
            ))
            .dim()
        ));
    }
    Ok(out)
}

// ──────────────────────────────────────────────────────────────────────────────
// ingest
// ──────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn handle_ingest(
    kb: Option<String>,
    url: Option<String>,
    file: Option<PathBuf>,
    text: Option<String>,
    focus: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<()> {
    let svc = service()?;
    let (kb_id, notice) = resolve_kb(&svc, kb)?;
    if let Some(notice) = notice {
        println!("{notice}");
    }

    let source = match (url, file, text) {
        (Some(u), None, None) => SourceInput::Url(u),
        (None, Some(p), None) => {
            if !p.exists() {
                bail!("File not found: {}", p.display());
            }
            SourceInput::Path(p)
        }
        (None, None, Some(t)) => SourceInput::Text {
            text: t,
            title: None,
        },
        (None, None, None) => {
            bail!("Provide a source: --url <url>, --file <path>, or --text <text>")
        }
        _ => bail!("Provide exactly one of --url, --file, or --text"),
    };

    let completer = build_completer(provider, model).await?;

    let spinner = cliclack::spinner();
    spinner.start(format!("ingesting into {}...", kb_id));

    let result = ingest_macro::ingest(
        &svc,
        ingest_macro::IngestArgs {
            kb_id: kb_id.clone(),
            source,
            completer,
            focus,
            bounds: ingest_bounds(),
            event_sink: None,
            cancel: None,
        },
    )
    .await;

    spinner.stop("");

    match result {
        Ok(res) => {
            println!(
                "  {} ingested into {} {}",
                style("✓").green(),
                style(&kb_id).fg(ACCENT).bold(),
                style(format!("({} steps)", res.steps)).dim()
            );
            println!(
                "    {} {}",
                style("source:").dim(),
                style(&res.source_id).dim()
            );
            println!(
                "    {} {}",
                style("commit:").dim(),
                style(short_sha(&res.commit_sha)).dim()
            );
            Ok(())
        }
        Err(e) => Err(anyhow!("Ingest failed: {}", e)),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// ingest-conversation
// ──────────────────────────────────────────────────────────────────────────────

/// Digest one or more chat sessions into a knowledge base.
pub async fn handle_ingest_conversation(
    kb: Option<String>,
    sessions: Vec<String>,
    new_kb: Option<String>,
    focus: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<()> {
    use biorouter::knowledge::conversation_ingest::{ingest_conversation, ConversationIngestArgs};
    use biorouter::session::session_manager::SessionManager;

    let svc = service()?;

    // Resolve the target KB: --new-kb creates one, else --kb, else active.
    let kb_id = if let Some(name) = new_kb {
        let id = biorouter::agents::knowledge_tool::slugify_kb_name(&name);
        if id.is_empty() {
            bail!("--new-kb must contain letters or numbers");
        }
        if !svc.list_bases()?.iter().any(|b| b.id == id) {
            svc.create_base(&id, name.trim(), None)?;
            println!(
                "  {} created knowledge base {}",
                style("✓").green(),
                style(&id).fg(ACCENT).bold()
            );
        }
        id
    } else {
        let (kb_id, notice) = resolve_kb(&svc, kb)?;
        if let Some(notice) = notice {
            println!("{notice}");
        }
        kb_id
    };

    // Resolve which sessions to ingest. Default: the most recent session.
    let manager = SessionManager::instance();
    let session_ids: Vec<String> = if sessions.is_empty() {
        let mut all = manager.list_sessions().await?;
        all.sort_by_key(|s| s.updated_at);
        match all.last() {
            Some(s) => {
                println!(
                    "  {} no --session given, using most recent session {}",
                    style("·").dim(),
                    style(&s.id).dim()
                );
                vec![s.id.clone()]
            }
            None => bail!("No sessions found to ingest."),
        }
    } else {
        sessions
    };

    let mut loaded = Vec::new();
    for sid in &session_ids {
        loaded.push(
            manager
                .get_session(sid, true)
                .await
                .map_err(|e| anyhow!("session '{sid}' not found: {e}"))?,
        );
    }

    let completer = build_completer(provider, model).await?;

    let spinner = cliclack::spinner();
    spinner.start(format!("digesting {} conversation(s)...", loaded.len()));

    let result = ingest_conversation(
        &svc,
        ConversationIngestArgs {
            kb_id: kb_id.clone(),
            sessions: loaded,
            completer,
            focus,
            bounds: ingest_bounds(),
            event_sink: None,
            cancel: None,
        },
    )
    .await;

    spinner.stop("");

    match result {
        Ok(res) => {
            println!(
                "  {} {} conversation(s) ingested into {} {}",
                style("✓").green(),
                session_ids.len(),
                style(&kb_id).fg(ACCENT).bold(),
                style(format!("({} steps)", res.steps)).dim()
            );
            println!(
                "    {} {}",
                style("source:").dim(),
                style(&res.source_id).dim()
            );
            println!(
                "    {} {}",
                style("commit:").dim(),
                style(short_sha(&res.commit_sha)).dim()
            );
            Ok(())
        }
        Err(e) => Err(anyhow!("Conversation ingest failed: {}", e)),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// lint
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_lint(
    kb: Option<String>,
    fix: bool,
    provider: Option<String>,
    model: Option<String>,
) -> Result<()> {
    let svc = service()?;
    let (kb_id, notice) = resolve_kb(&svc, kb)?;
    if let Some(notice) = notice {
        println!("{notice}");
    }

    let completer = if fix {
        Some(build_completer(provider, model).await?)
    } else {
        None
    };

    let spinner = cliclack::spinner();
    spinner.start(format!("linting {}...", kb_id));

    let result = lint_macro::lint(
        &svc,
        lint_macro::LintArgs {
            kb_id: kb_id.clone(),
            completer,
            autofix: fix,
            bounds: SubAgentBounds::default(),
            event_sink: None,
            cancel: None,
        },
    )
    .await
    .map_err(|e| anyhow!("Lint failed: {}", e))?;

    spinner.stop("");

    let r = &result.report;
    section(&format!("Lint report — {}", kb_id));

    let group = |label: &str, items: &[String]| {
        let count = items.len();
        let colored = if count == 0 {
            style(format!("{} {}", count, label)).green().to_string()
        } else {
            style(format!("{} {}", count, label)).yellow().to_string()
        };
        println!("    {}", colored);
        for item in items {
            println!("      {} {}", style("·").dim(), style(item).dim());
        }
    };

    group("orphan page(s)", &r.orphans);
    group("contradiction(s)", &r.contradictions);
    group("stale source(s)", &r.stale_sources);
    group("missing concept page(s)", &r.missing_concept_pages);

    if fix {
        println!(
            "  {} {} fix(es) applied",
            style("✓").green(),
            result.fixes_applied
        );
    } else if r.orphans.is_empty()
        && r.contradictions.is_empty()
        && r.stale_sources.is_empty()
        && r.missing_concept_pages.is_empty()
    {
        println!("  {} clean", style("✓").green());
    } else {
        println!(
            "  {}",
            style("re-run with --fix to let the sub-agent repair these").dim()
        );
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// query
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_query(
    question: String,
    kb: Option<String>,
    save: bool,
    provider: Option<String>,
    model: Option<String>,
) -> Result<()> {
    let svc = service()?;
    let (kb_id, notice) = resolve_kb(&svc, kb)?;
    if let Some(notice) = notice {
        println!("{notice}");
    }
    let completer = build_completer(provider, model).await?;

    let spinner = cliclack::spinner();
    spinner.start(format!("querying {}...", kb_id));

    let result = query_macro::query(
        &svc,
        query_macro::QueryArgs {
            kb_id: kb_id.clone(),
            question,
            completer,
            file_as_page: save,
            bounds: SubAgentBounds::default(),
            event_sink: None,
            cancel: None,
        },
    )
    .await
    .map_err(|e| anyhow!("Query failed: {}", e))?;

    spinner.stop("");

    section(&format!("Answer — {}", kb_id));
    println!("{}", result.answer);

    if !result.cited_pages.is_empty() {
        println!();
        println!("  {}", style("cited pages").dim());
        for page in &result.cited_pages {
            println!("    {} {}", style("·").dim(), style(page).fg(ACCENT));
        }
    }
    if save {
        if let Some(sha) = &result.commit_sha {
            println!(
                "  {} saved as a page {}",
                style("✓").green(),
                style(format!("({})", short_sha(sha))).dim()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{active_command, create_command, hide_command, render_list, unhide_command};
    use biorouter::knowledge::service::{KnowledgeService, PrimaryUpdate};

    fn svc() -> (tempfile::TempDir, KnowledgeService) {
        let tmp = tempfile::TempDir::new().unwrap();
        let svc = KnowledgeService::new(tmp.path().to_path_buf());
        svc.create_base("alpha", "Alpha", None).unwrap();
        svc.create_base("beta", "Beta", None).unwrap();
        (tmp, svc)
    }

    /// First-ever CLI coverage for the knowledge commands. `--set` used to
    /// validate only that the base existed, so it would happily pin a base the
    /// CLI hides from the agent — a primary outside the set.
    #[test]
    fn active_command_shows_sets_validates_and_clears_the_primary() -> anyhow::Result<()> {
        let (_tmp, svc) = svc();

        assert!(active_command(&svc, None, false)?.contains("no primary knowledge base"));
        assert!(active_command(&svc, Some("beta".to_string()), false)?.contains("beta"));
        assert!(active_command(&svc, None, false)?.contains("beta"));

        svc.set_hidden_persisted(&["alpha".to_string()])?;
        let err = active_command(&svc, Some("alpha".to_string()), false)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("alpha") && err.contains("knowledge list"),
            "a hidden base cannot be the primary, and the error must say how to look, got: {err}"
        );

        assert!(active_command(&svc, None, true)?.contains("cleared"));
        assert_eq!(svc.primary_for_session(None)?, None);
        Ok(())
    }

    /// `cli.rs:901` has promised "the active one is marked" since the
    /// focus/discovery split; `handle_list` only ever marked hidden-vs-visible.
    #[test]
    fn list_marks_the_primary_base() -> anyhow::Result<()> {
        let (_tmp, svc) = svc();
        svc.set_selection(None, None, PrimaryUpdate::Set("beta"))?;

        let text = render_list(&svc, "text")?;
        assert!(
            text.contains("beta") && text.contains("primary"),
            "got: {text}"
        );

        let json: serde_json::Value = serde_json::from_str(&render_list(&svc, "json")?)?;
        assert_eq!(json["primary_kb"], serde_json::json!("beta"));
        let beta = json["bases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == serde_json::json!("beta"))
            .unwrap();
        assert_eq!(beta["primary"], serde_json::json!(true));
        Ok(())
    }

    /// A `--kb`-less ingest/query/lint resolves its target silently. It must
    /// hand back a notice so the command can say where it is about to write —
    /// an ingest commits to that base's git history and is hard to notice
    /// afterwards.
    #[test]
    fn resolve_kb_names_the_primary_and_lists_candidates_when_there_is_none() -> anyhow::Result<()>
    {
        let (_tmp, svc) = svc();

        let err = super::resolve_kb(&svc, None).unwrap_err().to_string();
        assert!(
            err.contains("alpha, beta") && err.contains("--kb"),
            "with no primary the error must list the candidates, got: {err}"
        );

        svc.set_selection(None, None, PrimaryUpdate::Set("beta"))?;
        let (id, notice) = super::resolve_kb(&svc, None)?;
        assert_eq!(id, "beta");
        assert!(
            notice
                .expect("a resolved primary must be announced")
                .contains("beta"),
            "the notice must name the base"
        );

        let (id, notice) = super::resolve_kb(&svc, Some("alpha".to_string()))?;
        assert_eq!(id, "alpha");
        assert!(notice.is_none(), "an explicit --kb needs no notice");
        Ok(())
    }

    /// The primary is an explicit choice and is **never invented** — not even
    /// for the very first base on a fresh machine, the one case where guessing
    /// looks harmless. It is not: the next `--kb`-less ingest then commits into
    /// a base the user never picked, and an ingest is a git commit in that
    /// base's history that is hard to notice afterwards. Creating a base only
    /// creates it; a KB-less write must still fail with the candidate list.
    #[test]
    fn creating_a_base_never_invents_a_primary() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let svc = KnowledgeService::new(tmp.path().to_path_buf());

        let out = create_command(&svc, "alpha".to_string(), None, None)?;
        assert!(out.contains("alpha"), "got: {out}");
        assert_eq!(
            svc.primary_for_session(None)?,
            None,
            "creating the first base must not silently make it the primary"
        );
        assert!(
            out.contains("knowledge active --set"),
            "the user must be told how to choose one, got: {out}"
        );

        // …and the KB-less path is the candidate-listing error, not a guess.
        let err = super::resolve_kb(&svc, None).unwrap_err().to_string();
        assert!(err.contains("alpha") && err.contains("--kb"), "got: {err}");

        // An explicit choice sticks, and a later create does not move it.
        svc.set_selection(None, None, PrimaryUpdate::Set("alpha"))?;
        let out = create_command(&svc, "beta".to_string(), None, None)?;
        assert_eq!(svc.primary_for_session(None)?.as_deref(), Some("alpha"));
        assert!(
            !out.contains("knowledge active --set"),
            "no nudge once a primary exists, got: {out}"
        );
        Ok(())
    }

    /// `hide` / `unhide` used to read the hidden list, edit it in memory and
    /// write the whole thing back. Nothing serialises those two unlocked calls,
    /// so two overlapping invocations — separate processes, or the desktop app
    /// writing the same file while a shell command runs — each write a list
    /// computed before the other's edit and one hide silently vanishes. The
    /// user sees "✓ gamma is now hidden" and gamma is still visible.
    #[test]
    fn concurrent_hides_from_separate_invocations_all_survive() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let root = tmp.path().to_path_buf();
        let svc = KnowledgeService::new(root.clone());
        let ids = ["alpha", "beta", "gamma", "delta"];
        for id in ids {
            svc.create_base(id, id, None)?;
        }

        for round in 0..8 {
            svc.clear_hidden_persisted()?;
            let gate = std::sync::Arc::new(std::sync::Barrier::new(ids.len()));
            let handles: Vec<_> = ids
                .iter()
                .map(|id| {
                    // A fresh service per thread: each stands in for its own
                    // `biorouter knowledge hide <id>` process.
                    let svc = KnowledgeService::new(root.clone());
                    let id = id.to_string();
                    let gate = gate.clone();
                    std::thread::spawn(move || -> anyhow::Result<()> {
                        gate.wait();
                        hide_command(&svc, &id)?;
                        Ok(())
                    })
                })
                .collect();
            for handle in handles {
                handle.join().expect("hide thread panicked")?;
            }

            let mut hidden = svc.get_hidden_persisted()?;
            hidden.sort();
            assert_eq!(
                hidden.len(),
                ids.len(),
                "round {round}: a concurrent hide was lost, got {hidden:?}"
            );
            assert!(
                svc.session_kb_ids(None)?.is_empty(),
                "round {round}: every base was hidden, so the set must be empty"
            );
        }
        Ok(())
    }

    /// `list` is the surface that answers "what can the agent see?", so it must
    /// not answer from a file it failed to read. It took the hidden list with
    /// `unwrap_or_default()`, which turns an unreadable or corrupt
    /// `.hidden-kbs` into "nothing is hidden" — every base rendered as
    /// available to the agent, while the agent's own resolver errors on the
    /// same file. (With a primary pinned the error surfaced by accident
    /// through `primary_for_session`; with none pinned, which is now the
    /// default state after a create, nothing caught it.)
    #[test]
    fn list_refuses_to_report_a_hidden_list_it_could_not_read() -> anyhow::Result<()> {
        let (tmp, svc) = svc();
        std::fs::write(
            biorouter::knowledge::paths::hidden_kbs_path(tmp.path()),
            b"{ not json }",
        )?;

        assert!(
            svc.session_kb_ids(None).is_err(),
            "precondition: the agent's own resolver rejects this file"
        );
        assert!(
            render_list(&svc, "text").is_err(),
            "so the listing must not claim every base is visible"
        );
        assert!(render_list(&svc, "json").is_err());
        Ok(())
    }

    /// Unhiding a base nobody has heard of used to print "✓ … is now visible"
    /// and change nothing, so a typo read as success.
    #[test]
    fn hide_and_unhide_reject_a_base_that_does_not_exist() -> anyhow::Result<()> {
        let (_tmp, svc) = svc();

        for err in [
            hide_command(&svc, "ghost").unwrap_err().to_string(),
            unhide_command(&svc, "ghost").unwrap_err().to_string(),
        ] {
            assert!(
                err.contains("ghost") && err.contains("knowledge list"),
                "got: {err}"
            );
        }

        // The real round trip still works.
        hide_command(&svc, "alpha")?;
        assert_eq!(svc.session_kb_ids(None)?, vec!["beta".to_string()]);
        unhide_command(&svc, "alpha")?;
        assert_eq!(
            svc.session_kb_ids(None)?,
            vec!["alpha".to_string(), "beta".to_string()]
        );
        Ok(())
    }
}
