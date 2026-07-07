//! Comprehensive live survey of EVERY curated Llama Server model.
//!
//! Unlike `llamacpp_integration.rs` (one tiny model, hard assertions), this is
//! a *survey*: it walks the whole `MODEL_CATALOG`, downloads/loads each model
//! through the real Biorouter provider + managed sidecar, and runs a battery of
//! capability / correctness / speed / robustness checks, writing an
//! incrementally-updated markdown report. Individual model failures are
//! recorded, never panicked on, so one bad model never aborts the run.
//!
//! ```sh
//! BIOROUTER_LLAMACPP_BIN=ui/desktop/src/bin/llamacpp/llama-server \
//!   cargo test -p biorouter --test llamacpp_survey -- --ignored --nocapture
//! ```
//!
//! Env knobs:
//!   LLAMACPP_SURVEY_MODELS   comma-separated catalog names to limit the run
//!                            (default: every catalog entry, in order)
//!   LLAMACPP_SURVEY_OUT      report path (default: ~/Desktop/llamacpp-survey-report.md)
//!
//! First run of each model includes a multi-GB Hugging Face download.

use std::time::{Duration, Instant};

use biorouter::conversation::message::{Message, MessageContent};
use biorouter::model::ModelConfig;
use biorouter::providers::base::Provider;
use biorouter::providers::llamacpp::{resolve_hf_spec, LlamaCppProvider, MODEL_CATALOG};
use biorouter::providers::llamacpp_sidecar::{global, SidecarState};
use futures::StreamExt;
use rmcp::{model::Tool, object};

// Generous so large models on a slow link still finish DOWNLOADING within the
// window (the xet CDN here ranges from ~0.6 to ~20 MB/s; the 26B is ~16.9 GB).
// llama.cpp resumes partial downloads across restarts, so a kill/restart never
// loses progress. Override with LLAMACPP_SURVEY_READY_SECS.
fn ready_timeout() -> Duration {
    let secs = std::env::var("LLAMACPP_SURVEY_READY_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(28_800); // 8 hours
    Duration::from_secs(secs)
}

#[derive(Default)]
struct Check {
    pass: bool,
    detail: String,
}

impl Check {
    fn ok(detail: impl Into<String>) -> Self {
        Check {
            pass: true,
            detail: detail.into(),
        }
    }
    fn fail(detail: impl Into<String>) -> Self {
        Check {
            pass: false,
            detail: detail.into(),
        }
    }
    fn mark(&self) -> &'static str {
        if self.pass {
            "✅"
        } else {
            "❌"
        }
    }
}

#[derive(Default)]
struct ModelReport {
    name: String,
    display_name: String,
    hf_spec: String,
    download_size: String,
    context_limit: usize,
    /// Download + load time to first Ready.
    load_secs: Option<f64>,
    availability: Check,
    correctness: Check,
    thinking_off: Check,
    tool_calling: Check,
    streaming: Check,
    speed_toks_per_sec: Option<f64>,
    speed: Check,
}

fn weather_tool() -> Tool {
    Tool::new(
        "get_weather".to_string(),
        "Get current temperature for a given location.".to_string(),
        object!({
            "type": "object",
            "required": ["location"],
            "properties": { "location": {"type": "string"} }
        }),
    )
}

async fn provider_for(name: &str, max_tokens: usize) -> anyhow::Result<LlamaCppProvider> {
    let model = ModelConfig::new(name)?.with_max_tokens(Some(max_tokens as i32));
    LlamaCppProvider::from_env(model).await
}

