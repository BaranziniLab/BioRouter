use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::mcp_utils::ToolResult;
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, CallToolResult, Role};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Inspector name used by the repetition guard; the agent keys the honest
/// "repetition, not user decline" deny message off this.
pub const REPETITION_INSPECTOR_NAME: &str = "repetition";

/// Finding id for the hard stop (the call is denied).
pub const REPETITION_HARD_FINDING_ID: &str = "REP-001";

/// Finding id for the non-blocking soft warning (the call still runs, but the
/// model is told it is repeating itself) — BR-29.
pub const REPETITION_SOFT_FINDING_ID: &str = "REP-002";

/// BR-30 near-duplicate ("arg-tweak") repetition, soft stage.
pub const REPETITION_NEAR_DUP_SOFT_FINDING_ID: &str = "REP-003";
/// BR-30 near-duplicate ("arg-tweak") repetition, hard stage (opt-in).
pub const REPETITION_NEAR_DUP_HARD_FINDING_ID: &str = "REP-004";
/// BR-30 A/B/A/B oscillation, soft stage.
pub const REPETITION_OSCILLATION_SOFT_FINDING_ID: &str = "REP-005";
/// BR-30 A/B/A/B oscillation, hard stage (opt-in).
pub const REPETITION_OSCILLATION_HARD_FINDING_ID: &str = "REP-006";
/// BR-31 repeated-failing-result guard, hard stage: the next call to a tool that
/// has already failed the same way `hard_stop_at` times in a row is denied.
pub const FAILURE_LOOP_HARD_FINDING_ID: &str = "REP-007";

/// Nth consecutive *near*-identical call to the same tool that earns a
/// non-blocking warning.
pub const DEFAULT_NEAR_DUP_SOFT_WARN: u32 = 4;
/// Nth call of an `A/B/A/B` alternation that earns a non-blocking warning
/// (4 = two full cycles, the same trigger OpenHands' `StuckDetector` uses).
pub const DEFAULT_OSCILLATION_SOFT_WARN: u32 = 4;
/// How similar two normalized argument sets must be to count as "the same call
/// with a tweak". Deliberately high: a different filename or a different numeric
/// range scores far below this, so ordinary iteration over distinct inputs does
/// not trip the detector.
pub const DEFAULT_ARG_SIMILARITY_THRESHOLD: f32 = 0.9;

/// BR-31: Nth consecutive failure of the same tool with the same error that earns
/// a first, hedged nudge.
pub const DEFAULT_FAILURE_SOFT_WARN: u32 = 3;
/// BR-31: Nth consecutive identical failure that escalates the nudge to
/// "you are not making progress; change approach or ask the user".
pub const DEFAULT_FAILURE_ESCALATE_WARN: u32 = 5;
/// BR-31: Nth consecutive identical failure after which the *next* call to that
/// tool is denied outright.
pub const DEFAULT_FAILURE_HARD_STOP: u32 = 6;
/// BR-31: how similar two normalized error signatures must be to count as "the
/// same failure". Lower than the argument threshold because two runs of one
/// failing command produce near-identical prose, and because the signature is
/// already normalized (case, whitespace, digit runs).
pub const DEFAULT_ERROR_SIMILARITY_THRESHOLD: f32 = 0.85;

/// How many recent tool calls the semantic heuristics look back over.
const LOOP_WINDOW: usize = 20;

/// Longest error prefix kept in a signature. Tool errors can carry a whole
/// stack trace or a megabyte of stderr; the headline is what identifies the
/// failure, and comparing the tail is both slow and noisy.
const MAX_SIGNATURE_LEN: usize = 400;

/// Argument keys whose values are volatile plumbing (request ids, timestamps,
/// nonces). They differ on every call even when the call is *semantically* the
/// same, so byte-exact comparison misses the repeat. They are dropped before
/// comparison. Note `id` itself is **not** on this list: for many tools `id` is
/// the thing being fetched, and ignoring it would make legitimate iteration over
/// distinct ids look like a loop.
const VOLATILE_ARG_KEYS: &[&str] = &[
    "request_id",
    "requestid",
    "trace_id",
    "traceid",
    "correlation_id",
    "idempotency_key",
    "session_id",
    "sessionid",
    "nonce",
    "timestamp",
    "created_at",
    "updated_at",
    "_meta",
];

/// BR-30: configuration for the semantic (beyond byte-exact) loop heuristics.
///
/// Both heuristics ship **warn-only** by default: they inject an advisory nudge
/// into the model's context (via the BR-29 soft-warning plumbing) and never deny
/// a call. The hard stages exist and are wired, but are opt-in — heuristics can
/// false-positive, and the byte-exact guard (BR-29) plus the absolute tool-call
/// ceiling remain the enforcing backstops.
#[derive(Debug, Clone)]
pub struct SemanticLoopConfig {
    /// Master switch for both heuristics.
    pub enabled: bool,
    /// Normalized-argument similarity at or above which two calls to the same
    /// tool count as near-duplicates. `0.0..=1.0`.
    pub similarity_threshold: f32,
    /// Nth consecutive near-duplicate call that warns. `None` disables.
    pub near_dup_soft_warn: Option<u32>,
    /// Nth consecutive near-duplicate call that is denied. `None` disables.
    pub near_dup_hard_stop: Option<u32>,
    /// Nth call of an alternating `A/B/A/B` run that warns. `None` disables.
    pub oscillation_soft_warn: Option<u32>,
    /// Nth call of an alternating `A/B/A/B` run that is denied. `None` disables.
    pub oscillation_hard_stop: Option<u32>,
}

impl Default for SemanticLoopConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            similarity_threshold: DEFAULT_ARG_SIMILARITY_THRESHOLD,
            near_dup_soft_warn: Some(DEFAULT_NEAR_DUP_SOFT_WARN),
            near_dup_hard_stop: None,
            oscillation_soft_warn: Some(DEFAULT_OSCILLATION_SOFT_WARN),
            oscillation_hard_stop: None,
        }
    }
}

