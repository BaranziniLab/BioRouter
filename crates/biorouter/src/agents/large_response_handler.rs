//! BR-6: token-aware handling of an oversized tool response.
//!
//! The old handler applied a 200,000-**character** (~50k token) threshold to
//! each content item independently, and remediated by dumping the blob to
//! `std::env::temp_dir()` with no preview, no line count and no token
//! accounting. Several items each just under the threshold still blew the
//! context, the model got a bare path *outside* the session working dir (so a
//! sandboxed file/shell tool might not even be able to reach it), and a failed
//! write inlined the whole payload anyway.
//!
//! This module instead:
//!   * measures the **aggregate** token count of all text content in one tool
//!     result (real tokenizer, with a cheap char lower-bound pre-filter so a
//!     normal-sized result never pays for tokenization),
//!   * offloads the full output to a **handle inside the session working dir**
//!     (`.biorouter/tool-output/`, self-ignored from git), falling back to the
//!     legacy temp dir, and
//!   * replies with a bounded **head/tail preview** (via the BR-11
//!     [`truncate_middle_out`] helper it shares with compaction), a
//!     line/char/token summary, and instructions to search or read the handle.
//!
//! Runs at dispatch time, *before* the tool-output guardrail
//! (`guardrails::tool_output`, applied in `Agent::integrate_tool_result`), so
//! the guardrail never has to scan a multi-megabyte payload; the offloaded
//! content is scanned instead when the model reads the file back through a
//! later tool call.

use crate::config::Config;
use crate::context_mgmt::truncate_middle_out;
use crate::token_counter::create_token_counter;
use chrono::Utc;
use rmcp::model::{CallToolResult, Content, ErrorData};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

/// Aggregate inline token budget for a single tool result. Above this the
/// result is offloaded to a handle and replaced by a preview. ~25k tokens is
/// about an eighth of a 200k window — roomy enough for real grep/SQL/omics
/// output, small enough that one tool call can no longer eat the context.
/// Override with `BIOROUTER_LARGE_RESPONSE_TOKENS`.
pub const DEFAULT_LARGE_RESPONSE_TOKENS: usize = 25_000;

/// How much of an offloaded result stays inline as a head/tail preview.
/// Override with `BIOROUTER_LARGE_RESPONSE_PREVIEW_TOKENS`.
pub const DEFAULT_LARGE_RESPONSE_PREVIEW_TOKENS: usize = 4_000;

/// Used only when the tokenizer cannot be initialized: a deliberately
/// conservative (low) chars-per-token divisor, so the estimate errs high and we
/// offload slightly early rather than overflow the window.
const FALLBACK_CHARS_PER_TOKEN: usize = 3;

/// Directory (relative to the session working dir) that holds offloaded tool
/// output. It carries its own `.gitignore` so it never shows up in `git status`.
const HANDLE_DIR: &str = ".biorouter/tool-output";

/// Everything the handler needs to place the handle where the model's file and
/// shell tools can actually reach it.
#[derive(Clone, Debug)]
pub struct LargeResponseContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub tool_name: String,
}

/// Resolved token budgets (env / `config.yaml`, with defaults).
#[derive(Clone, Copy, Debug)]
struct LargeResponseLimits {
    max_tokens: usize,
    preview_tokens: usize,
}

impl LargeResponseLimits {
    fn from_config() -> Self {
        let config = Config::global();
        let max_tokens = config
            .get_param::<usize>("BIOROUTER_LARGE_RESPONSE_TOKENS")
            .unwrap_or(DEFAULT_LARGE_RESPONSE_TOKENS)
            .max(1);
        let preview_tokens = config
            .get_param::<usize>("BIOROUTER_LARGE_RESPONSE_PREVIEW_TOKENS")
            .unwrap_or(DEFAULT_LARGE_RESPONSE_PREVIEW_TOKENS)
            // A preview at least as large as the limit itself would make the
            // offload pointless.
            .min(max_tokens.saturating_sub(1).max(1));
        Self {
            max_tokens,
            preview_tokens,
        }
    }
}