/// Run the full battery against one already-Ready model.
async fn run_battery(report: &mut ModelReport) {
    let provider = match provider_for(&report.name, 256).await {
        Ok(p) => p,
        Err(e) => {
            report.correctness = Check::fail(format!("provider init failed: {e}"));
            return;
        }
    };

    // 1. Correctness + 2. thinking-off (content must be non-empty; reasoning
    //    models can otherwise burn the budget on thinking tokens).
    let msgs = vec![Message::user().with_text("Reply with exactly the word: pong")];
    match provider
        .complete("You are a terse assistant.", &msgs, &[])
        .await
    {
        Ok((message, usage)) => {
            let text = message.as_concat_text();
            let low = text.to_lowercase();
            report.thinking_off = if text.trim().is_empty() {
                Check::fail("empty content (thinking-off likely NOT applied)")
            } else {
                Check::ok("non-empty content")
            };
            report.correctness = if low.contains("pong") {
                Check::ok(format!(
                    "said pong ({} out tok)",
                    usage.usage.output_tokens.unwrap_or(0)
                ))
            } else {
                Check::fail(format!("no 'pong' in: {:?}", truncate(&text, 80)))
            };
        }
        Err(e) => {
            report.correctness = Check::fail(format!("complete error: {e}"));
            report.thinking_off = Check::fail("not evaluated");
        }
    }

    // 3. Tool calling.
    let msgs = vec![Message::user()
        .with_text("What is the weather in Paris? Use the get_weather tool to find out.")];
    match provider
        .complete(
            "You are an AI agent. Always use the provided tools to answer questions.",
            &msgs,
            &[weather_tool()],
        )
        .await
    {
        Ok((message, _)) => {
            let has_tool = message
                .content
                .iter()
                .any(|c| matches!(c, MessageContent::ToolRequest(_)));
            report.tool_calling = if has_tool {
                Check::ok("emitted get_weather tool request")
            } else {
                Check::fail(format!(
                    "no tool call; said: {:?}",
                    truncate(&message.as_concat_text(), 80)
                ))
            };
        }
        Err(e) => report.tool_calling = Check::fail(format!("error: {e}")),
    }

    // 4. Streaming.
    let msgs = vec![Message::user().with_text("Count from 1 to 5, digits only.")];
    match provider
        .stream("You are a terse assistant.", &msgs, &[])
        .await
    {
        Ok(mut stream) => {
            let mut chunks = 0usize;
            let mut text = String::new();
            let mut stream_err = None;
            while let Some(item) = stream.next().await {
                match item {
                    Ok((Some(m), _)) => {
                        text.push_str(&m.as_concat_text());
                        chunks += 1;
                    }
                    Ok((None, _)) => chunks += 1,
                    Err(e) => {
                        stream_err = Some(e.to_string());
                        break;
                    }
                }
            }
            report.streaming = match stream_err {
                Some(e) => Check::fail(format!("stream error: {e}")),
                None if chunks > 1 => Check::ok(format!("{chunks} chunks")),
                None => Check::fail(format!("only {chunks} chunk(s)")),
            };
        }
        Err(e) => report.streaming = Check::fail(format!("stream start error: {e}")),
    }

    // 5. Speed: generate a chunk of text, tokens/sec from reported output tokens.
    let msgs = vec![Message::user()
        .with_text("Write a single detailed paragraph (about 120 words) explaining what mitochondria do in a cell.")];
    let t0 = Instant::now();
    match provider
        .complete("You are a helpful science tutor.", &msgs, &[])
        .await
    {
        Ok((_message, usage)) => {
            let elapsed = t0.elapsed().as_secs_f64();
            let out = usage.usage.output_tokens.unwrap_or(0).max(0) as f64;
            if out > 0.0 && elapsed > 0.0 {
                let tps = out / elapsed;
                report.speed_toks_per_sec = Some(tps);
                report.speed = Check::ok(format!("{tps:.1} tok/s ({out:.0} tok in {elapsed:.1}s)"));
            } else {
                report.speed = Check::fail("no output tokens reported");
            }
        }
        Err(e) => report.speed = Check::fail(format!("error: {e}")),
    }
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() <= n {
        s
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

fn report_path() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("LLAMACPP_SURVEY_OUT") {
        return std::path::PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home).join("Desktop/llamacpp-survey-report.md")
}

fn write_report(reports: &[ModelReport], in_progress: Option<&str>) {
    let mut md = String::new();
    md.push_str("# Llama Server — Embedded llama.cpp Model Survey\n\n");
    md.push_str(
        "Engine: Biorouter managed `llama-server` sidecar (build pinned). \
         Host: Apple M4 Max / 128 GB. Each model tested through the real \
         `LlamaCppProvider` (thinking-off, q8_0 KV, 32k ctx).\n\n",
    );
    if let Some(m) = in_progress {
        md.push_str(&format!("> ⏳ Currently testing: **{m}** …\n\n"));
    }

    md.push_str("## Summary\n\n");
    md.push_str(
        "| Model | Size | Load(s) | Avail | Correct | Think-off | Tools | Stream | tok/s |\n",
    );
    md.push_str(
        "|-------|------|--------:|:-----:|:-------:|:---------:|:-----:|:------:|------:|\n",
    );
    for r in reports {
        md.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.name,
            r.download_size,
            r.load_secs
                .map(|s| format!("{s:.0}"))
                .unwrap_or_else(|| "—".into()),
            r.availability.mark(),
            r.correctness.mark(),
            r.thinking_off.mark(),
            r.tool_calling.mark(),
            r.streaming.mark(),
            r.speed_toks_per_sec
                .map(|t| format!("{t:.1}"))
                .unwrap_or_else(|| "—".into()),
        ));
    }

    md.push_str("\n## Details\n\n");
    for r in reports {
        md.push_str(&format!("### {} (`{}`)\n\n", r.display_name, r.name));
        md.push_str(&format!(
            "- HF spec: `{}` — {}, ctx {}\n",
            r.hf_spec, r.download_size, r.context_limit
        ));
        if let Some(s) = r.load_secs {
            md.push_str(&format!("- Load to Ready: {s:.1}s\n"));
        }
        md.push_str(&format!(
            "- {} Availability: {}\n",
            r.availability.mark(),
            r.availability.detail
        ));
        md.push_str(&format!(
            "- {} Correctness: {}\n",
            r.correctness.mark(),
            r.correctness.detail
        ));
        md.push_str(&format!(
            "- {} Thinking-off: {}\n",
            r.thinking_off.mark(),
            r.thinking_off.detail
        ));
        md.push_str(&format!(
            "- {} Tool calling: {}\n",
            r.tool_calling.mark(),
            r.tool_calling.detail
        ));
        md.push_str(&format!(
            "- {} Streaming: {}\n",
            r.streaming.mark(),
            r.streaming.detail
        ));
        md.push_str(&format!(
            "- {} Speed: {}\n\n",
            r.speed.mark(),
            r.speed.detail
        ));
    }

    let path = report_path();
    if let Err(e) = std::fs::write(&path, md) {
        eprintln!("failed to write report to {}: {e}", path.display());
    }
}

