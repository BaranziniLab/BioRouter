//! `biorouter knowledge` subcommands — manage personal knowledge bases from the
//! CLI with the same service the desktop GUI drives over HTTP: list bases, show
//! or set the active base, create a base, and run the ingest / lint / query
//! macros (each backed by a bounded knowledge sub-agent).

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

/// Resolve the knowledge base to operate on: the explicit `--kb` flag, else the
/// persisted active base, else an actionable error.
fn resolve_kb(svc: &KnowledgeService, explicit: Option<String>) -> Result<String> {
    if let Some(id) = explicit {
        return Ok(id);
    }
    match svc.get_primary_persisted()? {
        Some(id) => Ok(id),
        None => bail!(
            "No knowledge base selected. Pass --kb <id>, or set an active base with \
             `biorouter knowledge active --set <id>`."
        ),
    }
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
    println!("{}", render_list(&svc, format)?);
    Ok(())
}

/// Render the base list. Visible bases are the set the agent uses; exactly one
/// of them may be the **primary** — the base a `--kb`-less write lands in — and
/// it is marked, which `cli.rs` has promised since the focus/discovery split.
fn render_list(svc: &KnowledgeService, format: &str) -> Result<String> {
    let bases = svc.list_bases()?;
    let hidden = svc.get_hidden_persisted().unwrap_or_default();
    let primary = svc.primary_for_session(None)?;

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
    if !svc.list_bases()?.iter().any(|b| b.id == id) {
        bail!(
            "No knowledge base with id '{}'. Run `biorouter knowledge list` to see them.",
            id
        );
    }
    let mut hidden = svc.get_hidden_persisted().unwrap_or_default();
    if !hidden.contains(&id) {
        hidden.push(id.clone());
        svc.set_hidden_persisted(&hidden)?;
    }
    println!(
        "  {} {} is now hidden from the agent",
        style("✓").green(),
        style(&id).fg(ACCENT).bold()
    );
    Ok(())
}

pub async fn handle_unhide(id: String) -> Result<()> {
    let svc = service()?;
    let mut hidden = svc.get_hidden_persisted().unwrap_or_default();
    let before = hidden.len();
    hidden.retain(|h| h != &id);
    if hidden.len() != before {
        svc.set_hidden_persisted(&hidden)?;
    }
    println!(
        "  {} {} is now visible to the agent",
        style("✓").green(),
        style(&id).fg(ACCENT).bold()
    );
    Ok(())
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
    let name = name.unwrap_or_else(|| id.clone());
    let manifest = svc
        .create_base(&id, &name, color.as_deref())
        .map_err(|e| anyhow!("Failed to create knowledge base: {}", e))?;

    println!(
        "  {} created knowledge base {} {}",
        style("✓").green(),
        style(&manifest.id).fg(ACCENT).bold(),
        style(format!("({})", manifest.name)).dim()
    );

    // Make it the primary when there was no prior choice, so the next
    // --kb-less ingest/query "just works".
    if svc.primary_for_session(None)?.is_none() {
        svc.set_selection(None, None, PrimaryUpdate::Set(&manifest.id))?;
        println!("  {} set as the primary knowledge base", style("·").dim());
    }
    Ok(())
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
    let kb_id = resolve_kb(&svc, kb)?;

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
        resolve_kb(&svc, kb)?
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
    let kb_id = resolve_kb(&svc, kb)?;

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
    let kb_id = resolve_kb(&svc, kb)?;
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
    use super::{active_command, render_list};
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
}
