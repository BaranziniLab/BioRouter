//! Fixture generator for the artifact-panel browser harness.
//!
//! Ignored by default. It drives the real MCP transport — the same duplex path
//! `extension_manager` uses — so the HTML it writes is byte-for-byte what the
//! desktop artifact panel receives at runtime.
//!
//!   PREVIEW_FIXTURE_DIR=/tmp/fixtures cargo test -p biorouter-mcp \
//!     --test preview_fixture_dump -- --ignored --nocapture

use base64::{engine::general_purpose::STANDARD, Engine as _};
use biorouter_mcp::AgentDrafterServer;
use image::{Rgb, RgbImage};
use rmcp::model::{CallToolRequestParams, RawContent, ResourceContents};
use rmcp::ServiceExt;
use serde_json::json;
use tempfile::TempDir;

fn fixture_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("PREVIEW_FIXTURE_DIR").expect("set PREVIEW_FIXTURE_DIR"))
}

/// Pull the first `ui://` HTML blob out of a tool result.
fn blob_html(result: &rmcp::model::CallToolResult) -> Option<String> {
    for content in &result.content {
        if let RawContent::Resource(embedded) = &content.raw {
            if let ResourceContents::BlobResourceContents { blob, .. } = &embedded.resource {
                return Some(String::from_utf8(STANDARD.decode(blob).unwrap()).unwrap());
            }
        }
    }
    None
}

#[tokio::test]
#[ignore = "fixture generator; writes HTML to $PREVIEW_FIXTURE_DIR"]
async fn dump_agent_drafter_card() {
    let out = fixture_dir();
    std::fs::create_dir_all(&out).unwrap();

    let tmp = TempDir::new().unwrap();
    let server = AgentDrafterServer::with_root(tmp.path().to_path_buf());

    let (server_read, client_write) = tokio::io::duplex(1 << 22);
    let (client_read, server_write) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        if let Ok(running) = server.serve((server_read, server_write)).await {
            let _ = running.waiting().await;
        }
    });
    let client = ().serve((client_read, client_write)).await.unwrap();

    let created = client
        .call_tool(CallToolRequestParams {
            name: "create_app".into(),
            arguments: json!({
                "title": "Cohort Explorer",
                "description": "Ask questions about the trial cohort.",
                "kind": "agentic",
                "system_prompt": "Help researchers explore a paired tumour and normal cohort.",
                "greeting": "What would you like to learn from the cohort?",
                "html": r#"<!doctype html>
                    <html lang="en">
                      <head><meta charset="utf-8"><title>Cohort Explorer</title></head>
                      <body>
                        <main class="br-container">
                          <div class="br-row" style="align-items:end">
                            <div>
                              <div class="br-kicker">Agentic application</div>
                              <h1>Cohort Explorer</h1>
                              <p class="br-muted">48 paired tumour and normal samples are ready.</p>
                            </div>
                            <span class="br-badge br-badge--success">Connected</span>
                          </div>
                          <section class="br-card" aria-labelledby="question-title">
                            <h2 id="question-title">Ask about this cohort</h2>
                            <label class="br-label" for="question">Research question</label>
                            <textarea id="question" class="br-textarea">Which pathways separate tumour from normal tissue?</textarea>
                            <div class="br-row" style="margin-top:12px;align-items:center">
                              <label class="br-switch">
                                <input id="citations" type="checkbox" checked>
                                <span class="br-switch__track"></span>
                                Include source citations
                              </label>
                              <button id="go" class="br-btn" type="button">Summarise cohort</button>
                            </div>
                          </section>
                          <section id="result" class="br-panel" aria-live="polite">
                            Choose a question, then run the agent.
                          </section>
                        </main>
                        <script>
                          const button = document.getElementById('go');
                          const result = document.getElementById('result');
                          button.addEventListener('click', () => {
                            button.disabled = true;
                            button.textContent = 'Analysing…';
                            result.textContent = 'Comparing 24 paired samples…';
                            setTimeout(() => {
                              result.innerHTML = '<strong>MYC signalling is the strongest separator.</strong><p>412 genes pass FDR &lt; 0.05 across the paired analysis.</p>';
                              button.disabled = false;
                              button.textContent = 'Summarise again';
                            }, 120);
                          });
                        </script>
                      </body>
                    </html>"#
            })
            .as_object()
            .cloned(),
            task: None,
            meta: None,
        })
        .await
        .unwrap();

    let html = blob_html(&created).expect("create_app must return a ui:// preview card");
    std::fs::write(out.join("agent-drafter-card.html"), &html).unwrap();
    println!("wrote agent-drafter-card.html ({} bytes)", html.len());

    // `launch_app`/`build_app` also return this card; that is asserted in the
    // agent_drafter unit tests, where an SDK-wired app can pass the lint harness.

    client.cancel().await.unwrap();
}