impl SemanticLoopConfig {
    /// Byte-exact detection only (the pre-BR-30 behavior).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

/// BR-31: configuration for the repeated-failing-result ("no progress") detector.
///
/// The three stages are expressed as "the Nth consecutive failure of this tool
/// with the same error":
///
/// * `soft_warn_at` — a hedged nudge is injected right after the failing result.
/// * `escalate_at` — the nudge becomes an explicit "you are not making progress;
///   change approach or ask the user".
/// * `hard_stop_at` — the *next* call to that tool is denied (the failure already
///   happened; only a future call can be blocked).
///
/// Unlike BR-30's heuristics, the hard stage is **on by default**: a run of
/// identical failures is evidence, not a guess — the tool ran, and it produced
/// the same error every time. The stop is also self-clearing: the denial is
/// itself an error with a different signature, so it breaks the streak and the
/// model gets to try again (and will be stopped again if it resumes the loop).
#[derive(Debug, Clone)]
pub struct FailureLoopConfig {
    /// Master switch.
    pub enabled: bool,
    /// Normalized-error-signature similarity at or above which two failures of
    /// the same tool count as "the same failure". `0.0..=1.0`.
    pub similarity_threshold: f32,
    /// Nth consecutive identical failure that earns a nudge. `None` disables.
    pub soft_warn_at: Option<u32>,
    /// Nth consecutive identical failure that escalates the nudge. `None` disables.
    pub escalate_at: Option<u32>,
    /// Nth consecutive identical failure after which the next call to the tool is
    /// denied. `None` disables the hard stage (nudge-only).
    pub hard_stop_at: Option<u32>,
}

impl Default for FailureLoopConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            similarity_threshold: DEFAULT_ERROR_SIMILARITY_THRESHOLD,
            soft_warn_at: Some(DEFAULT_FAILURE_SOFT_WARN),
            escalate_at: Some(DEFAULT_FAILURE_ESCALATE_WARN),
            hard_stop_at: Some(DEFAULT_FAILURE_HARD_STOP),
        }
    }
}

impl FailureLoopConfig {
    /// No result-aware detection at all (the pre-BR-31 behavior).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

/// BR-31: one completed tool call, reduced to what the no-progress detector
/// needs — which tool ran, and, if it failed, a normalized signature of its error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutcome {
    pub tool_name: String,
    /// `None` when the call succeeded; `Some(signature)` when it failed.
    pub failure: Option<String>,
}

/// Normalize an error message into a comparison signature: lowercased,
/// whitespace-collapsed, digit runs folded to `#`, truncated to the headline.
///
/// Folding digits is what makes two runs of the *same* failure compare equal
/// when the error embeds a pid, a timestamp, a byte offset, or a line number.
fn error_signature(text: &str) -> String {
    let collapsed = collapse_whitespace(text).to_lowercase();
    let mut signature = String::with_capacity(collapsed.len().min(MAX_SIGNATURE_LEN));
    let mut in_digits = false;
    for ch in collapsed.chars() {
        if signature.chars().count() >= MAX_SIGNATURE_LEN {
            break;
        }
        if ch.is_ascii_digit() {
            if !in_digits {
                signature.push('#');
                in_digits = true;
            }
        } else {
            in_digits = false;
            signature.push(ch);
        }
    }
    signature
}

