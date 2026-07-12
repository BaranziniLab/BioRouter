//! Bounded assembly of injected context (BR-2).
//!
//! Nothing that assembles injected context — hint files (`.biorouterhints` /
//! `AGENTS.md` + their `@import`s), MCP-server instructions, and the MOIM
//! `<info-msg>` block — had any *aggregate* size accounting. The only guard was
//! a per-file 128 KB *parse* cap in `import_files.rs`, so a large `AGENTS.md`, a
//! chatty MCP server, or several `@import`s could silently blow the model's
//! context window (see `docs/agent-loop-review/internal/context-injection.md`
//! gap #1). This module is the shared, cheap, synchronous budgeter those call
//! sites use to rank, truncate, and drop injected blocks so worst-case token
//! spend is bounded, logging whatever it trims.
//!
//! Token counts are *estimated* at ~4 chars/token rather than run through the
//! real tiktoken tokenizer: the system-prompt assembler (`prompt_manager::build`)
//! is synchronous and on the hot path, while loading tiktoken is async and
//! CPU-heavy. The estimate only needs to be good enough to *bound* pathological
//! inputs; exact accounting still happens later in `context_mgmt`.
//!
//! The defaults are deliberately generous — they leave ordinary sessions
//! byte-identical and only clamp runaway inputs. Every threshold is configurable
//! (env var or `config.yaml`), and a value of `0` disables that particular cap.

use crate::config::Config;

/// Average characters per token for English/code. Used only for the coarse
/// budget estimate, never for exact accounting.
pub const CHARS_PER_TOKEN: usize = 4;

/// Total estimated-token budget for the injected system-prompt blocks — the
/// MCP extension instructions plus the hint files. Generous on purpose: it only
/// clamps pathological inputs (a runaway `AGENTS.md`, a multi-hundred-KB MCP
/// instructions string), leaving ordinary sessions untouched.
pub const DEFAULT_INJECTION_BUDGET_TOKENS: usize = 48_000;

/// Per-hint-file estimated-token cap (Codex's `project_doc_max_bytes` analog),
/// applied to each expanded hint file before the combined string is built.
pub const DEFAULT_MAX_HINTS_FILE_TOKENS: usize = 24_000;

/// Estimated-token cap for a single MOIM `<info-msg>` block injected into the
/// message array each action.
pub const DEFAULT_MAX_MOIM_TOKENS: usize = 8_000;

/// Estimated-token cap for a single explicitly-selected skill body inlined into
/// a turn's `<explicit-resource-context>` (BR-8). Skill bodies are usually a few
/// hundred to a couple thousand tokens, so this is generous; it only clamps a
/// pathological `SKILL.md` (the body is inlined at most once per session).
pub const DEFAULT_MAX_SKILL_BODY_TOKENS: usize = 16_000;

/// A block truncated to fewer than this many tokens carries almost no signal, so
/// the fitter drops it entirely rather than leaving a stub.
const MIN_USEFUL_TRUNCATION_TOKENS: usize = 128;

/// Tokens reserved for the elision marker appended by [`truncate_to_tokens`], so
/// a truncated block still lands at or under its requested budget.
const MARKER_RESERVE_TOKENS: usize = 24;

/// Rough, synchronous token estimate (~4 chars/token). See module docs.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARS_PER_TOKEN)
}

fn config_usize(key: &str, default: usize) -> usize {
    Config::global().get_param::<usize>(key).unwrap_or(default)
}

/// Total injection budget (estimated tokens), from `CONTEXT_INJECTION_BUDGET_TOKENS`.
pub fn injection_budget_tokens() -> usize {
    config_usize(
        "CONTEXT_INJECTION_BUDGET_TOKENS",
        DEFAULT_INJECTION_BUDGET_TOKENS,
    )
}

/// Per-hint-file cap (estimated tokens), from `MAX_HINTS_FILE_TOKENS`.
pub fn max_hints_file_tokens() -> usize {
    config_usize("MAX_HINTS_FILE_TOKENS", DEFAULT_MAX_HINTS_FILE_TOKENS)
}

/// MOIM block cap (estimated tokens), from `MAX_MOIM_TOKENS`.
pub fn max_moim_tokens() -> usize {
    config_usize("MAX_MOIM_TOKENS", DEFAULT_MAX_MOIM_TOKENS)
}