/// Measured size of the aggregate text payload of one tool result.
#[derive(Clone, Copy, Debug)]
struct Stats {
    tokens: usize,
    chars: usize,
    lines: usize,
}

/// A written handle: the absolute path plus — when it landed inside the session
/// working dir — the working-dir-relative path the model's tools understand.
#[derive(Clone, Debug)]
struct Handle {
    absolute: PathBuf,
    relative: Option<PathBuf>,
}

/// Process a tool response, offloading it when its aggregate text is over the
/// token budget. Errors and non-text content pass through unchanged.
pub async fn process_tool_response(
    response: Result<CallToolResult, ErrorData>,
    ctx: &LargeResponseContext,
) -> Result<CallToolResult, ErrorData> {
    let mut result = match response {
        Ok(result) => result,
        Err(e) => return Err(e),
    };

    let text_indices: Vec<usize> = result
        .content
        .iter()
        .enumerate()
        .filter(|(_, c)| c.as_text().is_some())
        .map(|(i, _)| i)
        .collect();
    if text_indices.is_empty() {
        return Ok(result);
    }

    let limits = LargeResponseLimits::from_config();

    let texts: Vec<&str> = text_indices
        .iter()
        .filter_map(|i| result.content[*i].as_text().map(|t| t.text.as_str()))
        .collect();

    // Aggregate across *all* text items: several items each just under a
    // per-item threshold used to slip through and blow the context together.
    //
    // Cheap lower bound first: a BPE token always covers at least one
    // character, so `tokens <= chars`. A payload under the token budget in
    // *characters* cannot be over it in tokens — so the common path neither
    // tokenizes nor copies the payload.
    let chars: usize =
        texts.iter().map(|t| t.chars().count()).sum::<usize>() + texts.len().saturating_sub(1); // the "\n" separators below
    if chars <= limits.max_tokens {
        return Ok(result);
    }

    let joined = Arc::new(texts.join("\n"));
    let tokens = count_tokens(Arc::clone(&joined)).await;
    if tokens <= limits.max_tokens {
        return Ok(result);
    }

    let lines = joined.lines().count();
    let handle = write_handle(&joined, ctx);
    if let Err(e) = &handle {
        warn!(
            tool = %ctx.tool_name,
            "BR-6: could not offload a {tokens}-token tool response to a file: {e}; \
             falling back to a preview-only reply"
        );
    }
    debug!(
        tool = %ctx.tool_name,
        tokens, chars, lines,
        "BR-6: offloading oversized tool response"
    );

    let summary = render_summary(
        &joined,
        Stats {
            tokens,
            chars,
            lines,
        },
        handle.ok().as_ref(),
        limits,
        ctx,
    );

    // Rebuild the content: the summary takes the place of the first text item,
    // the remaining text items are dropped (their content lives in the handle),
    // and every non-text item (images, resources) is preserved in order.
    let first_text = text_indices[0];
    let mut processed = Vec::with_capacity(result.content.len());
    for (i, content) in std::mem::take(&mut result.content).into_iter().enumerate() {
        if i == first_text {
            processed.push(Content::text(summary.clone()));
        } else if !text_indices.contains(&i) {
            processed.push(content);
        }
    }
    result.content = processed;
    Ok(result)
}

/// Token count of the payload. Tokenizing megabytes is CPU-bound, so it runs on
/// the blocking pool; if the tokenizer is unavailable we fall back to a
/// conservative char-based estimate rather than letting the payload through.
async fn count_tokens(text: Arc<String>) -> usize {
    let fallback = text.chars().count().div_ceil(FALLBACK_CHARS_PER_TOKEN);

    // The tokenizer itself is a process-wide `OnceCell`, so the BPE
    // construction cost is paid once, on the first oversized response.
    let Ok(counter) = create_token_counter().await else {
        warn!("BR-6: tokenizer unavailable; using a char-based token estimate");
        return fallback;
    };

    match tokio::task::spawn_blocking(move || counter.count_tokens(&text)).await {
        Ok(tokens) => tokens,
        Err(e) => {
            warn!("BR-6: token counting task failed ({e}); using a char-based token estimate");
            fallback
        }
    }
}