/// The error text of a failed tool result, or `None` when it succeeded. A tool
/// can fail two ways: a transport/protocol `Err`, or an `Ok` result flagged
/// `is_error` (the MCP "the tool ran and reported a failure" path) — both count.
fn failure_text(result: &ToolResult<CallToolResult>) -> Option<String> {
    match result {
        Err(error) => Some(error.message.to_string()),
        Ok(call) if call.is_error == Some(true) => {
            let text = call
                .content
                .iter()
                .filter_map(|content| content.as_text().map(|text| text.text.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            Some(if text.trim().is_empty() {
                "tool reported an error with no message".to_string()
            } else {
                text
            })
        }
        Ok(_) => None,
    }
}

/// Reduce a completed tool result to a [`ToolOutcome`].
pub fn tool_outcome(tool_name: &str, result: &ToolResult<CallToolResult>) -> ToolOutcome {
    ToolOutcome {
        tool_name: tool_name.to_string(),
        failure: failure_text(result)
            .as_deref()
            .map(error_signature)
            .filter(|signature| !signature.is_empty()),
    }
}

/// The outcome of `request_id` as recorded in a tool-response message the agent
/// has just written (the BR-31 result-collection seam), or `None` when that
/// message carries no response for it.
pub fn outcome_from_response_message(
    tool_name: &str,
    request_id: &str,
    message: &Message,
) -> Option<ToolOutcome> {
    message
        .content
        .iter()
        .filter_map(|content| content.as_tool_response())
        .find(|response| response.id == request_id)
        .map(|response| tool_outcome(tool_name, &response.tool_result))
}

/// Every completed tool call since the last genuine user turn, in order, paired
/// with whether it failed. Requests and responses are matched by id, so a batch
/// of parallel calls is attributed correctly.
pub fn tool_outcomes_since_last_user_turn(messages: &[Message]) -> Vec<ToolOutcome> {
    let start = last_user_turn_index(messages);
    let mut names: HashMap<&str, String> = HashMap::new();
    let mut outcomes = Vec::new();

    for message in &messages[start..] {
        for content in &message.content {
            if let Some(request) = content.as_tool_request() {
                if let Ok(tool_call) = &request.tool_call {
                    names.insert(request.id.as_str(), tool_call.name.to_string());
                }
            } else if let Some(response) = content.as_tool_response() {
                let Some(tool_name) = names.get(response.id.as_str()) else {
                    continue;
                };
                outcomes.push(tool_outcome(tool_name, &response.tool_result));
            }
        }
    }

    if outcomes.len() > LOOP_WINDOW {
        outcomes.drain(..outcomes.len() - LOOP_WINDOW);
    }
    outcomes
}

/// Length of the trailing run of failures of `tool_name` whose error signatures
/// all match the most recent one. `0` when that tool's last call succeeded, or
/// when it has not run since the last user turn.
///
/// Only that tool's own outcomes are considered, so interleaved work with other
/// tools neither resets nor hides the streak. A *success* of the tool ends the
/// run (progress), and so does a failure with a materially different error — a
/// new error is new information, which is the opposite of being stuck.
pub fn failing_streak(outcomes: &[ToolOutcome], tool_name: &str, threshold: f32) -> u32 {
    let mut own = outcomes
        .iter()
        .rev()
        .filter(|outcome| outcome.tool_name == tool_name);

    let Some(signature) = own.next().and_then(|outcome| outcome.failure.as_deref()) else {
        return 0;
    };

    let mut streak = 1u32;
    for outcome in own.take(LOOP_WINDOW) {
        match outcome.failure.as_deref() {
            Some(previous) if signatures_match(previous, signature, threshold) => streak += 1,
            _ => break,
        }
    }
    streak
}

fn signatures_match(a: &str, b: &str, threshold: f32) -> bool {
    a == b || string_similarity(a, b) >= threshold
}

/// BR-31: the nudge (if any) owed to `tool_name` after its latest result, given
/// every outcome since the last user turn (including the batch that just ran).
///
/// Returns `None` below the soft threshold, the hedged nudge between soft and
/// escalate, and the escalated nudge at or above it. The hard stop is *not*
/// expressed here — a failure that already happened cannot be un-run; it is
/// enforced on the next call by [`RepetitionInspector`].
pub fn failure_loop_nudge(
    config: &FailureLoopConfig,
    outcomes: &[ToolOutcome],
    tool_name: &str,
) -> Option<String> {
    if !config.enabled {
        return None;
    }
    let streak = failing_streak(outcomes, tool_name, config.similarity_threshold);
    let fired = |threshold: Option<u32>| threshold.is_some_and(|at| at > 0 && streak >= at);

    if fired(config.escalate_at) {
        Some(escalated_failure_nudge(
            tool_name,
            streak,
            config.hard_stop_at,
        ))
    } else if fired(config.soft_warn_at) {
        Some(soft_failure_nudge(tool_name, streak))
    } else {
        None
    }
}

fn soft_failure_nudge(tool_name: &str, streak: u32) -> String {
    format!(
        "No progress: '{tool_name}' has now failed {streak} times in a row with the \
         same error. Running it again will produce the same error again. Read what the \
         error actually says and fix its cause, or take a different route — a different \
         tool, a different input, or stopping to tell the user what is blocking you."
    )
}

fn escalated_failure_nudge(tool_name: &str, streak: u32, hard_stop_at: Option<u32>) -> String {
    let stop_clause = match hard_stop_at {
        Some(at) if at > streak => format!(
            " Further calls to '{tool_name}' will be blocked automatically once it has \
             failed the same way {at} times."
        ),
        Some(_) => format!(
            " The next call to '{tool_name}' will be blocked automatically unless \
             something changes."
        ),
        None => String::new(),
    };
    format!(
        "You are not making progress: '{tool_name}' has failed {streak} times in a row \
         with the same error, and the earlier warning changed nothing. Stop retrying it. \
         Do one of these instead: fix the underlying cause the error names, use a \
         different tool or approach, or stop and ask the user for what you are \
         missing.{stop_clause}"
    )
}

/// The message the model sees in place of the tool result when the no-progress
/// guard denies a call. Like BR-29's, it states the real reason rather than
/// claiming the user declined.
fn failure_stop_reason(tool_name: &str, streak: u32) -> String {
    format!(
        "BioRouter did not run this tool call: '{tool_name}' has already failed \
         {streak} times in a row with the same error, and the warnings changed nothing. \
         The user did NOT decline it — this is an automatic no-progress guard. Repeating \
         a call that keeps failing the same way cannot make progress. Fix the cause the \
         error names, use a different tool or approach, or stop and tell the user exactly \
         what is blocking you. (If you call '{tool_name}' again and it keeps failing the \
         same way, it will be stopped again.)"
    )
}

// Helper struct for internal tracking
#[derive(Debug, Clone)]
struct InternalToolCall {
    name: String,
    parameters: Value,
    /// `parameters` with volatile keys dropped and strings whitespace-collapsed.
    /// Two calls that differ only in plumbing compare equal here.
    normalized: Value,
}

impl InternalToolCall {
    /// Byte-exact match: same tool, byte-identical arguments (BR-29).
    fn matches(&self, other: &InternalToolCall) -> bool {
        self.name == other.name && self.parameters == other.parameters
    }

    /// Same tool and same *normalized* arguments — identical modulo volatile
    /// plumbing and whitespace. Used as the oscillation signature.
    fn same_shape(&self, other: &InternalToolCall) -> bool {
        self.name == other.name && self.normalized == other.normalized
    }

    /// Same tool, and normalized arguments at least `threshold` similar: the
    /// "same call with an argument tweak" relation.
    fn near_matches(&self, other: &InternalToolCall, threshold: f32) -> bool {
        self.name == other.name && arg_similarity(&self.normalized, &other.normalized) >= threshold
    }

    fn from_tool_call(tool_call: &CallToolRequestParams) -> Self {
        let name = tool_call.name.to_string();
        let parameters = tool_call
            .arguments
            .as_ref()
            .map(|obj| Value::Object(obj.clone()))
            .unwrap_or(Value::Null);
        let normalized = normalize_args(&parameters);
        Self {
            name,
            parameters,
            normalized,
        }
    }

    fn from_request(tool_request: &ToolRequest) -> Option<Self> {
        tool_request
            .tool_call
            .as_ref()
            .ok()
            .map(Self::from_tool_call)
    }
}

/// Drop volatile keys and collapse whitespace inside strings, recursively.
fn normalize_args(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, val) in map {
                if is_volatile_key(key) {
                    continue;
                }
                out.insert(key.clone(), normalize_args(val));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize_args).collect()),
        Value::String(text) => Value::String(collapse_whitespace(text)),
        other => other.clone(),
    }
}

fn is_volatile_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    VOLATILE_ARG_KEYS.contains(&lowered.as_str())
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Flatten a normalized argument value into `path -> leaf` pairs.
fn flatten_leaves(value: &Value, path: &str, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                out.insert(path.to_string(), Value::Object(Map::new()));
                return;
            }
            for (key, val) in map {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                flatten_leaves(val, &child, out);
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                out.insert(path.to_string(), Value::Array(Vec::new()));
                return;
            }
            for (index, item) in items.iter().enumerate() {
                flatten_leaves(item, &format!("{path}[{index}]"), out);
            }
        }
        leaf => {
            out.insert(path.to_string(), leaf.clone());
        }
    }
}