#[tokio::test]
#[ignore = "downloads and loads every catalog model (~41 GB); run explicitly"]
async fn survey_all_models() {
    let filter: Option<Vec<String>> = std::env::var("LLAMACPP_SURVEY_MODELS").ok().map(|s| {
        s.split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect()
    });

    // When a filter is given, honor its ORDER (so callers can put a cached or
    // small model first); otherwise walk the catalog in its natural order.
    let entries: Vec<_> = match &filter {
        Some(names) => names
            .iter()
            .filter_map(|n| MODEL_CATALOG.iter().find(|e| e.name == n.as_str()))
            .collect(),
        None => MODEL_CATALOG.iter().collect(),
    };

    println!("Surveying {} model(s)", entries.len());
    let mut reports: Vec<ModelReport> = Vec::new();

    for entry in entries {
        let mut report = ModelReport {
            name: entry.name.to_string(),
            display_name: entry.display_name.to_string(),
            hf_spec: entry.hf_spec.to_string(),
            download_size: entry.download_size.to_string(),
            context_limit: entry.context_limit,
            ..Default::default()
        };
        println!(
            "\n=== {} ({}) — {} ===",
            entry.display_name, entry.name, entry.download_size
        );

        // Push a placeholder so the in-progress report shows this model.
        reports.push(std::mem::take(&mut report));
        let idx = reports.len() - 1;
        write_report(&reports, Some(entry.name));

        // 1. Availability: ensure + wait_ready, timing the download+load.
        let spec = match resolve_hf_spec(entry.name) {
            Ok(s) => s,
            Err(e) => {
                reports[idx].availability = Check::fail(format!("resolve_hf_spec: {e}"));
                write_report(&reports, None);
                continue;
            }
        };
        let t0 = Instant::now();
        match global().ensure(entry.name, &spec).await {
            Ok(_port) => match global().wait_ready(ready_timeout()).await {
                Ok(_) => {
                    let load = t0.elapsed().as_secs_f64();
                    reports[idx].load_secs = Some(load);
                    let st = global().status().await;
                    if st.state == SidecarState::Ready {
                        reports[idx].availability = Check::ok(format!("Ready in {load:.1}s"));
                        println!("  ready in {load:.1}s; running battery…");
                        run_battery(&mut reports[idx]).await;
                    } else {
                        reports[idx].availability =
                            Check::fail(format!("state={:?} detail={:?}", st.state, st.detail));
                    }
                }
                Err(e) => reports[idx].availability = Check::fail(format!("not ready: {e}")),
            },
            Err(e) => reports[idx].availability = Check::fail(format!("ensure failed: {e}")),
        }

        // Free the multi-GB model from RAM before the next one.
        global().stop().await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        write_report(&reports, None);
        println!("  done: {}", summarize(&reports[idx]));
    }

    write_report(&reports, None);
    println!("\nReport written to {}", report_path().display());
}

fn summarize(r: &ModelReport) -> String {
    format!(
        "avail={} correct={} think-off={} tools={} stream={} speed={}",
        r.availability.mark(),
        r.correctness.mark(),
        r.thinking_off.mark(),
        r.tool_calling.mark(),
        r.streaming.mark(),
        r.speed_toks_per_sec
            .map(|t| format!("{t:.1}t/s"))
            .unwrap_or_else(|| "—".into()),
    )
}