#[test]
#[ignore = "fixture generator; writes sample files to $PREVIEW_FIXTURE_DIR"]
fn dump_file_previews() {
    let out = fixture_dir();
    std::fs::create_dir_all(&out).unwrap();

    let fixtures = [
        (
            "report.md",
            "# Differential expression report\n\n**412 genes** pass FDR < 0.05.\n\n| Pathway | NES |\n|---|---:|\n| MYC targets | 2.8 |\n| E2F targets | 2.2 |\n",
        ),
        (
            "genes.csv",
            "gene,log2_fold_change,fdr\nMYC,2.81,0.0004\nCDK4,1.92,0.003\n\"HLA-DRA\",-1.44,0.012\n",
        ),
        (
            "config.json",
            "{\n  \"cohort\": \"paired-tumour-normal\",\n  \"fdr\": 0.05,\n  \"replicates\": 24\n}\n",
        ),
        (
            "pipeline.yaml",
            "name: differential-expression\nsteps:\n  - normalize\n  - fit-model\n  - adjust-p-values\n",
        ),
        (
            "sample.xml",
            "<cohort name=\"paired\"><sample id=\"T01\" tissue=\"tumour\" /></cohort>\n",
        ),
        (
            "analysis.R",
            "library(DESeq2)\ndds <- DESeq(dds)\nresults(dds, alpha = 0.05)\n",
        ),
        (
            "summarize.py",
            "from pathlib import Path\n\nprint(Path(\"report.md\").read_text())\n",
        ),
        (
            "query.sql",
            "SELECT gene, log2_fold_change, fdr\nFROM differential_expression\nWHERE fdr < 0.05\nORDER BY fdr;\n",
        ),
        (
            "page.html",
            "<!doctype html><html><body><main><h1>Analysis complete</h1><p>412 genes pass FDR &lt; 0.05.</p></main></body></html>\n",
        ),
        (
            "theme.css",
            ":root { color-scheme: light dark; }\nmain { max-width: 70ch; margin: auto; }\n",
        ),
        (
            "run.sh",
            "#!/usr/bin/env bash\nset -euo pipefail\nRscript analysis.R\n",
        ),
        (
            "notes.txt",
            "Paired design. Adjust for donor and sequencing batch.\n",
        ),
        (
            "Cargo.toml",
            "[package]\nname = \"cohort-analysis\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "lib.rs",
            "pub fn significant(fdr: f64) -> bool {\n    fdr < 0.05\n}\n",
        ),
        (
            "app.ts",
            "export const cohort = { pairedSamples: 24, threshold: 0.05 } as const;\n",
        ),
    ];

    for (name, content) in fixtures {
        std::fs::write(out.join(name), content).unwrap();
    }

    let mut image = RgbImage::from_pixel(800, 480, Rgb([250, 248, 243]));
    for x in 60..760 {
        image.put_pixel(x, 420, Rgb([97, 90, 70]));
    }
    for y in 40..421 {
        image.put_pixel(60, y, Rgb([97, 90, 70]));
    }
    for index in 0..96 {
        let x = 78 + (index * 71 % 650);
        let y = 70 + (index * 137 % 320);
        let colour = if index % 5 == 0 {
            Rgb([207, 109, 71])
        } else {
            Rgb([97, 90, 70])
        };
        for dx in 0..5 {
            for dy in 0..5 {
                image.put_pixel(x + dx, y + dy, colour);
            }
        }
    }
    image.save(out.join("volcano.png")).unwrap();

    println!("wrote {} file preview fixtures", fixtures.len() + 1);
}