/// Similarity of two *normalized* argument sets, in `0.0..=1.0`.
///
/// Leaves are compared per path: equal leaves score 1, two strings score their
/// character-trigram Jaccard scaled by their length ratio, anything else scores
/// 0. A path present on only one side scores 0. The result is the mean over the
/// union of paths, so a single changed filename in a two-argument call drops the
/// score to ~0.5 — well below any usable threshold — while a one-token tweak
/// inside a long command stays high. That asymmetry is the point: it flags
/// "same call, nudged" and ignores "different input, same shape".
fn arg_similarity(a: &Value, b: &Value) -> f32 {
    if a == b {
        return 1.0;
    }
    let mut left = BTreeMap::new();
    let mut right = BTreeMap::new();
    flatten_leaves(a, "", &mut left);
    flatten_leaves(b, "", &mut right);

    let paths: HashSet<&String> = left.keys().chain(right.keys()).collect();
    if paths.is_empty() {
        return 1.0;
    }

    let total: f32 = paths
        .iter()
        .map(|path| match (left.get(*path), right.get(*path)) {
            (Some(lhs), Some(rhs)) => leaf_similarity(lhs, rhs),
            _ => 0.0,
        })
        .sum();

    total / paths.len() as f32
}

fn leaf_similarity(a: &Value, b: &Value) -> f32 {
    if a == b {
        return 1.0;
    }
    match (a, b) {
        (Value::String(lhs), Value::String(rhs)) => string_similarity(lhs, rhs),
        _ => 0.0,
    }
}

/// Character-trigram Jaccard similarity, scaled by the length ratio so that a
/// much longer string never scores high against a short prefix of itself.
fn string_similarity(a: &str, b: &str) -> f32 {
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    if left.len() < 3 || right.len() < 3 {
        // Too short for trigrams; only exact equality (handled above) counts.
        return 0.0;
    }

    let left_grams = trigrams(&left);
    let right_grams = trigrams(&right);
    let intersection = left_grams.intersection(&right_grams).count() as f32;
    let union = left_grams.union(&right_grams).count() as f32;
    if union == 0.0 {
        return 0.0;
    }

    let length_ratio = left.len().min(right.len()) as f32 / left.len().max(right.len()) as f32;
    (intersection / union) * length_ratio
}

fn trigrams(chars: &[char]) -> HashSet<String> {
    chars
        .windows(3)
        .map(|window| window.iter().collect::<String>())
        .collect()
}

/// Length of the trailing run of calls that are all near-duplicates of the most
/// recent call, plus whether that run is byte-exact throughout.
///
/// Every member is compared against the *last* call rather than against its
/// predecessor: chained pairwise comparison would let a slowly drifting sequence
/// (each call 0.9 similar to the one before, but nothing like the first)
/// accumulate an arbitrarily long "streak".
fn trailing_near_dup_run(seq: &[InternalToolCall], threshold: f32) -> (u32, bool) {
    let Some(current) = seq.last() else {
        return (0, true);
    };
    let mut run = 1u32;
    let mut all_exact = true;
    for previous in seq.iter().rev().skip(1).take(LOOP_WINDOW) {
        if !previous.near_matches(current, threshold) {
            break;
        }
        if !previous.matches(current) {
            all_exact = false;
        }
        run += 1;
    }
    (run, all_exact)
}

/// Length of the trailing `A/B/A/B` alternation ending at the last call, where
/// `A != B` and each of `A`, `B` recurs with the same normalized shape. Returns
/// 0 when the tail is not an alternation.
fn trailing_oscillation_run(seq: &[InternalToolCall]) -> u32 {
    let len = seq.len();
    if len < 4 {
        return 0;
    }
    let (current, previous) = (&seq[len - 1], &seq[len - 2]);
    if current.same_shape(previous) {
        // A/A is plain repetition, not oscillation — BR-29 owns that.
        return 0;
    }

    let mut run = 2u32;
    let mut index = len - 3;
    loop {
        let expected = if (len - 1 - index).is_multiple_of(2) {
            current
        } else {
            previous
        };
        if !seq[index].same_shape(expected) || run as usize >= LOOP_WINDOW {
            break;
        }
        run += 1;
        if index == 0 {
            break;
        }
        index -= 1;
    }

    if run >= 4 {
        run
    } else {
        0
    }
}

/// Index of the first message after the last genuine user turn. Loop-guard
/// nudges and other agent-authored injections are `Message::user()` with
/// `user_visible == false`, and tool *responses* are also user-role messages, so
/// neither resets the window.
fn last_user_turn_index(messages: &[Message]) -> usize {
    messages
        .iter()
        .rposition(|message| {
            message.role == Role::User
                && message.metadata.user_visible
                && !message
                    .content
                    .iter()
                    .any(|content| content.as_tool_response().is_some())
        })
        .map(|index| index + 1)
        .unwrap_or(0)
}

/// The tool calls made since the last genuine user turn, capped to the recent
/// window.
fn calls_since_last_user_turn(messages: &[Message]) -> Vec<InternalToolCall> {
    let start = last_user_turn_index(messages);

    let mut calls: Vec<InternalToolCall> = messages[start..]
        .iter()
        .flat_map(|message| message.content.iter())
        .filter_map(|content| content.as_tool_request())
        .filter_map(InternalToolCall::from_request)
        .collect();

    if calls.len() > LOOP_WINDOW {
        calls.drain(..calls.len() - LOOP_WINDOW);
    }
    calls
}

/// Staged repetition guard (BR-29).
///
/// Consecutive identical tool calls (same name, byte-identical arguments) are
/// counted across history + the current batch. Two thresholds, both expressed as
/// "the Nth identical call in a row":
///
/// * `soft_warn_at` — the call still runs, but a non-blocking warning is emitted
///   (`InspectionAction::Warn`) and injected into the model's context so it can
///   change approach before it is stopped.
/// * `hard_stop_at` — the call is denied (`InspectionAction::Deny`).
///
/// A single hard deny with no prior nudge was the old behavior; the soft stage
/// gives the model one chance to break the loop itself.
/// BR-30 adds two heuristics on top, over the calls made since the last user
/// turn: *near-duplicate* repeats (same tool, arguments only tweaked) and
/// *`A/B/A/B` oscillation*. Both are warn-only by default.
///
/// BR-31 adds the result-aware stage: a tool that keeps *failing the same way*
/// is denied here, on its next call. The nudges that precede that stop are
/// emitted at the result-collection seam in the agent loop (a failure cannot be
/// pre-empted — only the call after it can).
#[derive(Debug)]
pub struct RepetitionInspector {
    /// Nth identical call in a row that earns a non-blocking warning.
    /// `None` (or `>= hard_stop_at`) disables the soft stage.
    soft_warn_at: Option<u32>,
    /// Nth identical call in a row that is denied. `None` disables the guard.
    hard_stop_at: Option<u32>,
    /// BR-30 near-duplicate + oscillation heuristics.
    semantic: SemanticLoopConfig,
    /// BR-31 repeated-failing-result guard (hard stage only; see above).
    failure: FailureLoopConfig,
}