/// The inline replacement: what happened, how big it was, where the full output
/// lives, and a bounded head/tail preview of it.
fn render_summary(
    text: &str,
    stats: Stats,
    handle: Option<&Handle>,
    limits: LargeResponseLimits,
    ctx: &LargeResponseContext,
) -> String {
    // Convert the preview *token* budget into a char budget using this
    // payload's measured chars-per-token ratio, so a token-dense payload
    // (minified JSON, base64) is cut harder than prose.
    let chars_per_token = (stats.chars as f64 / stats.tokens.max(1) as f64).max(1.0);
    let preview_chars = ((limits.preview_tokens as f64) * chars_per_token) as usize;
    let preview = truncate_middle_out(text, preview_chars.max(1));

    let mut out = format!(
        "The `{tool}` tool returned a large result: {lines} lines, {chars} characters, \
         ~{tokens} tokens, over the {limit}-token inline limit, so it is not included in full.\n",
        tool = ctx.tool_name,
        lines = stats.lines,
        chars = stats.chars,
        tokens = stats.tokens,
        limit = limits.max_tokens,
    );

    match handle {
        Some(handle) => {
            out.push_str(&format!(
                "The complete output is saved at: {}\n",
                handle.absolute.display()
            ));
            if let Some(relative) = &handle.relative {
                out.push_str(&format!(
                    "(relative to the working directory: {})\n",
                    relative.display()
                ));
            }
            out.push_str(
                "Do NOT re-run the tool just to see the rest. Read or search that file with your \
                 file/shell tools instead (grep/ripgrep for a pattern, or read a specific line \
                 range).\n",
            );
        }
        None => out.push_str(
            "The full output could NOT be saved to a file, so only the preview below survives. \
             If you need more of it, re-run the tool with a narrower query, a filter, or a limit.\n",
        ),
    }

    out.push_str(&format!(
        "\n--- preview: head and tail, ~{} of ~{} tokens ---\n{}\n--- end of preview ---",
        limits.preview_tokens, stats.tokens, preview
    ));
    out
}

/// Write the full payload to a handle the model's tools can actually reach.
///
/// Preferred location is `<working_dir>/.biorouter/tool-output/` — inside the
/// session working dir, so a shell/file tool scoped to it can read the output
/// back. If that is not writable (read-only checkout, missing dir) we fall back
/// to the legacy temp-dir location rather than lose the output entirely.
fn write_handle(content: &str, ctx: &LargeResponseContext) -> Result<Handle, std::io::Error> {
    let filename = handle_filename(ctx);

    let in_workspace = ctx.working_dir.join(HANDLE_DIR);
    if prepare_handle_dir(&in_workspace).is_ok() {
        let path = in_workspace.join(&filename);
        if write_all(&path, content).is_ok() {
            return Ok(Handle {
                absolute: path,
                relative: Some(Path::new(HANDLE_DIR).join(&filename)),
            });
        }
    }

    // Fallback: the pre-BR-6 temp location. An awkward handle beats no handle.
    let temp_dir = std::env::temp_dir().join("biorouter_mcp_responses");
    std::fs::create_dir_all(&temp_dir)?;
    let path = temp_dir.join(&filename);
    write_all(&path, content)?;
    Ok(Handle {
        absolute: path,
        relative: None,
    })
}

fn write_all(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())
}

/// Create the handle dir and make it invisible to git (a `.gitignore` of `*`
/// also ignores itself), so offloading a tool result never dirties the user's
/// working tree.
fn prepare_handle_dir(dir: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dir)?;
    if let Some(parent) = dir.parent() {
        let gitignore = parent.join(".gitignore");
        if !gitignore.exists() {
            let _ = std::fs::write(&gitignore, "*\n");
        }
    }
    Ok(())
}

