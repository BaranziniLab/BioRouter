//! `biorouter knowledge` subcommands — manage personal knowledge bases from the
//! CLI with the same service the desktop GUI drives over HTTP: list bases, show
//! or set the primary base, create a base, and run the ingest / lint / query
//! macros (each backed by a bounded knowledge sub-agent).
//!
//! The CLI has no session of its own, so every command here is machine-wide by
//! default: the primary it reads and writes is the one in `.active-kb`, not a
//! chat's. The single exception is `knowledge active --session <id>`, which
//! addresses one chat's pointer on purpose — a chat can hold an explicit "no
//! primary" override that only a session-scoped gesture can lift, and the CLI
//! is the escape hatch when the GUI is not the surface in front of the user.

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
use biorouter::privacy::ProviderTier;
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
/// mirroring how the server builds one for the HTTP macros — **and** the tier of
/// the provider that was actually constructed (issue #56).
///
/// The tier comes back from here rather than being re-derived by each handler,
/// because `providers::create` intercepts `BIOROUTER_LEAD_MODEL` *before* the
/// registry lookup: `--provider ollama` can construct a lead/worker composite
/// whose tier is `least(lead, worker)` and therefore not the requested name's.
/// `ProviderCompleter::paired` reads the tier off the same `Arc` the completer
/// wraps, so the two cannot come from different providers.
async fn build_completer(
    provider: Option<String>,
    model: Option<String>,
) -> Result<(Box<dyn Completer>, ProviderTier)> {
    // Honour the same offline test-mode switch the server uses, so CLI knowledge
    // macros (ingest / query / ingest-conversation) can be exercised without a
    // reachable LLM provider.
    if biorouter::knowledge::test_mode::env_enabled() {
        // ⚠ The SECOND of the two named literal exemptions (the other is
        // `routes/knowledge.rs`'s twin branch). There is no provider here to
        // read a tier from, and the fail-safe direction for a *ratchet* is not
        // to privatise a base on a test path.
        return Ok((
            Box::new(biorouter::knowledge::test_mode::TestModeCompleter),
            ProviderTier::Public,
        ));
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
    let (completer, tier) = ProviderCompleter::paired(provider);
    Ok((Box::new(completer), tier))
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

pub async fn handle_active(
    set: Option<String>,
    clear: bool,
    inherit: bool,
    session: Option<String>,
) -> Result<()> {
    let svc = service()?;
    println!(
        "{}",
        active_command(&svc, session.as_deref(), set, clear, inherit)?
    );
    Ok(())
}

/// Show, set, clear or un-override the **primary** knowledge base — the base a
/// `--kb`-less ingest/query/lint writes to. Setting one validates membership: a
/// base the CLI hides from the agent can never be the primary.
///
/// `session` is the one thing the CLI addresses that is not its own: everything
/// else here is machine-wide (see the module header). It exists because
/// `--inherit` only means something at session scope — a chat can hold an
/// explicit "no primary" override that survives every other gesture, and
/// deleting the base a chat had pinned *installs* one. Without a way to drop
/// that override from outside the GUI, such a chat could never follow the
/// machine-wide default again.
fn active_command(
    svc: &KnowledgeService,
    session: Option<&str>,
    set: Option<String>,
    clear: bool,
    inherit: bool,
) -> Result<String> {
    // Three incompatible outcomes; clap rejects the combination first, but the
    // check belongs with the semantics, not only with the parser.
    if [set.is_some(), clear, inherit]
        .iter()
        .filter(|asked| **asked)
        .count()
        > 1
    {
        bail!("--set, --clear and --inherit are three different gestures; pass at most one.");
    }

    let scope = match session {
        Some(id) => format!(" for chat {}", style(id).fg(ACCENT).bold()),
        None => String::new(),
    };

    if inherit {
        let Some(session) = session else {
            bail!(
                "--inherit drops a chat's own primary so it follows the machine-wide one, so it \
                 needs --session <id>. The machine-wide primary has nothing above it to inherit \
                 — use --clear to unset it."
            );
        };
        let selection = svc.set_selection(Some(session), None, PrimaryUpdate::Inherit)?;
        return Ok(match selection.primary_kb {
            Some(id) => format!(
                "  {} chat {} now follows the machine-wide primary knowledge base ({})",
                style("✓").green(),
                style(session).fg(ACCENT).bold(),
                style(&id).fg(ACCENT).bold()
            ),
            None => format!(
                "  {} chat {} now follows the machine-wide primary knowledge base (none is set)",
                style("✓").green(),
                style(session).fg(ACCENT).bold()
            ),
        });
    }

    if clear {
        svc.set_selection(session, None, PrimaryUpdate::Clear)?;
        return Ok(format!(
            "  {} primary knowledge base cleared{}",
            style("✓").green(),
            scope
        ));
    }

    if let Some(id) = set {
        svc.set_selection(session, None, PrimaryUpdate::Set(&id))
            .map_err(|e| anyhow!("{e} Run `biorouter knowledge list` to see them."))?;
        return Ok(format!(
            "  {} primary knowledge base{} set to {}",
            style("✓").green(),
            scope,
            style(&id).fg(ACCENT).bold()
        ));
    }

    Ok(match svc.primary_for_session(session)? {
        Some(id) => format!(
            "  {} {}{}",
            style("primary:").dim(),
            style(id).fg(ACCENT).bold(),
            scope
        ),
        None => format!(
            "  {}",
            style(format!(
                "no primary knowledge base{} (use --set <id>)",
                match session {
                    Some(id) => format!(" for chat {id}"),
                    None => String::new(),
                }
            ))
            .dim()
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

    let (completer, caller_capability) = build_completer(provider, model).await?;

    let spinner = cliclack::spinner();
    spinner.start(format!("ingesting into {}...", kb_id));

    let result = ingest_macro::ingest(
        &svc,
        ingest_macro::IngestArgs {
            kb_id: kb_id.clone(),
            // Issue #56. The tier of the provider that was CONSTRUCTED, never
            // of the `--provider` name the user typed.
            caller_is_private: caller_capability.is_private(),
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

    let (completer, caller_capability) = build_completer(provider, model).await?;

    let spinner = cliclack::spinner();
    spinner.start(format!("digesting {} conversation(s)...", loaded.len()));

    let result = ingest_conversation(
        &svc,
        ConversationIngestArgs {
            kb_id: kb_id.clone(),
            // Issue #56. The tier of the provider that was CONSTRUCTED.
            caller_capability,
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
                session_ids.len().saturating_sub(res.refused),
                style(&kb_id).fg(ACCENT).bold(),
                style(format!("({} steps)", res.ingested.steps)).dim()
            );
            println!(
                "    {} {}",
                style("source:").dim(),
                style(&res.ingested.source_id).dim()
            );
            println!(
                "    {} {}",
                style("commit:").dim(),
                style(short_sha(&res.ingested.commit_sha)).dim()
            );
            // Issue #56, Gate G. A COUNT and nothing else — a session's id,
            // title and working directory are all content (§11.4).
            if res.refused > 0 {
                println!(
                    "  {} {} private conversation(s) skipped: this model is public. \
                     Re-run with a private --provider/--model to include them.",
                    style("!").yellow(),
                    res.refused
                );
            }
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

    // Issue #56: with no `--fix` no provider is constructed, so there is no
    // instance to read a tier from and no model that can write. Public, for the
    // same reason as the test-mode branch above — and Task 10C's barrier still
    // refuses a public caller reading a private base.
    let (completer, caller_capability) = if fix {
        let (c, tier) = build_completer(provider, model).await?;
        (Some(c), tier)
    } else {
        (None, ProviderTier::Public)
    };

    let spinner = cliclack::spinner();
    spinner.start(format!("linting {}...", kb_id));

    let result = lint_macro::lint(
        &svc,
        lint_macro::LintArgs {
            kb_id: kb_id.clone(),
            // Issue #56. The tier of the provider the autofix will run on.
            caller_is_private: caller_capability.is_private(),
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
    let (completer, caller_capability) = build_completer(provider, model).await?;

    let spinner = cliclack::spinner();
    spinner.start(format!("querying {}...", kb_id));

    let result = query_macro::query(
        &svc,
        query_macro::QueryArgs {
            kb_id: kb_id.clone(),
            // Issue #56. `query` writes — see `QueryArgs::caller_is_private`.
            caller_is_private: caller_capability.is_private(),
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

        assert!(
            active_command(&svc, None, None, false, false)?.contains("no primary knowledge base")
        );
        assert!(
            active_command(&svc, None, Some("beta".to_string()), false, false)?.contains("beta")
        );
        assert!(active_command(&svc, None, None, false, false)?.contains("beta"));

        svc.set_hidden_persisted(&["alpha".to_string()])?;
        let err = active_command(&svc, None, Some("alpha".to_string()), false, false)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("alpha") && err.contains("knowledge list"),
            "a hidden base cannot be the primary, and the error must say how to look, got: {err}"
        );

        assert!(active_command(&svc, None, None, true, false)?.contains("cleared"));
        assert_eq!(svc.primary_for_session(None)?, None);
        Ok(())
    }

    /// The explicit "no primary" override is durable by design — that is the
    /// whole point of the blank-file state — but it used to be a **one-way
    /// door**. `delete_base` installs one in every chat that had pinned the
    /// deleted base, and nothing on the CLI or the HTTP surface could take it
    /// back off, so such a chat could never follow the machine-wide default
    /// again. `--inherit` is the way back.
    #[test]
    fn inherit_reopens_a_chat_that_was_left_with_an_explicit_no_primary() -> anyhow::Result<()> {
        let (_tmp, svc) = svc();
        svc.create_base("gamma", "Gamma", None)?;
        svc.set_selection(None, None, PrimaryUpdate::Set("alpha"))?;

        // The chat pinned a base of its own, and that base was then deleted —
        // which must not silently hand the chat the machine-wide default.
        svc.set_primary_for_session("s1", Some("gamma"))?;
        svc.delete_base("gamma")?;
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);
        // Every other gesture leaves the override in place.
        assert!(active_command(&svc, Some("s1"), None, false, false)?
            .contains("no primary knowledge base for chat s1"));

        let out = active_command(&svc, Some("s1"), None, false, true)?;
        assert!(
            out.contains("s1") && out.contains("alpha"),
            "the confirmation must name the chat and the default it now follows, got: {out}"
        );
        assert_eq!(
            svc.primary_for_session(Some("s1"))?.as_deref(),
            Some("alpha"),
            "the chat must follow the machine-wide primary again"
        );
        // And it keeps following it, rather than having copied the value once.
        svc.set_selection(None, None, PrimaryUpdate::Set("beta"))?;
        assert_eq!(
            svc.primary_for_session(Some("s1"))?.as_deref(),
            Some("beta")
        );

        // The machine-wide scope has nothing above it to inherit, so asking is
        // a clean error naming the gesture that does apply …
        let err = active_command(&svc, None, None, false, true)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("--session") && err.contains("--clear"),
            "got: {err}"
        );
        // … and the three gestures are mutually exclusive.
        let err = active_command(&svc, Some("s1"), Some("beta".to_string()), false, true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("at most one"), "got: {err}");
        Ok(())
    }

    /// `--session` makes the machine-wide CLI able to address one chat's
    /// pointer, which is what `--inherit` needs to mean anything. It must not
    /// leak in either direction.
    #[test]
    fn a_session_scoped_primary_does_not_disturb_the_machine_one() -> anyhow::Result<()> {
        let (_tmp, svc) = svc();
        svc.set_selection(None, None, PrimaryUpdate::Set("alpha"))?;

        assert!(
            active_command(&svc, Some("s1"), Some("beta".to_string()), false, false)?
                .contains("beta")
        );
        assert_eq!(
            svc.primary_for_session(Some("s1"))?.as_deref(),
            Some("beta")
        );
        assert_eq!(
            svc.primary_for_session(None)?.as_deref(),
            Some("alpha"),
            "a chat's pin must not move the machine-wide pointer"
        );

        assert!(active_command(&svc, Some("s1"), None, true, false)?.contains("for chat s1"));
        assert_eq!(svc.primary_for_session(Some("s1"))?, None);
        assert_eq!(
            svc.primary_for_session(None)?.as_deref(),
            Some("alpha"),
            "clearing a chat's primary must not clear the machine-wide one"
        );
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

    // ── Issue #56, Task 10B: the CLI's capability ───────────────────────────

    mod privacy_tier {
        use super::super::{build_completer, handle_ingest};
        use biorouter::privacy::ProviderTier;
        use serial_test::serial;

        /// The variables every row below pins, under the workspace's
        /// process-wide environment lock.
        ///
        /// `env_lock` and not a hand-rolled guard plus `#[serial]`: `#[serial]`
        /// serialises against other `#[serial]` tests only, while every other
        /// test in this binary runs concurrently, and the rest of the workspace
        /// already takes this same lock for `BIOROUTER_PATH_ROOT`. Two
        /// mechanisms in one process do not compose.
        ///
        /// `BIOROUTER_KNOWLEDGE_TEST_MODE` must be OFF: `build_completer`'s
        /// early return hands back a `TestModeCompleter` and Public before any
        /// provider exists, which would make every row Public and the whole
        /// matrix vacuous.
        ///
        /// ⚠ The private rows' loopback port is **1**, never 11434.
        /// `is_loopback_host` reads the HOST, so any port is Private, and
        /// nothing can listen on port 1 without root — while 11434 would make
        /// `the_cli_ingest_handler_…` row drive a live local model on any
        /// developer machine running Ollama.
        fn base_env(host: &str) -> Vec<(&'static str, Option<String>)> {
            vec![
                ("BIOROUTER_KNOWLEDGE_TEST_MODE", None),
                ("BIOROUTER_LEAD_MODEL", None),
                ("BIOROUTER_LEAD_PROVIDER", None),
                ("OLLAMA_HOST", Some(host.to_string())),
                ("OLLAMA_TIMEOUT", Some("1".to_string())),
            ]
        }

        /// Take the workspace env lock over `pairs`.
        fn lock_env(pairs: Vec<(&'static str, Option<String>)>) -> env_lock::EnvGuard<'static> {
            env_lock::lock_env(pairs)
        }

        #[tokio::test]
        #[serial]
        async fn the_cli_sources_its_capability_from_the_provider_it_constructed() {
            // ⚠ THE PROVIDER NAME IS `ollama` IN BOTH ROWS, and that is the whole
            // construction: an implementation that keys on the requested name
            // gives the same answer twice and fails one row, and so does either
            // hardcoded literal.
            //
            // What varies is `OLLAMA_HOST`: Task 5 makes a loopback Ollama
            // Private and a non-loopback one Public (`is_loopback_host`), which
            // is exactly a same-name/different-tier pair with no credential and
            // no network anywhere in it. `ModelConfig::new` accepts an unknown
            // model name, so neither row needs a real model either.
            for (host, want) in [
                ("http://127.0.0.1:1", ProviderTier::Private),
                ("http://ollama.invalid:11434", ProviderTier::Public),
            ] {
                let _env = lock_env(base_env(host));
                let (_c, tier) = build_completer(Some("ollama".into()), Some("qwen3.5:4b".into()))
                    .await
                    .unwrap();
                assert_eq!(tier, want, "OLLAMA_HOST={host}");
            }
        }

        #[tokio::test]
        #[serial]
        async fn the_cli_capability_follows_the_instance_not_the_name_the_user_typed() {
            // The row that fails `provider_name == "ollama"`. `providers::create`
            // intercepts BIOROUTER_LEAD_MODEL *before* the registry lookup, so
            // `--provider ollama` can construct a lead/worker composite whose
            // tier is `least(lead, worker)` — and a PUBLIC lead makes the whole
            // instance Public even though the name typed was the private one.
            let mut env = base_env("http://127.0.0.1:1");
            env.push(("BIOROUTER_LEAD_MODEL", Some("gpt-5".to_string())));
            env.push((
                "BIOROUTER_LEAD_PROVIDER",
                Some("github_copilot".to_string()),
            ));
            let _env = lock_env(env);

            let (_c, tier) = build_completer(Some("ollama".into()), Some("qwen3.5:4b".into()))
                .await
                .unwrap();
            assert_eq!(
                tier,
                ProviderTier::Public,
                "the CLI keyed its capability on the name the user typed"
            );
        }

        /// `service()` is `KnowledgeService::new_default()`, whose root is
        /// `paths::knowledge_root()` → `$BIOROUTER_PATH_ROOT/config/knowledge`
        /// when the override is set. That is how this row gets a throwaway store
        /// instead of the developer's own.
        ///
        /// ⚠ The caller owns the `TempDir` and must keep it BOUND — a dropped
        /// one deletes the tree before the call runs. It also passes
        /// `BIOROUTER_PATH_ROOT` to `lock_env` itself, so that every variable
        /// this module touches is set under the workspace lock and none behind
        /// its back.
        fn cli_knowledge_root_with_base(tmp: &tempfile::TempDir, id: &str) -> std::path::PathBuf {
            let root = tmp.path().join("config").join("knowledge");
            std::fs::create_dir_all(&root).unwrap();
            let svc = biorouter::knowledge::service::KnowledgeService::new(root.clone());
            svc.create_base(id, id, None).unwrap();
            root
        }

        #[tokio::test]
        #[serial]
        async fn the_cli_ingest_handler_ratchets_from_the_instance_and_the_name_is_the_same_in_both_legs(
        ) {
            // Round 3 §7: a handler can call `paired`, ignore its tier, and derive
            // the capability from the requested provider name — every structural
            // count in Step 5 still passes. Only a handler-level behavioural row
            // sees that, and only if the NAME is constant across the legs.
            for (host, want_private) in [
                ("http://127.0.0.1:1", true),
                ("http://ollama.invalid:11434", false),
            ] {
                let tmp = tempfile::TempDir::new().unwrap();
                let mut env = base_env(host);
                env.push((
                    "BIOROUTER_PATH_ROOT",
                    Some(tmp.path().to_string_lossy().into_owned()),
                ));
                let _env = lock_env(env);
                let root = cli_knowledge_root_with_base(&tmp, "k");

                // The sub-agent WILL fail — nothing answers on either host — and
                // that is the point: CP2 raises before it runs, so the ratchet is
                // observable without an LLM.
                let _ = handle_ingest(
                    Some("k".into()),
                    None,
                    None,
                    Some("n=412".into()),
                    None,
                    Some("ollama".into()),
                    Some("qwen3.5:4b".into()),
                )
                .await;

                assert_eq!(
                    biorouter::knowledge::tier::is_private(&root, "k"),
                    want_private,
                    "OLLAMA_HOST={host}: the handler did not read the constructed instance"
                );
            }
        }
    }
}