impl RepetitionInspector {
    /// Hard-stop-only guard: deny once a call has repeated *more than*
    /// `max_repetitions` times in a row. No soft stage, no semantic heuristics.
    pub fn new(max_repetitions: Option<u32>) -> Self {
        Self {
            soft_warn_at: None,
            hard_stop_at: max_repetitions.map(|max| max.saturating_add(1)),
            semantic: SemanticLoopConfig::disabled(),
            failure: FailureLoopConfig::disabled(),
        }
    }

    /// Staged guard: warn on the `soft_warn_at`-th identical call, deny on the
    /// `hard_stop_at`-th. If `soft_warn_at >= hard_stop_at` the soft stage never
    /// fires and this degrades to a hard stop. The BR-30 semantic heuristics run
    /// with their default (warn-only) configuration; override with
    /// [`RepetitionInspector::with_semantic`].
    pub fn staged(soft_warn_at: u32, hard_stop_at: u32) -> Self {
        Self {
            soft_warn_at: Some(soft_warn_at),
            hard_stop_at: Some(hard_stop_at),
            semantic: SemanticLoopConfig::default(),
            failure: FailureLoopConfig::default(),
        }
    }

    /// Replace the BR-30 semantic-loop configuration.
    pub fn with_semantic(mut self, semantic: SemanticLoopConfig) -> Self {
        self.semantic = semantic;
        self
    }

    /// Replace the BR-31 repeated-failing-result configuration.
    pub fn with_failure_loop(mut self, failure: FailureLoopConfig) -> Self {
        self.failure = failure;
        self
    }

    /// The message the model sees in place of the tool result when a call is
    /// hard-stopped. It states the *real* reason — the repetition guard fired —
    /// rather than claiming the user declined (which was the old, misleading
    /// `DECLINED_RESPONSE`).
    fn hard_stop_reason(tool_name: &str, repeat_count: u32) -> String {
        format!(
            "BioRouter stopped this tool call: '{tool_name}' has now been called \
             with identical arguments {repeat_count} times in a row. The user did \
             NOT decline it — this is an automatic repetition guard. Repeating the \
             same call will not produce a different result. Change approach: vary \
             the arguments, use a different tool, or explain what is blocking you \
             and stop."
        )
    }

    /// Non-blocking nudge injected into the model's context. The call still ran.
    fn soft_warn_reason(tool_name: &str, repeat_count: u32, hard_stop_at: u32) -> String {
        format!(
            "Repetition warning: you have called '{tool_name}' with identical \
             arguments {repeat_count} times in a row. It will be stopped \
             automatically on the {hard_stop_at}th consecutive identical call. \
             Change approach now: vary the arguments, use a different tool, or \
             explain what is blocking you and stop."
        )
    }

    /// BR-30 near-duplicate nudge. Hedged, because the heuristic can be wrong:
    /// a model that really is iterating over distinct inputs should ignore it.
    fn near_dup_warn_reason(tool_name: &str, run: u32) -> String {
        format!(
            "Possible loop: '{tool_name}' has now been called {run} times in a row \
             with only minor argument tweaks (the calls are near-identical once \
             ids and whitespace are ignored). Small edits to the same call rarely \
             produce a different result. If you are deliberately iterating over \
             genuinely different inputs, ignore this and continue; otherwise stop \
             tweaking and change approach — use a different tool, re-read what the \
             last result actually said, or explain what is blocking you."
        )
    }

    fn near_dup_stop_reason(tool_name: &str, run: u32) -> String {
        format!(
            "BioRouter stopped this tool call: '{tool_name}' has been called {run} \
             times in a row with near-identical arguments (only minor tweaks). The \
             user did NOT decline it — this is an automatic loop guard. Change \
             approach: use a different tool, re-read the last result, or explain \
             what is blocking you and stop."
        )
    }

    /// BR-30 oscillation nudge (`A/B/A/B`).
    fn oscillation_warn_reason(tool_name: &str, run: u32) -> String {
        format!(
            "Possible loop: your last {run} tool calls alternate between the same \
             two calls (…the most recent being '{tool_name}'), each repeated with \
             the same arguments. An A/B/A/B cycle repeats work rather than making \
             progress. If you are polling for a result that genuinely changes over \
             time, say so and continue; otherwise break the cycle — try a different \
             approach or explain what is blocking you."
        )
    }

    fn oscillation_stop_reason(tool_name: &str, run: u32) -> String {
        format!(
            "BioRouter stopped this tool call: your last {run} tool calls alternate \
             between the same two calls (…the most recent being '{tool_name}'), each \
             with unchanged arguments. The user did NOT decline it — this is an \
             automatic loop guard. Break the cycle: try a different approach, or \
             explain what is blocking you and stop."
        )
    }

    /// BR-30: near-duplicate + oscillation heuristics over the recent call
    /// window. Returns at most one result, the most severe that applies.
    fn inspect_semantic(
        &self,
        tool_request_id: &str,
        tool_name: &str,
        window: &[InternalToolCall],
    ) -> Option<InspectionResult> {
        if !self.semantic.enabled {
            return None;
        }

        let (near_dup_run, all_exact) =
            trailing_near_dup_run(window, self.semantic.similarity_threshold);
        // A run of byte-identical calls is the BR-29 guard's business; only flag
        // it here when at least one member differs (an actual argument tweak).
        let near_dup_run = if all_exact { 0 } else { near_dup_run };
        let oscillation_run = trailing_oscillation_run(window);

        let fired = |threshold: Option<u32>, run: u32| -> bool {
            threshold.is_some_and(|at| at > 0 && run >= at)
        };

        let (action, reason, finding_id) = if fired(self.semantic.near_dup_hard_stop, near_dup_run)
        {
            (
                InspectionAction::Deny,
                Self::near_dup_stop_reason(tool_name, near_dup_run),
                REPETITION_NEAR_DUP_HARD_FINDING_ID,
            )
        } else if fired(self.semantic.oscillation_hard_stop, oscillation_run) {
            (
                InspectionAction::Deny,
                Self::oscillation_stop_reason(tool_name, oscillation_run),
                REPETITION_OSCILLATION_HARD_FINDING_ID,
            )
        } else if fired(self.semantic.near_dup_soft_warn, near_dup_run) {
            (
                InspectionAction::Warn,
                Self::near_dup_warn_reason(tool_name, near_dup_run),
                REPETITION_NEAR_DUP_SOFT_FINDING_ID,
            )
        } else if fired(self.semantic.oscillation_soft_warn, oscillation_run) {
            (
                InspectionAction::Warn,
                Self::oscillation_warn_reason(tool_name, oscillation_run),
                REPETITION_OSCILLATION_SOFT_FINDING_ID,
            )
        } else {
            return None;
        };

        Some(InspectionResult {
            tool_request_id: tool_request_id.to_string(),
            action,
            reason,
            // Heuristic, not a proof: below the 1.0 the byte-exact guard claims.
            confidence: 0.8,
            inspector_name: REPETITION_INSPECTOR_NAME.to_string(),
            finding_id: Some(finding_id.to_string()),
        })
    }