/// Per-skill-body cap (estimated tokens), from `MAX_SKILL_BODY_TOKENS`.
pub fn max_skill_body_tokens() -> usize {
    config_usize("MAX_SKILL_BODY_TOKENS", DEFAULT_MAX_SKILL_BODY_TOKENS)
}

/// Head-truncate `text` to at most `max_tokens` (estimated), appending a
/// one-line elision marker that names `label` and how many tokens were dropped.
/// Truncates on a char boundary. Returns the text unchanged when it already fits
/// or when `max_tokens` is `0` (cap disabled).
pub fn truncate_to_tokens(text: &str, max_tokens: usize, label: &str) -> String {
    let total = estimate_tokens(text);
    if max_tokens == 0 || total <= max_tokens {
        return text.to_string();
    }
    // Reserve room for the marker so the result stays within budget.
    let head_tokens = max_tokens.saturating_sub(MARKER_RESERVE_TOKENS).max(1);
    let char_budget = head_tokens.saturating_mul(CHARS_PER_TOKEN);
    let mut head: String = text.chars().take(char_budget).collect();
    let dropped = total.saturating_sub(estimate_tokens(&head));
    head.push_str(&format!(
        "\n\n… [{label}: {dropped} tokens elided to fit the context budget]"
    ));
    head
}

/// One injectable block competing for a shared budget.
#[derive(Clone, Debug)]
pub struct ContextBlock {
    /// Human-readable label, used only for logging what was dropped/truncated.
    pub label: String,
    pub content: String,
    /// Higher priority blocks are admitted first; ties break on input order.
    pub priority: i32,
}

/// What the fitter had to trim to stay within budget.
#[derive(Default, Debug, Clone)]
pub struct BudgetReport {
    /// Labels of blocks emptied entirely.
    pub dropped: Vec<String>,
    /// Labels of blocks head-truncated.
    pub truncated: Vec<String>,
}

impl BudgetReport {
    pub fn is_empty(&self) -> bool {
        self.dropped.is_empty() && self.truncated.is_empty()
    }
}