/// `<tool>-<session>-<timestamp>.txt`, with anything unusual scrubbed out of
/// the tool name and session id so a handle can never escape its directory.
fn handle_filename(ctx: &LargeResponseContext) -> String {
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S%.6f");
    format!(
        "{}-{}-{}.txt",
        sanitize(&ctx.tool_name, "tool"),
        sanitize(&ctx.session_id, "session"),
        timestamp
    )
}

fn sanitize(raw: &str, fallback: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    let trimmed = cleaned.trim_matches('_').to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ErrorCode;
    use std::borrow::Cow;
    use std::fs;
    use tempfile::TempDir;

    fn ctx(dir: &TempDir) -> LargeResponseContext {
        LargeResponseContext {
            session_id: "sess-123".to_string(),
            working_dir: dir.path().to_path_buf(),
            tool_name: "developer__shell".to_string(),
        }
    }

    fn ok(content: Vec<Content>) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult {
            content,
            structured_content: None,
            is_error: Some(false),
            meta: None,
        })
    }

    /// A payload comfortably over the default 25k-token budget.
    fn huge_text() -> String {
        (0..40_000)
            .map(|i| format!("line {i} of the oversized tool output\n"))
            .collect()
    }

    #[tokio::test]
    async fn small_text_response_passes_through() {
        let dir = TempDir::new().unwrap();
        let small = "This is a small text response";
        let processed = process_tool_response(ok(vec![Content::text(small)]), &ctx(&dir))
            .await
            .unwrap();

        assert_eq!(processed.content.len(), 1);
        assert_eq!(processed.content[0].as_text().unwrap().text, small);
        // A normal-sized result writes nothing into the working dir.
        assert!(!dir.path().join(".biorouter").exists());
    }

    /// The core BR-6 behaviour: an oversized result is replaced by a summary
    /// carrying a head/tail preview, a line count, and a working-dir handle
    /// that holds the *complete* output.
    #[tokio::test]
    async fn large_text_is_offloaded_with_preview_and_handle() {
        let dir = TempDir::new().unwrap();
        let large = huge_text();
        let processed = process_tool_response(ok(vec![Content::text(large.clone())]), &ctx(&dir))
            .await
            .unwrap();

        assert_eq!(processed.content.len(), 1);
        let text = &processed.content[0].as_text().unwrap().text;

        // Line count + token accounting in the header.
        assert!(text.contains(&format!("{} lines", large.lines().count())));
        assert!(text.contains("tokens"));
        // Head *and* tail survive.
        assert!(text.contains("line 0 of the oversized tool output"));
        assert!(text.contains("line 39999 of the oversized tool output"));
        assert!(text.contains("characters elided"));
        // Bounded: the preview is a small fraction of the original.
        assert!(
            text.chars().count() < large.chars().count() / 4,
            "the preview should be far smaller than the payload"
        );

        // The handle exists inside the working dir and holds the full payload.
        let entries: Vec<_> = fs::read_dir(dir.path().join(HANDLE_DIR))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(fs::read_to_string(entries[0].path()).unwrap(), large);

        // The path we told the model about is the one we wrote.
        assert!(text.contains(&entries[0].path().display().to_string()));
        assert!(text.contains(HANDLE_DIR));

        // And the directory hides itself from git.
        let gitignore = fs::read_to_string(dir.path().join(".biorouter/.gitignore")).unwrap();
        assert_eq!(gitignore.trim(), "*");
    }

    /// The old handler thresholded each content item on its own, so N items
    /// each just under the limit sailed through and blew the context together.
    /// The budget is now aggregate.
    #[tokio::test]
    async fn aggregate_over_budget_is_offloaded_even_when_each_item_is_small() {
        let dir = TempDir::new().unwrap();
        // Four items of ~10k tokens each: individually fine, together ~40k.
        let chunk = |n: usize| -> Content {
            Content::text(
                (0..10_000)
                    .map(|i| format!("chunk {n} row {i}\n"))
                    .collect::<String>(),
            )
        };
        let processed =
            process_tool_response(ok(vec![chunk(0), chunk(1), chunk(2), chunk(3)]), &ctx(&dir))
                .await
                .unwrap();

        // All four text items collapse into one summary.
        assert_eq!(processed.content.len(), 1);
        let text = &processed.content[0].as_text().unwrap().text;
        assert!(text.contains("inline limit"));

        // The handle holds every chunk, in order.
        let entries: Vec<_> = fs::read_dir(dir.path().join(HANDLE_DIR))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        let saved = fs::read_to_string(entries[0].path()).unwrap();
        assert!(saved.contains("chunk 0 row 0"));
        assert!(saved.contains("chunk 3 row 9999"));
    }

    #[tokio::test]
    async fn non_text_content_is_preserved_around_the_summary() {
        let dir = TempDir::new().unwrap();
        let processed = process_tool_response(
            ok(vec![
                Content::text(huge_text()),
                Content::image("image_data".to_string(), "image/jpeg".to_string()),
            ]),
            &ctx(&dir),
        )
        .await
        .unwrap();

        assert_eq!(processed.content.len(), 2);
        assert!(processed.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("inline limit"));
        let img = processed.content[1].as_image().unwrap();
        assert_eq!(img.data, "image_data");
        assert_eq!(img.mime_type, "image/jpeg");
    }

    #[tokio::test]
    async fn image_only_response_passes_through() {
        let dir = TempDir::new().unwrap();
        let processed = process_tool_response(
            ok(vec![Content::image(
                "base64data".to_string(),
                "image/png".to_string(),
            )]),
            &ctx(&dir),
        )
        .await
        .unwrap();

        assert_eq!(processed.content.len(), 1);
        let img = processed.content[0].as_image().unwrap();
        assert_eq!(img.data, "base64data");
        assert_eq!(img.mime_type, "image/png");
    }

    /// An unwritable working dir must not lose the output: the handle falls
    /// back to the temp dir (the pre-BR-6 location) and the preview still
    /// stands. The old handler inlined the whole payload here instead.
    #[tokio::test]
    async fn unwritable_working_dir_falls_back_to_temp_handle() {
        let base = TempDir::new().unwrap();
        let ctx = LargeResponseContext {
            session_id: "sess".to_string(),
            working_dir: base.path().join("nope\0invalid"),
            tool_name: "shell".to_string(),
        };
        let large = huge_text();
        let processed = process_tool_response(ok(vec![Content::text(large.clone())]), &ctx)
            .await
            .unwrap();

        let text = &processed.content[0].as_text().unwrap().text;
        assert!(text.contains("biorouter_mcp_responses"));
        assert!(text.contains("characters elided"));
        assert!(text.chars().count() < large.chars().count() / 4);

        // The temp handle holds the full payload; clean it up.
        let path = text
            .lines()
            .find_map(|l| l.strip_prefix("The complete output is saved at: "))
            .expect("summary should name the handle");
        assert_eq!(fs::read_to_string(path).unwrap(), large);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn error_response_passes_through() {
        let dir = TempDir::new().unwrap();
        let error = ErrorData {
            code: ErrorCode::INTERNAL_ERROR,
            message: Cow::from("Test error"),
            data: None,
        };
        let processed = process_tool_response(Err(error), &ctx(&dir)).await;

        match processed {
            Err(err) => {
                assert_eq!(err.code, ErrorCode::INTERNAL_ERROR);
                assert_eq!(err.message, "Test error");
            }
            Ok(_) => panic!("expected the error to pass through"),
        }
    }

    #[test]
    fn handle_filename_cannot_escape_its_directory() {
        let ctx = LargeResponseContext {
            session_id: "../../etc".to_string(),
            working_dir: PathBuf::from("/tmp"),
            tool_name: "../../../bin/sh".to_string(),
        };
        let name = handle_filename(&ctx);
        assert!(!name.contains('/'));
        assert!(!name.contains(".."));
    }

    #[test]
    fn preview_budget_stays_below_the_inline_limit() {
        let limits = LargeResponseLimits::from_config();
        assert!(limits.preview_tokens < limits.max_tokens);
    }
}