    /// BR-31 hard stage: deny a call to a tool that has already failed the same
    /// way `hard_stop_at` times in a row since the last user turn.
    fn inspect_failure_loop(
        &self,
        tool_request_id: &str,
        tool_name: &str,
        outcomes: &[ToolOutcome],
    ) -> Option<InspectionResult> {
        if !self.failure.enabled {
            return None;
        }
        let hard_stop_at = self.failure.hard_stop_at.filter(|at| *at > 0)?;
        let streak = failing_streak(outcomes, tool_name, self.failure.similarity_threshold);
        if streak < hard_stop_at {
            return None;
        }

        Some(InspectionResult {
            tool_request_id: tool_request_id.to_string(),
            action: InspectionAction::Deny,
            reason: failure_stop_reason(tool_name, streak),
            // The tool ran and failed, every time: this is observed, not guessed.
            confidence: 1.0,
            inspector_name: REPETITION_INSPECTOR_NAME.to_string(),
            finding_id: Some(FAILURE_LOOP_HARD_FINDING_ID.to_string()),
        })
    }
}

#[async_trait]
impl ToolInspector for RepetitionInspector {
    fn name(&self) -> &'static str {
        REPETITION_INSPECTOR_NAME
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        _biorouter_mode: BioRouterMode,
        _session: &crate::session::Session,
    ) -> Result<Vec<InspectionResult>> {
        let mut results = Vec::new();
        if self.hard_stop_at.is_none() && !self.semantic.enabled && !self.failure.enabled {
            return Ok(results);
        }

        let mut last_call: Option<InternalToolCall> = None;
        let mut repeat_count = 0u32;

        for call in messages
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(|content| content.as_tool_request())
            .filter_map(InternalToolCall::from_request)
        {
            if last_call.as_ref().is_some_and(|last| last.matches(&call)) {
                repeat_count += 1;
            } else {
                repeat_count = 1;
                last_call = Some(call);
            }
        }

        // BR-30: the semantic heuristics look at a bounded window of recent
        // calls (the current turn), not the whole transcript.
        let mut window = if self.semantic.enabled {
            calls_since_last_user_turn(messages)
        } else {
            Vec::new()
        };

        // BR-31: what the calls in this turn actually *returned*. Only history is
        // available here — the batch being inspected has not run yet — which is
        // exactly right: the guard blocks the call *after* a run of identical
        // failures.
        let outcomes = if self.failure.enabled {
            tool_outcomes_since_last_user_turn(messages)
        } else {
            Vec::new()
        };

        for tool_request in tool_requests {
            if let Some(call) = InternalToolCall::from_request(tool_request) {
                if last_call.as_ref().is_some_and(|last| last.matches(&call)) {
                    repeat_count += 1;
                } else {
                    repeat_count = 1;
                    last_call = Some(call.clone());
                }

                window.push(call);
                if window.len() > LOOP_WINDOW {
                    window.remove(0);
                }

                let tool_name = tool_request
                    .tool_call
                    .as_ref()
                    .map(|tool_call| tool_call.name.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());

                let exact_result = self.hard_stop_at.and_then(|hard_stop_at| {
                    if repeat_count >= hard_stop_at {
                        Some(InspectionResult {
                            tool_request_id: tool_request.id.clone(),
                            action: InspectionAction::Deny,
                            reason: Self::hard_stop_reason(&tool_name, repeat_count),
                            confidence: 1.0,
                            inspector_name: REPETITION_INSPECTOR_NAME.to_string(),
                            finding_id: Some(REPETITION_HARD_FINDING_ID.to_string()),
                        })
                    } else if self
                        .soft_warn_at
                        .is_some_and(|soft_warn_at| repeat_count >= soft_warn_at)
                    {
                        Some(InspectionResult {
                            tool_request_id: tool_request.id.clone(),
                            action: InspectionAction::Warn,
                            reason: Self::soft_warn_reason(&tool_name, repeat_count, hard_stop_at),
                            confidence: 1.0,
                            inspector_name: REPETITION_INSPECTOR_NAME.to_string(),
                            finding_id: Some(REPETITION_SOFT_FINDING_ID.to_string()),
                        })
                    } else {
                        None
                    }
                });

                let failure_result =
                    self.inspect_failure_loop(&tool_request.id, &tool_name, &outcomes);

                // One verdict per call, no double nudge. A Deny outranks a Warn;
                // between the two denies, the byte-exact guard's is the more
                // specific proof, so it keeps precedence.
                let verdict = match (exact_result, failure_result) {
                    (Some(exact), Some(_)) if exact.action == InspectionAction::Deny => Some(exact),
                    (_, Some(failure)) => Some(failure),
                    (Some(exact), None) => Some(exact),
                    (None, None) => self.inspect_semantic(&tool_request.id, &tool_name, &window),
                };
                if let Some(result) = verdict {
                    results.push(result);
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::object;

    fn call(name: &str, args: Value) -> InternalToolCall {
        InternalToolCall::from_tool_call(&CallToolRequestParams {
            task: None,
            meta: None,
            name: name.to_string().into(),
            arguments: args.as_object().cloned(),
        })
    }

    #[test]
    fn volatile_keys_and_whitespace_normalize_away() {
        let a = call(
            "search",
            object!({"query": "TP53  variants", "request_id": "abc-1"}).into(),
        );
        let b = call(
            "search",
            object!({"query": "TP53 variants", "request_id": "abc-2"}).into(),
        );

        assert!(
            !a.matches(&b),
            "byte-exact comparison must still see a diff"
        );
        assert!(
            a.same_shape(&b),
            "normalization must erase ids + whitespace"
        );
        assert!(a.near_matches(&b, DEFAULT_ARG_SIMILARITY_THRESHOLD));
    }

    #[test]
    fn a_tweaked_long_command_is_a_near_duplicate() {
        let a = call(
            "shell",
            object!({"command": "grep -rn 'fn assemble_turn_context' crates/biorouter/src/agents"})
                .into(),
        );
        let b = call(
            "shell",
            object!({"command": "grep -rn 'fn assemble_turn_context' crates/biorouter/src/agent"})
                .into(),
        );
        assert!(
            a.near_matches(&b, DEFAULT_ARG_SIMILARITY_THRESHOLD),
            "similarity was {}",
            arg_similarity(&a.normalized, &b.normalized)
        );
    }

    // The false-positive that matters: iterating over genuinely different inputs
    // has the same call *shape* but must NOT read as a near-duplicate.
    #[test]
    fn distinct_targets_are_not_near_duplicates() {
        let a = call("read_file", object!({"path": "src/one.rs"}).into());
        let b = call("read_file", object!({"path": "src/two.rs"}).into());
        assert!(!a.near_matches(&b, DEFAULT_ARG_SIMILARITY_THRESHOLD));

        let page1 = call(
            "read_file",
            object!({"path": "big.csv", "offset": 0}).into(),
        );
        let page2 = call(
            "read_file",
            object!({"path": "big.csv", "offset": 500}).into(),
        );
        assert!(
            !page1.near_matches(&page2, DEFAULT_ARG_SIMILARITY_THRESHOLD),
            "pagination is progress, not a loop (similarity {})",
            arg_similarity(&page1.normalized, &page2.normalized)
        );

        let x = call("shell", object!({"command": "pytest test_alpha.py"}).into());
        let y = call("shell", object!({"command": "pytest test_gamma.py"}).into());
        assert!(
            !x.near_matches(&y, DEFAULT_ARG_SIMILARITY_THRESHOLD),
            "similarity was {}",
            arg_similarity(&x.normalized, &y.normalized)
        );
    }

    #[test]
    fn different_tools_never_match() {
        let a = call("read_file", object!({"path": "a.rs"}).into());
        let b = call("write_file", object!({"path": "a.rs"}).into());
        assert!(!a.near_matches(&b, DEFAULT_ARG_SIMILARITY_THRESHOLD));
        assert!(!a.same_shape(&b));
    }

    #[test]
    fn near_dup_run_anchors_on_the_current_call() {
        // A slowly drifting sequence: each call resembles its predecessor but the
        // first and last are unrelated. It must not accumulate a long run.
        let seq = vec![
            call("shell", object!({"command": "aaaaaaaaaaaaaaaaaaaa"}).into()),
            call("shell", object!({"command": "aaaaaaaaaabbbbbbbbbb"}).into()),
            call("shell", object!({"command": "bbbbbbbbbbbbbbbbbbbb"}).into()),
        ];
        let (run, _) = trailing_near_dup_run(&seq, DEFAULT_ARG_SIMILARITY_THRESHOLD);
        assert_eq!(run, 1);
    }

    #[test]
    fn near_dup_run_reports_whether_the_run_is_byte_exact() {
        let identical = call("search", object!({"query": "TP53"}).into());
        let seq = vec![identical.clone(), identical.clone(), identical];
        let (run, all_exact) = trailing_near_dup_run(&seq, DEFAULT_ARG_SIMILARITY_THRESHOLD);
        assert_eq!(run, 3);
        assert!(all_exact, "a byte-exact run belongs to the BR-29 guard");

        let seq = vec![
            call("search", object!({"query": "TP53  variants"}).into()),
            call("search", object!({"query": "TP53 variants"}).into()),
        ];
        let (run, all_exact) = trailing_near_dup_run(&seq, DEFAULT_ARG_SIMILARITY_THRESHOLD);
        assert_eq!(run, 2);
        assert!(!all_exact);
    }

    #[test]
    fn oscillation_needs_two_full_cycles_of_two_distinct_calls() {
        let a = call("read_file", object!({"path": "a.rs"}).into());
        let b = call("shell", object!({"command": "cargo build"}).into());

        assert_eq!(trailing_oscillation_run(&[a.clone(), b.clone()]), 0);
        assert_eq!(
            trailing_oscillation_run(&[a.clone(), b.clone(), a.clone()]),
            0,
            "A/B/A is one and a half cycles — not enough"
        );
        assert_eq!(
            trailing_oscillation_run(&[a.clone(), b.clone(), a.clone(), b.clone()]),
            4
        );
        assert_eq!(
            trailing_oscillation_run(&[
                a.clone(),
                b.clone(),
                a.clone(),
                b.clone(),
                a.clone(),
                b.clone()
            ]),
            6
        );

        // A/A/A/A is plain repetition (BR-29), not oscillation.
        assert_eq!(
            trailing_oscillation_run(&[a.clone(), a.clone(), a.clone(), a.clone()]),
            0
        );
        // A third distinct call breaks the alternation.
        let c = call("shell", object!({"command": "cargo fmt"}).into());
        assert_eq!(trailing_oscillation_run(&[a.clone(), b.clone(), c, b]), 0);
    }

    #[test]
    fn oscillation_ignores_volatile_argument_churn() {
        let a1 = call("poll", object!({"job": "x", "request_id": "1"}).into());
        let b1 = call("wait", object!({"secs": 5, "request_id": "2"}).into());
        let a2 = call("poll", object!({"job": "x", "request_id": "3"}).into());
        let b2 = call("wait", object!({"secs": 5, "request_id": "4"}).into());
        assert_eq!(trailing_oscillation_run(&[a1, b1, a2, b2]), 4);
    }

    // ---------------------------------------------------------------------
    // BR-31: repeated-failing-result / no-progress detector
    // ---------------------------------------------------------------------

    fn failed(tool_name: &str, error: &str) -> ToolOutcome {
        ToolOutcome {
            tool_name: tool_name.to_string(),
            failure: Some(error_signature(error)),
        }
    }

    fn succeeded(tool_name: &str) -> ToolOutcome {
        ToolOutcome {
            tool_name: tool_name.to_string(),
            failure: None,
        }
    }

    fn error_result(text: &str) -> ToolResult<CallToolResult> {
        Ok(CallToolResult {
            content: vec![rmcp::model::Content::text(text)],
            structured_content: None,
            is_error: Some(true),
            meta: None,
        })
    }

    fn ok_result(text: &str) -> ToolResult<CallToolResult> {
        Ok(CallToolResult {
            content: vec![rmcp::model::Content::text(text)],
            structured_content: None,
            is_error: Some(false),
            meta: None,
        })
    }

    #[test]
    fn error_signature_folds_case_whitespace_and_digit_runs() {
        // The same failure re-run: only a pid and a timestamp differ.
        let first = error_signature("Error: process 4821 failed at 12:03:44\n  exit code 1");
        let second = error_signature("error: process 991 failed at 09:17:02   exit code 1");
        assert_eq!(first, second, "ids and clocks must not fork the signature");

        // A materially different error must not collapse into the same signature.
        assert_ne!(
            error_signature("no such file or directory: config.yaml"),
            error_signature("permission denied: config.yaml")
        );
    }

    #[test]
    fn failing_streak_counts_consecutive_identical_failures_of_one_tool() {
        let outcomes = vec![
            failed("shell", "cargo: command not found"),
            failed("shell", "cargo: command not found"),
            failed("shell", "cargo: command not found"),
        ];
        assert_eq!(
            failing_streak(&outcomes, "shell", DEFAULT_ERROR_SIMILARITY_THRESHOLD),
            3
        );
        // A tool that never ran has no streak.
        assert_eq!(
            failing_streak(&outcomes, "read_file", DEFAULT_ERROR_SIMILARITY_THRESHOLD),
            0
        );
    }

    #[test]
    fn a_success_or_a_new_error_breaks_the_streak() {
        // Success is progress.
        let outcomes = vec![
            failed("shell", "cargo: command not found"),
            failed("shell", "cargo: command not found"),
            succeeded("shell"),
        ];
        assert_eq!(
            failing_streak(&outcomes, "shell", DEFAULT_ERROR_SIMILARITY_THRESHOLD),
            0
        );

        // A *different* error is new information — the model learned something.
        let outcomes = vec![
            failed("shell", "cargo: command not found"),
            failed("shell", "cargo: command not found"),
            failed(
                "shell",
                "error[E0433]: failed to resolve: use of undeclared crate",
            ),
        ];
        assert_eq!(
            failing_streak(&outcomes, "shell", DEFAULT_ERROR_SIMILARITY_THRESHOLD),
            1
        );
    }

    #[test]
    fn interleaved_other_tools_neither_reset_nor_hide_a_streak() {
        let outcomes = vec![
            failed("shell", "cargo: command not found"),
            succeeded("read_file"),
            failed("shell", "cargo: command not found"),
            succeeded("read_file"),
            failed("shell", "cargo: command not found"),
        ];
        assert_eq!(
            failing_streak(&outcomes, "shell", DEFAULT_ERROR_SIMILARITY_THRESHOLD),
            3,
            "a tool's own failures are the streak; other tools are irrelevant"
        );
    }

    #[test]
    fn nudges_escalate_then_hand_off_to_the_hard_stop() {
        let config = FailureLoopConfig::default();
        let mut outcomes = vec![failed("shell", "cargo: command not found")];
        assert!(
            failure_loop_nudge(&config, &outcomes, "shell").is_none(),
            "one failure is not a loop"
        );

        outcomes.push(failed("shell", "cargo: command not found"));
        assert!(failure_loop_nudge(&config, &outcomes, "shell").is_none());

        outcomes.push(failed("shell", "cargo: command not found"));
        let soft = failure_loop_nudge(&config, &outcomes, "shell").expect("soft nudge at 3");
        assert!(soft.starts_with("No progress"), "{soft}");
        assert!(soft.contains("failed 3 times"), "{soft}");

        outcomes.push(failed("shell", "cargo: command not found"));
        outcomes.push(failed("shell", "cargo: command not found"));
        let escalated =
            failure_loop_nudge(&config, &outcomes, "shell").expect("escalated nudge at 5");
        assert!(
            escalated.starts_with("You are not making progress"),
            "{escalated}"
        );
        assert!(
            escalated.contains("ask the user"),
            "the escalation must offer the honest way out: {escalated}"
        );
        assert!(
            escalated.contains("blocked automatically"),
            "the model must be told the stop is coming: {escalated}"
        );
    }

    #[test]
    fn a_disabled_detector_never_nudges() {
        let outcomes = vec![
            failed("shell", "boom"),
            failed("shell", "boom"),
            failed("shell", "boom"),
            failed("shell", "boom"),
        ];
        assert!(failure_loop_nudge(&FailureLoopConfig::disabled(), &outcomes, "shell").is_none());
    }

    #[test]
    fn outcomes_pair_requests_with_responses_and_reset_at_a_user_turn() {
        let request = |id: &str, name: &str| {
            Message::assistant().with_tool_request(
                id,
                Ok(CallToolRequestParams {
                    task: None,
                    meta: None,
                    name: name.to_string().into(),
                    arguments: None,
                }),
            )
        };

        let messages = vec![
            Message::user().with_text("build it"),
            request("call_1", "shell"),
            Message::user().with_tool_response("call_1", error_result("cargo: command not found")),
            request("call_2", "shell"),
            Message::user().with_tool_response("call_2", ok_result("done")),
        ];

        let outcomes = tool_outcomes_since_last_user_turn(&messages);
        assert_eq!(
            outcomes,
            vec![
                failed("shell", "cargo: command not found"),
                succeeded("shell"),
            ]
        );

        // A genuine user turn is a fresh start: whatever failed before it is not
        // this turn's loop.
        let mut with_new_turn = messages.clone();
        with_new_turn.push(Message::user().with_text("try again"));
        assert!(tool_outcomes_since_last_user_turn(&with_new_turn).is_empty());

        // …but a loop-guard nudge (hidden, agent-authored) must NOT reset it.
        let mut with_nudge = messages;
        with_nudge.push(
            Message::user()
                .with_text("<biorouter-loop-guard>…</biorouter-loop-guard>")
                .with_visibility(false, true),
        );
        assert_eq!(tool_outcomes_since_last_user_turn(&with_nudge).len(), 2);
    }

    #[test]
    fn a_transport_error_counts_as_a_failure() {
        let outcome = tool_outcome(
            "shell",
            &Err(rmcp::model::ErrorData::new(
                rmcp::model::ErrorCode::INTERNAL_ERROR,
                "connection closed",
                None,
            )),
        );
        assert_eq!(outcome.failure.as_deref(), Some("connection closed"));
    }
}