/// Fit `blocks` into `max_tokens` (estimated). Blocks are admitted in descending
/// priority (stable within a priority). A block that doesn't fit whole is
/// head-truncated to the remaining budget if that leaves at least
/// [`MIN_USEFUL_TRUNCATION_TOKENS`], otherwise dropped; every block reached after
/// the budget is exhausted is dropped. `max_tokens == 0` disables budgeting
/// (returns the blocks unchanged). The returned `Vec` preserves the *input*
/// order (with dropped blocks' `content` set to an empty string) so the caller
/// can splice each block back into its original slot.
pub fn fit_context_blocks(
    mut blocks: Vec<ContextBlock>,
    max_tokens: usize,
) -> (Vec<ContextBlock>, BudgetReport) {
    let mut report = BudgetReport::default();
    if max_tokens == 0 {
        return (blocks, report);
    }

    // Admission order: highest priority first, ties broken by input order so the
    // pass is deterministic (important for prompt caching).
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.sort_by(|&a, &b| blocks[b].priority.cmp(&blocks[a].priority).then(a.cmp(&b)));

    let mut remaining = max_tokens;
    for i in order {
        let cost = estimate_tokens(&blocks[i].content);
        if cost == 0 {
            continue;
        }
        if cost <= remaining {
            remaining -= cost;
        } else if remaining >= MIN_USEFUL_TRUNCATION_TOKENS {
            let label = blocks[i].label.clone();
            blocks[i].content = truncate_to_tokens(&blocks[i].content, remaining, &label);
            report.truncated.push(label);
            remaining = 0;
        } else {
            report.dropped.push(blocks[i].label.clone());
            blocks[i].content = String::new();
        }
    }

    (blocks, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(label: &str, content: &str, priority: i32) -> ContextBlock {
        ContextBlock {
            label: label.to_string(),
            content: content.to_string(),
            priority,
        }
    }

    #[test]
    fn test_estimate_tokens_is_chars_over_four() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2); // ceil(5/4)
    }

    #[test]
    fn test_truncate_noop_when_within_budget() {
        let text = "short text";
        assert_eq!(truncate_to_tokens(text, 100, "x"), text);
    }

    #[test]
    fn test_truncate_disabled_with_zero() {
        let text = "a".repeat(10_000);
        assert_eq!(truncate_to_tokens(&text, 0, "x"), text);
    }

    #[test]
    fn test_truncate_shrinks_and_marks() {
        let text = "a".repeat(10_000); // ~2500 tokens
        let out = truncate_to_tokens(&text, 100, "big");
        assert!(out.len() < text.len());
        assert!(out.contains("big"));
        assert!(out.contains("elided to fit the context budget"));
        // Result stays at/under the requested budget.
        assert!(estimate_tokens(&out) <= 100);
    }

    #[test]
    fn test_fit_all_fits_unchanged() {
        let blocks = vec![block("a", "hello", 100), block("b", "world", 50)];
        let (out, report) = fit_context_blocks(blocks, 10_000);
        assert!(report.is_empty());
        assert_eq!(out[0].content, "hello");
        assert_eq!(out[1].content, "world");
    }

    #[test]
    fn test_fit_disabled_with_zero_budget() {
        let big = "a".repeat(10_000);
        let blocks = vec![block("a", &big, 100)];
        let (out, report) = fit_context_blocks(blocks, 0);
        assert!(report.is_empty());
        assert_eq!(out[0].content, big);
    }

    #[test]
    fn test_fit_drops_lowest_priority_first() {
        // High-priority block eats the whole budget; low-priority is dropped.
        let hi = "a".repeat(4_000); // ~1000 tokens
        let lo = "b".repeat(4_000);
        let blocks = vec![block("lo", &lo, 50), block("hi", &hi, 100)];
        let (out, report) = fit_context_blocks(blocks, 1_000);
        // Input order preserved: index 0 is "lo", index 1 is "hi".
        assert_eq!(out[1].content, hi, "highest priority kept in full");
        assert_eq!(out[0].content, "", "lowest priority dropped");
        assert_eq!(report.dropped, vec!["lo".to_string()]);
        assert!(report.truncated.is_empty());
    }

    #[test]
    fn test_fit_truncates_straddling_block() {
        let hi = "a".repeat(2_000); // ~500 tokens
        let lo = "b".repeat(8_000); // ~2000 tokens, straddles the remaining budget
        let blocks = vec![block("hi", &hi, 100), block("lo", &lo, 50)];
        let (out, report) = fit_context_blocks(blocks, 1_000);
        assert_eq!(out[0].content, hi);
        assert!(out[1].content.len() < lo.len());
        assert!(out[1].content.contains("elided to fit the context budget"));
        assert_eq!(report.truncated, vec!["lo".to_string()]);
        // Whole assembly stays within budget.
        let total: usize = out.iter().map(|b| estimate_tokens(&b.content)).sum();
        assert!(total <= 1_000);
    }

    #[test]
    fn test_max_skill_body_tokens_default() {
        // With no override configured, the getter returns the generous default.
        assert_eq!(max_skill_body_tokens(), DEFAULT_MAX_SKILL_BODY_TOKENS);
    }

    #[test]
    fn test_skill_body_cap_truncates_runaway_body() {
        // A pathological SKILL.md is clamped to the per-skill cap, with a marker.
        let body = "x".repeat(DEFAULT_MAX_SKILL_BODY_TOKENS * CHARS_PER_TOKEN * 4);
        let out = truncate_to_tokens(
            &body,
            DEFAULT_MAX_SKILL_BODY_TOKENS,
            "selected skill `demo`",
        );
        assert!(out.len() < body.len());
        assert!(out.contains("selected skill `demo`"));
        assert!(estimate_tokens(&out) <= DEFAULT_MAX_SKILL_BODY_TOKENS);
    }

    #[test]
    fn test_fit_preserves_input_order() {
        let blocks = vec![
            block("first", "x", 10),
            block("second", "y", 100),
            block("third", "z", 50),
        ];
        let (out, _) = fit_context_blocks(blocks, 10_000);
        assert_eq!(out[0].label, "first");
        assert_eq!(out[1].label, "second");
        assert_eq!(out[2].label, "third");
    }
}
