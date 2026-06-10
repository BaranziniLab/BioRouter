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
use biorouter::knowledge::service::KnowledgeService;
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
    match svc.get_active_persisted()? {
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
    let bases = svc.list_bases()?;
    let hidden = svc.get_hidden_persisted().unwrap_or_default();

    if format == "json" {
        println!(
            "{}",
            serde_json::json!({
                "bases": bases.iter().map(|b| serde_json::json!({
                    "id": b.id, "name": b.name, "color": b.color,
                    "hidden": hidden.contains(&b.id),
                })).collect::<Vec<_>>(),
            })
        );
        return Ok(());
    }

    if bases.is_empty() {
        section("Knowledge bases");
        println!(
            "    {}",
            style("none yet — create one with `biorouter knowledge create <id> --name <name>`")
                .dim()
        );
        return Ok(());
    }

    section("Knowledge bases");
    println!(
        "    {}",
        style("Visible bases are available to the agent; hide ones you don't want it to use.")
            .dim()
    );
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
        } else {
            String::new()
        };
        println!(
            "    {} {:<width$}  {}{}",
            marker,
            style(&base.id).bold(),
            style(&base.name).dim(),
            suffix,
            width = width
        );
    }
    Ok(())
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

    if clear {
        svc.set_active_persisted(None)?;
        println!("  {} active knowledge base cleared", style("✓").green());
        return Ok(());
    }

    if let Some(id) = set {
        // Validate the base exists before activating it.
        if !svc.list_bases()?.iter().any(|b| b.id == id) {
            bail!(
                "No knowledge base with id '{}'. Run `biorouter knowledge list` to see them.",
                id
            );
        }
        svc.set_active_persisted(Some(&id))?;
        println!(
            "  {} active knowledge base set to {}",
            style("✓").green(),
            style(&id).fg(ACCENT).bold()
        );
        return Ok(());
    }

    match svc.get_active_persisted()? {
        Some(id) => println!(
            "  {} {}",
            style("active:").dim(),
            style(id).fg(ACCENT).bold()
        ),
        None => println!(
            "  {}",
            style("no active knowledge base (use --set <id>)").dim()
        ),
    }
    Ok(())
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

    // Make it active when there was no prior selection, so the next ingest/query
    // "just works" without an explicit --kb.
    if svc.get_active_persisted()?.is_none() {
        svc.set_active_persisted(Some(&manifest.id))?;
        println!("  {} set as active", style("·").dim());
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
