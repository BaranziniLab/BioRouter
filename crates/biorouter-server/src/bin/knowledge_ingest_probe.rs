use anyhow::{Context, Result};
use biorouter::{config::Config, knowledge::ProviderCompleter, model::ModelConfig, providers};
use biorouter_mcp::knowledge::{
    convert::SourceInput,
    macros::ingest::{ingest, IngestArgs},
    service::KnowledgeService,
    subagent::{
        events::{DoneReason, SubAgentEvent},
        loop_::SubAgentBounds,
    },
};
use clap::Parser;
use std::{
    fs,
    path::PathBuf,
    process,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
struct Cli {
    #[arg(required = true)]
    files: Vec<PathBuf>,

    #[arg(long)]
    provider: Option<String>,

    #[arg(long)]
    model: Option<String>,

    #[arg(long)]
    root: Option<PathBuf>,

    #[arg(long, default_value = "probe")]
    kb_id: String,

    #[arg(long, default_value = "Knowledge Ingest Probe")]
    kb_name: String,

    #[arg(long)]
    keep_root: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let provider_name = cli
        .provider
        .clone()
        .unwrap_or(read_config_string("BIOROUTER_PROVIDER")?);
    let model_name = cli
        .model
        .clone()
        .unwrap_or(read_config_string("BIOROUTER_MODEL")?);
    let root = cli.root.clone().unwrap_or_else(default_probe_root);

    fs::create_dir_all(&root)?;

    let svc = KnowledgeService::new(root.clone());
    if !root.join(&cli.kb_id).exists() {
        svc.create_base(&cli.kb_id, &cli.kb_name, None)
            .with_context(|| format!("create base '{}'", cli.kb_id))?;
    }

    let model_config = ModelConfig::new(&model_name)
        .map_err(|err| anyhow::anyhow!("invalid model '{model_name}': {err}"))?;
    let provider = providers::create(&provider_name, model_config)
        .await
        .with_context(|| format!("create provider '{provider_name}'"))?;

    println!("Knowledge root: {}", root.display());
    println!("Provider: {provider_name}");
    println!("Model: {model_name}");
    println!("KB: {}", cli.kb_id);
    println!();

    let mut failures = 0usize;

    for file in &cli.files {
        println!("=== Digesting {} ===", file.display());
        if !file.exists() {
            failures += 1;
            println!("missing file");
            println!();
            continue;
        }

        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<SubAgentEvent>();
        let event_task = tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                print_event(&event);
            }
        });

        let svc_for_task = svc.clone();
        let provider_for_task = Arc::clone(&provider);
        let kb_id = cli.kb_id.clone();
        let file_path = file.clone();
        let ingest_task = tokio::spawn(async move {
            // Issue #56. The probe holds the `Arc<dyn Provider>` it wraps, so it
            // destructures `paired` rather than re-deriving a tier from
            // `cli.provider` — the completer and the capability come from the
            // one binding, which is this caller's whole gate (it has no
            // behavioural row: `[[bin]]` targets are never compiled by
            // `cargo test --lib`).
            let (completer, caller_capability, caller_affiliation) =
                ProviderCompleter::paired(provider_for_task);
            ingest(
                &svc_for_task,
                IngestArgs {
                    kb_id,
                    caller_is_private: caller_capability.is_private(),
                    caller_affiliation: biorouter::privacy::affiliation::caller_affiliation(
                        caller_affiliation,
                    ),
                    source: SourceInput::Path(file_path),
                    completer: Box::new(completer),
                    focus: None,
                    bounds: SubAgentBounds {
                        max_steps: 60,
                        max_wall: Duration::from_secs(900),
                        max_tokens: 200_000,
                    },
                    event_sink: Some(event_tx),
                    cancel: None,
                },
            )
            .await
        });

        match ingest_task.await {
            Ok(Ok(result)) => {
                println!("source_id: {}", result.source_id);
                println!("commit: {}", result.commit_sha);
                println!("steps: {}", result.steps);
                let graph = svc.get_graph(&cli.kb_id)?;
                println!("graph nodes: {}", graph.nodes.len());
                println!("graph edges: {}", graph.edges.len());
            }
            Ok(Err(err)) => {
                failures += 1;
                println!("error: {err:#}");
            }
            Err(join_err) => {
                failures += 1;
                println!("panic: {join_err}");
            }
        }

        let _ = event_task.await;
        println!();
    }

    if !cli.keep_root && failures == 0 {
        let _ = fs::remove_dir_all(&root);
    }

    if failures > 0 {
        anyhow::bail!("{failures} file(s) failed");
    }

    Ok(())
}

fn read_config_string(key: &str) -> Result<String> {
    Config::global()
        .get_param::<String>(key)
        .with_context(|| format!("read {key} from config"))
}

fn default_probe_root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!(
        "biorouter-knowledge-probe-{}-{stamp}",
        process::id()
    ))
}

fn print_event(event: &SubAgentEvent) {
    match event {
        SubAgentEvent::Step {
            index,
            assistant_text,
        } => {
            if assistant_text.trim().is_empty() {
                println!("step {index}:");
            } else {
                println!("step {index}: {}", assistant_text.trim());
            }
        }
        SubAgentEvent::ToolCall { name, args } => {
            println!("tool call {name}: {}", args);
        }
        SubAgentEvent::ToolResult { name, ok, summary } => {
            let status = if *ok { "ok" } else { "error" };
            println!("tool result {name} [{status}]: {}", summary.trim());
        }
        SubAgentEvent::Done { reason, final_text } => {
            println!(
                "done {:?}: {}",
                render_done_reason(reason),
                final_text.trim()
            );
        }
    }
}

fn render_done_reason(reason: &DoneReason) -> &'static str {
    match reason {
        DoneReason::CompleteSentinel => "complete_sentinel",
        DoneReason::NoMoreToolCalls => "no_more_tool_calls",
        DoneReason::StepBudgetReached => "step_budget_reached",
        DoneReason::TimeBudgetReached => "time_budget_reached",
        DoneReason::TokenBudgetReached => "token_budget_reached",
        DoneReason::VocabularyRetriesExhausted => "vocabulary_retries_exhausted",
        DoneReason::Cancelled => "cancelled",
        DoneReason::Error => "error",
    }
}
