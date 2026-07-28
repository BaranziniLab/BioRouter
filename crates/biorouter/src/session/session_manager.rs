use crate::config::paths::Paths;
use crate::conversation::message::{
    new_message_id, Message, MessageContent, MessageMetadata, TokenState,
};
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::providers::base::{Provider, MSG_COUNT_FOR_SESSION_NAME_GENERATION};
use crate::providers::pricing::{
    cost_with_pricing, provider_model_pricing, resolved_provider_model_pricing,
    ProviderModelPricing, TurnCost,
};
use crate::session::chat_fts;
use crate::session::extension_data::ExtensionData;
use crate::session::message_blobs;
use crate::workflow::Workflow;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rmcp::model::Role;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tracing::{debug, info, warn};
use utoipa::ToSchema;

pub const CURRENT_SCHEMA_VERSION: i32 = 16;
pub const SESSIONS_FOLDER: &str = "sessions";
pub const DB_NAME: &str = "sessions.db";

/// FTS5 mirror of user-visible message text, used for relevance-ranked chat
/// recall (BR-17). It is a contentful FTS5 table (it stores the flattened
/// text) maintained from Rust at the message write sites, because the searchable
/// text is a derived flattening of `content_json`, not a raw column, so SQLite
/// content-sync triggers can't produce it. `message_id`/`session_id` are stored
/// UNINDEXED so recall can join back to `messages`/`sessions` and delete a
/// session's rows on the compaction rewrite.
const MESSAGES_FTS_DDL: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    text,
    session_id UNINDEXED,
    message_id UNINDEXED,
    tokenize = 'porter unicode61'
)
"#;

const MESSAGES_FTS_INSERT: &str =
    "INSERT INTO messages_fts (text, session_id, message_id) VALUES (?, ?, ?)";

/// True when `err` is the `UNIQUE(messages.session_id, messages.msg_uid)`
/// violation (SQLite error 2067) from the message insert — the one failure
/// [`SessionStorage::add_message`] recovers from by re-minting the uid (#41).
/// Scoped to the msg_uid index by message text so an unrelated unique
/// violation still surfaces as an error.
fn is_msg_uid_unique_violation(err: &anyhow::Error) -> bool {
    match err.downcast_ref::<sqlx::Error>() {
        Some(sqlx::Error::Database(db_err)) => {
            db_err.is_unique_violation() && db_err.message().contains("messages.msg_uid")
        }
        _ => false,
    }
}

/// Whether a stored message should be indexed for recall. Recall searches what
/// the *user* saw, so only `user_visible` messages are indexed. A row with no
/// (or unparseable) metadata predates the flag and defaults to visible.
fn message_is_user_visible(metadata_json: Option<&str>) -> bool {
    metadata_json
        .and_then(|json| serde_json::from_str::<MessageMetadata>(json).ok())
        .map(|meta| meta.user_visible)
        .unwrap_or(true)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    #[default]
    User,
    Scheduled,
    SubAgent,
    Hidden,
    Terminal,
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionType::User => write!(f, "user"),
            SessionType::SubAgent => write!(f, "sub_agent"),
            SessionType::Hidden => write!(f, "hidden"),
            SessionType::Scheduled => write!(f, "scheduled"),
            SessionType::Terminal => write!(f, "terminal"),
        }
    }
}

impl std::str::FromStr for SessionType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(SessionType::User),
            "sub_agent" => Ok(SessionType::SubAgent),
            "hidden" => Ok(SessionType::Hidden),
            "scheduled" => Ok(SessionType::Scheduled),
            "terminal" => Ok(SessionType::Terminal),
            _ => Err(anyhow::anyhow!("Invalid session type: {}", s)),
        }
    }
}

static SESSION_STORAGE: LazyLock<Arc<SessionStorage>> =
    LazyLock::new(|| Arc::new(SessionStorage::new(Paths::data_dir())));

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Session {
    pub id: String,
    #[schema(value_type = String)]
    pub working_dir: PathBuf,
    #[serde(alias = "description")]
    pub name: String,
    #[serde(default)]
    pub user_set_name: bool,
    #[serde(default)]
    pub session_type: SessionType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub extension_data: ExtensionData,
    /// The *current* turn's usage — the live context-window occupancy. Bounded by
    /// the model's context limit, so `i32` is safe.
    pub total_tokens: Option<i32>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    /// Lifetime totals. New usage writes use the four-bucket billed total;
    /// databases created before billed-bucket accounting may contain legacy
    /// context totals here, so reporting and budgets use `token_events` instead.
    /// These counters grow without bound, so SQLite and Rust both use 64-bit.
    pub accumulated_total_tokens: Option<i64>,
    pub accumulated_input_tokens: Option<i64>,
    pub accumulated_output_tokens: Option<i64>,
    pub schedule_id: Option<String>,
    pub workflow: Option<Workflow>,
    pub user_workflow_values: Option<HashMap<String, String>>,
    pub conversation: Option<Conversation>,
    pub message_count: usize,
    pub provider_name: Option<String>,
    pub model_config: Option<ModelConfig>,
    /// Id of the session this one was diverged (branched) from, if any. Set by
    /// `diverge_session`; `None` for normally-created sessions. Lets the UI show
    /// a session's lineage ("branched from …").
    #[serde(default)]
    pub diverged_from: Option<String>,
    /// The durable `msg_uid` of the exact parent message this session was
    /// branched at — the divergence point (BR-45). Paired with `diverged_from`
    /// (parent session), it is the edge label of the branch forest. `None` for
    /// normally-created sessions. Anchoring on this stable id instead of a
    /// whole-second timestamp is what fixes the same-second over-truncation.
    #[serde(default)]
    pub branch_point_msg_uid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct SessionSummary {
    pub id: String,
    pub working_dir: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
}

/// One turn's token usage, applied additively and atomically in SQL.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenDelta {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub total: Option<i64>,
}

impl TokenDelta {
    fn is_empty(self) -> bool {
        self.input.is_none() && self.output.is_none() && self.total.is_none()
    }
}

/// One provider call's durable accounting payload. `event_key` identifies the
/// provider call for idempotent retries: the ledger row and the session's
/// accumulated counters are either both applied once or neither is applied.
#[derive(Debug, Clone)]
pub struct UsageLedgerEntry {
    pub event_key: String,
    pub session_id: String,
    pub schedule_id: Option<String>,
    pub current_total_tokens: Option<i32>,
    pub current_input_tokens: Option<i32>,
    pub current_output_tokens: Option<i32>,
    pub billed_total_tokens: Option<i64>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub model_id: Option<String>,
    pub provider: Option<String>,
    pub cache_read_tokens: Option<i32>,
    pub cache_creation_tokens: Option<i32>,
}

pub struct SessionUpdateBuilder<'a> {
    session_manager: &'a SessionManager,
    session_id: String,
    name: Option<String>,
    user_set_name: Option<bool>,
    session_type: Option<SessionType>,
    working_dir: Option<PathBuf>,
    extension_data: Option<ExtensionData>,
    total_tokens: Option<Option<i32>>,
    input_tokens: Option<Option<i32>>,
    output_tokens: Option<Option<i32>>,
    accumulated_total_tokens: Option<Option<i64>>,
    accumulated_input_tokens: Option<Option<i64>>,
    accumulated_output_tokens: Option<Option<i64>>,
    /// A per-turn DELTA, applied atomically as `col = COALESCE(col,0) + ?` in
    /// SQL. The old path read the row into Rust, added, and wrote back — a
    /// lost-update race whenever two turns raced on one session.
    token_delta: Option<TokenDelta>,
    schedule_id: Option<Option<String>>,
    workflow: Option<Option<Workflow>>,
    user_workflow_values: Option<Option<HashMap<String, String>>>,
    provider_name: Option<Option<String>>,
    model_config: Option<Option<ModelConfig>>,
    diverged_from: Option<Option<String>>,
    branch_point_msg_uid: Option<Option<String>>,
}

#[derive(Serialize, ToSchema, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionInsights {
    pub total_sessions: usize,
    pub total_tokens: Option<i64>,
    pub sessions_last_7_days: usize,
    pub sessions_last_30_days: usize,
    pub tokens_last_7_days: Option<i64>,
    pub tokens_last_30_days: Option<i64>,
}

/// The session types a user actually sees. `SubAgent`, `Hidden` and `Terminal`
/// sessions are internal machinery — one user task can spawn several — so
/// counting them made the insight tiles disagree with the session list printed
/// directly beneath them. This mirrors what `list_sessions` shows.
pub const USER_FACING_SESSION_TYPES: [&str; 2] = ["user", "scheduled"];

/// One calendar day of usage, for the Home heatmap.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailyActivity {
    /// Local calendar day, `YYYY-MM-DD`.
    pub date: String,
    /// Sessions *started* that day (exact; keyed on the immutable `created_at`).
    pub sessions: i64,
    /// Tokens processed that day, summed from per-turn `token_events`.
    pub tokens: i64,
    /// False when at least one token event that day lacks billed-token
    /// accounting. `tokens` is then a known subtotal; zero is unavailable, not
    /// a measured zero.
    pub tokens_complete: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Assistant + user messages exchanged that day.
    pub messages: i64,
    /// 1–4. Level 0 days are omitted from the response entirely.
    pub level: u8,
}

/// The Home heatmap payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityWindow {
    pub start: String,
    pub end: String,
    pub max_sessions: i64,
    pub max_tokens: i64,
    /// False when at least one event in the window lacks billed-token accounting.
    /// Consult each day's `tokens_complete` for display semantics.
    pub tokens_complete: bool,
    pub current_streak: i64,
    pub longest_streak: i64,
    /// Only days with activity. The client fills the rest of the grid with level 0.
    pub days: Vec<DailyActivity>,
}

/// One `(model, provider)` group of the per-model usage breakdown.
///
/// `model_id` / `provider` are `None` for turns recorded before model
/// attribution landed, or when the provider reported no model — those rows
/// aggregate together as the "unknown" group.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsageRow {
    pub model_id: Option<String>,
    pub provider: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Billed tokens across all four disjoint buckets. `None` means at least
    /// one contributing event has no reconstructable billed total.
    pub total_tokens: Option<i64>,
    /// Input tokens served from the prompt cache. `None` means at least one
    /// contributing event predates cache accounting or did not report it.
    pub cache_read_tokens: Option<i64>,
    /// Input tokens written to the prompt cache.
    pub cache_creation_tokens: Option<i64>,
    /// Number of billed turns attributed to this group.
    pub turns: i64,
}

/// How `get_usage_report` buckets the per-turn ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsageGroup {
    /// One row per local calendar day (models summed within the day).
    Day,
    /// One row per `(model, provider)` group over the whole range.
    Model,
    /// One row per `(day, model, provider)`.
    DayModel,
}

/// One bucket of the usage report.
///
/// `date` is present unless grouping by [`UsageGroup::Model`]; `modelId` is
/// present unless grouping by [`UsageGroup::Day`]. `cost` is the dollar cost of
/// the priced turns in the bucket, or `None` when *every* contributing turn was
/// unpriced (an unknown model) — a `null` cost never means "$0". `hasUnpriced`
/// flags a bucket that mixes priced, unpriced, or incomplete turns, so a day
/// cost can be read as "at least this much" rather than exact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageReportRow {
    pub date: Option<String>,
    pub model_id: Option<String>,
    pub provider: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Billed tokens, or `None` when the bucket includes incomplete history.
    pub total_tokens: Option<i64>,
    /// Prompt-cache read/creation tokens in the bucket. `None` preserves
    /// historical incompleteness; it must not be presented as a measured zero.
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub turns: i64,
    pub cost: Option<f64>,
    pub has_unpriced: bool,
    /// `true` when cache cost is omitted because a contributing model has no
    /// cache rate or an event did not report a required cache bucket.
    pub cost_excludes_cache: bool,
}

/// Token + cost totals for a time span, priced through [`model_cost_with_cache`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Billed tokens, or `None` when the span includes incomplete history.
    pub total_tokens: Option<i64>,
    /// Prompt-cache read/creation tokens in the span. `None` means the span
    /// includes at least one event without cache-bucket accounting.
    pub cache_read_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub turns: i64,
    /// Dollar cost of the priced turns, or `None` when nothing in the span is
    /// priced. Priced-but-partial spans return the priced sum with
    /// `has_unpriced = true`.
    pub cost: Option<f64>,
    pub has_unpriced: bool,
    /// `true` when cache cost is omitted because pricing or cache accounting is
    /// incomplete — the figure is then a lower bound.
    pub cost_excludes_cache: bool,
}

/// Month-to-date and all-time usage totals, for the summary gauge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    /// Current local month, `YYYY-MM`.
    pub month: String,
    pub month_to_date: UsageTotals,
    pub all_time: UsageTotals,
}

/// The finest per-`(day, model, provider)` grain the report SQL returns, before
/// Rust rolls it up into the requested [`UsageGroup`] and prices each bucket.
#[derive(Debug, Clone, sqlx::FromRow)]
struct UsageGrainRow {
    day: String,
    model_id: Option<String>,
    provider: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cache_creation_tokens: Option<i64>,
    turns: i64,
    input_complete: i64,
    output_complete: i64,
    cache_read_complete: i64,
    cache_creation_complete: i64,
}

/// Cost of one finest-grain row, including its cache buckets, or `None` when its
/// `(provider, model)` pair is unknown/unpriced. Both endpoints must be present
/// to price it — a row with no model (the "unknown" bucket) is always unpriced.
struct GrainPrice {
    cost: Option<TurnCost>,
    incomplete: bool,
}

type ResolvedPricing = HashMap<(String, String), ProviderModelPricing>;

async fn resolve_grain_pricing(grain: &[UsageGrainRow]) -> ResolvedPricing {
    let mut resolved = HashMap::new();
    for row in grain {
        let (Some(provider), Some(model)) = (row.provider.as_ref(), row.model_id.as_ref()) else {
            continue;
        };
        let key = (provider.to_ascii_lowercase(), model.to_ascii_lowercase());
        if resolved.contains_key(&key) {
            continue;
        }
        if let Some(pricing) = resolved_provider_model_pricing(provider, model).await {
            resolved.insert(key, pricing);
        }
    }
    resolved
}

fn price_grain(row: &UsageGrainRow, resolved: &ResolvedPricing) -> GrainPrice {
    let incomplete_input_output = row.input_complete == 0 || row.output_complete == 0;
    let (Some(provider), Some(model)) = (row.provider.as_deref(), row.model_id.as_deref()) else {
        return GrainPrice {
            cost: None,
            incomplete: true,
        };
    };
    let key = (provider.to_ascii_lowercase(), model.to_ascii_lowercase());
    let pricing = resolved
        .get(&key)
        .cloned()
        .or_else(|| provider_model_pricing(provider, model));
    let Some(pricing) = pricing else {
        return GrainPrice {
            cost: None,
            incomplete: true,
        };
    };
    let incomplete_cache = row.cache_read_complete == 0 || row.cache_creation_complete == 0;
    let incomplete = incomplete_input_output || incomplete_cache;

    let cost = Some(cost_with_pricing(
        &pricing,
        row.input_tokens,
        row.output_tokens,
        row.cache_read_tokens.unwrap_or(0),
        row.cache_creation_tokens.unwrap_or(0),
    ))
    .map(|mut cost| {
        cost.cache_excluded |= incomplete_cache;
        cost
    });

    // A known model with only a context total has no priceable token buckets.
    // Returning Some(0) would falsely turn "unknown" into "$0".
    let cost = match cost {
        Some(cost) if incomplete && cost.cost == 0.0 && row.total_tokens != Some(0) => None,
        other => other,
    };

    GrainPrice { cost, incomplete }
}

/// Accumulator for one output bucket while rolling grain rows up.
#[derive(Default)]
struct BucketAcc {
    date: Option<String>,
    model_id: Option<String>,
    provider: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cache_creation_tokens: Option<i64>,
    turns: i64,
    cost_sum: f64,
    priced_any: bool,
    unpriced_any: bool,
    /// Set when a priced grain row carried cache tokens the model has no rate
    /// for, so `cost_sum` understates the true figure.
    cache_excluded_any: bool,
}

impl BucketAcc {
    fn add(&mut self, row: &UsageGrainRow, resolved: &ResolvedPricing) {
        let first_row = self.turns == 0;
        self.input_tokens += row.input_tokens;
        self.output_tokens += row.output_tokens;
        self.total_tokens = if first_row {
            row.total_tokens
        } else {
            sum_complete(self.total_tokens, row.total_tokens)
        };
        self.cache_read_tokens = if first_row {
            row.cache_read_tokens
        } else {
            sum_complete(self.cache_read_tokens, row.cache_read_tokens)
        };
        self.cache_creation_tokens = if first_row {
            row.cache_creation_tokens
        } else {
            sum_complete(self.cache_creation_tokens, row.cache_creation_tokens)
        };
        self.turns += row.turns;
        let price = price_grain(row, resolved);
        self.unpriced_any |= price.incomplete;
        match price.cost {
            Some(TurnCost {
                cost,
                cache_excluded,
            }) => {
                self.cost_sum += cost;
                self.priced_any = true;
                self.cache_excluded_any |= cache_excluded;
            }
            None => self.unpriced_any = true,
        }
    }

    /// `cost` is `None` only when the bucket is *entirely* unpriced, so a null
    /// cost can never be misread as "$0"; a partially-priced bucket returns its
    /// priced sum with `has_unpriced = true`.
    fn cost(&self) -> Option<f64> {
        (self.priced_any && !(self.unpriced_any && self.cost_sum == 0.0)).then_some(self.cost_sum)
    }
}

fn sum_complete(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        _ => None,
    }
}

fn complete_sum(count: i64, known_count: i64, sum: Option<i64>) -> Option<i64> {
    if count == 0 {
        Some(0)
    } else if count == known_count {
        sum
    } else {
        None
    }
}

/// Roll the finest per-`(day, model, provider)` grain up into the requested
/// grouping, pricing every grain row through [`model_cost_with_cache`] once. Pure so the
/// grouping + cost math is unit-tested without a database.
#[cfg(test)]
fn rollup_report(grain: &[UsageGrainRow], group: UsageGroup) -> Vec<UsageReportRow> {
    rollup_report_with_pricing(grain, group, &ResolvedPricing::new())
}

fn rollup_report_with_pricing(
    grain: &[UsageGrainRow],
    group: UsageGroup,
    resolved: &ResolvedPricing,
) -> Vec<UsageReportRow> {
    // Bucket key: (date, model_id, provider); components are None per grouping.
    type BucketKey = (Option<String>, Option<String>, Option<String>);
    let mut map: std::collections::BTreeMap<BucketKey, BucketAcc> =
        std::collections::BTreeMap::new();

    for row in grain {
        let (date, model_id, provider) = match group {
            UsageGroup::Day => (Some(row.day.clone()), None, None),
            UsageGroup::Model => (None, row.model_id.clone(), row.provider.clone()),
            UsageGroup::DayModel => (
                Some(row.day.clone()),
                row.model_id.clone(),
                row.provider.clone(),
            ),
        };
        map.entry((date.clone(), model_id.clone(), provider.clone()))
            .or_insert_with(|| BucketAcc {
                date,
                model_id,
                provider,
                ..Default::default()
            })
            .add(row, resolved);
    }

    let mut rows: Vec<UsageReportRow> = map
        .into_values()
        .map(|a| UsageReportRow {
            cost: a.cost(),
            has_unpriced: a.unpriced_any,
            cost_excludes_cache: a.cache_excluded_any,
            date: a.date,
            model_id: a.model_id,
            provider: a.provider,
            input_tokens: a.input_tokens,
            output_tokens: a.output_tokens,
            total_tokens: a.total_tokens,
            cache_read_tokens: a.cache_read_tokens,
            cache_creation_tokens: a.cache_creation_tokens,
            turns: a.turns,
        })
        .collect();

    // Day-bearing groups read as a chronological series (day asc, then heaviest
    // model first within a day); a pure per-model report is heaviest-first.
    rows.sort_by(|a, b| {
        a.date
            .cmp(&b.date)
            .then(b.total_tokens.cmp(&a.total_tokens))
            .then(a.model_id.cmp(&b.model_id))
    });
    rows
}

/// Sum grain rows into a single priced total (used for MTD and all-time).
#[cfg(test)]
fn totals_from_grain(grain: &[UsageGrainRow]) -> UsageTotals {
    totals_from_grain_with_pricing(grain, &ResolvedPricing::new())
}

fn totals_from_grain_with_pricing(
    grain: &[UsageGrainRow],
    resolved: &ResolvedPricing,
) -> UsageTotals {
    if grain.is_empty() {
        return UsageTotals {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: Some(0),
            cache_read_tokens: Some(0),
            cache_creation_tokens: Some(0),
            turns: 0,
            cost: Some(0.0),
            has_unpriced: false,
            cost_excludes_cache: false,
        };
    }

    let mut acc = BucketAcc::default();
    for row in grain {
        acc.add(row, resolved);
    }
    UsageTotals {
        input_tokens: acc.input_tokens,
        output_tokens: acc.output_tokens,
        total_tokens: acc.total_tokens,
        cache_read_tokens: acc.cache_read_tokens,
        cache_creation_tokens: acc.cache_creation_tokens,
        turns: acc.turns,
        cost: acc.cost(),
        has_unpriced: acc.unpriced_any,
        cost_excludes_cache: acc.cache_excluded_any,
    }
}

/// A day's raw intensity, before bucketing.
///
/// Tokens lead; sessions break ties, so a day of deep work outranks a day of
/// three trivial sessions. Both are log-compressed because token counts are
/// heavy-tailed — one marathon day can be 30x a normal one, and on a linear
/// scale it flattens every other day to the faintest shade.
fn activity_score(sessions: i64, tokens: i64) -> f64 {
    if sessions == 0 && tokens == 0 {
        return 0.0;
    }
    (1.0 + tokens as f64).ln() + 0.5 * (1.0 + sessions as f64).ln()
}

/// Linear-interpolated quantile of a pre-sorted slice.
fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let pos = (sorted.len() - 1) as f64 * q;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// Merge the three per-day queries into the heatmap payload.
///
/// Bucketing uses the **quartiles of the active days in this window**, not the
/// window maximum. Dividing by the max saturates: `ln` compresses so hard that
/// nearly every active day lands at 0.75-1.0 of the maximum, so a 4k-token day
/// renders as dark as a 250k-token day and the faintest level goes unused.
/// Quartiles give an even spread and match the GitHub convention every user
/// already recognises. Absolute values live in the tooltip, where they belong.
fn build_activity_window(
    start: String,
    end: String,
    session_rows: &[(String, i64)],
    token_rows: &[(String, i64, i64, i64, bool)],
    message_rows: &[(String, i64)],
) -> ActivityWindow {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Day {
        sessions: i64,
        tokens: i64,
        input: i64,
        output: i64,
        messages: i64,
        has_incomplete_tokens: bool,
    }

    let mut by_day: BTreeMap<String, Day> = BTreeMap::new();

    for (day, n) in session_rows {
        by_day.entry(day.clone()).or_default().sessions += n;
    }
    for (day, tokens, input, output, tokens_complete) in token_rows {
        let d = by_day.entry(day.clone()).or_default();
        d.tokens += tokens;
        d.input += input;
        d.output += output;
        d.has_incomplete_tokens |= !tokens_complete;
    }
    for (day, n) in message_rows {
        by_day.entry(day.clone()).or_default().messages += n;
    }

    // A day is "active" if it started a session or spent a token. Messages alone
    // (an edited transcript, say) do not light a cell.
    let mut scores: Vec<f64> = by_day
        .values()
        .map(|d| activity_score(d.sessions, d.tokens))
        .filter(|s| *s > 0.0)
        .collect();
    scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let (q1, q2, q3) = (
        quantile(&scores, 0.25),
        quantile(&scores, 0.50),
        quantile(&scores, 0.75),
    );

    let max_sessions = by_day.values().map(|d| d.sessions).max().unwrap_or(0);
    let max_tokens = by_day.values().map(|d| d.tokens).max().unwrap_or(0);

    let days: Vec<DailyActivity> = by_day
        .iter()
        .filter_map(|(date, d)| {
            let score = activity_score(d.sessions, d.tokens);
            if score <= 0.0 {
                return None;
            }
            let level = if score <= q1 {
                1
            } else if score <= q2 {
                2
            } else if score <= q3 {
                3
            } else {
                4
            };
            Some(DailyActivity {
                date: date.clone(),
                sessions: d.sessions,
                tokens: d.tokens,
                tokens_complete: !d.has_incomplete_tokens,
                input_tokens: d.input,
                output_tokens: d.output,
                messages: d.messages,
                level,
            })
        })
        .collect();

    let (current_streak, longest_streak) = streaks(&start, &end, &days);

    ActivityWindow {
        start,
        end,
        max_sessions,
        max_tokens,
        tokens_complete: token_rows.iter().all(|row| row.4),
        current_streak,
        longest_streak,
        days,
    }
}

/// Consecutive active calendar days. `current` counts back from `end`.
fn streaks(start: &str, end: &str, days: &[DailyActivity]) -> (i64, i64) {
    use chrono::NaiveDate;
    use std::collections::HashSet;

    let active: HashSet<&str> = days.iter().map(|d| d.date.as_str()).collect();
    let (Ok(from), Ok(to)) = (
        NaiveDate::parse_from_str(start, "%Y-%m-%d"),
        NaiveDate::parse_from_str(end, "%Y-%m-%d"),
    ) else {
        return (0, 0);
    };

    let (mut longest, mut run) = (0i64, 0i64);
    let mut day = from;
    while day <= to {
        if active.contains(day.format("%Y-%m-%d").to_string().as_str()) {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
        day = day.succ_opt().unwrap_or(day + chrono::Duration::days(1));
    }

    // The current streak may legitimately end yesterday: a user who has not yet
    // opened the app today has not broken it.
    let mut current = 0i64;
    let mut cursor = to;
    if !active.contains(cursor.format("%Y-%m-%d").to_string().as_str()) {
        cursor = cursor.pred_opt().unwrap_or(cursor);
    }
    while cursor >= from && active.contains(cursor.format("%Y-%m-%d").to_string().as_str()) {
        current += 1;
        let Some(prev) = cursor.pred_opt() else { break };
        cursor = prev;
    }

    (current, longest)
}

impl<'a> SessionUpdateBuilder<'a> {
    fn new(session_manager: &'a SessionManager, session_id: String) -> Self {
        Self {
            session_manager,
            session_id,
            name: None,
            user_set_name: None,
            session_type: None,
            working_dir: None,
            extension_data: None,
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            accumulated_total_tokens: None,
            accumulated_input_tokens: None,
            accumulated_output_tokens: None,
            token_delta: None,
            schedule_id: None,
            workflow: None,
            user_workflow_values: None,
            provider_name: None,
            model_config: None,
            diverged_from: None,
            branch_point_msg_uid: None,
        }
    }

    pub async fn apply(self) -> Result<()> {
        self.session_manager.apply_update_inner(self).await
    }

    pub fn user_provided_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        if !name.is_empty() {
            self.name = Some(name);
            self.user_set_name = Some(true);
        }
        self
    }

    pub fn system_generated_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        if !name.is_empty() {
            self.name = Some(name);
            self.user_set_name = Some(false);
        }
        self
    }

    pub fn session_type(mut self, session_type: SessionType) -> Self {
        self.session_type = Some(session_type);
        self
    }

    // NOTE: there is deliberately no `working_dir` setter here. The working
    // directory is guarded (#44): go through
    // `SessionManager::try_update_working_dir_if_empty`, or — for the terminal
    // shell-following path only — `force_update_working_dir_unguarded`.

    pub fn extension_data(mut self, data: ExtensionData) -> Self {
        self.extension_data = Some(data);
        self
    }

    pub fn total_tokens(mut self, tokens: Option<i32>) -> Self {
        self.total_tokens = Some(tokens);
        self
    }

    pub fn input_tokens(mut self, tokens: Option<i32>) -> Self {
        self.input_tokens = Some(tokens);
        self
    }

    pub fn output_tokens(mut self, tokens: Option<i32>) -> Self {
        self.output_tokens = Some(tokens);
        self
    }

    pub fn accumulated_total_tokens(mut self, tokens: Option<i64>) -> Self {
        // A column may appear only once in a SET list.
        self.token_delta = None;
        self.accumulated_total_tokens = Some(tokens);
        self
    }

    pub fn accumulated_input_tokens(mut self, tokens: Option<i64>) -> Self {
        // A column may appear only once in a SET list.
        self.token_delta = None;
        self.accumulated_input_tokens = Some(tokens);
        self
    }

    pub fn accumulated_output_tokens(mut self, tokens: Option<i64>) -> Self {
        // A column may appear only once in a SET list.
        self.token_delta = None;
        self.accumulated_output_tokens = Some(tokens);
        self
    }

    /// Add one turn's usage to the session's lifetime counters, atomically.
    ///
    /// Prefer this over the absolute `accumulated_*` setters for anything on the
    /// hot path: it compiles to `col = COALESCE(col, 0) + ?` so two concurrent
    /// turns on the same session cannot lose an update, and the arithmetic
    /// happens in SQLite's 64-bit INTEGER rather than in `i32`.
    pub fn accumulate_tokens(mut self, delta: TokenDelta) -> Self {
        if !delta.is_empty() {
            // A column may appear only once in a SET list.
            self.accumulated_total_tokens = None;
            self.accumulated_input_tokens = None;
            self.accumulated_output_tokens = None;
            self.token_delta = Some(delta);
        }
        self
    }

    pub fn schedule_id(mut self, schedule_id: Option<String>) -> Self {
        self.schedule_id = Some(schedule_id);
        self
    }

    pub fn workflow(mut self, workflow: Option<Workflow>) -> Self {
        self.workflow = Some(workflow);
        self
    }

    pub fn user_workflow_values(
        mut self,
        user_workflow_values: Option<HashMap<String, String>>,
    ) -> Self {
        self.user_workflow_values = Some(user_workflow_values);
        self
    }

    pub fn provider_name(mut self, provider_name: impl Into<String>) -> Self {
        self.provider_name = Some(Some(provider_name.into()));
        self
    }

    pub fn model_config(mut self, model_config: ModelConfig) -> Self {
        self.model_config = Some(Some(model_config));
        self
    }

    /// Record (or clear) the id of the session this one was diverged from.
    pub fn diverged_from(mut self, diverged_from: Option<String>) -> Self {
        self.diverged_from = Some(diverged_from);
        self
    }

    /// Record (or clear) the durable `msg_uid` of the parent message this
    /// session was branched at (BR-45 divergence point).
    pub fn branch_point_msg_uid(mut self, branch_point_msg_uid: Option<String>) -> Self {
        self.branch_point_msg_uid = Some(branch_point_msg_uid);
        self
    }
}

/// The six token counters stored on a session row. Fetched cheaply on the
/// streaming hot path without the surrounding metadata or message count.
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct SessionTokenCounts {
    pub total_tokens: Option<i32>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub accumulated_total_tokens: Option<i64>,
    pub accumulated_input_tokens: Option<i64>,
    pub accumulated_output_tokens: Option<i64>,
}

/// BR-52: the one place a session's stored counters become the `TokenState` the
/// clients see. The agent reads it once per turn boundary and carries it in the
/// event stream; the server no longer re-derives it per streamed token.
impl From<SessionTokenCounts> for TokenState {
    fn from(counts: SessionTokenCounts) -> Self {
        TokenState {
            input_tokens: counts.input_tokens.unwrap_or(0),
            output_tokens: counts.output_tokens.unwrap_or(0),
            total_tokens: counts.total_tokens.unwrap_or(0),
            accumulated_input_tokens: counts.accumulated_input_tokens.unwrap_or(0),
            accumulated_output_tokens: counts.accumulated_output_tokens.unwrap_or(0),
            accumulated_total_tokens: counts.accumulated_total_tokens.unwrap_or(0),
        }
    }
}

impl From<&Session> for TokenState {
    fn from(session: &Session) -> Self {
        TokenState {
            input_tokens: session.input_tokens.unwrap_or(0),
            output_tokens: session.output_tokens.unwrap_or(0),
            total_tokens: session.total_tokens.unwrap_or(0),
            accumulated_input_tokens: session.accumulated_input_tokens.unwrap_or(0),
            accumulated_output_tokens: session.accumulated_output_tokens.unwrap_or(0),
            accumulated_total_tokens: session.accumulated_total_tokens.unwrap_or(0),
        }
    }
}

/// SQLite row shape for the BR-43 `checkpoints` table, mapped to the public
/// `checkpoint::CheckpointRecord`.
#[derive(sqlx::FromRow)]
struct CheckpointRow {
    id: String,
    session_id: String,
    turn_index: i64,
    anchor_ts: i64,
    kind: String,
    commit_sha: String,
    tree_sha: String,
    changed_paths_json: String,
    created_at: String,
}

impl CheckpointRow {
    fn into_record(self) -> Result<crate::checkpoint::CheckpointRecord> {
        Ok(crate::checkpoint::CheckpointRecord {
            id: self.id,
            session_id: self.session_id,
            turn_index: self.turn_index,
            anchor_ts: self.anchor_ts,
            kind: self.kind.parse()?,
            commit_sha: self.commit_sha,
            tree_sha: self.tree_sha,
            changed_paths: serde_json::from_str(&self.changed_paths_json).unwrap_or_default(),
            created_at: self.created_at,
        })
    }
}

/// A monotone revision marker for one session's stored message set.
///
/// `messages.id` is `INTEGER PRIMARY KEY AUTOINCREMENT`, so SQLite keeps a
/// high-water mark in `sqlite_sequence` and never reuses a rowid. Every append
/// raises `max_rowid`. [`SessionManager::replace_conversation`] DELETEs and
/// re-INSERTs the whole set, so a rewrite raises it too — even when the content
/// is byte-identical. A delete lowers `count` and frees rowids that are never
/// minted again. The pair therefore cannot ABA, which a message count alone
/// demonstrably can: an edit that drops one message plus the next turn's user
/// message nets to zero.
///
/// Read through the `idx_messages_session` covering index, so it is index-only:
/// no table rows, no JSON. Cheaper than the `COUNT(*)` `get_session` already
/// does.
///
/// `(count, max_rowid)` is only non-repeating within ONE INCARNATION of the
/// session row, which is why `incarnation` is part of the token. A session id
/// is REUSABLE: `create_session` allocates `YYYYMMDD_N` as `MAX(N) + 1` over
/// the `sessions` table, so once that table is emptied the ids restart at 1.
/// Pair that with a rewound message sequence and a one-message session at
/// `(1, 1)` is reproducible by an entirely different conversation — a
/// detached rewrite from the previous incarnation would then pass the prefix
/// check and overwrite it (#51 W3). `incarnation` is minted per session ROW
/// from `random()` and never reused, so a basis taken before a wipe can never
/// match after one, whatever the rowids do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConversationRevision {
    /// Identity of the session ROW this revision was read from. `0` means
    /// "unknown" — a row written before the column existed and somehow missed
    /// the backfill. Two unknowns compare equal, degrading to the rowid guard
    /// alone rather than refusing every rewrite on such a database.
    incarnation: i64,
    count: i64,
    max_rowid: i64,
}

impl ConversationRevision {
    /// How many messages the session held at this revision.
    pub fn message_count(&self) -> usize {
        self.count.max(0) as usize
    }

    #[cfg(test)]
    pub(crate) fn from_parts(incarnation: i64, count: i64, max_rowid: i64) -> Self {
        Self {
            incarnation,
            count,
            max_rowid,
        }
    }
}

/// Outcome of [`SessionManager::replace_conversation_preserving_tail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceOutcome {
    /// Nothing landed since the basis; the replacement was stored verbatim.
    Replaced,
    /// Messages the caller had never seen landed since the basis. They were
    /// carried over onto the tail of the replacement instead of being deleted.
    ReplacedPreservingTail { preserved: usize },
    /// The basis itself was truncated or wholesale-rewritten underneath us, so
    /// there is no sound prefix to merge onto. NOTHING was written.
    Stale,
    /// No session with that id. NOTHING was written.
    SessionNotFound,
}

impl ReplaceOutcome {
    /// Did the rewrite actually land? `false` means the store is untouched.
    pub fn stored(&self) -> bool {
        matches!(
            self,
            ReplaceOutcome::Replaced | ReplaceOutcome::ReplacedPreservingTail { .. }
        )
    }
}

/// Outcome of [`SessionManager::truncate_conversation_bounded`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruncateOutcome {
    /// The cut landed; `removed` message rows went with it.
    Truncated { removed: usize },
    /// The basis came from a previous incarnation of this session id, so its
    /// rowid watermark describes a conversation that no longer exists. NOTHING
    /// was deleted.
    Stale,
    /// No session with that id. NOTHING was deleted.
    SessionNotFound,
}

/// Outcome of [`SessionManager::try_update_working_dir_if_empty`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingDirUpdate {
    /// The session had no messages; the working dir was updated.
    Updated,
    /// The session already has at least one message; the working dir is fixed
    /// (#44) and was left untouched.
    RefusedNotEmpty,
    /// No session with that id exists; nothing was written.
    SessionNotFound,
}

pub struct SessionManager {
    storage: Arc<SessionStorage>,
}

impl SessionManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            storage: Arc::new(SessionStorage::new(data_dir)),
        }
    }

    pub fn instance() -> Self {
        Self {
            storage: Arc::clone(&SESSION_STORAGE),
        }
    }

    pub fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }

    pub async fn create_session(
        &self,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
    ) -> Result<Session> {
        self.storage
            .create_session(working_dir, name, session_type)
            .await
    }

    pub async fn get_session(&self, id: &str, include_messages: bool) -> Result<Session> {
        self.storage.get_session(id, include_messages).await
    }

    /// Resume (or create + bind) a durable session keyed by a stable external
    /// handle such as `"app:<app-id>:<client-id>"`. Returns `(session, resumed)`
    /// where `resumed` is true when an existing session was reused. Backs the
    /// BRSDK durable-app-session feature.
    pub async fn get_or_create_by_external_key(
        &self,
        external_key: &str,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
    ) -> Result<(Session, bool)> {
        self.storage
            .get_or_create_by_external_key(external_key, working_dir, name, session_type)
            .await
    }

    /// Fetch only the session's token counters, without the `COUNT(*)` over the
    /// messages table or deserializing the heavy metadata columns that
    /// `get_session` parses. Used on the per-streamed-event hot path where the
    /// message count and metadata are irrelevant.
    pub async fn get_token_counts(&self, id: &str) -> Result<SessionTokenCounts> {
        self.storage.get_token_counts(id).await
    }

    pub fn update(&self, id: &str) -> SessionUpdateBuilder<'_> {
        SessionUpdateBuilder::new(self, id.to_string())
    }

    /// Set the session's working directory **only if the chat is still empty**
    /// (#44), as one atomic conditional `UPDATE`: the emptiness check is the
    /// statement's own `WHERE NOT EXISTS (…messages…)` clause, so a first
    /// message landing concurrently can never slip between a check and a
    /// write — the update either sees no messages and applies, or sees the
    /// message and refuses. (The previous read-count-then-write sequence ran
    /// as two statements and had exactly that TOCTOU window.)
    ///
    /// This closes the *persisted-state* race only. It cannot order itself
    /// against a turn that has been accepted but has not yet persisted its
    /// user message; callers that can (the HTTP route) additionally hold the
    /// per-session turn guard across the update + agent restart.
    pub async fn try_update_working_dir_if_empty(
        &self,
        id: &str,
        working_dir: PathBuf,
    ) -> Result<WorkingDirUpdate> {
        self.storage
            .try_update_working_dir_if_empty(id, &working_dir)
            .await
    }

    /// Set the session's working directory **unconditionally**, bypassing the
    /// empty-chat-only guard of [`Self::try_update_working_dir_if_empty`].
    ///
    /// ONLY for the terminal shell-following path (`biorouter term run`),
    /// where the session's dir intentionally tracks the user's shell cwd
    /// mid-conversation. Every other caller must use the guarded method — a
    /// mid-chat switch breaks the session's own history (#44), which is why
    /// this is the sole unguarded writer and is named to make misuse obvious.
    pub async fn force_update_working_dir_unguarded(
        &self,
        id: &str,
        working_dir: PathBuf,
    ) -> Result<()> {
        let mut builder = self.update(id);
        builder.working_dir = Some(working_dir);
        builder.apply().await
    }

    async fn apply_update_inner(&self, builder: SessionUpdateBuilder<'_>) -> Result<()> {
        self.storage.apply_update(builder).await
    }

    /// Persist one message, returning the **effective** `msg_uid` it was stored
    /// under. Usually the message's own id (or a freshly minted one when the
    /// caller supplied none), but a uid collision re-mints — callers that keep
    /// the message in memory must adopt the returned uid so the in-memory and
    /// persisted ids agree (#41).
    /// Close the underlying SQLite pool, releasing the store's file handles
    /// (the db plus its WAL/-shm siblings). Ordering seam for #31: a private
    /// `--no-session` store's temp directory can only be deleted reliably on
    /// platforms where unlinking open files fails (Windows) if the pool is
    /// closed FIRST. Every later store operation fails with a pool-closed
    /// error, so call this only when the run is done with the store.
    pub async fn close(&self) {
        self.storage.close().await;
    }

    pub async fn add_message(&self, id: &str, message: &Message) -> Result<String> {
        self.storage.add_message(id, message).await
    }

    /// [`Self::add_message`] that also stamps the EFFECTIVE uid onto the
    /// caller's in-memory message (#41). The store mints a uid for idless
    /// messages and re-mints on a collision; a retained copy that keeps
    /// `id: None` (or a stale id) desynchronizes memory from storage — its
    /// next persist would insert a duplicate row under a fresh uid instead
    /// of being recognized as a replay. Use this on every path that both
    /// persists a message AND keeps/yields it.
    pub async fn add_message_adopting_uid(&self, id: &str, message: &mut Message) -> Result<()> {
        let effective_uid = self.storage.add_message(id, message).await?;
        if message.id.as_deref() != Some(effective_uid.as_str()) {
            message.id = Some(effective_uid);
        }
        Ok(())
    }

    /// Unconditional whole-history rewrite: DELETE every message of the session
    /// and re-INSERT the supplied ones.
    ///
    /// This is the NAMED EXCEPTION. It is only correct for a caller that
    /// genuinely owns the whole history — `/clear`, and the import/copy/diverge
    /// paths that write into a session they just created. A caller that
    /// computed `conversation` from a snapshot of a *live* session must use
    /// [`Self::replace_conversation_preserving_tail`] instead: anything another
    /// writer appended in between is destroyed here, silently, after that
    /// writer was already told its append succeeded.
    pub async fn replace_conversation(&self, id: &str, conversation: &Conversation) -> Result<()> {
        self.storage.replace_conversation(id, conversation).await
    }

    /// The current revision of a session's stored message set (see
    /// [`ConversationRevision`]). Cheap: one covering-index aggregate.
    pub async fn conversation_revision(&self, id: &str) -> Result<ConversationRevision> {
        self.storage.conversation_revision(id).await
    }

    /// Snapshot a session for a whole-history rewrite: its conversation plus the
    /// revision that view is based on.
    ///
    /// Reads the REVISION FIRST, then the conversation. A message landing
    /// between the two reads is then inside the returned conversation (so the
    /// rewrite already accounts for it) rather than looking foreign. Reading in
    /// the other order would leave such a message absent from the caller's view
    /// *and* at `id <= max_rowid`, i.e. invisible to tail recovery — a silent
    /// loss. The ordering is load-bearing.
    pub async fn snapshot_for_rewrite(&self, id: &str) -> Result<(Session, ConversationRevision)> {
        let revision = self.storage.conversation_revision(id).await?;
        let session = self.storage.get_session(id, true).await?;
        Ok((session, revision))
    }

    /// Whole-history rewrite that is safe against a concurrent append.
    ///
    /// `known` is the conversation `replacement` was derived from; `basis` is
    /// the revision at the moment that view began (both come from
    /// [`Self::snapshot_for_rewrite`]). Messages stored since `basis` whose ids
    /// are not in `known` are FOREIGN — another writer appended them while this
    /// caller was computing its rewrite — and are carried over onto the end of
    /// `replacement` instead of being destroyed.
    ///
    /// The check runs inside the rewrite's own transaction, under the write
    /// lock its first statement takes, so there is no window between the check
    /// and the DELETE at any timescale.
    ///
    /// Returns what was ACTUALLY stored, so the caller can keep its in-memory
    /// conversation, the database and any `HistoryReplaced` event in agreement.
    /// On [`ReplaceOutcome::Stale`] / [`ReplaceOutcome::SessionNotFound`]
    /// nothing was written and the returned conversation is `replacement`
    /// unchanged.
    ///
    /// Only a genuine basis mismatch is reported as `Stale`. A `SQLITE_BUSY`, an
    /// I/O error or a full disk propagates as `Err` — reporting a busy database
    /// as "stale" would look like data loss, and reporting it as "written"
    /// would be data loss.
    ///
    /// This makes concurrent writes SAFE, not RARE. Two turns on one session
    /// (two app sockets, or the CLI and the daemon on the same `sessions.db`)
    /// still both run; one of them now finds out it lost instead of silently
    /// deleting the other's messages.
    pub async fn replace_conversation_preserving_tail(
        &self,
        id: &str,
        replacement: &Conversation,
        basis: ConversationRevision,
        known: &Conversation,
    ) -> Result<(ReplaceOutcome, Conversation)> {
        self.storage
            .replace_conversation_preserving_tail(id, replacement, basis, known)
            .await
    }

    /// Fetch one externalized tool-result payload by its blob handle (BR-7).
    /// `None` when the handle is unknown to this session — blobs are scoped to
    /// the session that stored them, so one session can never read another's
    /// tool output through a guessed (or model-hallucinated) handle.
    pub async fn get_message_blob(
        &self,
        session_id: &str,
        blob_uid: &str,
    ) -> Result<Option<String>> {
        self.storage.get_message_blob(session_id, blob_uid).await
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.storage.list_sessions().await
    }

    pub async fn list_session_summaries(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SessionSummary>> {
        self.storage.list_session_summaries(limit, offset).await
    }

    pub async fn list_sessions_by_types(&self, types: &[SessionType]) -> Result<Vec<Session>> {
        self.storage.list_sessions_by_types(types).await
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        self.storage.delete_session(id).await
    }

    pub async fn clear_all_sessions(&self) -> Result<u64> {
        self.storage.clear_all_sessions().await
    }

    pub async fn count_all_sessions(&self) -> Result<u64> {
        self.storage.count_all_sessions().await
    }

    pub async fn get_insights(&self) -> Result<SessionInsights> {
        self.storage.get_insights().await
    }

    /// Per-day usage for the Home heatmap, over the last `days` calendar days.
    pub async fn get_activity(&self, days: i64) -> Result<ActivityWindow> {
        self.storage.get_activity(days).await
    }

    /// Append one turn's usage to the per-turn token ledger.
    ///
    /// `model` / `provider` attribute the turn for the per-model breakdown; pass
    /// `None` when the provider did not report a model (the row then aggregates
    /// under the 'unknown' group).
    #[allow(clippy::too_many_arguments)]
    pub async fn record_token_event(
        &self,
        session_id: &str,
        input: Option<i32>,
        output: Option<i32>,
        total: i64,
        model: Option<&str>,
        provider: Option<&str>,
        cache_read: Option<i32>,
        cache_creation: Option<i32>,
    ) -> Result<()> {
        self.storage
            .record_token_event(
                session_id,
                input,
                output,
                total,
                model,
                provider,
                cache_read,
                cache_creation,
            )
            .await
    }

    /// Atomically append a production usage event and apply the same event to
    /// the session's lifetime counters. Reusing `event_key` is a no-op, which
    /// makes retrying an ambiguous database result safe.
    pub async fn apply_usage_event(&self, entry: UsageLedgerEntry) -> Result<bool> {
        self.storage.apply_usage_event(entry).await
    }

    /// Per-model usage rollup for one session (for the cost popover breakdown).
    pub async fn get_session_model_usage(&self, session_id: &str) -> Result<Vec<ModelUsageRow>> {
        self.storage.get_session_model_usage(session_id).await
    }

    /// Global per-model usage rollup over `[from, to]` (inclusive, unix seconds).
    pub async fn get_model_usage(&self, from: i64, to: i64) -> Result<Vec<ModelUsageRow>> {
        self.storage.get_model_usage(from, to).await
    }

    /// Queryable, server-priced usage report over `[from, to]` (inclusive, unix
    /// seconds), bucketed by day, model, or day×model.
    pub async fn get_usage_report(
        &self,
        from: i64,
        to: i64,
        group: UsageGroup,
    ) -> Result<Vec<UsageReportRow>> {
        self.storage.get_usage_report(from, to, group).await
    }

    /// Month-to-date + all-time priced usage totals, for the summary gauge.
    pub async fn get_usage_summary(&self) -> Result<UsageSummary> {
        self.storage.get_usage_summary().await
    }

    pub async fn export_session(&self, id: &str) -> Result<String> {
        self.storage.export_session(id).await
    }

    pub async fn import_session(&self, json: &str) -> Result<Session> {
        self.storage.import_session(self, json).await
    }

    pub async fn copy_session(&self, session_id: &str, new_name: String) -> Result<Session> {
        self.storage.copy_session(self, session_id, new_name).await
    }

    /// Diverge before an edited user message, using the standard divergence
    /// naming and lineage rules while preserving the edit flow's truncation
    /// semantics.
    pub async fn diverge_session_for_edit(
        &self,
        session_id: &str,
        timestamp: i64,
    ) -> Result<Session> {
        self.storage
            .diverge_session_for_edit(self, session_id, timestamp)
            .await
    }

    /// Diverge (branch) a session: copy the full conversation into a fresh
    /// session that records its lineage (`diverged_from`) and gets a
    /// human-friendly, collision-free branch name.
    ///
    /// Naming:
    /// - `custom_name` (when non-blank) is used verbatim.
    /// - Otherwise the name is `"{base} (branch {N})"`, where `base` is the
    ///   parent's name (a placeholder like "New Session" is replaced with a
    ///   title derived from the conversation) with any existing `(branch K)`
    ///   suffix stripped, and `N` is the next free index across that family.
    ///
    /// The branch name is locked (`user_set_name = true`) so the auto-namer
    /// never overwrites the marker. Shared by the `/sessions/{id}/diverge`
    /// route and the CLI/TUI `/diverge` command.
    ///
    /// The branch conversation is trimmed to end at the last *complete*
    /// assistant answer (see `trim_to_last_complete_answer_at`), so a diverge
    /// triggered while the agent is still generating or calling tools never
    /// leaves a dangling, unanswered turn in the new session. `anchor_ms` (the
    /// `created` timestamp of the message a per-message Diverge button was
    /// clicked on) bounds the branch to that point; `None` uses the most recent
    /// complete answer.
    pub async fn diverge_session(
        &self,
        session_id: &str,
        custom_name: Option<String>,
        anchor_ms: Option<i64>,
    ) -> Result<Session> {
        self.diverge_session_at(session_id, custom_name, anchor_ms, None)
            .await
    }

    /// Diverge anchored by a durable message id (`anchor_uid`), the BR-45 divergence
    /// point. Preferred over the timestamp anchor: it is unambiguous when two
    /// messages share a whole second, and it records `branch_point_msg_uid` on
    /// the child. `anchor_ms` is kept as a back-compatible fallback for clients
    /// that still pass a timestamp.
    pub async fn diverge_session_at(
        &self,
        session_id: &str,
        custom_name: Option<String>,
        anchor_ms: Option<i64>,
        anchor_uid: Option<String>,
    ) -> Result<Session> {
        self.storage
            .diverge_session(self, session_id, custom_name, anchor_ms, anchor_uid)
            .await
    }

    /// Drop every message at or after `timestamp` — checkpoint restore and the
    /// message-edit flow.
    ///
    /// The range is open above, so it also takes anything appended between the
    /// caller reading the conversation and this call landing. That is only
    /// sound for a caller that owns the whole tail (the edit flow's
    /// just-created divergence). A caller working from a snapshot of a LIVE
    /// session should hold on to the revision it read and use
    /// [`Self::truncate_conversation_bounded`], which cuts only as far as that
    /// view reached.
    pub async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.storage
            .truncate_conversation(session_id, timestamp)
            .await
    }

    /// [`Self::truncate_conversation`] bounded by the caller's own view.
    ///
    /// `basis` is the revision the decision to cut at `timestamp` was made
    /// from (see [`Self::snapshot_for_rewrite`] /
    /// [`Self::conversation_revision`]). Messages stored above its watermark
    /// were appended after that view and are KEPT: they are not part of the
    /// tail the caller asked to drop, and their writer has already been told
    /// the append succeeded.
    ///
    /// A basis from a previous incarnation of this session id is refused
    /// outright — its watermark describes rowids that belonged to a different
    /// conversation (see [`ConversationRevision`]).
    pub async fn truncate_conversation_bounded(
        &self,
        session_id: &str,
        timestamp: i64,
        basis: ConversationRevision,
    ) -> Result<TruncateOutcome> {
        self.storage
            .truncate_conversation_bounded(session_id, timestamp, basis)
            .await
    }

    // BR-43 shadow-git checkpoints: `checkpoints` table access, delegated to
    // `SessionStorage` (which owns the pool) and called by `CheckpointManager`.

    pub async fn insert_checkpoint(&self, rec: &crate::checkpoint::CheckpointRecord) -> Result<()> {
        self.storage.insert_checkpoint(rec).await
    }

    pub async fn list_checkpoints(
        &self,
        session_id: &str,
    ) -> Result<Vec<crate::checkpoint::CheckpointRecord>> {
        self.storage.list_checkpoints(session_id).await
    }

    pub async fn last_checkpoint(
        &self,
        session_id: &str,
    ) -> Result<Option<crate::checkpoint::CheckpointRecord>> {
        self.storage.last_checkpoint(session_id).await
    }

    pub async fn get_checkpoint(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<Option<crate::checkpoint::CheckpointRecord>> {
        self.storage.get_checkpoint(session_id, checkpoint_id).await
    }

    pub async fn delete_checkpoints(&self, session_id: &str) -> Result<()> {
        self.storage.delete_checkpoints(session_id).await
    }

    pub async fn maybe_update_name(&self, id: &str, provider: Arc<dyn Provider>) -> Result<()> {
        let session = self.get_session(id, true).await?;

        // The user explicitly named the session — never override.
        if session.user_set_name {
            return Ok(());
        }

        // Whether the session is still on a placeholder title. A session that
        // already has a real, content-derived name should stop regenerating it
        // after the first few turns (no churn); a session still showing the
        // "New Session" placeholder must keep getting a chance to be named on
        // every turn, no matter how long it grows — otherwise an early naming
        // miss (e.g. an interrupted or errored first turn) leaves it stuck on
        // "New Session" forever once it crosses the message-count threshold.
        let still_default = is_default_session_name(&session.name);

        let conversation = session
            .conversation
            .ok_or_else(|| anyhow::anyhow!("No messages found"))?;

        let user_message_count = conversation
            .messages()
            .iter()
            .filter(|m| matches!(m.role, Role::User))
            .count();

        // No real exchange yet — nothing to name from.
        if user_message_count == 0 {
            return Ok(());
        }

        // After the first few exchanges the name has settled; stop regenerating
        // it so later turns don't churn the title — but only once it actually
        // has a real name. While still on the placeholder, keep trying.
        if user_message_count > MSG_COUNT_FOR_SESSION_NAME_GENERATION && !still_default {
            return Ok(());
        }

        // Prefer the LLM-generated, content-derived name. The naming call is
        // best-effort: if the provider errors (rate limit, auth, model issue)
        // or hands back an empty/whitespace string, fall back to a
        // deterministic title derived from the first user message so a session
        // is NEVER left as "New Session" after a real exchange.
        let name = match provider.generate_session_name(&conversation).await {
            Ok(name) if !name.trim().is_empty() => name,
            Ok(_) => {
                warn!(
                    "Session name generation for {} returned an empty name; using fallback",
                    id
                );
                Self::fallback_session_name(&conversation)
            }
            Err(e) => {
                warn!(
                    "Session name generation for {} failed ({}); using fallback",
                    id, e
                );
                Self::fallback_session_name(&conversation)
            }
        };

        // Both the LLM and the fallback produced nothing usable (e.g. the first
        // user message was only attachments). Leave the placeholder rather than
        // blanking the name.
        if name.trim().is_empty() {
            return Ok(());
        }

        self.update(id).system_generated_name(name).apply().await
    }

    /// Derive a short, deterministic session title from the first user message.
    /// Used as a fallback when the LLM-based namer is unavailable so a session
    /// with a real exchange never stays as the "New Session" placeholder.
    fn fallback_session_name(conversation: &Conversation) -> String {
        let first_user_text = conversation
            .messages()
            .iter()
            .find(|m| matches!(m.role, Role::User))
            .map(|m| m.as_concat_text())
            .unwrap_or_default();

        // Collapse whitespace and keep the leading words so the title fits the
        // limited UI space (mirrors the LLM namer's "4 words or less" intent).
        let snippet = first_user_text
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join(" ");

        crate::utils::safe_truncate(&snippet, 60)
    }

    pub async fn search_chat_history(
        &self,
        query: &str,
        limit: Option<usize>,
        after_date: Option<chrono::DateTime<chrono::Utc>>,
        before_date: Option<chrono::DateTime<chrono::Utc>>,
        exclude_session_id: Option<String>,
    ) -> Result<crate::session::chat_history_search::ChatRecallResults> {
        self.storage
            .search_chat_history(query, limit, after_date, before_date, exclude_session_id)
            .await
    }
}

pub struct SessionStorage {
    pool: Pool<Sqlite>,
    initialized: tokio::sync::OnceCell<()>,
    session_dir: PathBuf,
}

/// How `replace_conversation_inner` treats concurrent writers.
enum RewriteGuard<'a> {
    /// Overwrite whatever is there. Only for a caller that owns the whole
    /// history: `/clear`, and writes into a session it just created.
    Unconditional,
    /// Refuse if the basis moved out from under us, and carry over anything
    /// appended since it that the caller never saw.
    PreserveTail {
        basis: ConversationRevision,
        known: &'a Conversation,
    },
}

fn role_to_string(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: String::new(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            name: String::new(),
            user_set_name: false,
            session_type: SessionType::default(),
            created_at: Default::default(),
            updated_at: Default::default(),
            extension_data: ExtensionData::default(),
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            accumulated_total_tokens: None,
            accumulated_input_tokens: None,
            accumulated_output_tokens: None,
            schedule_id: None,
            workflow: None,
            user_workflow_values: None,
            conversation: None,
            message_count: 0,
            provider_name: None,
            model_config: None,
            diverged_from: None,
            branch_point_msg_uid: None,
        }
    }
}

impl Session {
    pub fn without_messages(mut self) -> Self {
        self.conversation = None;
        self
    }
}

/// True when `name` is a placeholder title (empty, "New Session", "CLI
/// Session", "New session N", "Session N") rather than a meaningful name —
/// mirrors the frontend `isDefaultSessionName`. Used so a diverged branch
/// doesn't inherit a useless placeholder.
pub(crate) fn is_default_session_name(name: &str) -> bool {
    let n = name.trim();
    if n.is_empty() {
        return true;
    }
    if n.eq_ignore_ascii_case("New Session") || n.eq_ignore_ascii_case("CLI Session") {
        return true;
    }
    // "New session <N>" or "Session <N>" (trailing digits).
    let lower = n.to_ascii_lowercase();
    for prefix in ["new session ", "session "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Strip a trailing `" (branch <digits>)"` from a name so branching a branch
/// re-numbers within the same family instead of nesting suffixes
/// ("Foo (branch 1)" → "Foo", then the next branch becomes "Foo (branch 2)").
pub(crate) fn strip_branch_suffix(name: &str) -> &str {
    let trimmed = name.trim_end();
    if let Some(idx) = trimmed.rfind(" (branch ") {
        let (base, suffix) = trimmed.split_at(idx);
        let inner = suffix.strip_prefix(" (branch ").unwrap_or(suffix);
        if let Some(digits) = inner.strip_suffix(')') {
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                return base.trim_end();
            }
        }
    }
    trimmed
}

/// Escape SQL LIKE wildcards in a literal so a name containing `%` or `_`
/// (or `\`) is matched literally. Pair with `ESCAPE '\'`.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// True when `m` is a *complete* assistant answer: an assistant message that
/// carries real text and has no pending tool call. These are the only points a
/// branch should end on — everything after the last one (an unanswered user
/// question, an empty "about to call a tool" assistant turn, a tool
/// request/response still mid-flight) is an in-progress exchange.
fn is_assistant_terminal_answer(m: &Message) -> bool {
    m.role == Role::Assistant && !m.is_tool_call() && !m.as_concat_text().trim().is_empty()
}

/// Trim a conversation for a diverged branch so it ends at the last complete
/// assistant answer. A diverge fired while the agent is still generating or
/// calling tools therefore branches from the previous finished response rather
/// than leaving a dangling, unanswered turn.
///
/// `anchor_ms` bounds the branch to messages created at or before it — used by
/// the per-message Diverge button to branch from exactly that answer. With
/// `None`, the most recent complete answer in the whole conversation is used.
/// If there is no complete answer at all (e.g. diverged before the very first
/// reply landed), the branch starts empty rather than carrying an orphaned
/// question.
/// Trim a branch to end at the last *complete* assistant answer within the
/// prefix up to (and including) the anchor.
///
/// The anchor is resolved by durable message id first (`anchor_uid`), which is
/// unambiguous even when two messages share a whole-second `created` — the
/// same-second collision that `anchor_ms` (`m.created <= ts`, inclusive) could
/// over-truncate (BR-45). A missing/unknown `anchor_uid` falls back to the
/// timestamp anchor, and `None`/`None` keeps the whole conversation.
pub(crate) fn trim_to_last_complete_answer_at(
    conversation: &Conversation,
    anchor_uid: Option<&str>,
    anchor_ms: Option<i64>,
) -> Conversation {
    let msgs = conversation.messages();
    let kept: Vec<&Message> =
        match anchor_uid.and_then(|uid| msgs.iter().position(|m| m.id.as_deref() == Some(uid))) {
            Some(end) => msgs[..=end].iter().collect(),
            None => msgs
                .iter()
                .filter(|m| anchor_ms.is_none_or(|ts| m.created <= ts))
                .collect(),
        };

    match kept.iter().rposition(|m| is_assistant_terminal_answer(m)) {
        Some(end) => Conversation::new_unvalidated(
            kept[..=end]
                .iter()
                .map(|m| (*m).clone())
                .collect::<Vec<Message>>(),
        ),
        None => Conversation::default(),
    }
}

impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for Session {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        let workflow_json: Option<String> = row.try_get("workflow_json")?;
        let workflow = workflow_json.and_then(|json| serde_json::from_str(&json).ok());

        let user_workflow_values_json: Option<String> = row.try_get("user_workflow_values_json")?;
        let user_workflow_values =
            user_workflow_values_json.and_then(|json| serde_json::from_str(&json).ok());

        let model_config_json: Option<String> = row.try_get("model_config_json").ok().flatten();
        let model_config = model_config_json.and_then(|json| serde_json::from_str(&json).ok());

        let name: String = {
            let name_val: String = row.try_get("name").unwrap_or_default();
            if !name_val.is_empty() {
                name_val
            } else {
                row.try_get("description").unwrap_or_default()
            }
        };

        let user_set_name = row.try_get("user_set_name").unwrap_or(false);

        let session_type_str: String = row
            .try_get("session_type")
            .unwrap_or_else(|_| "user".to_string());
        let session_type = session_type_str.parse().unwrap_or_default();

        Ok(Session {
            id: row.try_get("id")?,
            working_dir: PathBuf::from(row.try_get::<String, _>("working_dir")?),
            name,
            user_set_name,
            session_type,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            extension_data: serde_json::from_str(&row.try_get::<String, _>("extension_data")?)
                .unwrap_or_default(),
            total_tokens: row.try_get("total_tokens")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            accumulated_total_tokens: row.try_get("accumulated_total_tokens")?,
            accumulated_input_tokens: row.try_get("accumulated_input_tokens")?,
            accumulated_output_tokens: row.try_get("accumulated_output_tokens")?,
            schedule_id: row.try_get("schedule_id")?,
            workflow,
            user_workflow_values,
            conversation: None,
            message_count: row.try_get("message_count").unwrap_or(0) as usize,
            provider_name: row.try_get("provider_name").ok().flatten(),
            model_config,
            diverged_from: row.try_get("diverged_from").ok().flatten(),
            // Tolerant read: SELECTs that omit the column (e.g. the session
            // list) yield None rather than erroring, mirroring `model_config`.
            branch_point_msg_uid: row.try_get("branch_point_msg_uid").ok().flatten(),
        })
    }
}

impl SessionStorage {
    fn create_pool(path: &Path) -> Pool<Sqlite> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create session database directory");
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .busy_timeout(std::time::Duration::from_secs(5))
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            // Under WAL, NORMAL is durable across application crashes and only
            // risks the last commit on an OS/power crash, while avoiding an
            // fsync on every commit (the SQLite default is FULL). This removes
            // a per-message-write fsync from the agent hot path.
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        // SQLite serializes writes on a single write lock; fanning out to many
        // writer connections just produces lock contention rather than
        // parallelism. Cap the pool deliberately instead of inheriting sqlx's
        // default of 10.
        SqlitePoolOptions::new()
            .max_connections(4)
            .connect_lazy_with(options)
    }

    pub fn new(data_dir: PathBuf) -> Self {
        let session_dir = data_dir.join(SESSIONS_FOLDER);
        let db_path = session_dir.join(DB_NAME);
        Self {
            pool: Self::create_pool(&db_path),
            initialized: tokio::sync::OnceCell::new(),
            session_dir,
        }
    }

    /// Close the SQLite pool (see [`SessionManager::close`]). Safe to call
    /// even if the lazy pool never connected; idempotent.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn pool(&self) -> Result<&Pool<Sqlite>> {
        self.initialized
            .get_or_try_init(|| async {
                let schema_exists = sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version')"#,
                )
                .fetch_one(&self.pool)
                .await
                .unwrap_or(false);

                if schema_exists {
                    Self::run_migrations(&self.pool).await?;
                } else {
                    Self::create_schema(&self.pool).await?;
                    if let Err(e) = Self::import_legacy(&self.pool, &self.session_dir).await {
                        warn!("Failed to import some legacy sessions: {}", e);
                    }
                }
                Ok::<(), anyhow::Error>(())
            })
            .await?;
        Ok(&self.pool)
    }

    pub async fn create(session_dir: &Path) -> Result<Self> {
        let storage = Self::new(session_dir.to_path_buf());
        Self::create_schema(&storage.pool).await?;
        Ok(storage)
    }

    async fn create_schema(pool: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(CURRENT_SCHEMA_VERSION)
            .execute(pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                user_set_name BOOLEAN DEFAULT FALSE,
                session_type TEXT NOT NULL DEFAULT 'user',
                working_dir TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                extension_data TEXT DEFAULT '{}',
                total_tokens INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                accumulated_total_tokens INTEGER,
                accumulated_input_tokens INTEGER,
                accumulated_output_tokens INTEGER,
                schedule_id TEXT,
                workflow_json TEXT,
                user_workflow_values_json TEXT,
                provider_name TEXT,
                model_config_json TEXT,
                diverged_from TEXT,
                external_key TEXT,
                branch_point_msg_uid TEXT,
                incarnation INTEGER NOT NULL DEFAULT 0
            )
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                role TEXT NOT NULL,
                content_json TEXT NOT NULL,
                created_timestamp INTEGER NOT NULL,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                tokens INTEGER,
                metadata_json TEXT,
                msg_uid TEXT
            )
        "#,
        )
        .execute(pool)
        .await?;

        Self::create_usage_schema(pool).await?;

        // BR-43 shadow-git checkpoints (migration 13), created inline for fresh DBs.
        Self::create_checkpoints_table(pool).await?;

        // BR-7 externalized tool-result payloads (migration 16).
        Self::create_message_blobs_table(pool).await?;

        sqlx::query("CREATE INDEX idx_messages_session ON messages(session_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_messages_timestamp ON messages(timestamp)")
            .execute(pool)
            .await?;
        // BR-45: durable per-message id, unique within a session (ids are
        // intentionally carried into a diverged child, so uniqueness is
        // per-session, not global).
        sqlx::query("CREATE UNIQUE INDEX idx_messages_uid ON messages(session_id, msg_uid)")
            .execute(pool)
            .await?;

        // FTS5 index for relevance-ranked chat recall (BR-17). See migration 15
        // for details; a fresh DB starts already indexed.
        sqlx::query(MESSAGES_FTS_DDL).execute(pool).await?;
        sqlx::query("CREATE INDEX idx_sessions_updated ON sessions(updated_at DESC)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_sessions_type ON sessions(session_type)")
            .execute(pool)
            .await?;
        // BRSDK: stable external handle for durable, resumable app sessions
        // (e.g. "app:<app-id>:<client-id>"). Unique so a reconnecting client
        // resolves back to its existing session.
        sqlx::query(
            "CREATE UNIQUE INDEX idx_sessions_external_key ON sessions(external_key) WHERE external_key IS NOT NULL",
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn create_usage_schema(pool: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE token_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                ts INTEGER NOT NULL,
                input_tokens INTEGER,
                output_tokens INTEGER,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                billed_total_tokens INTEGER,
                model_id TEXT,
                provider TEXT,
                cache_read_tokens INTEGER,
                cache_creation_tokens INTEGER,
                event_key TEXT,
                session_type TEXT
            )
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX idx_token_events_ts ON token_events(ts)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_token_events_session ON token_events(session_id, ts)")
            .execute(pool)
            .await?;
        sqlx::query(
            "CREATE UNIQUE INDEX idx_token_events_event_key ON token_events(event_key) WHERE event_key IS NOT NULL",
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn import_legacy(pool: &Pool<Sqlite>, session_dir: &PathBuf) -> Result<()> {
        use crate::session::legacy;

        let sessions = match legacy::list_sessions(session_dir) {
            Ok(sessions) => sessions,
            Err(_) => {
                warn!("No legacy sessions found to import");
                return Ok(());
            }
        };

        if sessions.is_empty() {
            return Ok(());
        }

        let mut imported_count = 0;
        let mut failed_count = 0;

        for (session_name, session_path) in sessions {
            match legacy::load_session(&session_name, &session_path) {
                Ok(session) => match Self::import_legacy_session(pool, &session).await {
                    Ok(_) => {
                        imported_count += 1;
                        info!("  ✓ Imported: {}", session_name);
                    }
                    Err(e) => {
                        failed_count += 1;
                        info!("  ✗ Failed to import {}: {}", session_name, e);
                    }
                },
                Err(e) => {
                    failed_count += 1;
                    info!("  ✗ Failed to load {}: {}", session_name, e);
                }
            }
        }

        info!(
            "Import complete: {} successful, {} failed",
            imported_count, failed_count
        );
        Ok(())
    }

    async fn import_legacy_session(pool: &Pool<Sqlite>, session: &Session) -> Result<()> {
        let mut tx = pool.begin().await?;

        let workflow_json = match &session.workflow {
            Some(workflow) => Some(serde_json::to_string(workflow)?),
            None => None,
        };

        let user_workflow_values_json = match &session.user_workflow_values {
            Some(user_workflow_values) => Some(serde_json::to_string(user_workflow_values)?),
            None => None,
        };

        let model_config_json = match &session.model_config {
            Some(model_config) => Some(serde_json::to_string(model_config)?),
            None => None,
        };

        sqlx::query(
            r#"
        INSERT INTO sessions (
            id, name, user_set_name, session_type, working_dir, created_at, updated_at, extension_data,
            total_tokens, input_tokens, output_tokens,
            accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens,
            schedule_id, workflow_json, user_workflow_values_json,
            provider_name, model_config_json, diverged_from, incarnation
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, random())
        "#,
        )
        .bind(&session.id)
        .bind(&session.name)
        .bind(session.user_set_name)
        .bind(session.session_type.to_string())
        .bind(session.working_dir.to_string_lossy().as_ref())
        .bind(session.created_at)
        .bind(session.updated_at)
        .bind(serde_json::to_string(&session.extension_data)?)
        .bind(session.total_tokens)
        .bind(session.input_tokens)
        .bind(session.output_tokens)
        .bind(session.accumulated_total_tokens)
        .bind(session.accumulated_input_tokens)
        .bind(session.accumulated_output_tokens)
        .bind(&session.schedule_id)
        .bind(workflow_json)
        .bind(user_workflow_values_json)
        .bind(&session.provider_name)
        .bind(model_config_json)
        .bind(&session.diverged_from)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        if let Some(conversation) = &session.conversation {
            Self::replace_conversation_inner(
                pool,
                &session.id,
                conversation,
                RewriteGuard::Unconditional,
            )
            .await?;

            // ...and put the historical mtime back. `replace_conversation_inner`
            // opens with `UPDATE sessions SET updated_at = datetime('now')` —
            // that write IS how the transaction takes SQLite's write lock up
            // front, so it is not optional and it beats the back-dated value
            // this function just INSERTed. Right for a live rewrite, wrong for
            // an import: without this every legacy JSONL session would sort as
            // "just now" under `ORDER BY updated_at DESC`, collapsing a user's
            // whole history to today in the session list on the one migration
            // run that imports it.
            sqlx::query("UPDATE sessions SET updated_at = ? WHERE id = ?")
                .bind(session.updated_at)
                .bind(&session.id)
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
        let current_version = Self::get_schema_version(pool).await?;

        if current_version < CURRENT_SCHEMA_VERSION {
            info!(
                "Running database migrations from v{} to v{}...",
                current_version, CURRENT_SCHEMA_VERSION
            );

            for version in (current_version + 1)..=CURRENT_SCHEMA_VERSION {
                info!("  Applying migration v{}...", version);
                Self::apply_migration(pool, version).await?;
                Self::update_schema_version(pool, version).await?;
                info!("  ✓ Migration v{} complete", version);
            }

            info!("All migrations complete");
        }

        // Development builds shipped overlapping v11-v14 migration numbers for
        // the usage and loop feature branches. Reconcile both additive schemas
        // from their actual table shapes so those databases upgrade safely even
        // when a version number caused one branch's migration arm to be skipped.
        Self::reconcile_usage_schema(pool).await?;
        Self::reconcile_loop_schema(pool).await?;

        Ok(())
    }

    async fn reconcile_loop_schema(pool: &Pool<Sqlite>) -> Result<()> {
        Self::create_checkpoints_table(pool).await?;
        Self::ensure_message_identity_schema(pool).await?;
        Self::ensure_session_incarnation_schema(pool).await?;
        Self::create_and_backfill_messages_fts(pool, false).await?;
        Self::create_message_blobs_table(pool).await?;
        Ok(())
    }

    async fn reconcile_usage_schema(pool: &Pool<Sqlite>) -> Result<()> {
        // BEGIN IMMEDIATE serializes the check-then-ALTER sequence across
        // concurrently running Biorouter processes. A deferred transaction lets
        // two readers both observe a missing column before either ALTERs it.
        let mut connection = pool.acquire().await?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await?;
        let result = Self::reconcile_usage_schema_locked(&mut connection).await;
        match result {
            Ok(()) => {
                sqlx::query("COMMIT").execute(&mut *connection).await?;
                Ok(())
            }
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                Err(error)
            }
        }
    }

    async fn reconcile_usage_schema_locked(connection: &mut sqlx::SqliteConnection) -> Result<()> {
        for (column, sql_type) in [
            ("model_id", "TEXT"),
            ("provider", "TEXT"),
            ("cache_read_tokens", "INTEGER"),
            ("cache_creation_tokens", "INTEGER"),
            ("billed_total_tokens", "INTEGER"),
            ("event_key", "TEXT"),
            ("session_type", "TEXT"),
        ] {
            let exists: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_table_info('token_events') WHERE name = ?1",
            )
            .bind(column)
            .fetch_one(&mut *connection)
            .await?;
            if exists == 0 {
                sqlx::query(&format!(
                    "ALTER TABLE token_events ADD COLUMN {column} {sql_type}"
                ))
                .execute(&mut *connection)
                .await?;
            }
        }

        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_token_events_event_key ON token_events(event_key) WHERE event_key IS NOT NULL",
        )
        .execute(&mut *connection)
        .await?;

        // Capture the session classification while it still exists. Rows whose
        // parent session was already deleted remain NULL and are conservatively
        // excluded from user/subagent spend rather than assumed billable.
        sqlx::query(
            r#"
            UPDATE token_events
            SET session_type = (
                SELECT s.session_type FROM sessions s WHERE s.id = token_events.session_id
            )
            WHERE session_type IS NULL
            "#,
        )
        .execute(&mut *connection)
        .await?;

        // Old v11/v12 development migrations copied each session's final model
        // backward and sometimes materialized unknown cache buckets as zero.
        // Without either a durable event identity or a billed total, there is no
        // trustworthy evidence that those values describe the original call.
        sqlx::query(
            r#"
            UPDATE token_events
            SET model_id = NULL,
                provider = NULL,
                cache_read_tokens = NULL,
                cache_creation_tokens = NULL
            WHERE event_key IS NULL
              AND billed_total_tokens IS NULL
              AND (model_id IS NOT NULL
                   OR provider IS NOT NULL
                   OR cache_read_tokens IS NOT NULL
                   OR cache_creation_tokens IS NOT NULL)
            "#,
        )
        .execute(&mut *connection)
        .await?;

        Ok(())
    }

    async fn get_schema_version(pool: &Pool<Sqlite>) -> Result<i32> {
        let table_exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT name FROM sqlite_master
                WHERE type='table' AND name='schema_version'
            )
        "#,
        )
        .fetch_one(pool)
        .await?;

        if !table_exists {
            return Ok(0);
        }

        let version = sqlx::query_scalar::<_, i32>("SELECT MAX(version) FROM schema_version")
            .fetch_one(pool)
            .await?;

        Ok(version)
    }

    async fn update_schema_version(pool: &Pool<Sqlite>, version: i32) -> Result<()> {
        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(version)
            .execute(pool)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn apply_migration(pool: &Pool<Sqlite>, version: i32) -> Result<()> {
        match version {
            1 => {
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS schema_version (
                        version INTEGER PRIMARY KEY,
                        applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    )
                "#,
                )
                .execute(pool)
                .await?;
            }
            2 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN user_workflow_values_json TEXT
                "#,
                )
                .execute(pool)
                .await?;
            }
            3 => {
                sqlx::query(
                    r#"
                    ALTER TABLE messages ADD COLUMN metadata_json TEXT
                "#,
                )
                .execute(pool)
                .await?;
            }
            4 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN name TEXT DEFAULT ''
                "#,
                )
                .execute(pool)
                .await?;

                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN user_set_name BOOLEAN DEFAULT FALSE
                "#,
                )
                .execute(pool)
                .await?;
            }
            5 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN session_type TEXT NOT NULL DEFAULT 'user'
                "#,
                )
                .execute(pool)
                .await?;

                sqlx::query("CREATE INDEX idx_sessions_type ON sessions(session_type)")
                    .execute(pool)
                    .await?;
            }
            6 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN provider_name TEXT
                "#,
                )
                .execute(pool)
                .await?;

                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN model_config_json TEXT
                "#,
                )
                .execute(pool)
                .await?;
            }
            7 => {
                // Rename pre-v1.50.0 columns: recipe_json → workflow_json
                // and user_recipe_values_json → user_workflow_values_json
                let recipe_col_count: i32 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'recipe_json'",
                )
                .fetch_one(pool)
                .await?;

                if recipe_col_count > 0 {
                    sqlx::query("ALTER TABLE sessions RENAME COLUMN recipe_json TO workflow_json")
                        .execute(pool)
                        .await?;
                }

                let user_recipe_col_count: i32 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'user_recipe_values_json'",
                )
                .fetch_one(pool)
                .await?;

                if user_recipe_col_count > 0 {
                    sqlx::query(
                        "ALTER TABLE sessions RENAME COLUMN user_recipe_values_json TO user_workflow_values_json",
                    )
                    .execute(pool)
                    .await?;
                }
            }
            8 => {
                // Lineage pointer for diverged (branched) sessions.
                sqlx::query("ALTER TABLE sessions ADD COLUMN diverged_from TEXT")
                    .execute(pool)
                    .await?;
            }
            9 => {
                // BRSDK durable sessions: a stable external handle so an app
                // client can resume its session across reconnects.
                sqlx::query("ALTER TABLE sessions ADD COLUMN external_key TEXT")
                    .execute(pool)
                    .await?;
                sqlx::query(
                    "CREATE UNIQUE INDEX idx_sessions_external_key ON sessions(external_key) WHERE external_key IS NOT NULL",
                )
                .execute(pool)
                .await?;
            }
            10 => {
                // Per-turn token accounting.
                //
                // Before this, tokens existed only as one lifetime total per
                // session with a single created_at/updated_at, so there was no
                // way to answer "how many tokens did I use on Tuesday?" — and
                // the "past 7 days" tile summed the whole lifetime of any
                // session merely *touched* in the window.
                //
                // This is an append-only side table, deliberately NOT
                // `messages.tokens`: `replace_conversation` DELETEs and
                // re-inserts the whole message list, which would drop or
                // re-stamp historical token rows on every edit.
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS token_events (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        session_id TEXT NOT NULL,
                        ts INTEGER NOT NULL,
                        input_tokens INTEGER,
                        output_tokens INTEGER,
                        total_tokens INTEGER NOT NULL DEFAULT 0
                    )
                "#,
                )
                .execute(pool)
                .await?;

                sqlx::query("CREATE INDEX idx_token_events_ts ON token_events(ts)")
                    .execute(pool)
                    .await?;
                sqlx::query(
                    "CREATE INDEX idx_token_events_session ON token_events(session_id, ts)",
                )
                .execute(pool)
                .await?;

                // Seed history so the heatmap is not empty before instrumentation
                // landed. Each pre-existing session's lifetime total is attributed
                // wholesale to the day it was created — the only anchor that
                // exists, and a stable one (created_at never moves).
                sqlx::query(
                    r#"
                    INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens)
                    SELECT id,
                           CAST(strftime('%s', created_at) AS INTEGER),
                           accumulated_input_tokens,
                           accumulated_output_tokens,
                           COALESCE(accumulated_total_tokens, total_tokens, 0)
                    FROM sessions
                    WHERE COALESCE(accumulated_total_tokens, total_tokens, 0) > 0
                "#,
                )
                .execute(pool)
                .await?;
            }
            11 => {
                // Per-turn model attribution. Before this, `token_events` recorded
                // only token counts, so a thread that switched models mid-way (the
                // reported UCSF workflow) could not be split per model — the
                // `ProviderUsage.model` was dropped at record time.
                //
                // Guard each ADD COLUMN with a pragma check so re-running the
                // migration on a DB that already has the column is a no-op.
                let model_col: i32 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM pragma_table_info('token_events') WHERE name = 'model_id'",
                )
                .fetch_one(pool)
                .await?;
                if model_col == 0 {
                    sqlx::query("ALTER TABLE token_events ADD COLUMN model_id TEXT")
                        .execute(pool)
                        .await?;
                }

                let provider_col: i32 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM pragma_table_info('token_events') WHERE name = 'provider'",
                )
                .fetch_one(pool)
                .await?;
                if provider_col == 0 {
                    sqlx::query("ALTER TABLE token_events ADD COLUMN provider TEXT")
                        .execute(pool)
                        .await?;
                }

                // Historical rows deliberately remain NULL. A session stores
                // only its final model/provider, so copying that value backward
                // would fabricate attribution for sessions that switched models.
            }
            12 => {
                // This branch handles a normal v11 → v12 upgrade. The same
                // additive reconciliation also runs unconditionally after the
                // version loop for databases created by early v12 builds.
                Self::reconcile_usage_schema(pool).await?;
            }
            13 => {
                // BR-43 shadow-git checkpoints. Additive side table keyed by the
                // turn's anchor `created_timestamp` (NOT the positional message
                // id) so checkpoints survive the stable-UUID migration.
                Self::create_checkpoints_table(pool).await?;
            }
            14 => {
                // BR-45: stable, durable per-message ids plus an exact branch
                // divergence point. Shape guards make this safe when an experimental
                // v12 database already applied the same feature.
                Self::ensure_message_identity_schema(pool).await?;
            }
            15 => {
                // BR-17 relevance-ranked chat recall. Rebuilding the derived
                // index prevents duplicate rows when an experimental v13/v14
                // database already contains the FTS table and backfill.
                Self::create_and_backfill_messages_fts(pool, true).await?;
            }
            16 => {
                // BR-7 externalized tool-result payload storage. Existing
                // inline messages remain untouched.
                Self::create_message_blobs_table(pool).await?;
            }
            _ => {
                anyhow::bail!("Unknown migration version: {}", version);
            }
        }

        Ok(())
    }

    async fn table_has_column(pool: &Pool<Sqlite>, table: &str, column: &str) -> Result<bool> {
        let query = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = ?1");
        let count: i64 = sqlx::query_scalar(&query)
            .bind(column)
            .fetch_one(pool)
            .await?;
        Ok(count > 0)
    }

    async fn ensure_message_identity_schema(pool: &Pool<Sqlite>) -> Result<()> {
        if !Self::table_has_column(pool, "messages", "msg_uid").await? {
            sqlx::query("ALTER TABLE messages ADD COLUMN msg_uid TEXT")
                .execute(pool)
                .await?;
        }

        sqlx::query("UPDATE messages SET msg_uid = 'm' || id WHERE msg_uid IS NULL")
            .execute(pool)
            .await?;
        sqlx::query(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_uid ON messages(session_id, msg_uid)",
        )
        .execute(pool)
        .await?;

        if !Self::table_has_column(pool, "sessions", "branch_point_msg_uid").await? {
            sqlx::query("ALTER TABLE sessions ADD COLUMN branch_point_msg_uid TEXT")
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    /// #51 W3: give every session ROW a token that is never handed to a later
    /// row with the same id, so a [`ConversationRevision`] taken from one
    /// incarnation can never be satisfied by another.
    ///
    /// Idempotent and version-independent, like the rest of
    /// `reconcile_loop_schema`. `ALTER TABLE ... ADD COLUMN` may not carry an
    /// expression default, so the column lands as `0` ("unknown") and is
    /// backfilled here; `random()` is re-evaluated per row, so the UPDATE gives
    /// each existing session its own value.
    async fn ensure_session_incarnation_schema(pool: &Pool<Sqlite>) -> Result<()> {
        if !Self::table_has_column(pool, "sessions", "incarnation").await? {
            sqlx::query("ALTER TABLE sessions ADD COLUMN incarnation INTEGER NOT NULL DEFAULT 0")
                .execute(pool)
                .await?;
        }
        sqlx::query("UPDATE sessions SET incarnation = random() WHERE IFNULL(incarnation, 0) = 0")
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn create_and_backfill_messages_fts(pool: &Pool<Sqlite>, rebuild: bool) -> Result<()> {
        let existed = Self::messages_fts_exists(pool).await;
        sqlx::query(MESSAGES_FTS_DDL).execute(pool).await?;
        if existed && !rebuild {
            return Ok(());
        }

        sqlx::query("DELETE FROM messages_fts")
            .execute(pool)
            .await?;
        let rows = sqlx::query_as::<_, (i64, String, String, Option<String>)>(
            "SELECT id, session_id, content_json, metadata_json FROM messages",
        )
        .fetch_all(pool)
        .await?;

        for (id, session_id, content_json, metadata_json) in rows {
            if !message_is_user_visible(metadata_json.as_deref()) {
                continue;
            }
            let Ok(content) = serde_json::from_str::<Vec<MessageContent>>(&content_json) else {
                continue;
            };
            let text = chat_fts::extract_searchable_text(&content);
            if text.is_empty() {
                continue;
            }
            sqlx::query(MESSAGES_FTS_INSERT)
                .bind(&text)
                .bind(&session_id)
                .bind(id)
                .execute(pool)
                .await?;
        }
        Ok(())
    }

    /// The BR-43 `checkpoints` side table (migration 13 + fresh-DB schema).
    async fn create_checkpoints_table(pool: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS checkpoints (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                turn_index INTEGER NOT NULL,
                anchor_ts INTEGER NOT NULL,
                kind TEXT NOT NULL,
                commit_sha TEXT NOT NULL,
                tree_sha TEXT NOT NULL,
                changed_paths_json TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        "#,
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_checkpoints_session ON checkpoints(session_id, turn_index)",
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    /// BR-7: side table holding tool-result payloads too large to keep inline in
    /// `messages.content_json`. Keyed `(session_id, blob_uid)` rather than by the
    /// message rowid: `replace_conversation_inner` DELETEs and re-INSERTs every
    /// message on each compaction/edit, so a rowid reference would dangle on the
    /// first rewrite. The composite key also lets a diverged/copied session own
    /// its own row for the same payload, so the parent's orphan sweep can never
    /// pull a blob out from under a branch.
    async fn create_message_blobs_table(pool: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS message_blobs (
                blob_uid TEXT NOT NULL,
                session_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                bytes INTEGER NOT NULL,
                content TEXT NOT NULL,
                PRIMARY KEY (session_id, blob_uid)
            )
        "#,
        )
        .execute(pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_message_blobs_uid ON message_blobs(blob_uid)")
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn create_session(
        &self,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
    ) -> Result<Session> {
        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;

        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        let session = sqlx::query_as(
            r#"
                INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, incarnation)
                VALUES (
                    ? || '_' || CAST(COALESCE((
                        SELECT MAX(CAST(SUBSTR(id, 10) AS INTEGER))
                        FROM sessions
                        WHERE id LIKE ? || '_%'
                    ), 0) + 1 AS TEXT),
                    ?,
                    FALSE,
                    ?,
                    ?,
                    '{}',
                    random()
                )
                RETURNING *
                "#,
        )
            .bind(&today)
            .bind(&today)
            .bind(&name)
            .bind(session_type.to_string())
            .bind(working_dir.to_string_lossy().as_ref())
            .fetch_one(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(session)
    }

    /// Resume the session bound to `external_key`, or create a fresh one and bind
    /// it. Returns `(session, resumed)`. The session's real primary key remains
    /// the allocated `YYYYMMDD_N` id; `external_key` is only a stable lookup
    /// handle for durable, resumable app sessions.
    async fn get_or_create_by_external_key(
        &self,
        external_key: &str,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
    ) -> Result<(Session, bool)> {
        let pool = self.pool().await?;
        if let Some(id) =
            sqlx::query_scalar::<_, String>("SELECT id FROM sessions WHERE external_key = ?")
                .bind(external_key)
                .fetch_optional(pool)
                .await?
        {
            return Ok((self.get_session(&id, true).await?, true));
        }

        let session = self.create_session(working_dir, name, session_type).await?;
        match sqlx::query("UPDATE sessions SET external_key = ? WHERE id = ?")
            .bind(external_key)
            .bind(&session.id)
            .execute(pool)
            .await
        {
            Ok(_) => Ok((session, false)),
            // ONLY a UNIQUE-constraint violation means another connection bound
            // this key first (a genuine lost race) — recover by discarding our
            // duplicate and resuming the winner. Any OTHER error (SQLITE_BUSY,
            // I/O, disk-full, …) is transient/retryable and must NOT destroy our
            // freshly-created session, so propagate it unchanged.
            Err(e)
                if matches!(
                    e.as_database_error().map(|d| d.kind()),
                    Some(sqlx::error::ErrorKind::UniqueViolation)
                ) =>
            {
                let _ = sqlx::query("DELETE FROM sessions WHERE id = ?")
                    .bind(&session.id)
                    .execute(pool)
                    .await;
                let id = sqlx::query_scalar::<_, String>(
                    "SELECT id FROM sessions WHERE external_key = ?",
                )
                .bind(external_key)
                .fetch_one(pool)
                .await?;
                Ok((self.get_session(&id, true).await?, true))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn get_session(&self, id: &str, include_messages: bool) -> Result<Session> {
        let pool = self.pool().await?;
        let mut session = sqlx::query_as::<_, Session>(
            r#"
        SELECT id, working_dir, name, description, user_set_name, session_type, created_at, updated_at, extension_data,
               total_tokens, input_tokens, output_tokens,
               accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens,
               schedule_id, workflow_json, user_workflow_values_json,
               provider_name, model_config_json, diverged_from, branch_point_msg_uid
        FROM sessions
        WHERE id = ?
    "#,
        )
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        if include_messages {
            let conv = self.get_conversation(&session.id).await?;
            session.message_count = conv.messages().len();
            session.conversation = Some(conv);
        } else {
            let count =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE session_id = ?")
                    .bind(&session.id)
                    .fetch_one(pool)
                    .await? as usize;
            session.message_count = count;
        }

        Ok(session)
    }

    async fn get_token_counts(&self, id: &str) -> Result<SessionTokenCounts> {
        let pool = self.pool().await?;
        let counts = sqlx::query_as::<_, SessionTokenCounts>(
            r#"
        SELECT total_tokens, input_tokens, output_tokens,
               accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens
        FROM sessions
        WHERE id = ?
    "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Session not found"))?;
        Ok(counts)
    }

    /// The atomic conditional update behind
    /// [`SessionManager::try_update_working_dir_if_empty`]: one `UPDATE` whose
    /// `WHERE` clause carries the emptiness check, so check and write cannot
    /// be interleaved by a concurrent message insert (SQLite serializes
    /// writers; the `NOT EXISTS` is evaluated within the same statement).
    async fn try_update_working_dir_if_empty(
        &self,
        id: &str,
        working_dir: &Path,
    ) -> Result<WorkingDirUpdate> {
        let pool = self.pool().await?;
        let result = sqlx::query(
            "UPDATE sessions SET working_dir = ?, updated_at = datetime('now') \
             WHERE id = ? AND NOT EXISTS (SELECT 1 FROM messages WHERE session_id = ?)",
        )
        .bind(working_dir.to_string_lossy().as_ref())
        .bind(id)
        .bind(id)
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            return Ok(WorkingDirUpdate::Updated);
        }

        // 0 rows: either the session has messages or it does not exist.
        let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;
        if exists > 0 {
            Ok(WorkingDirUpdate::RefusedNotEmpty)
        } else {
            Ok(WorkingDirUpdate::SessionNotFound)
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn apply_update(&self, builder: SessionUpdateBuilder<'_>) -> Result<()> {
        let mut updates = Vec::new();
        let mut query = String::from("UPDATE sessions SET ");

        macro_rules! add_update {
            ($field:expr, $name:expr) => {
                if $field.is_some() {
                    if !updates.is_empty() {
                        query.push_str(", ");
                    }
                    updates.push($name);
                    query.push_str($name);
                    query.push_str(" = ?");
                }
            };
        }

        add_update!(builder.name, "name");
        add_update!(builder.user_set_name, "user_set_name");
        add_update!(builder.session_type, "session_type");
        add_update!(builder.working_dir, "working_dir");
        add_update!(builder.extension_data, "extension_data");
        add_update!(builder.total_tokens, "total_tokens");
        add_update!(builder.input_tokens, "input_tokens");
        add_update!(builder.output_tokens, "output_tokens");
        add_update!(builder.accumulated_total_tokens, "accumulated_total_tokens");
        add_update!(builder.accumulated_input_tokens, "accumulated_input_tokens");
        add_update!(
            builder.accumulated_output_tokens,
            "accumulated_output_tokens"
        );

        // Additive, atomic accumulation. Emitted as `col = COALESCE(col,0) + ?`
        // so a concurrent turn on the same session cannot lose an update.
        if let Some(delta) = builder.token_delta {
            for (value, name) in [
                (delta.total, "accumulated_total_tokens"),
                (delta.input, "accumulated_input_tokens"),
                (delta.output, "accumulated_output_tokens"),
            ] {
                if value.is_none() {
                    continue;
                }
                if !updates.is_empty() {
                    query.push_str(", ");
                }
                updates.push(name);
                query.push_str(name);
                query.push_str(" = COALESCE(");
                query.push_str(name);
                query.push_str(", 0) + ?");
            }
        }

        add_update!(builder.schedule_id, "schedule_id");
        add_update!(builder.workflow, "workflow_json");
        add_update!(builder.user_workflow_values, "user_workflow_values_json");
        add_update!(builder.provider_name, "provider_name");
        add_update!(builder.model_config, "model_config_json");
        add_update!(builder.diverged_from, "diverged_from");
        add_update!(builder.branch_point_msg_uid, "branch_point_msg_uid");

        if updates.is_empty() {
            return Ok(());
        }

        query.push_str(", ");
        query.push_str("updated_at = datetime('now') WHERE id = ?");

        let mut q = sqlx::query(&query);

        if let Some(name) = builder.name {
            q = q.bind(name);
        }
        if let Some(user_set_name) = builder.user_set_name {
            q = q.bind(user_set_name);
        }
        if let Some(session_type) = builder.session_type {
            q = q.bind(session_type.to_string());
        }
        if let Some(wd) = builder.working_dir {
            q = q.bind(wd.to_string_lossy().to_string());
        }
        if let Some(ed) = builder.extension_data {
            q = q.bind(serde_json::to_string(&ed)?);
        }
        if let Some(tt) = builder.total_tokens {
            q = q.bind(tt);
        }
        if let Some(it) = builder.input_tokens {
            q = q.bind(it);
        }
        if let Some(ot) = builder.output_tokens {
            q = q.bind(ot);
        }
        if let Some(att) = builder.accumulated_total_tokens {
            q = q.bind(att);
        }
        if let Some(ait) = builder.accumulated_input_tokens {
            q = q.bind(ait);
        }
        if let Some(aot) = builder.accumulated_output_tokens {
            q = q.bind(aot);
        }
        if let Some(delta) = builder.token_delta {
            // Bind order must match the clause order appended above.
            for value in [delta.total, delta.input, delta.output]
                .into_iter()
                .flatten()
            {
                q = q.bind(value);
            }
        }
        if let Some(sid) = builder.schedule_id {
            q = q.bind(sid);
        }
        if let Some(workflow) = builder.workflow {
            let workflow_json = workflow.map(|r| serde_json::to_string(&r)).transpose()?;
            q = q.bind(workflow_json);
        }
        if let Some(user_workflow_values) = builder.user_workflow_values {
            let user_workflow_values_json = user_workflow_values
                .map(|urv| serde_json::to_string(&urv))
                .transpose()?;
            q = q.bind(user_workflow_values_json);
        }
        if let Some(provider_name) = builder.provider_name {
            q = q.bind(provider_name);
        }
        if let Some(model_config) = builder.model_config {
            let model_config_json = model_config
                .map(|mc| serde_json::to_string(&mc))
                .transpose()?;
            q = q.bind(model_config_json);
        }
        if let Some(diverged_from) = builder.diverged_from {
            q = q.bind(diverged_from);
        }
        if let Some(branch_point_msg_uid) = builder.branch_point_msg_uid {
            q = q.bind(branch_point_msg_uid);
        }

        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;
        q = q.bind(&builder.session_id);
        q.execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_conversation(&self, session_id: &str) -> Result<Conversation> {
        // BR-7: hydrating is the default, so every existing consumer — the UI
        // transcript, exports, a resumed agent — sees exactly the bytes it saw
        // before externalization existed. `BIOROUTER_SESSION_BLOB_LAZY_LOAD`
        // opts into the lazy read, where an oversized tool result stays a stub
        // and the model pulls it back with `platform__read_session_blob`.
        self.get_conversation_inner(session_id, !message_blobs::lazy_load_enabled())
            .await
    }

    async fn get_conversation_inner(
        &self,
        session_id: &str,
        hydrate_blobs: bool,
    ) -> Result<Conversation> {
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>)>(
            "SELECT role, content_json, created_timestamp, metadata_json, msg_uid FROM messages WHERE session_id = ? ORDER BY id",
        )
            .bind(session_id)
            .fetch_all(pool)
            .await?;

        // The payloads to splice back into any externalized tool result. Fetched
        // once, and only when a row actually carries a stub — a session that
        // never externalized anything (every session before this schema, and
        // every ordinary one after it) pays a substring scan and nothing else.
        let hydrate = hydrate_blobs
            && rows
                .iter()
                .any(|(_, content_json, ..)| message_blobs::content_json_has_stub(content_json));
        let blobs = if hydrate {
            self.load_blobs(session_id).await?
        } else {
            HashMap::new()
        };

        let mut messages = Vec::new();
        for (idx, (role_str, content_json, created_timestamp, metadata_json, msg_uid)) in
            rows.into_iter().enumerate()
        {
            let role = match role_str.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => continue,
            };

            let mut content: Vec<MessageContent> = serde_json::from_str(&content_json)?;
            message_blobs::hydrate(&mut content, &blobs);
            let metadata = metadata_json
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            let mut message = Message::new(role, created_timestamp, content);
            message.metadata = metadata;
            // Dual-read: prefer the durable `msg_uid`; fall back to the legacy
            // positional id only for a row an in-flight upgrade hasn't
            // backfilled yet (migration 14 backfills all existing rows).
            let id = msg_uid.unwrap_or_else(|| format!("msg_{}_{}", session_id, idx));
            message = message.with_id(id);
            messages.push(message);
        }

        Ok(Conversation::new_unvalidated(messages))
    }

    /// Every externalized payload of one session, keyed by blob handle (BR-7).
    async fn load_blobs(&self, session_id: &str) -> Result<HashMap<String, String>> {
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT blob_uid, content FROM message_blobs WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// One externalized payload, by handle. The lazy read path's retrieval seam:
    /// what `platform__read_session_blob` calls when the model asks for the full
    /// output behind a stub.
    async fn get_message_blob(&self, session_id: &str, blob_uid: &str) -> Result<Option<String>> {
        let pool = self.pool().await?;
        let content = sqlx::query_scalar::<_, String>(
            "SELECT content FROM message_blobs WHERE session_id = ? AND blob_uid = ?",
        )
        .bind(session_id)
        .bind(blob_uid)
        .fetch_optional(pool)
        .await?;
        Ok(content)
    }

    /// Returns the **effective** `msg_uid` the message was stored under, so a
    /// caller keeping the message in memory can adopt it (#41). Usually the
    /// caller-supplied id (or a freshly minted one when there was none); only
    /// a uid collision with *different* content re-mints.
    async fn add_message(&self, session_id: &str, message: &Message) -> Result<String> {
        // Runs on the turn path once per message (including every tool
        // response); the transaction covers the message row, blob spill and
        // FTS index write, so it is a plausible per-tool-call fixed cost.
        let _phase = crate::agents::phase_timing::Phase::start("session.add_message");

        // Persist the message's stable id, minting a fresh UUIDv7 when the
        // caller didn't supply one (BR-45).
        let msg_uid = message.id.clone().unwrap_or_else(new_message_id);
        match self.insert_message(session_id, message, &msg_uid).await {
            Ok(()) => Ok(msg_uid),
            Err(err) if is_msg_uid_unique_violation(&err) => {
                // #41: an EXACT replay (same uid, identical role + content +
                // metadata — e.g. a caller retrying a write it believes
                // failed) is idempotent success, not an anomaly. No second
                // row, and the in-memory id already agrees.
                if self
                    .existing_row_matches(session_id, message, &msg_uid)
                    .await?
                {
                    debug!(
                        session_id,
                        %msg_uid,
                        "message uid already persisted with identical content; \
                         treating the replay as success"
                    );
                    return Ok(msg_uid);
                }
                // #41 resilience: a caller-supplied id that already exists in
                // this session with DIFFERENT content (an id-reuse bug
                // upstream — a decoder stamping one shared id on several
                // messages) must degrade to a logged anomaly with a re-minted
                // uid, not abort the whole turn. Retried exactly once with a
                // freshly minted UUIDv7, which cannot collide again. The
                // fresh uid is returned so the caller's in-memory message can
                // adopt it.
                let fresh_uid = new_message_id();
                warn!(
                    session_id,
                    old_uid = %msg_uid,
                    new_uid = %fresh_uid,
                    "message uid already exists in this session; retrying the \
                     insert with a re-minted uid instead of failing the turn"
                );
                self.insert_message(session_id, message, &fresh_uid).await?;
                Ok(fresh_uid)
            }
            Err(err) => Err(err),
        }
    }

    /// Whether the row already stored under `msg_uid` is identical to what
    /// inserting `message` would store (role + created_timestamp + content +
    /// metadata) — i.e. the insert is an exact replay, not an id collision
    /// between two distinct messages.
    ///
    /// `created_timestamp` is part of the comparison (#41): two genuinely
    /// distinct messages that happen to share uid, role, content and metadata
    /// but were created at different times must NOT be collapsed into one —
    /// treating the second as a replay would silently drop it.
    async fn existing_row_matches(
        &self,
        session_id: &str,
        message: &Message,
        msg_uid: &str,
    ) -> Result<bool> {
        let pool = self.pool().await?;
        let row = sqlx::query_as::<_, (String, String, i64, Option<String>)>(
            "SELECT role, content_json, created_timestamp, metadata_json FROM messages \
             WHERE session_id = ? AND msg_uid = ?",
        )
        .bind(session_id)
        .bind(msg_uid)
        .fetch_optional(pool)
        .await?;
        let Some((row_role, row_content, row_created, row_metadata)) = row else {
            return Ok(false);
        };

        if row_role != role_to_string(&message.role) || row_created != message.created {
            return Ok(false);
        }

        // `metadata_json` is nullable for rows migrated from older schemas
        // (#41): a NULL there is the stored form of "no metadata was
        // recorded", which can only be an exact replay of a message whose
        // in-memory metadata is still the default. Decoding it as a bare
        // `String` made the replay probe *error* on such rows, aborting the
        // very turn the idempotent-replay path exists to save.
        let metadata_matches = match row_metadata {
            Some(row_metadata) => row_metadata == serde_json::to_string(&message.metadata)?,
            None => message.metadata == crate::conversation::message::MessageMetadata::default(),
        };
        if !metadata_matches {
            return Ok(false);
        }

        self.stored_content_matches(session_id, &row_content, message)
            .await
    }

    /// Whether `row_content` (the stored `content_json`) and the candidate
    /// message's content are the same payload.
    ///
    /// The comparison must be *stable* across externalization (#41):
    /// [`message_blobs::externalize`] mints a fresh blob uid per call, so
    /// serializing a freshly-externalized candidate could never equal the
    /// stored row for an oversized message — every large-message replay
    /// compared unequal and was re-inserted under a re-minted uid. Instead,
    /// compare the pre-externalization forms: hydrate the stored stubs back
    /// to their payloads (and any stubs the candidate itself carries, e.g. a
    /// re-persisted already-externalized conversation) and compare those.
    async fn stored_content_matches(
        &self,
        session_id: &str,
        row_content: &str,
        message: &Message,
    ) -> Result<bool> {
        if !message_blobs::content_json_has_stub(row_content) {
            return Ok(row_content == serde_json::to_string(&message.content)?);
        }

        let mut stored: Vec<MessageContent> = serde_json::from_str(row_content)?;
        let mut candidate = message.content.clone();
        let mut uids = message_blobs::referenced_uids(&stored);
        uids.extend(message_blobs::referenced_uids(&candidate));
        let mut blobs = std::collections::HashMap::new();
        for uid in uids {
            if let Some(content) = self.get_message_blob(session_id, &uid).await? {
                blobs.insert(uid, content);
            }
        }
        message_blobs::hydrate(&mut stored, &blobs);
        message_blobs::hydrate(&mut candidate, &blobs);
        Ok(serde_json::to_string(&stored)? == serde_json::to_string(&candidate)?)
    }

    /// One attempt at the transactional message insert (row + blob spill +
    /// FTS index + session touch), with an explicit `msg_uid`. Split out of
    /// [`Self::add_message`] so a uid collision can be retried with a fresh
    /// uid on a clean transaction.
    async fn insert_message(
        &self,
        session_id: &str,
        message: &Message,
        msg_uid: &str,
    ) -> Result<()> {
        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;

        let metadata_json = serde_json::to_string(&message.metadata)?;

        // BR-7: lift an oversized tool-result payload into the blob side table
        // so the `messages` row stays small. `None` (the common case) stores the
        // content exactly as before, with no extra allocation.
        let externalized = message_blobs::externalize(&message.content);
        let (content_json, blobs) = match &externalized {
            Some((content, blobs)) => (serde_json::to_string(content)?, blobs.as_slice()),
            None => (serde_json::to_string(&message.content)?, [].as_slice()),
        };

        let insert = sqlx::query(
            r#"
            INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json, msg_uid)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(session_id)
        .bind(role_to_string(&message.role))
        .bind(content_json)
        .bind(message.created)
        .bind(metadata_json)
        .bind(msg_uid)
        .execute(&mut *tx)
        .await?;

        // Same transaction as the message row: a stub can never be persisted
        // without the payload it points at.
        Self::insert_blobs(&mut tx, session_id, blobs).await?;

        // Keep the FTS recall index in sync with the new row (BR-17). Indexed
        // from the *original* message: recall renders a tool response as a
        // placeholder, so externalization cannot change what is searchable.
        let fts_available = Self::messages_fts_exists(&mut *tx).await;
        Self::index_message_fts(
            &mut tx,
            session_id,
            insert.last_insert_rowid(),
            message,
            fts_available,
        )
        .await?;

        sqlx::query("UPDATE sessions SET updated_at = datetime('now') WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// True when the FTS5 mirror table exists (created by schema migration 15).
    /// The read path guards on this too; a DB that reached its version without
    /// `messages_fts` (e.g. a future migration renumber, or a partial upgrade)
    /// must degrade gracefully instead of hard-failing every message save.
    async fn messages_fts_exists<'e, E>(executor: E) -> bool
    where
        E: sqlx::Executor<'e, Database = Sqlite>,
    {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='messages_fts'",
        )
        .fetch_one(executor)
        .await
        .map(|c| c > 0)
        .unwrap_or(false)
    }

    /// Insert one message's flattened text into the FTS recall index, within
    /// the caller's transaction. Only user-visible, non-empty messages are
    /// indexed (BR-17). `fts_available` is resolved once by the caller so a
    /// bulk rewrite doesn't re-probe the catalog per message.
    async fn index_message_fts(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        message_id: i64,
        message: &Message,
        fts_available: bool,
    ) -> Result<()> {
        if !fts_available || !message.metadata.user_visible {
            return Ok(());
        }
        let text = chat_fts::extract_searchable_text(&message.content);
        if text.is_empty() {
            return Ok(());
        }
        sqlx::query(MESSAGES_FTS_INSERT)
            .bind(&text)
            .bind(session_id)
            .bind(message_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    async fn replace_conversation_inner(
        pool: &Pool<Sqlite>,
        session_id: &str,
        conversation: &Conversation,
        guard: RewriteGuard<'_>,
    ) -> Result<(ReplaceOutcome, Vec<Message>)> {
        let mut tx = pool.begin().await?;

        // LOAD-BEARING FIRST STATEMENT, AND IT MUST BE A WRITE. DO NOT REORDER.
        //
        // sqlx's `pool.begin()` emits a bare (DEFERRED) `BEGIN`. Under WAL, a
        // deferred transaction that READS first pins a read snapshot; if another
        // connection commits before our first write, the upgrade to a writer
        // returns SQLITE_BUSY_SNAPSHOT *immediately* — measured at 0.0000s,
        // i.e. the 5s `busy_timeout` is bypassed, because a busy handler is not
        // consulted for a snapshot upgrade. Opening with a WRITE takes the
        // single per-file write lock up front, so any SELECT that follows reads
        // true latest-committed state and the DELETE below cannot fail that way.
        // (A concurrent writer then blocks on the busy timeout, which is
        // correct.) The freshness guard added on top of this relies on it.
        //
        // It also fixes a real gap: this rewrite never bumped
        // `sessions.updated_at`, so a compaction or an edit was invisible to the
        // `ORDER BY updated_at DESC` session list even though it changed the
        // session's content. `insert_message` has always bumped it.
        let touched = sqlx::query("UPDATE sessions SET updated_at = datetime('now') WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        // The freshness guard, evaluated UNDER the write lock the statement
        // above just took — so nothing can interleave between the check and the
        // DELETE. `Unconditional` skips it entirely and keeps this function's
        // original semantics byte for byte.
        let mut outcome = ReplaceOutcome::Replaced;
        let mut recovered: Vec<Message> = Vec::new();
        if let RewriteGuard::PreserveTail { basis, known } = guard {
            if touched.rows_affected() == 0 {
                // Decided before any destructive work has happened.
                tx.rollback().await?;
                return Ok((ReplaceOutcome::SessionNotFound, Vec::new()));
            }

            // Row identity FIRST (#51 W3). A session id outlives the session:
            // `/reset` History empties `sessions`, and the next
            // `create_session` on the same day hands the id straight back. A
            // rewrite that snapshotted the previous occupant must never be
            // allowed to reason about rowids in the new one's message set —
            // `(count, max_rowid)` is only meaningful within one incarnation.
            let incarnation = Self::read_incarnation(&mut tx, session_id).await?;
            if incarnation != basis.incarnation {
                tx.rollback().await?;
                return Ok((ReplaceOutcome::Stale, Vec::new()));
            }

            // Prefix integrity: every message the basis covered must still be
            // there, unmoved. A concurrent truncate lowers this; a concurrent
            // wholesale rewrite renumbers every row and drives it to 0. Either
            // way there is no sound prefix to merge onto, so refuse.
            let prefix = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM messages WHERE session_id = ? AND id <= ?",
            )
            .bind(session_id)
            .bind(basis.max_rowid)
            .fetch_one(&mut *tx)
            .await?;
            if prefix != basis.count {
                tx.rollback().await?;
                return Ok((ReplaceOutcome::Stale, Vec::new()));
            }

            recovered = Self::scan_foreign_tail(&mut tx, session_id, basis, known).await?;
            if !recovered.is_empty() {
                outcome = ReplaceOutcome::ReplacedPreservingTail {
                    preserved: recovered.len(),
                };
            }
        }

        // ONE merged list, built BEFORE the insert loop. Appending the recovered
        // messages in a second pass instead would run the blob accounting below
        // without their handles, so `sweep_orphan_blobs` would delete the
        // payload of a recovered tool response and leave a dangling stub —
        // silent, and only visible on the next read.
        let mut merged: Vec<Message> = conversation
            .messages()
            .iter()
            .cloned()
            .chain(recovered)
            .collect();

        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        // Rebuild the FTS recall index for this session in lockstep with the
        // message rewrite, so a compacted/edited session stays searchable
        // without double-counting (BR-17). Skip entirely when the mirror table
        // is absent so message rewrites still succeed on such a DB.
        let fts_available = Self::messages_fts_exists(&mut *tx).await;
        if fts_available {
            sqlx::query("DELETE FROM messages_fts WHERE session_id = ?")
                .bind(session_id)
                .execute(&mut *tx)
                .await?;
        }

        // Every blob still referenced after the rewrite (BR-7): the handles that
        // survive inside kept stubs, plus the ones minted below. Anything else
        // belonged to a message this rewrite dropped and is swept at the end.
        let mut live_blob_uids: Vec<String> = Vec::new();

        for message in merged.iter_mut() {
            let metadata_json = serde_json::to_string(&message.metadata)?;
            // PRESERVE each kept message's stable id across the rewrite (this is
            // the exact op — DELETE + re-INSERT — that used to renumber ids).
            // Only a newly-minted message (e.g. a compaction summary) with no id
            // gets a fresh one (BR-45).
            let msg_uid = message.id.clone().unwrap_or_else(new_message_id);

            let externalized = message_blobs::externalize(&message.content);
            let (content_json, blobs) = match &externalized {
                Some((content, blobs)) => (serde_json::to_string(content)?, blobs.as_slice()),
                None => (serde_json::to_string(&message.content)?, [].as_slice()),
            };
            live_blob_uids.extend(message_blobs::referenced_uids(
                externalized
                    .as_ref()
                    .map_or(message.content.as_slice(), |(content, _)| {
                        content.as_slice()
                    }),
            ));

            let insert = sqlx::query(
                r#"
            INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json, msg_uid)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
            )
            .bind(session_id)
            .bind(role_to_string(&message.role))
            .bind(content_json)
            .bind(message.created)
            .bind(metadata_json)
            .bind(msg_uid.clone())
            .execute(&mut *tx)
            .await?;

            Self::insert_blobs(&mut tx, session_id, blobs).await?;

            Self::index_message_fts(
                &mut tx,
                session_id,
                insert.last_insert_rowid(),
                message,
                fts_available,
            )
            .await?;

            // Stamp the effective uid so the conversation handed back to the
            // caller carries the same ids as the rows (#41's contract, applied
            // to the rewrite path).
            message.id = Some(msg_uid);
        }

        // A conversation written here can carry stubs minted under *another*
        // session — a diverge/copy re-inserts the parent's messages verbatim
        // into the child. Give the child its own row for each such payload
        // before the sweep, so the two sessions' lifetimes stay independent.
        Self::adopt_blobs(&mut tx, session_id, &live_blob_uids).await?;
        Self::sweep_orphan_blobs(&mut tx, session_id, &live_blob_uids).await?;

        tx.commit().await?;
        Ok((outcome, merged))
    }

    /// The messages stored since `basis` that the caller's view never contained
    /// — i.e. what another writer appended while the caller was computing its
    /// rewrite. Read inside the rewrite's own transaction, under the write lock.
    ///
    /// Decoding mirrors `get_conversation_inner` exactly, INCLUDING the legacy
    /// `msg_{session}_{idx}` fallback for a row an in-flight upgrade has not
    /// backfilled — an id that decoded differently here would never match
    /// `known` and would look foreign forever. The absolute index is
    /// `basis.count + relative`, which is exact because the prefix check the
    /// caller just ran proved there are exactly `basis.count` rows at or below
    /// the watermark, and both reads order by `id`.
    ///
    /// Blobs are deliberately NOT hydrated. A recovered row's `content_json`
    /// may carry a BR-7 stub; re-inserting the stub verbatim means
    /// `externalize` mints nothing, the existing handle joins `live_blob_uids`,
    /// and the sweep spares it. Hydrating and re-externalizing would mint a
    /// duplicate blob row for the same payload.
    ///
    /// That makes this function's output STORAGE-shaped, not caller-shaped.
    /// The conversation handed back to a live turn is hydrated separately, on
    /// the returned copy only — see
    /// [`SessionStorage::hydrate_recovered_tail`]. Do not "simplify" by
    /// hydrating here.
    ///
    /// Parsing `content_json` into `Vec<MessageContent>` only for the insert
    /// loop to re-serialize it looks like a removable round-trip. It is not:
    /// the parsed form is what `referenced_uids` reads for blob liveness, what
    /// `index_message_fts` extracts search text from, and what the returned
    /// `Conversation` is made of. Only the *re-serialization* is avoidable, and
    /// only by threading a parallel raw-JSON array through the insert loop and
    /// branching its `content_json` binding — which costs more clarity than the
    /// microseconds it saves on a rare path, and gives up the guarantee that
    /// every row this rewrite writes went through one serializer.
    async fn scan_foreign_tail(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        basis: ConversationRevision,
        known: &Conversation,
    ) -> Result<Vec<Message>> {
        let rows = sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>)>(
            "SELECT role, content_json, created_timestamp, metadata_json, msg_uid \
             FROM messages WHERE session_id = ? AND id > ? ORDER BY id",
        )
        .bind(session_id)
        .bind(basis.max_rowid)
        .fetch_all(&mut **tx)
        .await?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let known_uids: std::collections::HashSet<&str> = known
            .messages()
            .iter()
            .filter_map(|m| m.id.as_deref())
            .collect();

        let mut foreign = Vec::new();
        for (relative, (role_str, content_json, created_timestamp, metadata_json, msg_uid)) in
            rows.into_iter().enumerate()
        {
            let role = match role_str.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                // Same as the read path: a row with an unrecognized role is not
                // representable and is dropped by any rewrite.
                _ => continue,
            };
            let id = msg_uid.unwrap_or_else(|| {
                format!("msg_{}_{}", session_id, basis.count as usize + relative)
            });
            if known_uids.contains(id.as_str()) {
                continue;
            }
            let content: Vec<MessageContent> = serde_json::from_str(&content_json)?;
            let metadata = metadata_json
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();
            let mut message = Message::new(role, created_timestamp, content);
            message.metadata = metadata;
            foreign.push(message.with_id(id));
        }
        Ok(foreign)
    }

    /// Write the payloads lifted out of one message, in the caller's transaction.
    async fn insert_blobs(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        blobs: &[message_blobs::PendingBlob],
    ) -> Result<()> {
        for blob in blobs {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO message_blobs (blob_uid, session_id, created_at, bytes, content)
                VALUES (?, ?, ?, ?, ?)
            "#,
            )
            .bind(&blob.uid)
            .bind(session_id)
            .bind(Utc::now().timestamp())
            .bind(blob.bytes())
            .bind(&blob.content)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Copy any referenced blob this session does not own yet from whichever
    /// session does (the parent of a diverge/copy). No-op for the common case
    /// where every handle was minted here.
    async fn adopt_blobs(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        uids: &[String],
    ) -> Result<()> {
        for uid in uids {
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO message_blobs (blob_uid, session_id, created_at, bytes, content)
                SELECT blob_uid, ?, created_at, bytes, content
                FROM message_blobs WHERE blob_uid = ? LIMIT 1
            "#,
            )
            .bind(session_id)
            .bind(uid)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    /// Drop this session's blobs that no surviving message points at — the
    /// payloads of tool responses that compaction (or an edit) just removed.
    /// Without this the side table would only ever grow, trading one kind of DB
    /// bloat for another.
    async fn sweep_orphan_blobs(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
        live_uids: &[String],
    ) -> Result<()> {
        if live_uids.is_empty() {
            sqlx::query("DELETE FROM message_blobs WHERE session_id = ?")
                .bind(session_id)
                .execute(&mut **tx)
                .await?;
            return Ok(());
        }

        let placeholders = vec!["?"; live_uids.len()].join(", ");
        let sql = format!(
            "DELETE FROM message_blobs WHERE session_id = ? AND blob_uid NOT IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql).bind(session_id);
        for uid in live_uids {
            query = query.bind(uid);
        }
        query.execute(&mut **tx).await?;
        Ok(())
    }

    pub async fn replace_conversation(
        &self,
        session_id: &str,
        conversation: &Conversation,
    ) -> Result<()> {
        let pool = self.pool().await?;
        Self::replace_conversation_inner(
            pool,
            session_id,
            conversation,
            RewriteGuard::Unconditional,
        )
        .await
        .map(|_| ())
    }

    /// Whole-history rewrite that carries over anything appended since `basis`.
    /// See [`SessionManager::replace_conversation_preserving_tail`].
    ///
    /// PRECONDITION: `basis` and `known` must come from ONE
    /// [`Self::snapshot_for_rewrite`] of `session_id`. In particular a
    /// default/synthetic `(0, 0)` basis paired with a NON-EMPTY session is
    /// incoherent and is not refused: the prefix check trivially passes
    /// (`COUNT(*) WHERE id <= 0` is 0), every existing row scans as foreign,
    /// and the whole real history is appended AFTER `replacement` — i.e. the
    /// replacement is silently prepended to the transcript rather than
    /// replacing it. No in-tree caller constructs a basis by hand (all three
    /// compaction sites thread one through from `snapshot_for_rewrite`); keep
    /// it that way.
    pub async fn replace_conversation_preserving_tail(
        &self,
        session_id: &str,
        replacement: &Conversation,
        basis: ConversationRevision,
        known: &Conversation,
    ) -> Result<(ReplaceOutcome, Conversation)> {
        let pool = self.pool().await?;
        let (outcome, mut stored) = Self::replace_conversation_inner(
            pool,
            session_id,
            replacement,
            RewriteGuard::PreserveTail { basis, known },
        )
        .await?;
        if !outcome.stored() {
            return Ok((outcome, replacement.clone()));
        }
        if let ReplaceOutcome::ReplacedPreservingTail { preserved } = outcome {
            self.hydrate_recovered_tail(session_id, &mut stored, preserved)
                .await?;
        }
        Ok((outcome, Conversation::new_unvalidated(stored)))
    }

    /// Splice the payloads back into the tail `scan_foreign_tail` recovered,
    /// on the copy handed BACK to the caller only.
    ///
    /// The rewrite deliberately re-inserts a recovered row's `content_json`
    /// verbatim, stubs and all, so `externalize` mints no duplicate blob (see
    /// `scan_foreign_tail`). But those same `Message` objects are what the
    /// caller adopts as live turn state — `conversation = stored` and
    /// `AgentEvent::HistoryReplaced(stored)` in the agent loop. Handing back the
    /// stub means the rest of the turn reasons over a ~1 KB placeholder where a
    /// concurrently-appended oversized tool response should be, and the UI
    /// transcript renders the placeholder too, until a reload heals it.
    ///
    /// So: hydrate the returned copy, never the one written. Mirrors
    /// `get_conversation_inner` exactly, including honouring
    /// `BIOROUTER_SESSION_BLOB_LAZY_LOAD` — under lazy load a stub is what a
    /// re-read would give, and the model pulls the payload back with
    /// `platform__read_session_blob`.
    async fn hydrate_recovered_tail(
        &self,
        session_id: &str,
        merged: &mut [Message],
        preserved: usize,
    ) -> Result<()> {
        if preserved == 0 || message_blobs::lazy_load_enabled() {
            return Ok(());
        }
        // The recovered rows are exactly the suffix: `merged` is
        // `replacement ++ recovered`, built in that order by the rewrite.
        let start = merged.len().saturating_sub(preserved);
        let tail = &mut merged[start..];
        // A session that never externalized anything pays one uid scan and no
        // query at all — the same "only when a row actually carries a stub"
        // discipline as the read path.
        if !tail
            .iter()
            .any(|m| !message_blobs::referenced_uids(&m.content).is_empty())
        {
            return Ok(());
        }
        let blobs = self.load_blobs(session_id).await?;
        for message in tail.iter_mut() {
            message_blobs::hydrate(&mut message.content, &blobs);
        }
        Ok(())
    }

    /// [`ConversationRevision`] of one session, read from the pool.
    pub async fn conversation_revision(&self, session_id: &str) -> Result<ConversationRevision> {
        let pool = self.pool().await?;
        Self::read_revision(pool, session_id).await
    }

    /// The revision read itself, over any executor — the pool for the public
    /// reader, the open transaction for the guard inside a rewrite.
    async fn read_revision<'e, E>(executor: E, session_id: &str) -> Result<ConversationRevision>
    where
        E: sqlx::Executor<'e, Database = Sqlite>,
    {
        // The incarnation rides along as an uncorrelated scalar subquery — one
        // primary-key probe of `sessions`, evaluated once, so this is still a
        // single round trip and a single scan of `idx_messages_session`.
        let (count, max_rowid, incarnation) = sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT COUNT(*), IFNULL(MAX(id), 0), \
             IFNULL((SELECT incarnation FROM sessions WHERE id = ?), 0) \
             FROM messages WHERE session_id = ?",
        )
        .bind(session_id)
        .bind(session_id)
        .fetch_one(executor)
        .await?;
        Ok(ConversationRevision {
            incarnation,
            count,
            max_rowid,
        })
    }

    /// The session row's incarnation token, read over the rewrite's own
    /// transaction. `0` for a row that predates the column (or does not exist).
    async fn read_incarnation(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<i64> {
        let incarnation = sqlx::query_scalar::<_, i64>(
            "SELECT IFNULL(incarnation, 0) FROM sessions WHERE id = ?",
        )
        .bind(session_id)
        .fetch_optional(&mut **tx)
        .await?
        .unwrap_or(0);
        Ok(incarnation)
    }

    async fn list_sessions_by_types(&self, types: &[SessionType]) -> Result<Vec<Session>> {
        if types.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: String = types.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let query = format!(
            r#"
            SELECT s.id, s.working_dir, s.name, s.description, s.user_set_name, s.session_type, s.created_at, s.updated_at, s.extension_data,
                   s.total_tokens, s.input_tokens, s.output_tokens,
                   s.accumulated_total_tokens, s.accumulated_input_tokens, s.accumulated_output_tokens,
                   s.schedule_id, s.workflow_json, s.user_workflow_values_json,
                   s.provider_name, s.model_config_json, s.diverged_from,
                   COUNT(m.id) as message_count
            FROM sessions s
            INNER JOIN messages m ON s.id = m.session_id
            WHERE s.session_type IN ({})
            GROUP BY s.id
            ORDER BY s.updated_at DESC
            "#,
            placeholders
        );

        let mut q = sqlx::query_as::<_, Session>(&query);
        for t in types {
            q = q.bind(t.to_string());
        }

        let pool = self.pool().await?;
        q.fetch_all(pool).await.map_err(Into::into)
    }

    async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.list_sessions_by_types(&[SessionType::User, SessionType::Scheduled])
            .await
    }

    async fn list_session_summaries(&self, limit: u32, offset: u32) -> Result<Vec<SessionSummary>> {
        let pool = self.pool().await?;
        sqlx::query_as::<_, SessionSummary>(
            r#"
            SELECT s.id,
                   s.working_dir,
                   COALESCE(NULLIF(s.name, ''), NULLIF(s.description, ''), 'Untitled chat') AS name,
                   s.created_at,
                   s.updated_at,
                   COUNT(m.id) AS message_count
            FROM sessions s
            INNER JOIN messages m ON s.id = m.session_id
            WHERE s.session_type IN ('user', 'scheduled')
            GROUP BY s.id
            ORDER BY s.updated_at DESC, s.id ASC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(i64::from(limit))
        .bind(i64::from(offset))
        .fetch_all(pool)
        .await
        .map_err(Into::into)
    }

    async fn delete_session(&self, session_id: &str) -> Result<()> {
        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;

        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?)")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;

        if !exists {
            return Err(anyhow::anyhow!("Session not found"));
        }

        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        // The session's externalized tool-result payloads go with it (BR-7).
        sqlx::query("DELETE FROM message_blobs WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn count_all_sessions(&self) -> Result<u64> {
        let pool = self.pool().await?;
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
            .fetch_one(pool)
            .await?;
        Ok(count as u64)
    }

    async fn clear_all_sessions(&self) -> Result<u64> {
        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
            .fetch_one(&mut *tx)
            .await?;

        if Self::messages_fts_exists(&mut *tx).await {
            sqlx::query("DELETE FROM messages_fts")
                .execute(&mut *tx)
                .await?;
        }
        for table in [
            "message_blobs",
            "checkpoints",
            "messages",
            "token_events",
            "sessions",
        ] {
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&mut *tx)
                .await?;
        }
        // The AUTOINCREMENT high-water marks are deliberately LEFT ALONE
        // (#51 W3). `messages.id` is what [`ConversationRevision`] is built
        // from, and its whole value is that a rowid is never minted twice for
        // the lifetime of the database. Rewinding `sqlite_sequence` here made a
        // wipe hand the next session the rowids of the one it deleted, so a
        // detached rewrite holding a pre-wipe revision could pass the freshness
        // guard against a brand-new conversation and destroy it. `token_events`
        // rides along for the same reason: its ids are referenced by the usage
        // ledger's own dedupe, and there is nothing to gain from replaying
        // them. A cleared database is empty either way; only the two counters
        // survive, at 8 bytes each.
        tx.commit().await?;
        Ok(count as u64)
    }

    async fn get_insights(&self) -> Result<SessionInsights> {
        let pool = self.pool().await?;

        // Sessions: totals plus 7d/30d windows.
        //
        // The session windows key on `updated_at` deliberately — an active
        // session counts as recent even if it was started earlier. Only
        // user-facing session types are counted, so these tiles agree with the
        // session list rendered beneath them.
        let sessions = sqlx::query_as::<_, (i64, i64, i64)>(
            r#"
            SELECT
              COUNT(*) AS total_sessions,
              COALESCE(SUM(CASE WHEN updated_at >= datetime('now', '-7 days') THEN 1 ELSE 0 END), 0) AS sessions_7d,
              COALESCE(SUM(CASE WHEN updated_at >= datetime('now', '-30 days') THEN 1 ELSE 0 END), 0) AS sessions_30d
            FROM sessions
            WHERE session_type IN ('user', 'scheduled')
            "#,
        )
        .fetch_one(pool)
        .await?;

        // Tokens: summed from per-turn events inside the window.
        //
        // The old query summed each session's WHOLE lifetime total if the session
        // had merely been touched in the window, so a 60-day-old session holding
        // 2,000,000 tokens that received one reply today contributed all
        // 2,000,000 to "past 7 days".
        let tokens = sqlx::query_as::<
            _,
            (
                i64,
                i64,
                Option<i64>,
                i64,
                i64,
                Option<i64>,
                i64,
                i64,
                Option<i64>,
            ),
        >(
            r#"
            SELECT
              COUNT(*),
              COUNT(te.billed_total_tokens),
              SUM(te.billed_total_tokens),
              COUNT(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-7 days') AS INTEGER) THEN 1 END),
              COUNT(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-7 days') AS INTEGER) THEN te.billed_total_tokens END),
              SUM(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-7 days') AS INTEGER) THEN te.billed_total_tokens END),
              COUNT(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-30 days') AS INTEGER) THEN 1 END),
              COUNT(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-30 days') AS INTEGER) THEN te.billed_total_tokens END),
              SUM(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-30 days') AS INTEGER) THEN te.billed_total_tokens END)
            FROM token_events te
            WHERE te.session_type IN ('user', 'scheduled')
            "#,
        )
        .fetch_one(pool)
        .await?;

        Ok(SessionInsights {
            total_sessions: sessions.0 as usize,
            total_tokens: complete_sum(tokens.0, tokens.1, tokens.2),
            sessions_last_7_days: sessions.1.max(0) as usize,
            sessions_last_30_days: sessions.2.max(0) as usize,
            tokens_last_7_days: complete_sum(tokens.3, tokens.4, tokens.5),
            tokens_last_30_days: complete_sum(tokens.6, tokens.7, tokens.8),
        })
    }

    /// Record one turn's usage. Append-only; never updated, never deleted.
    #[allow(clippy::too_many_arguments)]
    async fn record_token_event(
        &self,
        session_id: &str,
        input: Option<i32>,
        output: Option<i32>,
        total: i64,
        model: Option<&str>,
        provider: Option<&str>,
        cache_read: Option<i32>,
        cache_creation: Option<i32>,
    ) -> Result<()> {
        let pool = self.pool().await?;
        // An empty model/provider string is stored as NULL so it aggregates with
        // the genuinely-unknown rows rather than as a distinct "" group.
        let model = model.filter(|m| !m.is_empty());
        let provider = provider.filter(|p| !p.is_empty());
        sqlx::query(
            r#"
            INSERT INTO token_events
                (session_id, ts, input_tokens, output_tokens, total_tokens, billed_total_tokens, model_id, provider,
                 cache_read_tokens, cache_creation_tokens, session_type)
            VALUES (?, CAST(strftime('%s', 'now') AS INTEGER), ?, ?, ?, ?, ?, ?, ?, ?,
                    (SELECT session_type FROM sessions WHERE id = ?))
            "#,
        )
        .bind(session_id)
        .bind(input.map(i64::from))
        .bind(output.map(i64::from))
        .bind(total)
        .bind(total)
        .bind(model)
        .bind(provider)
        .bind(cache_read.map(i64::from))
        .bind(cache_creation.map(i64::from))
        .bind(session_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn apply_usage_event(&self, entry: UsageLedgerEntry) -> Result<bool> {
        if entry.event_key.trim().is_empty() {
            anyhow::bail!("usage event key must not be empty");
        }

        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;
        let model = entry.model_id.as_deref().filter(|model| !model.is_empty());
        let provider = entry
            .provider
            .as_deref()
            .filter(|provider| !provider.is_empty());
        let context_or_legacy_total = entry
            .current_total_tokens
            .map(i64::from)
            .or(entry.billed_total_tokens)
            .unwrap_or(0);
        let inserted = sqlx::query(
            r#"
            INSERT INTO token_events
                (session_id, ts, input_tokens, output_tokens, total_tokens, billed_total_tokens, model_id, provider,
                 cache_read_tokens, cache_creation_tokens, event_key, session_type)
            VALUES (?, CAST(strftime('%s', 'now') AS INTEGER), ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    (SELECT session_type FROM sessions WHERE id = ?))
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&entry.session_id)
        .bind(entry.input_tokens.map(i64::from))
        .bind(entry.output_tokens.map(i64::from))
        .bind(context_or_legacy_total)
        .bind(entry.billed_total_tokens)
        .bind(model)
        .bind(provider)
        .bind(entry.cache_read_tokens.map(i64::from))
        .bind(entry.cache_creation_tokens.map(i64::from))
        .bind(&entry.event_key)
        .bind(&entry.session_id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;

        if !inserted {
            tx.commit().await?;
            return Ok(false);
        }

        let update_current = entry.current_total_tokens.is_some()
            || entry.current_input_tokens.is_some()
            || entry.current_output_tokens.is_some();
        let input_delta = entry.input_tokens.map(i64::from);
        let output_delta = entry.output_tokens.map(i64::from);
        let updated = sqlx::query(
            r#"
            UPDATE sessions SET
                accumulated_total_tokens = CASE
                    WHEN ? IS NULL THEN accumulated_total_tokens
                    ELSE COALESCE(accumulated_total_tokens, 0) + ? END,
                accumulated_input_tokens = CASE
                    WHEN ? IS NULL THEN accumulated_input_tokens
                    ELSE COALESCE(accumulated_input_tokens, 0) + ? END,
                accumulated_output_tokens = CASE
                    WHEN ? IS NULL THEN accumulated_output_tokens
                    ELSE COALESCE(accumulated_output_tokens, 0) + ? END,
                total_tokens = CASE WHEN ? THEN ? ELSE total_tokens END,
                input_tokens = CASE WHEN ? THEN ? ELSE input_tokens END,
                output_tokens = CASE WHEN ? THEN ? ELSE output_tokens END,
                schedule_id = ?,
                updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(entry.billed_total_tokens)
        .bind(entry.billed_total_tokens)
        .bind(input_delta)
        .bind(input_delta)
        .bind(output_delta)
        .bind(output_delta)
        .bind(update_current)
        .bind(entry.current_total_tokens)
        .bind(update_current)
        .bind(entry.current_input_tokens)
        .bind(update_current)
        .bind(entry.current_output_tokens)
        .bind(&entry.schedule_id)
        .bind(&entry.session_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if updated != 1 {
            anyhow::bail!("session not found");
        }

        tx.commit().await?;
        Ok(true)
    }

    /// Per-model rollup for one session. NULL `model_id` groups as its own row
    /// (the caller surfaces it as "unknown").
    async fn get_session_model_usage(&self, session_id: &str) -> Result<Vec<ModelUsageRow>> {
        let pool = self.pool().await?;
        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?)")
                .bind(session_id)
                .fetch_one(pool)
                .await?;
        if !exists {
            anyhow::bail!("session not found");
        }
        let rows = sqlx::query_as::<_, ModelUsageRow>(
            r#"
            SELECT model_id,
                   provider,
                   COALESCE(SUM(input_tokens), 0)  AS input_tokens,
                   COALESCE(SUM(output_tokens), 0) AS output_tokens,
                   CASE WHEN COUNT(billed_total_tokens) = COUNT(*)
                        THEN SUM(billed_total_tokens) END AS total_tokens,
                   CASE WHEN COUNT(cache_read_tokens) = COUNT(*)
                        THEN SUM(cache_read_tokens) END AS cache_read_tokens,
                   CASE WHEN COUNT(cache_creation_tokens) = COUNT(*)
                        THEN SUM(cache_creation_tokens) END AS cache_creation_tokens,
                   COUNT(*)                        AS turns
            FROM token_events
            WHERE session_id = ?1
            GROUP BY model_id, provider
            ORDER BY total_tokens DESC
            "#,
        )
        .bind(session_id)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Global per-model rollup over the inclusive `[from, to]` unix-second window,
    /// restricted to billable session types. Subagent calls are real provider
    /// spend even though their internal sessions stay hidden from session lists.
    async fn get_model_usage(&self, from: i64, to: i64) -> Result<Vec<ModelUsageRow>> {
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, ModelUsageRow>(
            r#"
            SELECT te.model_id,
                   te.provider,
                   COALESCE(SUM(te.input_tokens), 0)  AS input_tokens,
                   COALESCE(SUM(te.output_tokens), 0) AS output_tokens,
                   CASE WHEN COUNT(te.billed_total_tokens) = COUNT(*)
                        THEN SUM(te.billed_total_tokens) END AS total_tokens,
                   CASE WHEN COUNT(te.cache_read_tokens) = COUNT(*)
                        THEN SUM(te.cache_read_tokens) END AS cache_read_tokens,
                   CASE WHEN COUNT(te.cache_creation_tokens) = COUNT(*)
                        THEN SUM(te.cache_creation_tokens) END AS cache_creation_tokens,
                   COUNT(*)                           AS turns
            FROM token_events te
            WHERE te.session_type IN ('user', 'scheduled', 'sub_agent')
              AND te.ts >= ?1 AND te.ts <= ?2
            GROUP BY te.model_id, te.provider
            ORDER BY total_tokens DESC
            "#,
        )
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
        Ok(rows)
    }

    /// Queryable usage report over the inclusive `[from, to]` unix-second window.
    ///
    /// The SQL always groups at the finest `(day, model, provider)` grain; Rust
    /// then prices each grain row once and rolls it up into `group`. That order
    /// is what lets a `Day` bucket report a correct dollar cost even though the
    /// day mixes models at different prices.
    async fn get_usage_report(
        &self,
        from: i64,
        to: i64,
        group: UsageGroup,
    ) -> Result<Vec<UsageReportRow>> {
        let pool = self.pool().await?;
        let grain = sqlx::query_as::<_, UsageGrainRow>(
            r#"
            SELECT date(te.ts, 'unixepoch', 'localtime') AS day,
                   te.model_id,
                   te.provider,
                   COALESCE(SUM(te.input_tokens), 0)  AS input_tokens,
                   COALESCE(SUM(te.output_tokens), 0) AS output_tokens,
                   CASE WHEN COUNT(te.billed_total_tokens) = COUNT(*)
                        THEN SUM(te.billed_total_tokens) END AS total_tokens,
                   CASE WHEN COUNT(te.cache_read_tokens) = COUNT(*)
                        THEN SUM(te.cache_read_tokens) END AS cache_read_tokens,
                   CASE WHEN COUNT(te.cache_creation_tokens) = COUNT(*)
                        THEN SUM(te.cache_creation_tokens) END AS cache_creation_tokens,
                   COUNT(*) AS turns,
                   CAST(COUNT(te.input_tokens) = COUNT(*) AS INTEGER) AS input_complete,
                   CAST(COUNT(te.output_tokens) = COUNT(*) AS INTEGER) AS output_complete,
                   CAST(COUNT(te.cache_read_tokens) = COUNT(*) AS INTEGER) AS cache_read_complete,
                   CAST(COUNT(te.cache_creation_tokens) = COUNT(*) AS INTEGER) AS cache_creation_complete
            FROM token_events te
            WHERE te.session_type IN ('user', 'scheduled', 'sub_agent')
              AND te.ts >= ?1 AND te.ts <= ?2
            GROUP BY day, te.model_id, te.provider
            "#,
        )
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await?;
        let pricing = resolve_grain_pricing(&grain).await;
        Ok(rollup_report_with_pricing(&grain, group, &pricing))
    }

    /// Month-to-date (current local month) + all-time priced totals.
    async fn get_usage_summary(&self) -> Result<UsageSummary> {
        let pool = self.pool().await?;

        // Per-model grain is required so each model prices at its own rate before
        // summing; `day` is unused here, so a constant keeps the shared struct.
        let mtd_grain = sqlx::query_as::<_, UsageGrainRow>(
            r#"
            SELECT '' AS day,
                   te.model_id,
                   te.provider,
                   COALESCE(SUM(te.input_tokens), 0)  AS input_tokens,
                   COALESCE(SUM(te.output_tokens), 0) AS output_tokens,
                   CASE WHEN COUNT(te.billed_total_tokens) = COUNT(*)
                        THEN SUM(te.billed_total_tokens) END AS total_tokens,
                   CASE WHEN COUNT(te.cache_read_tokens) = COUNT(*)
                        THEN SUM(te.cache_read_tokens) END AS cache_read_tokens,
                   CASE WHEN COUNT(te.cache_creation_tokens) = COUNT(*)
                        THEN SUM(te.cache_creation_tokens) END AS cache_creation_tokens,
                   COUNT(*) AS turns,
                   CAST(COUNT(te.input_tokens) = COUNT(*) AS INTEGER) AS input_complete,
                   CAST(COUNT(te.output_tokens) = COUNT(*) AS INTEGER) AS output_complete,
                   CAST(COUNT(te.cache_read_tokens) = COUNT(*) AS INTEGER) AS cache_read_complete,
                   CAST(COUNT(te.cache_creation_tokens) = COUNT(*) AS INTEGER) AS cache_creation_complete
            FROM token_events te
            WHERE te.session_type IN ('user', 'scheduled', 'sub_agent')
              AND strftime('%Y-%m', te.ts, 'unixepoch', 'localtime')
                  = strftime('%Y-%m', 'now', 'localtime')
            GROUP BY te.model_id, te.provider
            "#,
        )
        .fetch_all(pool)
        .await?;

        let all_grain = sqlx::query_as::<_, UsageGrainRow>(
            r#"
            SELECT '' AS day,
                   te.model_id,
                   te.provider,
                   COALESCE(SUM(te.input_tokens), 0)  AS input_tokens,
                   COALESCE(SUM(te.output_tokens), 0) AS output_tokens,
                   CASE WHEN COUNT(te.billed_total_tokens) = COUNT(*)
                        THEN SUM(te.billed_total_tokens) END AS total_tokens,
                   CASE WHEN COUNT(te.cache_read_tokens) = COUNT(*)
                        THEN SUM(te.cache_read_tokens) END AS cache_read_tokens,
                   CASE WHEN COUNT(te.cache_creation_tokens) = COUNT(*)
                        THEN SUM(te.cache_creation_tokens) END AS cache_creation_tokens,
                   COUNT(*) AS turns,
                   CAST(COUNT(te.input_tokens) = COUNT(*) AS INTEGER) AS input_complete,
                   CAST(COUNT(te.output_tokens) = COUNT(*) AS INTEGER) AS output_complete,
                   CAST(COUNT(te.cache_read_tokens) = COUNT(*) AS INTEGER) AS cache_read_complete,
                   CAST(COUNT(te.cache_creation_tokens) = COUNT(*) AS INTEGER) AS cache_creation_complete
            FROM token_events te
            WHERE te.session_type IN ('user', 'scheduled', 'sub_agent')
            GROUP BY te.model_id, te.provider
            "#,
        )
        .fetch_all(pool)
        .await?;

        let month: String = sqlx::query_scalar("SELECT strftime('%Y-%m', 'now', 'localtime')")
            .fetch_one(pool)
            .await?;
        let mtd_pricing = resolve_grain_pricing(&mtd_grain).await;
        let all_pricing = resolve_grain_pricing(&all_grain).await;

        Ok(UsageSummary {
            month,
            month_to_date: totals_from_grain_with_pricing(&mtd_grain, &mtd_pricing),
            all_time: totals_from_grain_with_pricing(&all_grain, &all_pricing),
        })
    }

    async fn get_activity(&self, days: i64) -> Result<ActivityWindow> {
        let pool = self.pool().await?;
        let days = days.clamp(1, 371);
        // SQLite's `-N days` modifier takes a literal, so build it once.
        let window = format!("-{days} days");

        // Sessions started per LOCAL calendar day. `created_at` never moves, so a
        // day's session count is stable across renders — unlike `updated_at`.
        let session_rows = sqlx::query_as::<_, (String, i64)>(
            r#"
            SELECT date(created_at, 'localtime') AS day, COUNT(*) AS n
            FROM sessions
            WHERE session_type IN ('user', 'scheduled')
              AND created_at >= datetime('now', ?1)
            GROUP BY day
            "#,
        )
        .bind(&window)
        .fetch_all(pool)
        .await?;

        let token_rows = sqlx::query_as::<_, (String, i64, i64, i64, bool)>(
            r#"
            SELECT date(te.ts, 'unixepoch', 'localtime') AS day,
                   COALESCE(SUM(te.billed_total_tokens), 0) AS tokens,
                   COALESCE(SUM(te.input_tokens), 0)  AS input_tokens,
                   COALESCE(SUM(te.output_tokens), 0) AS output_tokens,
                   COUNT(te.billed_total_tokens) = COUNT(*) AS tokens_complete
            FROM token_events te
            WHERE te.session_type IN ('user', 'scheduled')
              AND te.ts >= CAST(strftime('%s', 'now', ?1) AS INTEGER)
            GROUP BY day
            "#,
        )
        .bind(&window)
        .fetch_all(pool)
        .await?;

        // `messages.created_timestamp` is unix SECONDS (Message::new uses
        // `Utc::now().timestamp()`), not milliseconds.
        let message_rows = sqlx::query_as::<_, (String, i64)>(
            r#"
            SELECT date(m.created_timestamp, 'unixepoch', 'localtime') AS day, COUNT(*) AS n
            FROM messages m
            JOIN sessions s ON s.id = m.session_id
            WHERE s.session_type IN ('user', 'scheduled')
              AND m.created_timestamp >= CAST(strftime('%s', 'now', ?1) AS INTEGER)
            GROUP BY day
            "#,
        )
        .bind(&window)
        .fetch_all(pool)
        .await?;

        let bounds = sqlx::query_as::<_, (String, String)>(
            "SELECT date('now', ?1, 'localtime'), date('now', 'localtime')",
        )
        .bind(&window)
        .fetch_one(pool)
        .await?;

        Ok(build_activity_window(
            bounds.0,
            bounds.1,
            &session_rows,
            &token_rows,
            &message_rows,
        ))
    }

    async fn export_session(&self, id: &str) -> Result<String> {
        let session = self.get_session(id, true).await?;
        serde_json::to_string_pretty(&session).map_err(Into::into)
    }

    async fn import_session(
        &self,
        session_manager: &SessionManager,
        json: &str,
    ) -> Result<Session> {
        let import: Session = serde_json::from_str(json)?;

        let session = self
            .create_session(
                import.working_dir.clone(),
                import.name.clone(),
                import.session_type,
            )
            .await?;

        let mut builder = session_manager
            .update(&session.id)
            .extension_data(import.extension_data)
            .total_tokens(import.total_tokens)
            .input_tokens(import.input_tokens)
            .output_tokens(import.output_tokens)
            .accumulated_total_tokens(import.accumulated_total_tokens)
            .accumulated_input_tokens(import.accumulated_input_tokens)
            .accumulated_output_tokens(import.accumulated_output_tokens)
            .schedule_id(import.schedule_id)
            .workflow(import.workflow)
            .user_workflow_values(import.user_workflow_values);

        if import.user_set_name {
            builder = builder.user_provided_name(import.name.clone());
        }

        builder.apply().await?;

        if let Some(conversation) = import.conversation {
            self.replace_conversation(&session.id, &conversation)
                .await?;
        }

        self.get_session(&session.id, true).await
    }

    async fn copy_session(
        &self,
        session_manager: &SessionManager,
        session_id: &str,
        new_name: String,
    ) -> Result<Session> {
        let original_session = self.get_session(session_id, true).await?;

        let new_session = self
            .create_session(
                original_session.working_dir.clone(),
                new_name,
                original_session.session_type,
            )
            .await?;

        session_manager
            .update(&new_session.id)
            .extension_data(original_session.extension_data)
            .schedule_id(original_session.schedule_id)
            .workflow(original_session.workflow)
            .user_workflow_values(original_session.user_workflow_values)
            .apply()
            .await?;

        if let Some(conversation) = original_session.conversation {
            self.replace_conversation(&new_session.id, &conversation)
                .await?;
        }

        self.get_session(&new_session.id, true).await
    }

    async fn diverge_session_for_edit(
        &self,
        session_manager: &SessionManager,
        session_id: &str,
        timestamp: i64,
    ) -> Result<Session> {
        let original = self.get_session(session_id, true).await?;
        let new_name = self.compute_branch_name(&original).await?;
        let branch_point = original.conversation.as_ref().and_then(|conversation| {
            conversation
                .messages()
                .iter()
                .rfind(|message| message.created < timestamp)
                .and_then(|message| message.id.clone())
        });

        let new_session = self
            .copy_session(session_manager, session_id, new_name.clone())
            .await?;

        session_manager
            .update(&new_session.id)
            .user_provided_name(new_name)
            .diverged_from(Some(session_id.to_string()))
            .branch_point_msg_uid(branch_point)
            .apply()
            .await?;

        self.truncate_conversation(&new_session.id, timestamp)
            .await?;
        self.get_session(&new_session.id, true).await
    }

    async fn diverge_session(
        &self,
        session_manager: &SessionManager,
        session_id: &str,
        custom_name: Option<String>,
        anchor_ms: Option<i64>,
        anchor_uid: Option<String>,
    ) -> Result<Session> {
        // Load original first (with conversation) so we can derive a name and
        // confirm it exists.
        let original = self.get_session(session_id, true).await?;

        let new_name = match custom_name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => self.compute_branch_name(&original).await?,
        };

        // Build the branch conversation: the parent's history trimmed to end at
        // the last complete assistant answer (so a mid-generation diverge never
        // carries over an unanswered question or a dangling tool call). The
        // durable message id (`anchor_uid`) is preferred over the timestamp so a
        // divergence at one of two same-second messages does not over-truncate.
        let branch_conversation = original
            .conversation
            .as_ref()
            .map(|c| trim_to_last_complete_answer_at(c, anchor_uid.as_deref(), anchor_ms))
            .unwrap_or_default();

        // Record the divergence point: the explicit anchor id when supplied, else the
        // id of the last message actually carried into the branch.
        let branch_point = anchor_uid.clone().or_else(|| {
            branch_conversation
                .messages()
                .last()
                .and_then(|m| m.id.clone())
        });

        // Mint the branch session and copy the carry-over metadata (mirrors
        // copy_session, but writes the *trimmed* conversation rather than the
        // full one).
        let new_session = self
            .create_session(
                original.working_dir.clone(),
                new_name.clone(),
                original.session_type,
            )
            .await?;

        session_manager
            .update(&new_session.id)
            .extension_data(original.extension_data)
            .schedule_id(original.schedule_id)
            .workflow(original.workflow)
            .user_workflow_values(original.user_workflow_values)
            // Lock the computed/custom name (so the auto-namer never clobbers
            // the branch marker) and record the lineage pointer + divergence point.
            .user_provided_name(new_name)
            .diverged_from(Some(session_id.to_string()))
            .branch_point_msg_uid(branch_point)
            .apply()
            .await?;

        self.replace_conversation(&new_session.id, &branch_conversation)
            .await?;

        self.get_session(&new_session.id, true).await
    }

    /// Derive `"{base} (branch {N})"` for a diverged session. `base` is the
    /// parent's name with any `(branch K)` suffix stripped, falling back to a
    /// conversation-derived title when the parent's name is just a placeholder.
    /// `N` is the next free index across that base's branch family.
    async fn compute_branch_name(&self, original: &Session) -> Result<String> {
        let stripped = strip_branch_suffix(&original.name);
        let base = if is_default_session_name(stripped) {
            let derived = original
                .conversation
                .as_ref()
                .map(SessionManager::fallback_session_name)
                .unwrap_or_default();
            if derived.trim().is_empty() {
                "Conversation".to_string()
            } else {
                derived
            }
        } else {
            stripped.to_string()
        };

        let next = self.count_branch_siblings(&base).await? + 1;
        Ok(format!("{base} (branch {next})"))
    }

    /// Count existing sessions named `"{base} (branch <digits>)"` so the next
    /// branch gets a unique index. The base is escaped for use in a SQL LIKE.
    async fn count_branch_siblings(&self, base: &str) -> Result<i64> {
        let pool = self.pool().await?;
        let pattern = format!("{} (branch %)", like_escape(base));
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sessions WHERE name LIKE ? ESCAPE '\\'",
        )
        .bind(pattern)
        .fetch_one(pool)
        .await?;
        Ok(count)
    }

    /// Delete every message of `session_id` at or after `timestamp`, together
    /// with the side state those rows owned, in ONE transaction.
    ///
    /// `bound` bounds the delete by rowid. `None` means "whatever exists when
    /// this transaction takes the write lock", which is the historical
    /// timestamp-only behaviour; `Some(basis)` restricts it to the rows that
    /// caller's own view covered, so an append that landed after that view was
    /// taken — necessarily newer, therefore necessarily inside an open-ended
    /// `created_timestamp >= ?` range — is NOT part of the tail being dropped.
    /// A `basis` from a previous incarnation of the id is refused: its rowids
    /// describe a conversation that no longer exists.
    ///
    /// Three things used to be skipped here that the rewrite path has always
    /// done, and all three are why this is a transaction rather than a
    /// statement:
    /// - the FTS recall mirror kept a row per deleted message forever (every
    ///   checkpoint restore leaked more; only an unrelated `INNER JOIN
    ///   messages` in the search query kept them from surfacing as hits),
    /// - the BR-7 payload of a deleted oversized tool response was stranded in
    ///   `message_blobs` with nothing referencing it,
    /// - `sessions.updated_at` never moved, so an edited or restored session
    ///   sorted as untouched in the `ORDER BY updated_at DESC` list.
    ///
    /// Returns how many message rows were removed.
    async fn truncate_conversation_inner(
        &self,
        session_id: &str,
        timestamp: i64,
        bound: Option<ConversationRevision>,
    ) -> Result<TruncateOutcome> {
        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;

        // LOAD-BEARING FIRST STATEMENT, AND IT MUST BE A WRITE. DO NOT REORDER.
        // Identical reasoning to `replace_conversation_inner`: a deferred
        // transaction that reads first pins a WAL snapshot and the later
        // upgrade to a writer fails SQLITE_BUSY_SNAPSHOT immediately, bypassing
        // the busy timeout. Opening with the `updated_at` bump takes the write
        // lock up front, which is also what makes the row set selected below
        // identical to the row set deleted afterwards.
        let touched = sqlx::query("UPDATE sessions SET updated_at = datetime('now') WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        if touched.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(TruncateOutcome::SessionNotFound);
        }

        let upper = match bound {
            // Row identity is checked UNDER THE LOCK, alongside the watermark it
            // qualifies — reading it from the pool first would leave a window in
            // which the id is re-issued between the check and the delete.
            Some(basis) => {
                if Self::read_incarnation(&mut tx, session_id).await? != basis.incarnation {
                    tx.rollback().await?;
                    return Ok(TruncateOutcome::Stale);
                }
                basis.max_rowid
            }
            // Read under the lock, so it names exactly the rows that exist now.
            None => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT IFNULL(MAX(id), 0) FROM messages WHERE session_id = ?",
                )
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?
            }
        };

        const DOOMED: &str =
            "FROM messages WHERE session_id = ? AND created_timestamp >= ? AND id <= ?";

        // The doomed rows' payload references, captured before they go. Nothing
        // can interleave under the write lock, so re-evaluating the same
        // predicate in the DELETE below selects exactly this set — which is why
        // there is no need to marshal thousands of ids through an IN list.
        let doomed = sqlx::query_scalar::<_, String>(&format!("SELECT content_json {DOOMED}"))
            .bind(session_id)
            .bind(timestamp)
            .bind(upper)
            .fetch_all(&mut *tx)
            .await?;
        if doomed.is_empty() {
            tx.commit().await?;
            return Ok(TruncateOutcome::Truncated { removed: 0 });
        }
        let dropped_a_stub = doomed
            .iter()
            .any(|content_json| message_blobs::content_json_has_stub(content_json));

        if Self::messages_fts_exists(&mut *tx).await {
            sqlx::query(&format!(
                "DELETE FROM messages_fts WHERE session_id = ? \
                 AND message_id IN (SELECT id {DOOMED})"
            ))
            .bind(session_id)
            .bind(session_id)
            .bind(timestamp)
            .bind(upper)
            .execute(&mut *tx)
            .await?;
        }

        let removed = sqlx::query(&format!("DELETE {DOOMED}"))
            .bind(session_id)
            .bind(timestamp)
            .bind(upper)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;

        // Only when a dropped row actually carried a stub — the same "pay for
        // the scan only when there is something to sweep" discipline as the
        // read path.
        if dropped_a_stub {
            let survivors = sqlx::query_scalar::<_, String>(
                "SELECT content_json FROM messages WHERE session_id = ?",
            )
            .bind(session_id)
            .fetch_all(&mut *tx)
            .await?;
            let mut live_blob_uids: Vec<String> = Vec::new();
            for content_json in &survivors {
                if !message_blobs::content_json_has_stub(content_json) {
                    continue;
                }
                let content: Vec<MessageContent> = serde_json::from_str(content_json)?;
                live_blob_uids.extend(message_blobs::referenced_uids(&content));
            }
            Self::sweep_orphan_blobs(&mut tx, session_id, &live_blob_uids).await?;
        }

        tx.commit().await?;
        Ok(TruncateOutcome::Truncated { removed })
    }

    async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.truncate_conversation_inner(session_id, timestamp, None)
            .await
            .map(|_| ())
    }

    /// See [`SessionManager::truncate_conversation_bounded`].
    async fn truncate_conversation_bounded(
        &self,
        session_id: &str,
        timestamp: i64,
        basis: ConversationRevision,
    ) -> Result<TruncateOutcome> {
        self.truncate_conversation_inner(session_id, timestamp, Some(basis))
            .await
    }

    // BR-43 shadow-git checkpoints: the `checkpoints` side-table CRUD. Kept here
    // (rather than the `checkpoint` module) because `SessionStorage` owns the
    // SQLite pool; `CheckpointManager` calls these through `SessionManager`.

    async fn insert_checkpoint(&self, rec: &crate::checkpoint::CheckpointRecord) -> Result<()> {
        let pool = self.pool().await?;
        let changed = serde_json::to_string(&rec.changed_paths)?;
        sqlx::query(
            r#"INSERT INTO checkpoints
                (id, session_id, turn_index, anchor_ts, kind, commit_sha, tree_sha, changed_paths_json, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&rec.id)
        .bind(&rec.session_id)
        .bind(rec.turn_index)
        .bind(rec.anchor_ts)
        .bind(rec.kind.as_str())
        .bind(&rec.commit_sha)
        .bind(&rec.tree_sha)
        .bind(changed)
        .bind(&rec.created_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn list_checkpoints(
        &self,
        session_id: &str,
    ) -> Result<Vec<crate::checkpoint::CheckpointRecord>> {
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, CheckpointRow>(
            "SELECT id, session_id, turn_index, anchor_ts, kind, commit_sha, tree_sha, changed_paths_json, created_at
             FROM checkpoints WHERE session_id = ? ORDER BY turn_index DESC",
        )
        .bind(session_id)
        .fetch_all(pool)
        .await?;
        rows.into_iter().map(CheckpointRow::into_record).collect()
    }

    /// The highest-`turn_index` checkpoint, for the next ordinal + `tree_sha`
    /// dedup baseline.
    async fn last_checkpoint(
        &self,
        session_id: &str,
    ) -> Result<Option<crate::checkpoint::CheckpointRecord>> {
        let pool = self.pool().await?;
        let row = sqlx::query_as::<_, CheckpointRow>(
            "SELECT id, session_id, turn_index, anchor_ts, kind, commit_sha, tree_sha, changed_paths_json, created_at
             FROM checkpoints WHERE session_id = ? ORDER BY turn_index DESC LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
        row.map(CheckpointRow::into_record).transpose()
    }

    async fn get_checkpoint(
        &self,
        session_id: &str,
        checkpoint_id: &str,
    ) -> Result<Option<crate::checkpoint::CheckpointRecord>> {
        let pool = self.pool().await?;
        let row = sqlx::query_as::<_, CheckpointRow>(
            "SELECT id, session_id, turn_index, anchor_ts, kind, commit_sha, tree_sha, changed_paths_json, created_at
             FROM checkpoints WHERE session_id = ? AND id = ?",
        )
        .bind(session_id)
        .bind(checkpoint_id)
        .fetch_optional(pool)
        .await?;
        row.map(CheckpointRow::into_record).transpose()
    }

    async fn delete_checkpoints(&self, session_id: &str) -> Result<()> {
        let pool = self.pool().await?;
        sqlx::query("DELETE FROM checkpoints WHERE session_id = ?")
            .bind(session_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn search_chat_history(
        &self,
        query: &str,
        limit: Option<usize>,
        after_date: Option<chrono::DateTime<chrono::Utc>>,
        before_date: Option<chrono::DateTime<chrono::Utc>>,
        exclude_session_id: Option<String>,
    ) -> Result<crate::session::chat_history_search::ChatRecallResults> {
        use crate::session::chat_history_search::ChatHistorySearch;

        let pool = self.pool().await?;
        ChatHistorySearch::new(
            pool,
            query,
            limit,
            after_date,
            before_date,
            exclude_session_id,
        )
        .execute()
        .await
    }
}

#[cfg(test)]
mod blob_tests {
    //! BR-7: externalizing an oversized tool result out of `content_json`.
    //!
    //! These drive the storage layer directly (`get_conversation_inner`) rather
    //! than through the `BIOROUTER_SESSION_BLOB_LAZY_LOAD` env var, so both read
    //! modes are covered deterministically under a parallel test runner.

    use super::*;
    use crate::conversation::message::{Message, ToolResponse};
    use rmcp::model::{CallToolResult, Content};
    use tempfile::TempDir;

    /// Comfortably over the 64 KiB default threshold.
    fn huge(marker: &str) -> String {
        (0..3_000)
            .map(|i| format!("{marker} row {i} of a very large tool result\n"))
            .collect()
    }

    fn tool_response_message(call_id: &str, text: String) -> Message {
        Message::assistant().with_content(MessageContent::ToolResponse(ToolResponse {
            id: call_id.to_string(),
            tool_result: Ok(CallToolResult {
                content: vec![Content::text(text)],
                structured_content: None,
                is_error: Some(false),
                meta: None,
            }),
            metadata: None,
        }))
    }

    fn response_text(conv: &Conversation, idx: usize) -> String {
        let MessageContent::ToolResponse(response) = &conv.messages()[idx].content[0] else {
            panic!("expected a tool response");
        };
        response.tool_result.as_ref().unwrap().content[0]
            .as_text()
            .unwrap()
            .text
            .clone()
    }

    async fn stored_content_json(sm: &SessionManager, session_id: &str) -> String {
        let pool = sm.storage().pool().await.unwrap();
        sqlx::query_scalar::<_, String>(
            "SELECT content_json FROM messages WHERE session_id = ? ORDER BY id LIMIT 1",
        )
        .bind(session_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn blob_count(sm: &SessionManager, session_id: &str) -> i64 {
        let pool = sm.storage().pool().await.unwrap();
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM message_blobs WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn session(sm: &SessionManager) -> String {
        sm.create_session(
            PathBuf::from("/tmp"),
            "blobs".to_string(),
            SessionType::User,
        )
        .await
        .unwrap()
        .id
    }

    /// #41: replaying an OVERSIZED message must be as idempotent as any other
    /// replay. The probe used to re-externalize the candidate — minting a
    /// fresh blob uid on every comparison — so a large-message replay never
    /// compared equal to its stored row and was re-inserted (duplicated)
    /// under a re-minted uid. The comparison now hydrates the stored stubs
    /// and compares pre-externalization payloads, which is stable.
    #[tokio::test]
    async fn an_oversized_replay_is_idempotent_not_a_reminted_duplicate() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        let message = tool_response_message("call_1", huge("r")).with_id("big-uid");
        let first = sm.add_message(&id, &message).await.unwrap();
        assert_eq!(first, "big-uid");

        let replay = sm
            .add_message(&id, &message)
            .await
            .expect("an oversized replay must be idempotent success");
        assert_eq!(
            replay, "big-uid",
            "the replay returns the SAME uid — no re-mint for identical content"
        );

        let loaded = sm.get_session(&id, true).await.unwrap();
        assert_eq!(
            loaded.conversation.unwrap().len(),
            1,
            "an oversized replay must not create a duplicate row"
        );
        assert_eq!(
            blob_count(&sm, &id).await,
            1,
            "an oversized replay must not spill a duplicate blob"
        );
    }

    /// The core of BR-7: the oversized payload leaves `content_json` for the side
    /// table, and the default (hydrating) read puts it back byte for byte — so
    /// no existing consumer can tell the difference.
    #[tokio::test]
    async fn an_oversized_tool_result_is_externalized_and_hydrated_back_exactly() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        let payload = huge("a");
        sm.add_message(&id, &tool_response_message("call_1", payload.clone()))
            .await
            .unwrap();

        // The message row is now tiny and carries only a stub.
        let row = stored_content_json(&sm, &id).await;
        assert!(
            row.len() < payload.len() / 10,
            "the stored row should be a stub, not the payload ({} vs {})",
            row.len(),
            payload.len()
        );
        assert!(message_blobs::content_json_has_stub(&row));
        assert_eq!(blob_count(&sm, &id).await, 1);

        // ...and the default read is indistinguishable from before.
        let conv = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(response_text(&conv, 0), payload);
    }

    #[tokio::test]
    async fn an_ordinary_tool_result_still_stores_inline() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        sm.add_message(&id, &tool_response_message("call_1", "small result".into()))
            .await
            .unwrap();

        assert!(stored_content_json(&sm, &id).await.contains("small result"));
        assert_eq!(blob_count(&sm, &id).await, 0);
    }

    /// The lazy read: the model gets the stub (preview + handle) and pulls the
    /// payload back only when it asks, through `get_message_blob`.
    #[tokio::test]
    async fn the_lazy_read_keeps_the_stub_and_the_handle_resolves() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        let payload = huge("b");
        sm.add_message(&id, &tool_response_message("call_1", payload.clone()))
            .await
            .unwrap();

        let conv = sm
            .storage()
            .get_conversation_inner(&id, false)
            .await
            .unwrap();
        let stub = response_text(&conv, 0);
        assert!(stub.len() < payload.len() / 10);
        assert!(stub.contains("platform__read_session_blob"));
        assert!(stub.contains("b row 0 of a very large tool result"));

        let uid = message_blobs::blob_uid_of(&stub).expect("the stub names a blob");
        assert_eq!(
            sm.get_message_blob(&id, uid).await.unwrap(),
            Some(payload.clone())
        );

        // A handle is scoped to its session: another session cannot read it.
        let other = session(&sm).await;
        assert_eq!(sm.get_message_blob(&other, uid).await.unwrap(), None);
    }

    /// Compaction/edit rewrites the whole conversation. Kept messages must keep
    /// their payload, and a dropped message's blob must not linger — otherwise
    /// BR-7 would just move the bloat from one table to another.
    #[tokio::test]
    async fn a_conversation_rewrite_keeps_live_blobs_and_sweeps_dropped_ones() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        let kept = huge("keep");
        let dropped = huge("drop");
        sm.add_message(&id, &tool_response_message("call_1", kept.clone()))
            .await
            .unwrap();
        sm.add_message(&id, &tool_response_message("call_2", dropped))
            .await
            .unwrap();
        assert_eq!(blob_count(&sm, &id).await, 2);

        // Rewrite with only the first message — exactly what compaction does.
        let full = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        let compacted = Conversation::new_unvalidated(vec![full.messages()[0].clone()]);
        sm.replace_conversation(&id, &compacted).await.unwrap();

        assert_eq!(blob_count(&sm, &id).await, 1);
        let conv = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(conv.messages().len(), 1);
        assert_eq!(response_text(&conv, 0), kept);
    }

    /// #51 W4: truncation drops message rows too, so it owes the side table the
    /// same sweep a rewrite does. Without it a checkpoint restore or a message
    /// edit strands every externalized payload behind the cut — megabytes per
    /// oversized tool result, kept alive by nothing and reachable by nobody.
    #[tokio::test]
    async fn a_truncation_sweeps_the_blobs_it_orphaned() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        let kept = huge("keep");
        let mut first = tool_response_message("call_1", kept.clone());
        first.created = 100;
        sm.add_message(&id, &first).await.unwrap();
        let mut dropped = tool_response_message("call_2", huge("drop"));
        dropped.created = 500;
        sm.add_message(&id, &dropped).await.unwrap();
        assert_eq!(blob_count(&sm, &id).await, 2);

        sm.truncate_conversation(&id, 500).await.unwrap();

        assert_eq!(
            blob_count(&sm, &id).await,
            1,
            "the truncated message's payload must go with it"
        );
        let conv = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(conv.messages().len(), 1);
        assert_eq!(
            response_text(&conv, 0),
            kept,
            "...and the surviving message's payload must still hydrate"
        );
    }

    /// A message RECOVERED by the freshness guard carries its externalized
    /// payload with it. The recovered rows must join the merged list BEFORE the
    /// blob accounting runs — build the list afterwards and `sweep_orphan_blobs`
    /// deletes the payload out from under a message that survives, leaving a
    /// dangling stub. Silent, and only visible on the next read.
    #[tokio::test]
    async fn preserving_tail_keeps_blobs_of_recovered_messages() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        sm.add_message(&id, &Message::user().with_text("prompt"))
            .await
            .unwrap();
        let (snap, basis) = sm.snapshot_for_rewrite(&id).await.unwrap();
        let known = snap.conversation.unwrap();

        // A concurrent writer appends an OVERSIZED tool result, which is
        // externalized into `message_blobs`.
        let payload = huge("recovered");
        sm.add_message(&id, &tool_response_message("call_1", payload.clone()))
            .await
            .unwrap();
        assert_eq!(blob_count(&sm, &id).await, 1);

        // ...and the caller compacts what it saw away.
        let replacement = Conversation::new_unvalidated(vec![Message::user().with_text("summary")]);
        let (outcome, _) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            ReplaceOutcome::ReplacedPreservingTail { preserved: 1 }
        );

        // Exactly one blob row: spared, not swept, and not duplicated by a
        // hydrate-then-re-externalize round trip.
        assert_eq!(blob_count(&sm, &id).await, 1);
        let conv = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(conv.messages().len(), 2);
        assert_eq!(
            response_text(&conv, 1),
            payload,
            "the recovered message must still hydrate to its full payload"
        );
    }

    /// The conversation RETURNED by a tail-preserving rewrite is what the live
    /// turn adopts (`conversation = stored`) and what the UI is told to render
    /// (`HistoryReplaced`). It must therefore carry the recovered tail's real
    /// payload, not the storage-shaped stub the rewrite (correctly) re-inserted.
    ///
    /// Before this was fixed the returned tail was ~1 KB of stub while the row
    /// on disk held the full 130 KB — so the model spent the rest of the turn
    /// reasoning over a placeholder, and only a reload healed the transcript.
    #[tokio::test]
    async fn preserving_tail_returns_a_hydrated_conversation_to_the_caller() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        sm.add_message(&id, &Message::user().with_text("prompt"))
            .await
            .unwrap();
        let (snap, basis) = sm.snapshot_for_rewrite(&id).await.unwrap();
        let known = snap.conversation.unwrap();

        // A concurrent writer appends an oversized tool result while the
        // summarizer runs; it is externalized on the way in.
        let payload = huge("recovered");
        sm.add_message(&id, &tool_response_message("call_1", payload.clone()))
            .await
            .unwrap();

        let replacement = Conversation::new_unvalidated(vec![Message::user().with_text("summary")]);
        let (outcome, returned) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();
        assert_eq!(
            outcome,
            ReplaceOutcome::ReplacedPreservingTail { preserved: 1 }
        );

        assert_eq!(returned.messages().len(), 2);
        assert_eq!(
            response_text(&returned, 1),
            payload,
            "the RETURNED tail must carry the payload, not the stub"
        );

        // And hydrating the returned copy must not have disturbed storage: one
        // blob, still exactly one, and a re-read still agrees.
        assert_eq!(blob_count(&sm, &id).await, 1);
        let reread = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(response_text(&reread, 1), payload);
    }

    /// A rewrite of a *lazily* loaded conversation carries stubs, not payloads.
    /// The stub must keep pointing at a live blob — the sweep must not mistake a
    /// surviving handle for an orphan and delete the payload out from under it.
    #[tokio::test]
    async fn rewriting_a_lazily_loaded_conversation_does_not_lose_the_payload() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        let payload = huge("c");
        sm.add_message(&id, &tool_response_message("call_1", payload.clone()))
            .await
            .unwrap();

        let lazy = sm
            .storage()
            .get_conversation_inner(&id, false)
            .await
            .unwrap();
        sm.replace_conversation(&id, &lazy).await.unwrap();

        assert_eq!(blob_count(&sm, &id).await, 1);
        let conv = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(response_text(&conv, 0), payload);
    }

    /// Diverging copies the parent's messages into the child. When those messages
    /// are stubs (lazy mode), the child must end up owning its own copy of the
    /// payload, so the two sessions' lifetimes are independent.
    #[tokio::test]
    async fn a_branch_that_inherits_a_stub_adopts_the_payload() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let parent = session(&sm).await;
        let child = session(&sm).await;

        let payload = huge("d");
        sm.add_message(&parent, &tool_response_message("call_1", payload.clone()))
            .await
            .unwrap();

        // The parent's history as a lazy reader sees it: stubs.
        let lazy = sm
            .storage()
            .get_conversation_inner(&parent, false)
            .await
            .unwrap();
        sm.replace_conversation(&child, &lazy).await.unwrap();
        assert_eq!(blob_count(&sm, &child).await, 1);

        // Deleting the parent leaves the branch intact.
        sm.delete_session(&parent).await.unwrap();
        let conv = sm
            .get_session(&child, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(response_text(&conv, 0), payload);
    }

    #[tokio::test]
    async fn deleting_a_session_takes_its_blobs_with_it() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = session(&sm).await;

        sm.add_message(&id, &tool_response_message("call_1", huge("e")))
            .await
            .unwrap();
        assert_eq!(blob_count(&sm, &id).await, 1);

        sm.delete_session(&id).await.unwrap();
        assert_eq!(blob_count(&sm, &id).await, 0);
    }

    /// The production upgrade path: a v15 DB gains `message_blobs` and keeps
    /// every inline message it already had (they are never rewritten).
    #[tokio::test]
    async fn migrates_v15_db_to_v16_message_blobs() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        {
            let opts = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();

            sqlx::query("CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)")
                .execute(&pool).await.unwrap();
            for v in 1..=15 {
                sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
                    .bind(v)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            sqlx::query(
                r#"CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
                    user_set_name BOOLEAN DEFAULT FALSE, session_type TEXT NOT NULL DEFAULT 'user', working_dir TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    extension_data TEXT DEFAULT '{}', total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER,
                    accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER, accumulated_output_tokens INTEGER,
                    schedule_id TEXT, workflow_json TEXT, user_workflow_values_json TEXT, provider_name TEXT,
                    model_config_json TEXT, diverged_from TEXT, external_key TEXT, branch_point_msg_uid TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            sqlx::query(
                r#"CREATE TABLE messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES sessions(id),
                    role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL,
                    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP, tokens INTEGER, metadata_json TEXT, msg_uid TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            SessionStorage::create_usage_schema(&pool).await.unwrap();
            sqlx::query(MESSAGES_FTS_DDL).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO sessions (id, name, working_dir) VALUES ('20240101_1', 'old', '/tmp/old')")
                .execute(&pool).await.unwrap();
            // A pre-v16 message keeps its content inline, forever.
            let legacy = serde_json::to_string(&vec![MessageContent::text("kept inline")]).unwrap();
            sqlx::query("INSERT INTO messages (session_id, role, content_json, created_timestamp, msg_uid) VALUES ('20240101_1', 'user', ?, 1, 'm1')")
                .bind(&legacy)
                .execute(&pool).await.unwrap();
            pool.close().await;
        }

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let pool = sm.storage().pool().await.unwrap();
        assert_eq!(
            SessionStorage::get_schema_version(pool).await.unwrap(),
            CURRENT_SCHEMA_VERSION
        );
        // The new table exists...
        assert_eq!(blob_count(&sm, "20240101_1").await, 0);
        // ...and the legacy message is untouched.
        let conv = sm
            .get_session("20240101_1", true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(conv.messages()[0].as_concat_text(), "kept inline");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent};
    use tempfile::TempDir;

    const NUM_CONCURRENT_SESSIONS: i32 = 10;

    // #44 — the atomic empty-chat-only working-dir update. The emptiness check
    // lives in the UPDATE's own WHERE clause, so "insert a message between the
    // check and the write" is impossible by construction; these tests pin the
    // SQL path's three outcomes and the unguarded escape hatch.
    mod working_dir_guard {
        use super::*;
        use crate::session::session_manager::WorkingDirUpdate;

        #[tokio::test]
        async fn updates_an_empty_session_and_persists() {
            let store = TempDir::new().unwrap();
            let sm = SessionManager::new(store.path().to_path_buf());
            let session = sm
                .create_session(PathBuf::from("/tmp/old"), "s".into(), SessionType::User)
                .await
                .unwrap();

            let outcome = sm
                .try_update_working_dir_if_empty(&session.id, PathBuf::from("/tmp/new"))
                .await
                .unwrap();
            assert_eq!(outcome, WorkingDirUpdate::Updated);

            let reloaded = sm.get_session(&session.id, false).await.unwrap();
            assert_eq!(reloaded.working_dir, PathBuf::from("/tmp/new"));
        }

        #[tokio::test]
        async fn refuses_once_any_message_exists() {
            let store = TempDir::new().unwrap();
            let sm = SessionManager::new(store.path().to_path_buf());
            let session = sm
                .create_session(PathBuf::from("/tmp/old"), "s".into(), SessionType::User)
                .await
                .unwrap();
            sm.add_message(&session.id, &Message::user().with_text("hello"))
                .await
                .unwrap();

            let outcome = sm
                .try_update_working_dir_if_empty(&session.id, PathBuf::from("/tmp/new"))
                .await
                .unwrap();
            assert_eq!(outcome, WorkingDirUpdate::RefusedNotEmpty);

            // The refused update must not have touched the row.
            let reloaded = sm.get_session(&session.id, false).await.unwrap();
            assert_eq!(reloaded.working_dir, PathBuf::from("/tmp/old"));
        }

        #[tokio::test]
        async fn reports_a_missing_session_without_writing() {
            let store = TempDir::new().unwrap();
            let sm = SessionManager::new(store.path().to_path_buf());

            let outcome = sm
                .try_update_working_dir_if_empty("no_such_session", PathBuf::from("/tmp/new"))
                .await
                .unwrap();
            assert_eq!(outcome, WorkingDirUpdate::SessionNotFound);
        }

        #[tokio::test]
        async fn force_update_bypasses_the_guard_for_shell_following() {
            let store = TempDir::new().unwrap();
            let sm = SessionManager::new(store.path().to_path_buf());
            let session = sm
                .create_session(PathBuf::from("/tmp/old"), "s".into(), SessionType::User)
                .await
                .unwrap();
            sm.add_message(&session.id, &Message::user().with_text("hello"))
                .await
                .unwrap();

            // The `biorouter term run` shell-following path may move the dir
            // mid-conversation; nothing else may.
            sm.force_update_working_dir_unguarded(&session.id, PathBuf::from("/tmp/new"))
                .await
                .unwrap();

            let reloaded = sm.get_session(&session.id, false).await.unwrap();
            assert_eq!(reloaded.working_dir, PathBuf::from("/tmp/new"));
        }
    }

    #[tokio::test]
    async fn clear_all_sessions_removes_history_usage_and_side_tables() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let first = sm
            .create_session(temp_dir.path().into(), "First".into(), SessionType::User)
            .await
            .unwrap();
        sm.create_session(temp_dir.path().into(), "Hidden".into(), SessionType::Hidden)
            .await
            .unwrap();
        sm.record_token_event(
            &first.id,
            Some(100),
            Some(20),
            120,
            Some("model"),
            Some("provider"),
            Some(0),
            Some(0),
        )
        .await
        .unwrap();

        let pool = sm.storage().pool().await.unwrap();
        sqlx::query(
            "INSERT INTO checkpoints (id, session_id, turn_index, anchor_ts, kind, commit_sha, tree_sha) VALUES ('cp', ?, 0, 1, 'pre_step', 'commit', 'tree')",
        )
        .bind(&first.id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO message_blobs (blob_uid, session_id, created_at, bytes, content) VALUES ('blob', ?, 1, 4, 'data')",
        )
        .bind(&first.id)
        .execute(pool)
        .await
        .unwrap();

        assert_eq!(sm.count_all_sessions().await.unwrap(), 2);
        assert_eq!(sm.clear_all_sessions().await.unwrap(), 2);
        assert_eq!(sm.count_all_sessions().await.unwrap(), 0);
        for table in [
            "sessions",
            "messages",
            "messages_fts",
            "token_events",
            "checkpoints",
            "message_blobs",
        ] {
            let count = sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(count, 0, "{table} should be empty");
        }
        assert_eq!(sm.get_usage_summary().await.unwrap().all_time.turns, 0);
    }

    #[tokio::test]
    async fn fresh_database_contains_full_v16_schema() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let pool = sm.storage().pool().await.unwrap();

        assert_eq!(
            SessionStorage::get_schema_version(pool).await.unwrap(),
            CURRENT_SCHEMA_VERSION
        );
        for table in [
            "token_events",
            "checkpoints",
            "messages_fts",
            "message_blobs",
        ] {
            let exists: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = ?1")
                    .bind(table)
                    .fetch_one(pool)
                    .await
                    .unwrap();
            assert_eq!(exists, 1, "missing fresh-schema table {table}");
        }
        for column in [
            "model_id",
            "provider",
            "cache_read_tokens",
            "cache_creation_tokens",
            "billed_total_tokens",
            "event_key",
            "session_type",
        ] {
            assert!(
                SessionStorage::table_has_column(pool, "token_events", column)
                    .await
                    .unwrap(),
                "missing fresh usage column {column}"
            );
        }
        assert!(
            SessionStorage::table_has_column(pool, "messages", "msg_uid")
                .await
                .unwrap()
        );
        assert!(
            SessionStorage::table_has_column(pool, "sessions", "branch_point_msg_uid")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn checkpoints_table_crud_roundtrip() {
        // A fresh DB (create_schema path) must carry the migration-13 `checkpoints`
        // table, and the CRUD helpers `CheckpointManager` relies on must roundtrip.
        use crate::checkpoint::{CheckpointKind, CheckpointRecord};
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        assert!(sm.list_checkpoints("s1").await.unwrap().is_empty());
        assert!(sm.last_checkpoint("s1").await.unwrap().is_none());

        let rec = CheckpointRecord {
            id: "cp-1".to_string(),
            session_id: "s1".to_string(),
            turn_index: 0,
            anchor_ts: 1234,
            kind: CheckpointKind::PreStep,
            commit_sha: "deadbeef".to_string(),
            tree_sha: "cafef00d".to_string(),
            changed_paths: vec!["a.txt".to_string()],
            created_at: "2026-07-12T00:00:00Z".to_string(),
        };
        sm.insert_checkpoint(&rec).await.unwrap();

        let listed = sm.list_checkpoints("s1").await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].kind, CheckpointKind::PreStep);
        assert_eq!(listed[0].tree_sha, "cafef00d");
        assert_eq!(listed[0].changed_paths, vec!["a.txt".to_string()]);

        let got = sm.get_checkpoint("s1", "cp-1").await.unwrap().unwrap();
        assert_eq!(got.commit_sha, "deadbeef");
        assert!(sm.get_checkpoint("s1", "missing").await.unwrap().is_none());
        // Scoped by session.
        assert!(sm.get_checkpoint("other", "cp-1").await.unwrap().is_none());

        sm.delete_checkpoints("s1").await.unwrap();
        assert!(sm.list_checkpoints("s1").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_session_creation() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));

        let mut handles = vec![];

        for i in 0..NUM_CONCURRENT_SESSIONS {
            let sm = Arc::clone(&session_manager);
            let handle = tokio::spawn(async move {
                let working_dir = PathBuf::from(format!("/tmp/test_{}", i));
                let description = format!("Test session {}", i);

                let session = sm
                    .create_session(working_dir.clone(), description, SessionType::User)
                    .await
                    .unwrap();

                sm.add_message(
                    &session.id,
                    &Message {
                        id: None,
                        role: Role::User,
                        created: chrono::Utc::now().timestamp_millis(),
                        content: vec![MessageContent::text("hello world")],
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();

                sm.add_message(
                    &session.id,
                    &Message {
                        id: None,
                        role: Role::Assistant,
                        created: chrono::Utc::now().timestamp_millis(),
                        content: vec![MessageContent::text("sup world?")],
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();

                sm.update(&session.id)
                    .user_provided_name(format!("Updated session {}", i))
                    .total_tokens(Some(100 * i))
                    .apply()
                    .await
                    .unwrap();
                sm.record_token_event(
                    &session.id,
                    Some(100 * i),
                    Some(0),
                    i64::from(100 * i),
                    Some("test-model"),
                    Some("test-provider"),
                    Some(0),
                    Some(0),
                )
                .await
                .unwrap();

                let updated = sm.get_session(&session.id, true).await.unwrap();
                assert_eq!(updated.message_count, 2);
                assert_eq!(updated.total_tokens, Some(100 * i));

                session.id
            });
            handles.push(handle);
        }

        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        assert_eq!(results.len(), NUM_CONCURRENT_SESSIONS as usize);

        let unique_ids: std::collections::HashSet<_> = results.iter().collect();
        assert_eq!(unique_ids.len(), NUM_CONCURRENT_SESSIONS as usize);

        let sessions = session_manager.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), NUM_CONCURRENT_SESSIONS as usize);

        for session in &sessions {
            assert_eq!(session.message_count, 2);
            assert!(session.name.starts_with("Updated session"));
        }

        let insights = session_manager.get_insights().await.unwrap();
        assert_eq!(insights.total_sessions, NUM_CONCURRENT_SESSIONS as usize);
        let expected_tokens = 100 * NUM_CONCURRENT_SESSIONS * (NUM_CONCURRENT_SESSIONS - 1) / 2;
        assert_eq!(insights.total_tokens, Some(expected_tokens as i64));
    }

    #[tokio::test]
    async fn test_export_import_roundtrip() {
        const DESCRIPTION: &str = "Original session";
        const TOTAL_TOKENS: i32 = 500;
        const INPUT_TOKENS: i32 = 300;
        const OUTPUT_TOKENS: i32 = 200;
        const ACCUMULATED_TOKENS: i64 = 1000;
        const USER_MESSAGE: &str = "test message";
        const ASSISTANT_MESSAGE: &str = "test response";

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                DESCRIPTION.to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        sm.update(&original.id)
            .total_tokens(Some(TOTAL_TOKENS))
            .input_tokens(Some(INPUT_TOKENS))
            .output_tokens(Some(OUTPUT_TOKENS))
            .accumulated_total_tokens(Some(ACCUMULATED_TOKENS))
            .apply()
            .await
            .unwrap();

        sm.add_message(
            &original.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text(USER_MESSAGE)],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        sm.add_message(
            &original.id,
            &Message {
                id: None,
                role: Role::Assistant,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text(ASSISTANT_MESSAGE)],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let exported = sm.export_session(&original.id).await.unwrap();
        let imported = sm.import_session(&exported).await.unwrap();

        assert_ne!(imported.id, original.id);
        assert_eq!(imported.name, DESCRIPTION);
        assert_eq!(imported.working_dir, PathBuf::from("/tmp/test"));
        assert_eq!(imported.total_tokens, Some(TOTAL_TOKENS));
        assert_eq!(imported.input_tokens, Some(INPUT_TOKENS));
        assert_eq!(imported.output_tokens, Some(OUTPUT_TOKENS));
        assert_eq!(imported.accumulated_total_tokens, Some(ACCUMULATED_TOKENS));
        assert_eq!(imported.message_count, 2);

        let conversation = imported.conversation.unwrap();
        assert_eq!(conversation.messages().len(), 2);
        assert_eq!(conversation.messages()[0].role, Role::User);
        assert_eq!(conversation.messages()[1].role, Role::Assistant);
    }

    #[tokio::test]
    async fn durable_session_resumes_by_external_key() {
        // BRSDK durable sessions: the same external key resumes the same
        // session (with its conversation), distinct keys stay isolated, and a
        // reconnect recovers prior messages — the "recover what it lost" path.
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let key1 = "app:demo:client-1";
        let (s1, resumed1) = sm
            .get_or_create_by_external_key(
                key1,
                PathBuf::from("/tmp/app"),
                "app:demo".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert!(!resumed1, "first call creates, does not resume");

        // Simulate a turn of conversation on this session.
        sm.add_message(
            &s1.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text("what is CFTR?")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        // Reconnect with the SAME external key → resume the SAME session.
        let (s2, resumed2) = sm
            .get_or_create_by_external_key(
                key1,
                PathBuf::from("/tmp/app"),
                "app:demo".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert!(resumed2, "same key must resume");
        assert_eq!(s2.id, s1.id, "resumed session keeps its id");
        assert_eq!(s2.message_count, 1, "prior conversation is recovered");
        let convo = s2
            .conversation
            .expect("resumed session carries conversation");
        assert_eq!(convo.messages().len(), 1);
        assert_eq!(convo.messages()[0].role, Role::User);

        // A different client key → a separate, isolated session.
        let (s3, resumed3) = sm
            .get_or_create_by_external_key(
                "app:demo:client-2",
                PathBuf::from("/tmp/app"),
                "app:demo".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert!(!resumed3);
        assert_ne!(s3.id, s1.id, "distinct keys are isolated");
        assert_eq!(s3.message_count, 0);

        // A different app entirely → also isolated.
        let (s4, _) = sm
            .get_or_create_by_external_key(
                "app:other:client-1",
                PathBuf::from("/tmp/app"),
                "app:other".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert_ne!(s4.id, s1.id);
    }

    #[tokio::test]
    async fn durable_session_survives_a_fresh_manager_on_same_dir() {
        // The external_key binding is persisted, so a brand-new SessionManager
        // over the same data dir (e.g. a daemon restart) still resumes it.
        let temp_dir = TempDir::new().unwrap();
        let key = "app:persist:client-x";

        let id_first = {
            let sm = SessionManager::new(temp_dir.path().to_path_buf());
            let (s, resumed) = sm
                .get_or_create_by_external_key(
                    key,
                    PathBuf::from("/tmp/app"),
                    "app:persist".to_string(),
                    SessionType::User,
                )
                .await
                .unwrap();
            assert!(!resumed);
            s.id
        };

        // New manager instance, same on-disk DB.
        let sm2 = SessionManager::new(temp_dir.path().to_path_buf());
        let (s2, resumed2) = sm2
            .get_or_create_by_external_key(
                key,
                PathBuf::from("/tmp/app"),
                "app:persist".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert!(resumed2, "binding persists across manager instances");
        assert_eq!(s2.id, id_first);
    }

    #[tokio::test]
    async fn migrates_v7_db_to_v8_external_key() {
        // The production upgrade path: every existing user has a v7 DB. Hand-roll
        // a v7-shaped DB, then open the real manager and confirm it migrates to
        // v8 (adds external_key + the partial unique index) WITHOUT losing data.
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        {
            let opts = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();

            sqlx::query("CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)")
                .execute(&pool).await.unwrap();
            for v in 1..=7 {
                sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
                    .bind(v)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            // The v7 sessions table = current schema MINUS external_key.
            sqlx::query(
                r#"CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
                    user_set_name BOOLEAN DEFAULT FALSE, session_type TEXT NOT NULL DEFAULT 'user', working_dir TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    extension_data TEXT DEFAULT '{}', total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER,
                    accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER, accumulated_output_tokens INTEGER,
                    schedule_id TEXT, workflow_json TEXT, user_workflow_values_json TEXT, provider_name TEXT, model_config_json TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            sqlx::query(
                r#"CREATE TABLE messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES sessions(id),
                    role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL,
                    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP, tokens INTEGER, metadata_json TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            // A pre-existing (legacy) session that must survive the migration.
            sqlx::query("INSERT INTO sessions (id, name, working_dir) VALUES ('20240101_1', 'legacy session', '/tmp/old')")
                .execute(&pool).await.unwrap();
            pool.close().await;
        }

        // Opening the real manager triggers run_migrations → the `8 =>` arm.
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        // Legacy data survived the ALTER TABLE.
        let legacy = sm.get_session("20240101_1", true).await.unwrap();
        assert_eq!(legacy.name, "legacy session");
        assert_eq!(legacy.working_dir, PathBuf::from("/tmp/old"));

        // Messages table still works post-migration.
        sm.add_message(
            "20240101_1",
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text("post-migration message")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            sm.get_session("20240101_1", true)
                .await
                .unwrap()
                .message_count,
            1
        );

        // The migrated external_key column + unique index are queryable: a
        // second call with the same key resumes (proves the index exists).
        let (s1, r1) = sm
            .get_or_create_by_external_key(
                "app:x:c1",
                PathBuf::from("/tmp/app"),
                "app:x".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert!(!r1);
        let (s2, r2) = sm
            .get_or_create_by_external_key(
                "app:x:c1",
                PathBuf::from("/tmp/app"),
                "app:x".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert!(r2, "external_key index works after migration");
        assert_eq!(s1.id, s2.id);

        // Two NULL external_keys are allowed (partial unique index) — the legacy
        // row + a fresh plain session both have NULL and coexist.
        let plain = sm
            .create_session(
                PathBuf::from("/tmp"),
                "plain".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        assert_ne!(plain.id, "20240101_1");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_same_key_callers_converge_on_one_session() {
        // The race-safe claim: many truly-concurrent callers with the SAME key
        // must all resolve to exactly ONE session (no duplicates, no errors).
        let temp_dir = TempDir::new().unwrap();
        let sm = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let key = "app:race:client-1";

        let mut handles = Vec::new();
        for _ in 0..8 {
            let sm = sm.clone();
            let key = key.to_string();
            handles.push(tokio::spawn(async move {
                sm.get_or_create_by_external_key(
                    &key,
                    PathBuf::from("/tmp/app"),
                    "app:race".to_string(),
                    SessionType::User,
                )
                .await
            }));
        }

        let mut ids = std::collections::HashSet::new();
        for h in handles {
            let (s, _resumed) = h.await.unwrap().expect("no caller should error");
            ids.insert(s.id);
        }
        assert_eq!(
            ids.len(),
            1,
            "all concurrent same-key callers must converge on exactly one session"
        );
    }

    #[tokio::test]
    async fn test_import_session_with_description_field() {
        const OLD_FORMAT_JSON: &str = r#"{
            "id": "20240101_1",
            "description": "Old format session",
            "user_set_name": true,
            "working_dir": "/tmp/test",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "extension_data": {},
            "message_count": 0
        }"#;

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let imported = sm.import_session(OLD_FORMAT_JSON).await.unwrap();

        assert_eq!(imported.name, "Old format session");
        assert!(imported.user_set_name);
        assert_eq!(imported.working_dir, PathBuf::from("/tmp/test"));
    }

    // ── Diverge (copy_session) tests ────────────────────────────────────────
    //
    // `copy_session` is the engine behind both the edit-diverge path and the
    // `/diverge` feature (Diverge button + `/diverge` slash command). Diverge
    // copies the *entire* conversation with no truncation, so the new session
    // resumes from exactly where the original left off while the original stays
    // put. These tests exercise that contract from many angles.

    /// Seed a User session with `n` user/assistant message pairs and return it
    /// (loaded with its conversation).
    async fn seed_session_with_messages(sm: &SessionManager, n: usize) -> Session {
        let session = sm
            .create_session(
                PathBuf::from("/tmp/diverge_test"),
                "Original".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        for i in 0..n {
            sm.add_message(
                &session.id,
                &Message {
                    id: None,
                    role: Role::User,
                    created: chrono::Utc::now().timestamp_millis() + (i as i64) * 2,
                    content: vec![MessageContent::text(format!("question {i}"))],
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();
            sm.add_message(
                &session.id,
                &Message {
                    id: None,
                    role: Role::Assistant,
                    created: chrono::Utc::now().timestamp_millis() + (i as i64) * 2 + 1,
                    content: vec![MessageContent::text(format!("answer {i}"))],
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();
        }

        sm.get_session(&session.id, true).await.unwrap()
    }

    #[tokio::test]
    async fn session_summaries_are_lightweight_and_paginated() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let created = vec![
            seed_session_with_messages(&sm, 1).await,
            seed_session_with_messages(&sm, 2).await,
            seed_session_with_messages(&sm, 3).await,
        ];

        let first_page = sm.list_session_summaries(2, 0).await.unwrap();
        let second_page = sm.list_session_summaries(2, 2).await.unwrap();

        assert_eq!(first_page.len(), 2);
        assert_eq!(second_page.len(), 1);
        assert!(first_page.iter().all(|session| session.message_count > 0));
        assert!(first_page
            .iter()
            .all(|session| session.working_dir == "/tmp/diverge_test"));

        let expected_ids: std::collections::HashSet<_> =
            created.into_iter().map(|session| session.id).collect();
        let actual_ids: std::collections::HashSet<_> = first_page
            .into_iter()
            .chain(second_page)
            .map(|session| session.id)
            .collect();
        assert_eq!(actual_ids, expected_ids);
    }

    #[tokio::test]
    async fn test_diverge_preserves_full_history_and_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 3).await;
        assert_eq!(original.message_count, 6);

        let diverged = sm
            .copy_session(&original.id, "Branch".to_string())
            .await
            .unwrap();

        // New, distinct id.
        assert_ne!(diverged.id, original.id);
        // Name applied.
        assert_eq!(diverged.name, "Branch");
        // Working dir carried over.
        assert_eq!(diverged.working_dir, original.working_dir);
        // Full conversation copied verbatim, in order.
        assert_eq!(diverged.message_count, 6);
        let orig_texts: Vec<_> = original
            .conversation
            .as_ref()
            .unwrap()
            .messages()
            .iter()
            .map(|m| m.as_concat_text())
            .collect();
        let new_texts: Vec<_> = diverged
            .conversation
            .as_ref()
            .unwrap()
            .messages()
            .iter()
            .map(|m| m.as_concat_text())
            .collect();
        assert_eq!(orig_texts, new_texts);
    }

    #[tokio::test]
    async fn test_diverge_leaves_original_untouched() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 2).await;
        let diverged = sm
            .copy_session(&original.id, "Branch".to_string())
            .await
            .unwrap();

        // Mutate the diverged session by appending a new message.
        sm.add_message(
            &diverged.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis() + 10_000,
                content: vec![MessageContent::text("only in the branch")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        // Original is completely unaffected: same id, same message count.
        let original_after = sm.get_session(&original.id, true).await.unwrap();
        assert_eq!(original_after.message_count, 4);
        let branch_after = sm.get_session(&diverged.id, true).await.unwrap();
        assert_eq!(branch_after.message_count, 5);

        // Both sessions still exist independently.
        let sessions = sm.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn test_diverge_resets_token_counts() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 1).await;
        sm.update(&original.id)
            .total_tokens(Some(1234))
            .input_tokens(Some(1000))
            .output_tokens(Some(234))
            .apply()
            .await
            .unwrap();

        let diverged = sm
            .copy_session(&original.id, "Branch".to_string())
            .await
            .unwrap();

        // Diverged session starts with fresh token accounting.
        assert_eq!(diverged.total_tokens, None);
        assert_eq!(diverged.input_tokens, None);
        assert_eq!(diverged.output_tokens, None);

        // Original keeps its counts.
        let original_after = sm.get_session(&original.id, false).await.unwrap();
        assert_eq!(original_after.total_tokens, Some(1234));
    }

    /// The old path read the row into Rust, added, and wrote it back. Two turns
    /// racing on the same session silently lost one update.
    #[tokio::test]
    async fn accumulate_tokens_is_additive_and_atomic() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;

        for _ in 0..3 {
            sm.update(&session.id)
                .accumulate_tokens(TokenDelta {
                    input: Some(100),
                    output: Some(20),
                    total: Some(120),
                })
                .apply()
                .await
                .unwrap();
        }

        let counts = sm.get_token_counts(&session.id).await.unwrap();
        assert_eq!(counts.accumulated_total_tokens, Some(360));
        assert_eq!(counts.accumulated_input_tokens, Some(300));
        assert_eq!(counts.accumulated_output_tokens, Some(60));
    }

    #[tokio::test]
    async fn usage_event_updates_ledger_and_counters_once_in_one_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;
        let entry = UsageLedgerEntry {
            event_key: "provider-call-1".to_string(),
            session_id: session.id.clone(),
            schedule_id: None,
            current_total_tokens: Some(820),
            current_input_tokens: Some(100),
            current_output_tokens: Some(20),
            billed_total_tokens: Some(900),
            input_tokens: Some(100),
            output_tokens: Some(20),
            model_id: Some("claude-sonnet-4-20250514".to_string()),
            provider: Some("anthropic".to_string()),
            cache_read_tokens: Some(700),
            cache_creation_tokens: Some(80),
        };

        assert!(sm.apply_usage_event(entry.clone()).await.unwrap());
        assert!(
            !sm.apply_usage_event(entry).await.unwrap(),
            "retrying the same provider call is a no-op"
        );

        let counts = sm.get_token_counts(&session.id).await.unwrap();
        assert_eq!(
            counts.total_tokens,
            Some(820),
            "live context stays separate"
        );
        assert_eq!(counts.accumulated_total_tokens, Some(900));
        assert_eq!(counts.accumulated_input_tokens, Some(100));
        assert_eq!(counts.accumulated_output_tokens, Some(20));

        let rows = sm.get_session_model_usage(&session.id).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].turns, 1);
        assert_eq!(rows[0].total_tokens, Some(900));
        assert_eq!(rows[0].cache_read_tokens, Some(700));
        assert_eq!(rows[0].cache_creation_tokens, Some(80));
    }

    #[tokio::test]
    async fn total_only_usage_is_retained_but_never_priced_as_zero() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;

        sm.apply_usage_event(UsageLedgerEntry {
            event_key: "total-only-provider-call".to_string(),
            session_id: session.id.clone(),
            schedule_id: None,
            current_total_tokens: Some(500),
            current_input_tokens: None,
            current_output_tokens: None,
            billed_total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            model_id: Some("glm-5.2".to_string()),
            provider: Some("zai".to_string()),
            cache_read_tokens: None,
            cache_creation_tokens: None,
        })
        .await
        .unwrap();

        let counts = sm.get_token_counts(&session.id).await.unwrap();
        assert_eq!(counts.total_tokens, Some(500));
        assert_eq!(counts.accumulated_total_tokens, None);

        let rows = sm
            .get_usage_report(0, i64::MAX, UsageGroup::Model)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_tokens, None);
        assert_eq!(rows[0].cost, None, "unknown cost is null, never $0");
        assert!(rows[0].has_unpriced);
        let insights = sm.get_insights().await.unwrap();
        assert_eq!(insights.total_tokens, None);
        assert_eq!(insights.tokens_last_7_days, None);
        let activity = sm.get_activity(7).await.unwrap();
        assert!(!activity.tokens_complete);
        assert_eq!(activity.days[0].tokens, 0);
        assert!(!activity.days[0].tokens_complete);
    }

    #[tokio::test]
    async fn modern_zero_cache_buckets_persist_as_complete_measurements() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;

        sm.apply_usage_event(UsageLedgerEntry {
            event_key: "no-cache-provider-call".to_string(),
            session_id: session.id,
            schedule_id: None,
            current_total_tokens: Some(120),
            current_input_tokens: Some(100),
            current_output_tokens: Some(20),
            billed_total_tokens: Some(120),
            input_tokens: Some(100),
            output_tokens: Some(20),
            model_id: Some("glm-5.2".to_string()),
            provider: Some("zai".to_string()),
            cache_read_tokens: Some(0),
            cache_creation_tokens: Some(0),
        })
        .await
        .unwrap();

        let rows = sm
            .get_usage_report(0, i64::MAX, UsageGroup::Model)
            .await
            .unwrap();
        assert_eq!(rows[0].cache_read_tokens, Some(0));
        assert_eq!(rows[0].cache_creation_tokens, Some(0));
        assert!(!rows[0].has_unpriced);
        assert!(!rows[0].cost_excludes_cache);
        assert!(rows[0].cost.is_some());
    }

    /// SQLite's INTEGER is 64-bit; the Rust side used to be `i32`, which wraps
    /// negative past ~2.1e9 and then *subtracts* from the insights SUM.
    #[tokio::test]
    async fn accumulated_tokens_exceed_i32() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;

        let beyond_i32 = i64::from(i32::MAX) + 1_000;
        sm.update(&session.id)
            .accumulated_total_tokens(Some(beyond_i32))
            .apply()
            .await
            .unwrap();

        let counts = sm.get_token_counts(&session.id).await.unwrap();
        assert_eq!(counts.accumulated_total_tokens, Some(beyond_i32));

        sm.record_token_event(
            &session.id,
            None,
            None,
            beyond_i32,
            Some("m"),
            Some("p"),
            None,
            None,
        )
        .await
        .unwrap();

        let insights = sm.get_insights().await.unwrap();
        assert_eq!(
            insights.total_tokens,
            Some(beyond_i32),
            "no wrap, no negative sum"
        );
    }

    /// The tiles on Home must agree with the session list printed beneath them.
    #[tokio::test]
    async fn insights_exclude_internal_session_types() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        for (session_type, tokens) in [
            (SessionType::User, 1_000i64),
            (SessionType::Scheduled, 500),
            (SessionType::SubAgent, 9_000),
            (SessionType::Hidden, 9_000),
            (SessionType::Terminal, 9_000),
        ] {
            let s = sm
                .create_session("/tmp".into(), String::new(), session_type)
                .await
                .unwrap();
            sm.record_token_event(&s.id, None, None, tokens, None, None, None, None)
                .await
                .unwrap();
        }

        let insights = sm.get_insights().await.unwrap();
        assert_eq!(insights.total_sessions, 2, "user + scheduled only");
        assert_eq!(insights.total_tokens, Some(1_500));
    }

    /// The per-turn ledger is what makes a real per-day token series possible —
    /// and what makes "tokens in the last 7 days" mean what it says.
    #[tokio::test]
    async fn token_events_drive_the_windowed_totals() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;

        // No events yet: the lifetime total is non-zero but the window is empty.
        sm.update(&session.id)
            .accumulated_total_tokens(Some(50_000))
            .apply()
            .await
            .unwrap();
        let insights = sm.get_insights().await.unwrap();
        assert_eq!(insights.total_tokens, Some(0));
        assert_eq!(
            insights.tokens_last_7_days,
            Some(0),
            "a lifetime total is not a 7-day total"
        );

        sm.record_token_event(
            &session.id,
            Some(80),
            Some(20),
            100,
            Some("m1"),
            Some("p1"),
            None,
            None,
        )
        .await
        .unwrap();
        let insights = sm.get_insights().await.unwrap();
        assert_eq!(insights.tokens_last_7_days, Some(100));
        assert_eq!(insights.tokens_last_30_days, Some(100));

        let activity = sm.get_activity(30).await.unwrap();
        assert_eq!(activity.days.len(), 1);
        assert_eq!(activity.days[0].tokens, 100);
        assert!(activity.days[0].tokens_complete);
        assert_eq!(activity.days[0].sessions, 1);
        assert!(activity.days[0].level >= 1);
        assert_eq!(activity.current_streak, 1);
    }

    #[tokio::test]
    async fn per_model_usage_sums_across_models_and_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;

        // Two turns on model A, one on model B, one with no model (unknown).
        sm.record_token_event(
            &session.id,
            Some(100),
            Some(20),
            120,
            Some("gpt-5"),
            Some("openai"),
            None,
            None,
        )
        .await
        .unwrap();
        sm.record_token_event(
            &session.id,
            Some(200),
            Some(50),
            250,
            Some("gpt-5"),
            Some("openai"),
            None,
            None,
        )
        .await
        .unwrap();
        sm.record_token_event(
            &session.id,
            Some(10),
            Some(5),
            15,
            Some("claude-fable-5"),
            Some("anthropic"),
            None,
            None,
        )
        .await
        .unwrap();
        // Unknown: no model / provider reported.
        sm.record_token_event(&session.id, Some(1), Some(2), 3, None, None, None, None)
            .await
            .unwrap();

        let rows = sm.get_session_model_usage(&session.id).await.unwrap();
        assert_eq!(
            rows.len(),
            3,
            "gpt-5, claude, and unknown are distinct groups"
        );

        let gpt = rows
            .iter()
            .find(|r| r.model_id.as_deref() == Some("gpt-5"))
            .expect("gpt-5 row present");
        // Hand-computed: 100+200 in, 20+50 out, 120+250 total, 2 turns.
        assert_eq!(gpt.provider.as_deref(), Some("openai"));
        assert_eq!(gpt.input_tokens, 300);
        assert_eq!(gpt.output_tokens, 70);
        assert_eq!(gpt.total_tokens, Some(370));
        assert_eq!(gpt.turns, 2);

        let claude = rows
            .iter()
            .find(|r| r.model_id.as_deref() == Some("claude-fable-5"))
            .expect("claude row present");
        assert_eq!(claude.input_tokens, 10);
        assert_eq!(claude.output_tokens, 5);
        assert_eq!(claude.total_tokens, Some(15));
        assert_eq!(claude.turns, 1);

        let unknown = rows
            .iter()
            .find(|r| r.model_id.is_none())
            .expect("unknown row present");
        assert_eq!(unknown.provider, None);
        assert_eq!(unknown.input_tokens, 1);
        assert_eq!(unknown.output_tokens, 2);
        assert_eq!(unknown.total_tokens, Some(3));
        assert_eq!(unknown.turns, 1);

        // Ordered by total_tokens DESC.
        assert_eq!(rows[0].model_id.as_deref(), Some("gpt-5"));
    }

    /// Shared fixture for the pure rollup tests. Prices use the real zai
    /// `glm-5.2` card ($1.40 / 1M input, $4.40 / 1M output); the unknown-model
    /// row is unpriced. Chosen so every dollar figure is an exact hand value.
    fn usage_grain_fixture() -> Vec<UsageGrainRow> {
        vec![
            // Day 10, priced: 1M input → $1.40.
            UsageGrainRow {
                day: "2026-07-10".into(),
                model_id: Some("glm-5.2".into()),
                provider: Some("zai".into()),
                input_tokens: 1_000_000,
                output_tokens: 0,
                total_tokens: Some(1_000_000),
                cache_read_tokens: Some(0),
                cache_creation_tokens: Some(0),
                turns: 5,
                input_complete: 1,
                output_complete: 1,
                cache_read_complete: 1,
                cache_creation_complete: 1,
            },
            // Day 10, unknown model → unpriced.
            UsageGrainRow {
                day: "2026-07-10".into(),
                model_id: None,
                provider: None,
                input_tokens: 500,
                output_tokens: 500,
                total_tokens: Some(1_000),
                cache_read_tokens: Some(0),
                cache_creation_tokens: Some(0),
                turns: 2,
                input_complete: 1,
                output_complete: 1,
                cache_read_complete: 1,
                cache_creation_complete: 1,
            },
            // Day 11, priced: 2M input + 1M output → 2.80 + 4.40 = $7.20.
            UsageGrainRow {
                day: "2026-07-11".into(),
                model_id: Some("glm-5.2".into()),
                provider: Some("zai".into()),
                input_tokens: 2_000_000,
                output_tokens: 1_000_000,
                total_tokens: Some(3_000_000),
                cache_read_tokens: Some(0),
                cache_creation_tokens: Some(0),
                turns: 3,
                input_complete: 1,
                output_complete: 1,
                cache_read_complete: 1,
                cache_creation_complete: 1,
            },
        ]
    }

    fn approx(a: Option<f64>, b: f64) -> bool {
        matches!(a, Some(v) if (v - b).abs() < 1e-9)
    }

    #[test]
    fn rollup_by_day_prices_each_model_before_summing() {
        let rows = rollup_report(&usage_grain_fixture(), UsageGroup::Day);
        assert_eq!(rows.len(), 2);

        // Chronological: day 10 then day 11.
        let d10 = &rows[0];
        assert_eq!(d10.date.as_deref(), Some("2026-07-10"));
        assert_eq!(d10.model_id, None, "day grouping drops the model");
        assert_eq!(d10.input_tokens, 1_000_500);
        assert_eq!(d10.output_tokens, 500);
        assert_eq!(d10.total_tokens, Some(1_001_000));
        assert_eq!(d10.turns, 7);
        // Only the priced glm row contributes dollars; the unknown row flags it.
        assert!(approx(d10.cost, 1.40), "got {:?}", d10.cost);
        assert!(d10.has_unpriced);

        let d11 = &rows[1];
        assert_eq!(d11.date.as_deref(), Some("2026-07-11"));
        assert!(approx(d11.cost, 7.20), "got {:?}", d11.cost);
        assert!(!d11.has_unpriced);
    }

    #[test]
    fn rollup_by_model_sums_days_and_isolates_unknown() {
        let rows = rollup_report(&usage_grain_fixture(), UsageGroup::Model);
        assert_eq!(rows.len(), 2);

        // Heaviest model first.
        let glm = &rows[0];
        assert_eq!(glm.model_id.as_deref(), Some("glm-5.2"));
        assert_eq!(glm.date, None, "model grouping drops the day");
        assert_eq!(glm.input_tokens, 3_000_000);
        assert_eq!(glm.output_tokens, 1_000_000);
        assert_eq!(glm.total_tokens, Some(4_000_000));
        assert_eq!(glm.turns, 8);
        // 1.40 (day 10) + 7.20 (day 11) = 8.60.
        assert!(approx(glm.cost, 8.60), "got {:?}", glm.cost);
        assert!(!glm.has_unpriced);

        let unknown = &rows[1];
        assert_eq!(unknown.model_id, None);
        assert_eq!(unknown.cost, None, "unknown model is null cost, never $0");
        assert!(unknown.has_unpriced);
    }

    #[test]
    fn rollup_by_day_model_keeps_every_bucket() {
        let rows = rollup_report(&usage_grain_fixture(), UsageGroup::DayModel);
        assert_eq!(rows.len(), 3);
        // Sorted day asc, then heaviest model within a day.
        assert_eq!(
            (rows[0].date.as_deref(), rows[0].model_id.as_deref()),
            (Some("2026-07-10"), Some("glm-5.2"))
        );
        assert!(approx(rows[0].cost, 1.40));
        assert_eq!(
            (rows[1].date.as_deref(), rows[1].model_id.as_deref()),
            (Some("2026-07-10"), None)
        );
        assert_eq!(rows[1].cost, None);
        assert_eq!(
            (rows[2].date.as_deref(), rows[2].model_id.as_deref()),
            (Some("2026-07-11"), Some("glm-5.2"))
        );
        assert!(approx(rows[2].cost, 7.20));
    }

    #[test]
    fn totals_from_grain_sum_and_price() {
        let totals = totals_from_grain(&usage_grain_fixture());
        assert_eq!(totals.input_tokens, 3_000_500);
        assert_eq!(totals.output_tokens, 1_000_500);
        assert_eq!(totals.total_tokens, Some(4_001_000));
        assert_eq!(totals.turns, 10);
        assert!(approx(totals.cost, 8.60), "got {:?}", totals.cost);
        assert!(totals.has_unpriced, "the unknown row leaves it partial");
    }

    #[test]
    fn totals_are_fully_null_when_nothing_is_priced() {
        let grain = vec![UsageGrainRow {
            day: "".into(),
            model_id: None,
            provider: None,
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: Some(150),
            cache_read_tokens: Some(0),
            cache_creation_tokens: Some(0),
            turns: 1,
            input_complete: 1,
            output_complete: 1,
            cache_read_complete: 1,
            cache_creation_complete: 1,
        }];
        let totals = totals_from_grain(&grain);
        assert_eq!(totals.total_tokens, Some(150));
        assert_eq!(totals.cost, None, "wholly-unpriced span is null, not $0");
        assert!(totals.has_unpriced);
    }

    #[test]
    fn partial_bucket_with_zero_known_subtotal_has_null_cost() {
        let grain = vec![UsageGrainRow {
            day: "".into(),
            model_id: Some("glm-5.2".into()),
            provider: Some("zai".into()),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: None,
            cache_read_tokens: None,
            cache_creation_tokens: None,
            turns: 1,
            input_complete: 0,
            output_complete: 0,
            cache_read_complete: 0,
            cache_creation_complete: 0,
        }];
        let totals = totals_from_grain(&grain);
        assert_eq!(totals.cost, None, "an unknown cost must not render as $0");
        assert!(totals.has_unpriced);
    }

    #[test]
    fn legacy_null_cache_buckets_remain_incomplete_for_models_without_cache_rates() {
        let grain = vec![UsageGrainRow {
            day: "".into(),
            model_id: Some("glm-5.2".into()),
            provider: Some("zai".into()),
            input_tokens: 1_000_000,
            output_tokens: 0,
            total_tokens: None,
            cache_read_tokens: None,
            cache_creation_tokens: None,
            turns: 1,
            input_complete: 1,
            output_complete: 1,
            cache_read_complete: 0,
            cache_creation_complete: 0,
        }];
        let totals = totals_from_grain(&grain);
        assert!(approx(totals.cost, 1.40));
        assert!(totals.has_unpriced);
        assert!(totals.cost_excludes_cache);
        assert_eq!(totals.cache_read_tokens, None);
        assert_eq!(totals.cache_creation_tokens, None);
    }

    #[test]
    fn cache_incomplete_nonzero_total_with_zero_known_subtotal_has_no_price() {
        let row = UsageGrainRow {
            day: "".into(),
            model_id: Some("glm-5.2".into()),
            provider: Some("zai".into()),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: Some(100),
            cache_read_tokens: None,
            cache_creation_tokens: None,
            turns: 1,
            input_complete: 1,
            output_complete: 1,
            cache_read_complete: 0,
            cache_creation_complete: 0,
        };
        let price = price_grain(&row, &ResolvedPricing::new());
        assert_eq!(price.cost, None);
        assert!(price.incomplete);
    }

    #[test]
    fn report_uses_resolved_declarative_provider_pricing() {
        let metadata = crate::providers::base::ProviderMetadata::with_models(
            "custom_acme",
            "Acme",
            "test",
            "acme-model",
            vec![crate::providers::base::ModelInfo::with_cost(
                "acme-model",
                32_000,
                0.000_002,
                0.000_008,
            )],
            "",
            vec![],
        );
        let pricing =
            crate::providers::pricing::pricing_from_provider_metadata(&metadata, "acme-model")
                .unwrap();
        let mut resolved = ResolvedPricing::new();
        resolved.insert(
            ("custom_acme".to_string(), "acme-model".to_string()),
            pricing,
        );
        let grain = vec![UsageGrainRow {
            day: "2026-07-13".into(),
            model_id: Some("acme-model".into()),
            provider: Some("custom_acme".into()),
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            total_tokens: Some(1_500_000),
            cache_read_tokens: Some(0),
            cache_creation_tokens: Some(0),
            turns: 1,
            input_complete: 1,
            output_complete: 1,
            cache_read_complete: 1,
            cache_creation_complete: 1,
        }];
        let rows = rollup_report_with_pricing(&grain, UsageGroup::Model, &resolved);
        assert!(approx(rows[0].cost, 6.0));
        assert!(!rows[0].has_unpriced);
    }

    #[test]
    fn empty_span_is_exactly_zero_not_unknown() {
        let totals = totals_from_grain(&[]);
        assert_eq!(totals.total_tokens, Some(0));
        assert_eq!(totals.cache_read_tokens, Some(0));
        assert_eq!(totals.cache_creation_tokens, Some(0));
        assert_eq!(totals.cost, Some(0.0));
        assert!(!totals.has_unpriced);
        assert!(!totals.cost_excludes_cache);
    }

    #[test]
    fn rollup_prices_cache_buckets_and_flags_exclusion() {
        // Two grain rows:
        //  - Claude Sonnet (cache-priced): input 1M, output 200k, cache_read
        //    500k, cache_creation 100k. Hand-computed cost = 6.525 (see the
        //    pricing crate's model_cost_with_cache test).
        //  - zai glm-5.2 (no cache rate) with cache tokens: input 1M -> $1.40,
        //    cache omitted, so the bucket flags cost_excludes_cache.
        let grain = vec![
            UsageGrainRow {
                day: "2026-07-12".into(),
                model_id: Some("claude-sonnet-4-20250514".into()),
                provider: Some("anthropic".into()),
                input_tokens: 1_000_000,
                output_tokens: 200_000,
                total_tokens: Some(1_600_000),
                cache_read_tokens: Some(500_000),
                cache_creation_tokens: Some(100_000),
                turns: 1,
                input_complete: 1,
                output_complete: 1,
                cache_read_complete: 1,
                cache_creation_complete: 1,
            },
            UsageGrainRow {
                day: "2026-07-12".into(),
                model_id: Some("glm-5.2".into()),
                provider: Some("zai".into()),
                input_tokens: 1_000_000,
                output_tokens: 0,
                total_tokens: Some(1_900_000),
                cache_read_tokens: Some(900_000),
                cache_creation_tokens: Some(0),
                turns: 1,
                input_complete: 1,
                output_complete: 1,
                cache_read_complete: 1,
                cache_creation_complete: 1,
            },
        ];
        let day = &rollup_report(&grain, UsageGroup::Day)[0];
        assert_eq!(day.cache_read_tokens, Some(1_400_000));
        assert_eq!(day.cache_creation_tokens, Some(100_000));
        // 6.525 (sonnet incl. cache) + 1.40 (zai, cache excluded) = 7.925.
        assert!(approx(day.cost, 7.925), "got {:?}", day.cost);
        assert!(
            day.cost_excludes_cache,
            "zai carried cache with no cache rate"
        );

        let totals = totals_from_grain(&grain);
        assert_eq!(totals.cache_read_tokens, Some(1_400_000));
        assert_eq!(totals.cache_creation_tokens, Some(100_000));
        assert!(approx(totals.cost, 7.925));
        assert!(totals.cost_excludes_cache);
    }

    #[tokio::test]
    async fn empty_model_strings_collapse_into_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = seed_session_with_messages(&sm, 1).await;

        // Empty strings must be stored as NULL so they aggregate with genuine
        // unknowns rather than forming a "" group.
        sm.record_token_event(
            &session.id,
            Some(4),
            Some(1),
            5,
            Some(""),
            Some(""),
            None,
            None,
        )
        .await
        .unwrap();
        sm.record_token_event(&session.id, Some(6), Some(0), 6, None, None, None, None)
            .await
            .unwrap();

        let rows = sm.get_session_model_usage(&session.id).await.unwrap();
        assert_eq!(rows.len(), 1, "the empty-string turn folded into unknown");
        assert_eq!(rows[0].model_id, None);
        assert_eq!(rows[0].total_tokens, Some(11));
        assert_eq!(rows[0].turns, 2);
    }

    #[tokio::test]
    async fn global_model_usage_window_is_boundary_inclusive() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        // Creating a session initializes the schema on disk and gives us a
        // real (user-type) session row for the aggregation join.
        let session = seed_session_with_messages(&sm, 1).await;

        // Insert events at controlled timestamps by opening a second connection
        // to the same DB file — `record_token_event` always stamps `now`.
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        for (ts, total) in [(1000_i64, 10_i64), (2000, 20), (3000, 30)] {
            sqlx::query(
                "INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens, billed_total_tokens, model_id, provider, session_type) VALUES (?, ?, ?, ?, ?, ?, 'm', 'p', 'user')",
            )
            .bind(&session.id)
            .bind(ts)
            .bind(total)
            .bind(0_i64)
            .bind(total)
            .bind(total)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.close().await;

        // Both ends inclusive: [2000, 2000] catches exactly the ts=2000 row.
        let mid = sm.get_model_usage(2000, 2000).await.unwrap();
        assert_eq!(mid.len(), 1);
        assert_eq!(mid[0].total_tokens, Some(20));
        assert_eq!(mid[0].turns, 1);

        // A window straddling only the middle row.
        let straddle = sm.get_model_usage(1500, 2500).await.unwrap();
        assert_eq!(straddle[0].total_tokens, Some(20));

        // Full span includes all three: from == first ts, to == last ts.
        let all = sm.get_model_usage(1000, 3000).await.unwrap();
        assert_eq!(all.len(), 1, "one model group");
        assert_eq!(all[0].total_tokens, Some(60));
        assert_eq!(all[0].turns, 3);

        // Just below the lowest ts excludes everything.
        let none = sm.get_model_usage(0, 999).await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn usage_report_includes_subagent_spend_and_excludes_hidden_sessions() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let user = seed_session_with_messages(&sm, 1).await;
        let subagent = sm
            .create_session(
                PathBuf::from("/tmp/sub"),
                "subagent".into(),
                SessionType::SubAgent,
            )
            .await
            .unwrap();
        let hidden = sm
            .create_session(
                PathBuf::from("/tmp/h"),
                "hidden".into(),
                SessionType::Hidden,
            )
            .await
            .unwrap();

        let t0: i64 = 1_700_000_000;
        let t2 = t0 + 2 * 86_400; // two days later

        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();

        let insert = |sid: String,
                      ts: i64,
                      input: i64,
                      output: i64,
                      total: i64,
                      model: Option<&'static str>,
                      provider: Option<&'static str>,
                      pool: &sqlx::SqlitePool| {
            let pool = pool.clone();
            async move {
                sqlx::query("INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens, billed_total_tokens, model_id, provider, cache_read_tokens, cache_creation_tokens, session_type) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, 0, (SELECT session_type FROM sessions WHERE id = ?))")
                    .bind(sid.clone()).bind(ts).bind(input).bind(output).bind(total).bind(total).bind(model).bind(provider).bind(sid)
                    .execute(&pool).await.unwrap();
            }
        };

        // Day 0: a priced glm turn (1M input → $1.40) + an unknown turn.
        insert(
            user.id.clone(),
            t0,
            1_000_000,
            0,
            1_000_000,
            Some("glm-5.2"),
            Some("zai"),
            &pool,
        )
        .await;
        insert(user.id.clone(), t0, 100, 0, 100, None, None, &pool).await;
        // Day 2: a priced glm turn (1M output → $4.40).
        insert(
            user.id.clone(),
            t2,
            0,
            1_000_000,
            1_000_000,
            Some("glm-5.2"),
            Some("zai"),
            &pool,
        )
        .await;
        // Subagent work is hidden from the session list but is real spend.
        insert(
            subagent.id.clone(),
            t0,
            2_000_000,
            0,
            2_000_000,
            Some("glm-5.2"),
            Some("zai"),
            &pool,
        )
        .await;
        // Hidden bookkeeping sessions do not represent user workload.
        insert(
            hidden.id.clone(),
            t0,
            9_999_999,
            0,
            9_999_999,
            Some("glm-5.2"),
            Some("zai"),
            &pool,
        )
        .await;

        // The local-day strings are tz-dependent; ask SQLite so the assertion
        // holds in any timezone the test runs in.
        let day0: String = sqlx::query_scalar("SELECT date(?, 'unixepoch', 'localtime')")
            .bind(t0)
            .fetch_one(&pool)
            .await
            .unwrap();
        let day2: String = sqlx::query_scalar("SELECT date(?, 'unixepoch', 'localtime')")
            .bind(t2)
            .fetch_one(&pool)
            .await
            .unwrap();
        pool.close().await;

        let report = sm
            .get_usage_report(t0 - 1, t2 + 1, UsageGroup::Day)
            .await
            .unwrap();
        assert_eq!(report.len(), 2, "two active local days, hidden excluded");

        let b0 = report
            .iter()
            .find(|r| r.date.as_deref() == Some(&day0))
            .unwrap();
        assert_eq!(
            b0.input_tokens, 3_000_100,
            "subagent included, hidden excluded"
        );
        assert_eq!(b0.output_tokens, 0);
        assert_eq!(b0.total_tokens, Some(3_000_100));
        assert_eq!(b0.turns, 3);
        assert!(
            matches!(b0.cost, Some(c) if (c - 4.20).abs() < 1e-9),
            "got {:?}",
            b0.cost
        );
        assert!(b0.has_unpriced, "the unknown turn flags the day partial");

        let b2 = report
            .iter()
            .find(|r| r.date.as_deref() == Some(&day2))
            .unwrap();
        assert_eq!(b2.total_tokens, Some(1_000_000));
        assert!(
            matches!(b2.cost, Some(c) if (c - 4.40).abs() < 1e-9),
            "got {:?}",
            b2.cost
        );
        assert!(!b2.has_unpriced);

        // Window that ends before day 2 drops that bucket entirely.
        let narrow = sm
            .get_usage_report(t0 - 1, t0 + 1, UsageGroup::Day)
            .await
            .unwrap();
        assert_eq!(narrow.len(), 1);
        assert_eq!(narrow[0].date.as_deref(), Some(day0.as_str()));
    }

    #[tokio::test]
    async fn usage_summary_month_to_date_respects_the_local_month_boundary() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let user = seed_session_with_messages(&sm, 1).await;

        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();

        // Unix second of local midnight on the 1st of the current month. The
        // 'utc' modifier reads the wall-clock string as localtime, so this is a
        // real instant regardless of the runner's timezone.
        let month_start: i64 = sqlx::query_scalar(
            "SELECT CAST(strftime('%s', strftime('%Y-%m-01 00:00:00', 'now', 'localtime'), 'utc') AS INTEGER)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let first_of_month = month_start; // inside MTD
        let last_of_prev_month = month_start - 1; // one second earlier: previous month

        let insert = |ts: i64, input: i64, output: i64, total: i64, pool: &sqlx::SqlitePool| {
            let sid = user.id.clone();
            let pool = pool.clone();
            async move {
                sqlx::query("INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens, billed_total_tokens, model_id, provider, cache_read_tokens, cache_creation_tokens, session_type) VALUES (?, ?, ?, ?, ?, ?, 'glm-5.2', 'zai', 0, 0, 'user')")
                    .bind(sid).bind(ts).bind(input).bind(output).bind(total).bind(total)
                    .execute(&pool).await.unwrap();
            }
        };

        // In the current month: 1M input → $1.40.
        insert(first_of_month, 1_000_000, 0, 1_000_000, &pool).await;
        // One second into the previous month: 5M input → must be excluded from MTD.
        insert(last_of_prev_month, 5_000_000, 0, 5_000_000, &pool).await;
        pool.close().await;

        let summary = sm.get_usage_summary().await.unwrap();

        // MTD sees only the first-of-month row.
        assert_eq!(
            summary.month_to_date.total_tokens,
            Some(1_000_000),
            "the last-second-of-previous-month row must be excluded"
        );
        assert_eq!(summary.month_to_date.input_tokens, 1_000_000);
        assert!(
            matches!(summary.month_to_date.cost, Some(c) if (c - 1.40).abs() < 1e-9),
            "got {:?}",
            summary.month_to_date.cost
        );
        assert!(!summary.month_to_date.has_unpriced);

        // All-time sees both rows: 1M + 5M = 6M input → $8.40.
        assert_eq!(summary.all_time.total_tokens, Some(6_000_000));
        assert!(
            matches!(summary.all_time.cost, Some(c) if (c - 8.40).abs() < 1e-9),
            "got {:?}",
            summary.all_time.cost
        );

        // `month` is the current local YYYY-MM.
        assert_eq!(summary.month.len(), 7);
        assert_eq!(summary.month.chars().nth(4), Some('-'));
    }

    #[tokio::test]
    async fn migration_11_preserves_unknown_model_history_and_is_idempotent() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        // Build a DB frozen at schema v10: token_events WITHOUT the model_id /
        // provider columns, so opening the manager must run migration 11.
        {
            let opts = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();

            sqlx::query("CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)")
                .execute(&pool).await.unwrap();
            for v in 1..=10 {
                sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
                    .bind(v)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            sqlx::query(
                r#"CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
                    user_set_name BOOLEAN DEFAULT FALSE, session_type TEXT NOT NULL DEFAULT 'user', working_dir TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    extension_data TEXT DEFAULT '{}', total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER,
                    accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER, accumulated_output_tokens INTEGER,
                    schedule_id TEXT, workflow_json TEXT, user_workflow_values_json TEXT, provider_name TEXT,
                    model_config_json TEXT, diverged_from TEXT, external_key TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            sqlx::query(
                r#"CREATE TABLE messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES sessions(id),
                    role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL,
                    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP, tokens INTEGER, metadata_json TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            // v10-shape token_events: no model_id / provider columns.
            sqlx::query(
                r#"CREATE TABLE token_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, ts INTEGER NOT NULL,
                    input_tokens INTEGER, output_tokens INTEGER, total_tokens INTEGER NOT NULL DEFAULT 0
                )"#,
            ).execute(&pool).await.unwrap();

            // Session S1 has only the session's final model/provider, which is
            // not valid attribution for earlier turns.
            sqlx::query(
                "INSERT INTO sessions (id, name, working_dir, provider_name, model_config_json) VALUES ('s1', 'has model', '/tmp/a', 'openai', '{\"model_name\":\"gpt-5\"}')",
            ).execute(&pool).await.unwrap();
            // Session S2 has no model config → its events stay unknown.
            sqlx::query(
                "INSERT INTO sessions (id, name, working_dir) VALUES ('s2', 'no model', '/tmp/b')",
            )
            .execute(&pool)
            .await
            .unwrap();

            sqlx::query("INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens) VALUES ('s1', 100, 8, 2, 10)")
                .execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens) VALUES ('s1', 200, 40, 10, 50)")
                .execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens) VALUES ('s2', 300, 5, 1, 6)")
                .execute(&pool).await.unwrap();
            pool.close().await;
        }

        // Opening the real manager triggers run_migrations → the `11 =>` arm.
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        // Both sessions' legacy rows remain explicitly unattributed. Copying
        // S1's final model backward would fabricate history after a model switch.
        let s1 = sm.get_session_model_usage("s1").await.unwrap();
        assert_eq!(s1.len(), 1);
        assert_eq!(s1[0].model_id, None);
        assert_eq!(s1[0].provider, None);
        assert_eq!(s1[0].total_tokens, None);
        assert_eq!(s1[0].turns, 2);

        // S2 has no stored model, so its row stays unknown (NULL model_id).
        let s2 = sm.get_session_model_usage("s2").await.unwrap();
        assert_eq!(s2.len(), 1);
        assert_eq!(s2[0].model_id, None);
        assert_eq!(s2[0].provider, None);
        assert_eq!(s2[0].total_tokens, None);

        // Idempotency: re-applying migration 11 on the already-migrated DB is a
        // no-op — the pragma guards skip the ADD COLUMNs and neither invocation
        // fabricates model attribution.
        let db_path2 = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        let opts = SqliteConnectOptions::new().filename(&db_path2);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        assert_eq!(
            SessionStorage::get_schema_version(&pool).await.unwrap(),
            CURRENT_SCHEMA_VERSION,
            "v10 must run usage v11/v12 before loop v13-v16"
        );
        SessionStorage::apply_migration(&pool, 11).await.unwrap();
        SessionStorage::apply_migration(&pool, 11).await.unwrap();
        pool.close().await;

        let s1_again = sm.get_session_model_usage("s1").await.unwrap();
        assert_eq!(s1_again[0].total_tokens, None);
        assert_eq!(s1_again[0].model_id, None);
        let s2_again = sm.get_session_model_usage("s2").await.unwrap();
        assert_eq!(s2_again[0].model_id, None);
    }

    #[tokio::test]
    async fn migration_12_adds_nullable_accounting_columns_and_is_idempotent() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        // Build a DB frozen at schema v11: token_events WITH model_id/provider
        // but WITHOUT the cache columns, so opening the manager runs migration 12.
        {
            let opts = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();

            sqlx::query("CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)")
                .execute(&pool).await.unwrap();
            for v in 1..=11 {
                sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
                    .bind(v)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            sqlx::query(
                r#"CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
                    user_set_name BOOLEAN DEFAULT FALSE, session_type TEXT NOT NULL DEFAULT 'user', working_dir TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    extension_data TEXT DEFAULT '{}', total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER,
                    accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER, accumulated_output_tokens INTEGER,
                    schedule_id TEXT, workflow_json TEXT, user_workflow_values_json TEXT, provider_name TEXT,
                    model_config_json TEXT, diverged_from TEXT, external_key TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            sqlx::query(
                r#"CREATE TABLE messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES sessions(id),
                    role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL,
                    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP, tokens INTEGER, metadata_json TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            // v11-shape token_events: model_id/provider but no cache columns.
            sqlx::query(
                r#"CREATE TABLE token_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, ts INTEGER NOT NULL,
                    input_tokens INTEGER, output_tokens INTEGER, total_tokens INTEGER NOT NULL DEFAULT 0,
                    model_id TEXT, provider TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            sqlx::query(
                "INSERT INTO sessions (id, name, working_dir) VALUES ('s1', 'legacy', '/tmp/a')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens, model_id, provider) VALUES ('s1', 100, 8, 2, 10, 'm', 'p')")
                .execute(&pool).await.unwrap();
            pool.close().await;
        }

        // Opening the manager triggers run_migrations → the `12 =>` arm, which
        // adds nullable accounting columns; the pre-existing legacy row remains
        // unknown instead of being rewritten as measured zero.
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let rows = sm.get_session_model_usage("s1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model_id, None);
        assert_eq!(rows[0].provider, None);
        assert_eq!(rows[0].total_tokens, None);
        assert_eq!(rows[0].cache_read_tokens, None);
        assert_eq!(rows[0].cache_creation_tokens, None);

        // A new turn records real cache values through the added columns.
        sm.record_token_event(
            "s1",
            Some(1),
            Some(1),
            902,
            Some("m-new"),
            Some("p"),
            Some(700),
            Some(200),
        )
        .await
        .unwrap();
        let rows = sm.get_session_model_usage("s1").await.unwrap();
        let current = rows
            .iter()
            .find(|row| row.model_id.as_deref() == Some("m-new"))
            .unwrap();
        assert_eq!(current.total_tokens, Some(902));
        assert_eq!(current.cache_read_tokens, Some(700));
        assert_eq!(current.cache_creation_tokens, Some(200));

        // Idempotency: re-applying migration 12 twice more is a no-op (the pragma
        // guards skip the ADD COLUMNs) and does not disturb the recorded data.
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        SessionStorage::apply_migration(&pool, 12).await.unwrap();
        SessionStorage::apply_migration(&pool, 12).await.unwrap();
        for column in [
            "billed_total_tokens",
            "cache_read_tokens",
            "cache_creation_tokens",
            "event_key",
            "session_type",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_table_info('token_events') WHERE name = ?1",
            )
            .bind(column)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(exists, 1, "missing migrated column {column}");
        }
        let event_index: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_token_events_event_key'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event_index, 1);
        pool.close().await;

        let rows = sm.get_session_model_usage("s1").await.unwrap();
        let legacy = rows.iter().find(|row| row.model_id.is_none()).unwrap();
        assert_eq!(legacy.total_tokens, None);
        assert_eq!(legacy.cache_read_tokens, None);
        assert_eq!(legacy.cache_creation_tokens, None);
        let current = rows
            .iter()
            .find(|row| row.model_id.as_deref() == Some("m-new"))
            .unwrap();
        assert_eq!(current.total_tokens, Some(902));
        assert_eq!(current.cache_read_tokens, Some(700));
        assert_eq!(current.cache_creation_tokens, Some(200));
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn experimental_loop_v11_through_v14_shapes_upgrade_without_loss() {
        for legacy_version in 11..=14 {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir
                .path()
                .join(format!("experimental-v{legacy_version}.db"));
            let options = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .unwrap();

            sqlx::query(
                "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO schema_version (version) VALUES (?1)")
                .bind(legacy_version)
                .execute(&pool)
                .await
                .unwrap();

            let branch_column = if legacy_version >= 12 {
                ", branch_point_msg_uid TEXT"
            } else {
                ""
            };
            sqlx::query(&format!(
                "CREATE TABLE sessions (id TEXT PRIMARY KEY, session_type TEXT NOT NULL DEFAULT 'user'{branch_column})"
            ))
            .execute(&pool)
            .await
            .unwrap();
            let message_uid_column = if legacy_version >= 12 {
                ", msg_uid TEXT"
            } else {
                ""
            };
            sqlx::query(&format!(
                "CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, content_json TEXT NOT NULL, metadata_json TEXT{message_uid_column})"
            ))
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE token_events (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, ts INTEGER NOT NULL, input_tokens INTEGER, output_tokens INTEGER, total_tokens INTEGER NOT NULL DEFAULT 0)",
            )
            .execute(&pool)
            .await
            .unwrap();

            if legacy_version >= 12 {
                sqlx::query(
                    "INSERT INTO sessions (id, session_type, branch_point_msg_uid) VALUES ('s1', 'user', 'legacy-anchor')",
                )
                .execute(&pool)
                .await
                .unwrap();
            } else {
                sqlx::query("INSERT INTO sessions (id, session_type) VALUES ('s1', 'user')")
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            let content_json =
                serde_json::to_string(&vec![MessageContent::text("legacy searchable")]).unwrap();
            if legacy_version >= 12 {
                sqlx::query(
                    "INSERT INTO messages (session_id, content_json, msg_uid) VALUES ('s1', ?1, 'legacy-uid')",
                )
                .bind(&content_json)
                .execute(&pool)
                .await
                .unwrap();
            } else {
                sqlx::query("INSERT INTO messages (session_id, content_json) VALUES ('s1', ?1)")
                    .bind(&content_json)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            sqlx::query(
                "INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens) VALUES ('s1', 100, 8, 2, 10)",
            )
            .execute(&pool)
            .await
            .unwrap();

            SessionStorage::create_checkpoints_table(&pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO checkpoints (id, session_id, turn_index, anchor_ts, kind, commit_sha, tree_sha) VALUES ('cp1', 's1', 0, 100, 'pre_step', 'commit', 'tree')",
            )
            .execute(&pool)
            .await
            .unwrap();
            if legacy_version >= 13 {
                sqlx::query(MESSAGES_FTS_DDL).execute(&pool).await.unwrap();
                sqlx::query(MESSAGES_FTS_INSERT)
                    .bind("legacy searchable")
                    .bind("s1")
                    .bind(1_i64)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            if legacy_version >= 14 {
                SessionStorage::create_message_blobs_table(&pool)
                    .await
                    .unwrap();
                sqlx::query(
                    "INSERT INTO message_blobs (blob_uid, session_id, created_at, bytes, content) VALUES ('blob1', 's1', 100, 7, 'payload')",
                )
                .execute(&pool)
                .await
                .unwrap();
            }

            SessionStorage::run_migrations(&pool).await.unwrap();

            assert_eq!(
                SessionStorage::get_schema_version(&pool).await.unwrap(),
                CURRENT_SCHEMA_VERSION,
                "experimental v{legacy_version} did not reach v16"
            );
            for column in [
                "model_id",
                "provider",
                "cache_read_tokens",
                "cache_creation_tokens",
                "billed_total_tokens",
                "event_key",
                "session_type",
            ] {
                assert!(
                    SessionStorage::table_has_column(&pool, "token_events", column)
                        .await
                        .unwrap(),
                    "experimental v{legacy_version} missed usage column {column}"
                );
            }
            let usage: (i64, Option<String>) = sqlx::query_as(
                "SELECT total_tokens, session_type FROM token_events WHERE session_id = 's1'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(usage, (10, Some("user".into())));

            let checkpoint_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM checkpoints WHERE id = 'cp1'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(checkpoint_count, 1);
            let message_uid: String =
                sqlx::query_scalar("SELECT msg_uid FROM messages WHERE id = 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                message_uid,
                if legacy_version >= 12 {
                    "legacy-uid"
                } else {
                    "m1"
                }
            );
            let branch_point: Option<String> =
                sqlx::query_scalar("SELECT branch_point_msg_uid FROM sessions WHERE id = 's1'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                branch_point.as_deref(),
                (legacy_version >= 12).then_some("legacy-anchor")
            );
            let fts_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM messages_fts WHERE session_id = 's1' AND messages_fts MATCH 'legacy'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(fts_count, 1, "FTS rows were duplicated or lost");
            let blob_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM message_blobs WHERE session_id = 's1'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(blob_count, i64::from(legacy_version >= 14));
        }
    }

    #[tokio::test]
    async fn pr13_v12_database_reconciles_usage_and_adds_loop_schema() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        // Early v12 builds had model/cache columns and marked the schema as
        // current, but lacked billed totals, durable event keys, and event-level
        // session types. They also copied final-session attribution backward and
        // could materialize unknown cache values as zero.
        {
            let options = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO schema_version (version) VALUES (12)")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE sessions (id TEXT PRIMARY KEY, session_type TEXT NOT NULL DEFAULT 'user')",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT NOT NULL, content_json TEXT NOT NULL DEFAULT '[]', metadata_json TEXT)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                r#"CREATE TABLE token_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id TEXT NOT NULL,
                    ts INTEGER NOT NULL,
                    input_tokens INTEGER,
                    output_tokens INTEGER,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    model_id TEXT,
                    provider TEXT,
                    cache_read_tokens INTEGER,
                    cache_creation_tokens INTEGER
                )"#,
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO sessions (id, session_type) VALUES ('s1', 'user')")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens, model_id, provider, cache_read_tokens, cache_creation_tokens) VALUES ('s1', 100, 8, 2, 10, 'fabricated-final-model', 'fabricated-provider', 0, 0)",
            )
            .execute(&pool)
            .await
            .unwrap();
            pool.close().await;
        }

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let rows = sm.get_session_model_usage("s1").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model_id, None);
        assert_eq!(rows[0].provider, None);
        assert_eq!(rows[0].total_tokens, None);
        assert_eq!(rows[0].cache_read_tokens, None);
        assert_eq!(rows[0].cache_creation_tokens, None);

        let pool = sm.storage.pool().await.unwrap();
        SessionStorage::reconcile_usage_schema(pool).await.unwrap();
        SessionStorage::reconcile_usage_schema(pool).await.unwrap();
        SessionStorage::reconcile_loop_schema(pool).await.unwrap();
        SessionStorage::reconcile_loop_schema(pool).await.unwrap();
        let version: i32 = sqlx::query_scalar("SELECT MAX(version) FROM schema_version")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        for table in ["checkpoints", "messages_fts", "message_blobs"] {
            let exists: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = ?1")
                    .bind(table)
                    .fetch_one(pool)
                    .await
                    .unwrap();
            assert_eq!(exists, 1, "missing reconciled table {table}");
        }
        assert!(
            SessionStorage::table_has_column(pool, "messages", "msg_uid")
                .await
                .unwrap()
        );
        assert!(
            SessionStorage::table_has_column(pool, "sessions", "branch_point_msg_uid")
                .await
                .unwrap()
        );
        type ReconciledUsageRow = (
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<String>,
        );
        let raw: ReconciledUsageRow = sqlx::query_as(
            "SELECT model_id, provider, cache_read_tokens, cache_creation_tokens, billed_total_tokens, session_type FROM token_events WHERE session_id = 's1'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(raw, (None, None, None, None, None, Some("user".into())));

        let before_delete = sm.get_usage_summary().await.unwrap();
        assert_eq!(before_delete.all_time.turns, 1);
        sm.delete_session("s1").await.unwrap();
        let after_delete = sm.get_usage_summary().await.unwrap();
        assert_eq!(after_delete.all_time.turns, 1);
        assert_eq!(after_delete.all_time.total_tokens, None);
    }

    #[tokio::test]
    async fn test_get_token_counts_matches_get_session() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = seed_session_with_messages(&sm, 2).await;
        sm.update(&session.id)
            .total_tokens(Some(4321))
            .input_tokens(Some(4000))
            .output_tokens(Some(321))
            .accumulated_total_tokens(Some(9999))
            .accumulated_input_tokens(Some(9000))
            .accumulated_output_tokens(Some(999))
            .apply()
            .await
            .unwrap();

        // The lightweight token-only query must return exactly the same token
        // counters that the full `get_session` exposes (it just skips the
        // COUNT(*) and metadata columns).
        let full = sm.get_session(&session.id, false).await.unwrap();
        let counts = sm.get_token_counts(&session.id).await.unwrap();

        assert_eq!(counts.total_tokens, full.total_tokens);
        assert_eq!(counts.input_tokens, full.input_tokens);
        assert_eq!(counts.output_tokens, full.output_tokens);
        assert_eq!(
            counts.accumulated_total_tokens,
            full.accumulated_total_tokens
        );
        assert_eq!(
            counts.accumulated_input_tokens,
            full.accumulated_input_tokens
        );
        assert_eq!(
            counts.accumulated_output_tokens,
            full.accumulated_output_tokens
        );
        assert_eq!(counts.total_tokens, Some(4321));
        assert_eq!(counts.accumulated_total_tokens, Some(9999));
    }

    /// BR-52: both ways of turning stored counters into the `TokenState` clients
    /// see must agree, since one seeds the SSE stream (from the session row the
    /// route already read) and the other refreshes it (from the agent's own
    /// boundary read). If they disagreed, the token readout would jump.
    #[tokio::test]
    async fn token_state_from_counts_and_from_session_agree() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = seed_session_with_messages(&sm, 1).await;
        sm.update(&session.id)
            .total_tokens(Some(4321))
            .input_tokens(Some(4000))
            .output_tokens(Some(321))
            .accumulated_total_tokens(Some(9999))
            .accumulated_input_tokens(Some(9000))
            .accumulated_output_tokens(Some(999))
            .apply()
            .await
            .unwrap();

        let full = sm.get_session(&session.id, false).await.unwrap();
        let from_session = TokenState::from(&full);
        let from_counts = TokenState::from(sm.get_token_counts(&session.id).await.unwrap());

        assert_eq!(from_session.total_tokens, 4321);
        assert_eq!(from_session.input_tokens, 4000);
        assert_eq!(from_session.output_tokens, 321);
        assert_eq!(from_session.accumulated_total_tokens, 9999);
        assert_eq!(from_session.accumulated_input_tokens, 9000);
        assert_eq!(from_session.accumulated_output_tokens, 999);

        assert_eq!(from_counts.total_tokens, from_session.total_tokens);
        assert_eq!(from_counts.input_tokens, from_session.input_tokens);
        assert_eq!(from_counts.output_tokens, from_session.output_tokens);
        assert_eq!(
            from_counts.accumulated_total_tokens,
            from_session.accumulated_total_tokens
        );
        assert_eq!(
            from_counts.accumulated_input_tokens,
            from_session.accumulated_input_tokens
        );
        assert_eq!(
            from_counts.accumulated_output_tokens,
            from_session.accumulated_output_tokens
        );
    }

    /// A brand-new session has NULL counters; both conversions must read as zero
    /// rather than panicking or surfacing a negative default.
    #[tokio::test]
    async fn token_state_of_a_fresh_session_is_zero() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                PathBuf::from("/tmp/fresh"),
                "Fresh".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let state = TokenState::from(&session);
        assert_eq!(state.total_tokens, 0);
        assert_eq!(state.accumulated_total_tokens, 0);

        let state = TokenState::from(sm.get_token_counts(&session.id).await.unwrap());
        assert_eq!(state.total_tokens, 0);
        assert_eq!(state.accumulated_total_tokens, 0);
    }

    #[tokio::test]
    async fn test_diverge_empty_conversation() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = sm
            .create_session(
                PathBuf::from("/tmp/empty"),
                "Empty".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let diverged = sm
            .copy_session(&original.id, "Branch of empty".to_string())
            .await
            .unwrap();

        assert_ne!(diverged.id, original.id);
        assert_eq!(diverged.message_count, 0);
        assert_eq!(diverged.name, "Branch of empty");
    }

    #[tokio::test]
    async fn test_diverge_of_a_diverge_chains() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 2).await;
        let first = sm
            .copy_session(&original.id, "First branch".to_string())
            .await
            .unwrap();
        let second = sm
            .copy_session(&first.id, "Second branch".to_string())
            .await
            .unwrap();

        // Three distinct sessions, all sharing the same history.
        let ids: std::collections::HashSet<_> =
            [&original.id, &first.id, &second.id].into_iter().collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(second.message_count, 4);
        assert_eq!(sm.list_sessions().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_diverge_nonexistent_session_errors() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let result = sm
            .copy_session("does_not_exist", "Branch".to_string())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_diverge_produces_unique_ids() {
        let temp_dir = TempDir::new().unwrap();
        let sm = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));

        let original = seed_session_with_messages(&sm, 2).await;

        let mut handles = vec![];
        for i in 0..NUM_CONCURRENT_SESSIONS {
            let sm = Arc::clone(&sm);
            let oid = original.id.clone();
            handles.push(tokio::spawn(async move {
                sm.copy_session(&oid, format!("Branch {i}"))
                    .await
                    .unwrap()
                    .id
            }));
        }

        let mut ids = std::collections::HashSet::new();
        for h in handles {
            ids.insert(h.await.unwrap());
        }
        // Every concurrent diverge yields a unique id, none colliding with the
        // original.
        assert_eq!(ids.len(), NUM_CONCURRENT_SESSIONS as usize);
        assert!(!ids.contains(&original.id));

        // Original + all branches persisted; original still has its 4 messages.
        assert_eq!(
            sm.list_sessions().await.unwrap().len(),
            NUM_CONCURRENT_SESSIONS as usize + 1
        );
        assert_eq!(
            sm.get_session(&original.id, true)
                .await
                .unwrap()
                .message_count,
            4
        );
    }

    // ── Branch naming + lineage (diverge_session) ───────────────────────────

    #[test]
    fn test_strip_branch_suffix() {
        assert_eq!(strip_branch_suffix("Foo"), "Foo");
        assert_eq!(strip_branch_suffix("Foo (branch 1)"), "Foo");
        assert_eq!(strip_branch_suffix("Foo (branch 42)"), "Foo");
        // Only strips one level / a real numeric suffix.
        assert_eq!(
            strip_branch_suffix("Foo (branch 1) (branch 2)"),
            "Foo (branch 1)"
        );
        assert_eq!(strip_branch_suffix("Foo (branch)"), "Foo (branch)");
        assert_eq!(strip_branch_suffix("Foo (branch abc)"), "Foo (branch abc)");
        // A name that merely contains the word branch is untouched.
        assert_eq!(strip_branch_suffix("My branch plan"), "My branch plan");
    }

    #[test]
    fn test_is_default_session_name() {
        assert!(is_default_session_name(""));
        assert!(is_default_session_name("   "));
        assert!(is_default_session_name("New Session"));
        assert!(is_default_session_name("new session"));
        assert!(is_default_session_name("CLI Session"));
        assert!(is_default_session_name("New session 3"));
        assert!(is_default_session_name("Session 12"));
        assert!(!is_default_session_name("Glycolysis explained"));
        assert!(!is_default_session_name("Session about sessions"));
    }

    #[tokio::test]
    async fn test_diverge_session_names_branches_and_sets_lineage() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 2).await;
        sm.update(&original.id)
            .user_provided_name("Glycolysis")
            .apply()
            .await
            .unwrap();

        let b1 = sm.diverge_session(&original.id, None, None).await.unwrap();
        let b2 = sm.diverge_session(&original.id, None, None).await.unwrap();

        // Sibling-numbered, collision-free names.
        assert_eq!(b1.name, "Glycolysis (branch 1)");
        assert_eq!(b2.name, "Glycolysis (branch 2)");
        // Lineage recorded; original has none.
        assert_eq!(b1.diverged_from.as_deref(), Some(original.id.as_str()));
        assert_eq!(b2.diverged_from.as_deref(), Some(original.id.as_str()));
        assert_eq!(
            sm.get_session(&original.id, false)
                .await
                .unwrap()
                .diverged_from,
            None
        );
        // Full history carried over.
        assert_eq!(b1.message_count, 4);
    }

    #[tokio::test]
    async fn test_edit_diverge_uses_branch_naming_lineage_and_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let original = sm
            .create_session(
                PathBuf::from("/tmp/edit_diverge"),
                "Original".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        sm.update(&original.id)
            .user_provided_name("Weather analysis")
            .apply()
            .await
            .unwrap();
        for message in [
            umsg(10, "first question"),
            amsg(11, "first answer"),
            umsg(20, "message to edit"),
            amsg(21, "answer to replace"),
        ] {
            sm.add_message(&original.id, &message).await.unwrap();
        }

        let loaded = sm.get_session(&original.id, true).await.unwrap();
        let expected_branch_point = loaded.conversation.as_ref().unwrap().messages()[1]
            .id
            .clone();

        let first = sm.diverge_session_for_edit(&original.id, 20).await.unwrap();
        let second = sm.diverge_session_for_edit(&original.id, 20).await.unwrap();

        assert_eq!(first.name, "Weather analysis (branch 1)");
        assert_eq!(second.name, "Weather analysis (branch 2)");
        assert!(first.user_set_name);
        assert_eq!(first.diverged_from.as_deref(), Some(original.id.as_str()));
        assert_eq!(first.branch_point_msg_uid, expected_branch_point);
        assert_eq!(first.message_count, 2);
        assert_eq!(
            first
                .conversation
                .as_ref()
                .unwrap()
                .messages()
                .iter()
                .map(Message::as_concat_text)
                .collect::<Vec<_>>(),
            vec!["first question", "first answer"]
        );
        assert_eq!(
            sm.get_session(&original.id, true)
                .await
                .unwrap()
                .message_count,
            4
        );
    }

    #[tokio::test]
    async fn test_diverge_of_a_branch_flattens_numbering() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 1).await;
        sm.update(&original.id)
            .user_provided_name("Topic")
            .apply()
            .await
            .unwrap();

        let b1 = sm.diverge_session(&original.id, None, None).await.unwrap();
        assert_eq!(b1.name, "Topic (branch 1)");

        // Diverging the *branch* strips its suffix and continues the family
        // count rather than nesting "(branch 1) (branch 1)".
        let b2 = sm.diverge_session(&b1.id, None, None).await.unwrap();
        assert_eq!(b2.name, "Topic (branch 2)");
        // Its lineage points at the immediate parent (the branch).
        assert_eq!(b2.diverged_from.as_deref(), Some(b1.id.as_str()));
    }

    #[tokio::test]
    async fn test_diverge_placeholder_name_derives_from_conversation() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        // Session left with the default placeholder name.
        let session = sm
            .create_session(
                PathBuf::from("/tmp/ph"),
                "New Session".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        sm.add_message(
            &session.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text("Explain the citric acid cycle")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let branch = sm.diverge_session(&session.id, None, None).await.unwrap();
        // Name derives from the first user message, not "New Session".
        assert!(
            branch.name.starts_with("Explain the citric acid cycle"),
            "unexpected branch name: {}",
            branch.name
        );
        assert!(branch.name.ends_with("(branch 1)"));
    }

    #[tokio::test]
    async fn test_diverge_custom_name_overrides() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 1).await;
        let branch = sm
            .diverge_session(&original.id, Some("  Hand Picked  ".to_string()), None)
            .await
            .unwrap();
        // Trimmed, used verbatim (no "(branch N)" suffix), lineage still set.
        assert_eq!(branch.name, "Hand Picked");
        assert_eq!(branch.diverged_from.as_deref(), Some(original.id.as_str()));
    }

    #[tokio::test]
    async fn test_diverge_branch_name_survives_like_wildcards() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = seed_session_with_messages(&sm, 1).await;
        // A name containing SQL LIKE metacharacters must not break sibling
        // counting.
        sm.update(&original.id)
            .user_provided_name("100%_done")
            .apply()
            .await
            .unwrap();

        let b1 = sm.diverge_session(&original.id, None, None).await.unwrap();
        let b2 = sm.diverge_session(&original.id, None, None).await.unwrap();
        assert_eq!(b1.name, "100%_done (branch 1)");
        assert_eq!(b2.name, "100%_done (branch 2)");
    }

    // ── Branch trimming (start exactly from the last complete answer) ───────

    fn umsg(created: i64, text: &str) -> Message {
        Message {
            id: None,
            role: Role::User,
            created,
            content: vec![MessageContent::text(text)],
            metadata: Default::default(),
        }
    }
    fn amsg(created: i64, text: &str) -> Message {
        Message {
            id: None,
            role: Role::Assistant,
            created,
            content: vec![MessageContent::text(text)],
            metadata: Default::default(),
        }
    }
    fn atool(created: i64) -> Message {
        let mut m = Message::assistant().with_tool_request(
            "call_1",
            Ok(rmcp::model::CallToolRequestParams {
                task: None,
                name: "shell".into(),
                arguments: None,
                meta: None,
            }),
        );
        m.created = created;
        m
    }

    #[test]
    fn test_trim_keeps_through_last_complete_answer() {
        let conv = Conversation::new_unvalidated(vec![
            umsg(1, "q1"),
            amsg(2, "a1"),
            umsg(3, "q2"),
            amsg(4, "a2"),
        ]);
        let t = trim_to_last_complete_answer_at(&conv, None, None);
        assert_eq!(t.messages().len(), 4);
        assert_eq!(t.messages().last().unwrap().as_concat_text(), "a2");
    }

    #[test]
    fn test_trim_drops_trailing_unanswered_question() {
        // The reported bug: diverge fired while the agent was still generating
        // the answer to q2, so the DB has q2 persisted with no answer yet.
        let conv = Conversation::new_unvalidated(vec![umsg(1, "q1"), amsg(2, "a1"), umsg(3, "q2")]);
        let t = trim_to_last_complete_answer_at(&conv, None, None);
        assert_eq!(t.messages().len(), 2);
        assert_eq!(t.messages().last().unwrap().as_concat_text(), "a1");
    }

    #[test]
    fn test_trim_drops_trailing_empty_assistant_and_tool_call() {
        // Mid tool-call: assistant("") then a pending tool request, no final
        // answer yet → branch ends at the previous complete answer.
        let conv = Conversation::new_unvalidated(vec![
            umsg(1, "q1"),
            amsg(2, "a1"),
            umsg(3, "q2"),
            amsg(4, ""),
            atool(5),
        ]);
        let t = trim_to_last_complete_answer_at(&conv, None, None);
        assert_eq!(t.messages().len(), 2);
        assert_eq!(t.messages().last().unwrap().as_concat_text(), "a1");
    }

    #[test]
    fn test_trim_anchor_bounds_branch_to_clicked_answer() {
        let conv = Conversation::new_unvalidated(vec![
            umsg(10, "q1"),
            amsg(20, "a1"),
            umsg(30, "q2"),
            amsg(40, "a2"),
        ]);
        // Per-message Diverge button clicked on a1 (created=20).
        let t = trim_to_last_complete_answer_at(&conv, None, Some(20));
        assert_eq!(t.messages().len(), 2);
        assert_eq!(t.messages().last().unwrap().as_concat_text(), "a1");
    }

    #[test]
    fn test_trim_empty_when_no_complete_answer() {
        // Diverged before the first reply landed: only an unanswered question.
        let conv = Conversation::new_unvalidated(vec![umsg(1, "q1")]);
        let t = trim_to_last_complete_answer_at(&conv, None, None);
        assert!(t.messages().is_empty());
    }

    #[tokio::test]
    async fn test_diverge_trims_in_flight_turn_end_to_end() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        // q0 → a0 (complete), then a follow-up question whose answer is still
        // being generated when the user hits Diverge.
        let original = seed_session_with_messages(&sm, 1).await;
        let now = chrono::Utc::now().timestamp_millis();
        sm.add_message(&original.id, &umsg(now + 10_000, "follow-up?"))
            .await
            .unwrap();

        let branch = sm.diverge_session(&original.id, None, None).await.unwrap();
        // The unanswered follow-up is NOT carried over; the branch ends at a0.
        assert_eq!(branch.message_count, 2);
        let last = branch
            .conversation
            .unwrap()
            .messages()
            .last()
            .unwrap()
            .as_concat_text();
        assert_eq!(last, "answer 0");
    }

    // ── BR-45: stable per-message ids + branch divergence point ──────────────

    /// Ids survive the exact operation that used to renumber them — a full
    /// history rewrite (compaction/edit). Every kept message keeps its id; only
    /// a newly-inserted message gets a fresh, non-positional id.
    #[tokio::test]
    async fn msg_uid_stable_across_replace_conversation() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = seed_session_with_messages(&sm, 2).await; // 4 messages
        let before: Vec<String> = session
            .conversation
            .as_ref()
            .unwrap()
            .messages()
            .iter()
            .map(|m| m.id.clone().unwrap())
            .collect();
        assert_eq!(before.len(), 4);
        // Durable ids are UUIDs, not the old positional `msg_{session}_{idx}`.
        assert!(before.iter().all(|id| !id.starts_with("msg_")));

        // Simulate a compaction: rewrite the same messages plus one brand-new
        // summary message that carries no id yet.
        let mut msgs: Vec<Message> = session.conversation.as_ref().unwrap().messages().to_vec();
        msgs.push(amsg(chrono::Utc::now().timestamp_millis() + 99, "summary"));
        sm.replace_conversation(&session.id, &Conversation::new_unvalidated(msgs))
            .await
            .unwrap();

        let after = sm.get_session(&session.id, true).await.unwrap();
        let after_ids: Vec<String> = after
            .conversation
            .unwrap()
            .messages()
            .iter()
            .map(|m| m.id.clone().unwrap())
            .collect();
        assert_eq!(after_ids.len(), 5);
        // The four kept messages preserved their ids across the rewrite.
        assert_eq!(&after_ids[..4], &before[..]);
        // The new summary got a fresh, distinct, non-positional id.
        let new_id = &after_ids[4];
        assert!(!new_id.starts_with("msg_"));
        assert!(!before.contains(new_id));
    }

    // ── conversation revision token ──────────────────────────────────────────

    async fn revision_session(sm: &SessionManager) -> String {
        sm.create_session(PathBuf::from("/tmp/rev"), "rev".into(), SessionType::User)
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn conversation_revision_advances_on_append() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;

        let empty = sm.conversation_revision(&id).await.unwrap();
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        let one = sm.conversation_revision(&id).await.unwrap();
        sm.add_message(&id, &umsg(2, "two")).await.unwrap();
        let two = sm.conversation_revision(&id).await.unwrap();

        assert_ne!(empty, one);
        assert_ne!(one, two);
        assert_eq!(two.message_count(), 2);
    }

    /// The property a message COUNT cannot have: rewriting the same messages
    /// back changes the revision. This is why the freshness guard is not the
    /// length compare BR-12 shipped — an edit that drops one message plus the
    /// next turn's user message nets to zero and would pass a length check.
    #[tokio::test]
    async fn conversation_revision_advances_on_identical_rewrite() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        sm.add_message(&id, &umsg(2, "two")).await.unwrap();

        let before = sm.conversation_revision(&id).await.unwrap();
        let same = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        sm.replace_conversation(&id, &same).await.unwrap();
        let after = sm.conversation_revision(&id).await.unwrap();

        assert_eq!(before.message_count(), after.message_count());
        assert_ne!(
            before, after,
            "an identical-content rewrite must still move the revision"
        );
    }

    /// AUTOINCREMENT never rewinds `sqlite_sequence`, so truncating and
    /// refilling to the same count cannot reproduce an earlier revision.
    #[tokio::test]
    async fn conversation_revision_never_ababs_across_truncate_and_refill() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        sm.add_message(&id, &umsg(2, "two")).await.unwrap();
        let original = sm.conversation_revision(&id).await.unwrap();

        sm.truncate_conversation(&id, 2).await.unwrap();
        sm.add_message(&id, &umsg(3, "three")).await.unwrap();
        let refilled = sm.conversation_revision(&id).await.unwrap();

        assert_eq!(original.message_count(), refilled.message_count());
        assert_ne!(original, refilled, "revision must not ABA");
    }

    /// The mechanism the test below turns into data loss. An AUTOINCREMENT
    /// sequence exists precisely to keep a rowid non-reusable for the LIFETIME
    /// OF THE DATABASE; `DELETE FROM messages` already leaves it alone (that is
    /// what makes the truncate-and-refill test above hold). Deleting the
    /// `sqlite_sequence` row hands the next session the rowids of a session
    /// that is gone.
    #[tokio::test]
    async fn clearing_all_sessions_does_not_rewind_the_message_rowids() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let first = revision_session(&sm).await;
        sm.add_message(&first, &umsg(1, "one")).await.unwrap();
        sm.add_message(&first, &umsg(2, "two")).await.unwrap();
        let before = sm.conversation_revision(&first).await.unwrap();

        sm.clear_all_sessions().await.unwrap();

        let second = revision_session(&sm).await;
        sm.add_message(&second, &umsg(3, "three")).await.unwrap();
        let after = sm.conversation_revision(&second).await.unwrap();

        assert!(
            after.max_rowid > before.max_rowid,
            "a message written after the wipe must not reuse a rowid the wipe \
             freed ({} -> {})",
            before.max_rowid,
            after.max_rowid
        );
    }

    /// #51 W3: the revision must not ABA across a `/reset` History wipe either.
    ///
    /// `clear_all_sessions` empties every table, and `create_session` allocates
    /// `YYYYMMDD_N` as `MAX(suffix) + 1` over a now-empty `sessions` table — so
    /// the next session created the same day REUSES the id. If the message
    /// AUTOINCREMENT sequence is rewound with it, a rewrite still holding a
    /// revision from the previous incarnation finds its basis satisfied by a
    /// brand-new session's brand-new message and destroys it. Eager compaction
    /// is detached from its turn, so it really can still be in flight across a
    /// wipe.
    #[tokio::test]
    async fn a_guarded_rewrite_cannot_cross_a_clear_and_recreate() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let old = revision_session(&sm).await;
        sm.add_message(&old, &umsg(1, "first incarnation"))
            .await
            .unwrap();
        // A detached rewrite (eager compaction) snapshots the session...
        let (known, basis) = snapshot(&sm, &old).await;

        // ...and while it is still working, the user wipes History from /reset.
        sm.clear_all_sessions().await.unwrap();

        let new = revision_session(&sm).await;
        assert_eq!(new, old, "the ABA needs the session id to be reused");
        sm.add_message(&new, &umsg(2, "second incarnation"))
            .await
            .unwrap();

        let replacement =
            Conversation::new_unvalidated(vec![umsg(9, "summary of the first incarnation")]);
        let (outcome, _) = sm
            .replace_conversation_preserving_tail(&new, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(
            outcome,
            ReplaceOutcome::Stale,
            "a revision from a previous incarnation of this id must never match"
        );
        assert_eq!(
            stored_texts(&sm, &new).await,
            vec!["second incarnation".to_string()],
            "the new session's acknowledged message must survive the stale rewrite"
        );
    }

    /// ...and the incarnation token has to close it BY ITSELF, not merely as a
    /// consequence of leaving `sqlite_sequence` alone. Any number of things
    /// present a rewound sequence to a running process — a database restored
    /// from a backup, a hand-copied `sessions.db`, a future wipe path that
    /// re-creates the file. Here the pre-fix conditions are reproduced
    /// EXACTLY: the old session's rowids are replayed one for one, so
    /// `(count, max_rowid)` is byte-identical across the two incarnations and
    /// only the row identity can tell them apart.
    #[tokio::test]
    async fn a_guarded_rewrite_is_refused_even_when_the_rowids_are_replayed_exactly() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let old = revision_session(&sm).await;
        sm.add_message(&old, &umsg(1, "first incarnation"))
            .await
            .unwrap();
        let (known, basis) = snapshot(&sm, &old).await;

        sm.clear_all_sessions().await.unwrap();
        // Rewind the message sequence behind the store's back.
        let pool = sm.storage().pool().await.unwrap();
        sqlx::query("DELETE FROM sqlite_sequence WHERE name = 'messages'")
            .execute(pool)
            .await
            .unwrap();

        let new = revision_session(&sm).await;
        assert_eq!(new, old);
        sm.add_message(&new, &umsg(2, "second incarnation"))
            .await
            .unwrap();

        let replayed = sm.conversation_revision(&new).await.unwrap();
        assert_eq!(
            (replayed.count, replayed.max_rowid),
            (basis.count, basis.max_rowid),
            "this test is only meaningful if the rowids really were replayed"
        );

        let replacement = Conversation::new_unvalidated(vec![umsg(9, "summary")]);
        let (outcome, _) = sm
            .replace_conversation_preserving_tail(&new, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(outcome, ReplaceOutcome::Stale);
        assert_eq!(
            stored_texts(&sm, &new).await,
            vec!["second incarnation".to_string()]
        );
    }

    #[tokio::test]
    async fn conversation_revision_is_zero_for_an_empty_session() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        let revision = sm.conversation_revision(&id).await.unwrap();
        assert_eq!(revision.count, 0);
        assert_eq!(revision.max_rowid, 0);
    }

    /// A rewrite changes the session's content, so it must move `updated_at`
    /// like any other write — otherwise a compacted or edited session sorts as
    /// untouched in the `ORDER BY updated_at DESC` session list. (The bump is
    /// also the rewrite transaction's write-first lock acquisition; see the
    /// comment on `replace_conversation_inner`.)
    #[tokio::test]
    async fn a_conversation_rewrite_bumps_the_session_updated_at() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let s = sm
            .create_session(PathBuf::from("/tmp/a"), "s".into(), SessionType::User)
            .await
            .unwrap();
        sm.add_message(&s.id, &umsg(1, "hello")).await.unwrap();

        // Back-date it so the one-second resolution of `datetime('now')` cannot
        // make a real bump look like no change.
        let pool = sm.storage().pool().await.unwrap();
        sqlx::query("UPDATE sessions SET updated_at = '2000-01-01 00:00:00' WHERE id = ?")
            .bind(&s.id)
            .execute(pool)
            .await
            .unwrap();
        let before = sm.get_session(&s.id, false).await.unwrap().updated_at;

        sm.replace_conversation(&s.id, &Conversation::new_unvalidated(vec![umsg(2, "bye")]))
            .await
            .unwrap();

        let after = sm.get_session(&s.id, false).await.unwrap().updated_at;
        assert!(
            after > before,
            "a whole-history rewrite must bump updated_at ({before} -> {after})"
        );
    }

    /// ...but an IMPORT is the one caller that must not inherit that bump.
    ///
    /// `import_legacy_session` INSERTs the historical `created_at`/`updated_at`
    /// and then writes the conversation through `replace_conversation_inner`,
    /// whose write-first statement stamps `updated_at = datetime('now')` and
    /// beats the back-dated value (the test above is what proves it beats it).
    /// Every legacy JSONL session would then sort as "just now", collapsing a
    /// user's whole history to today in the sidebar on the single migration run
    /// that imports it.
    #[tokio::test]
    async fn importing_a_legacy_session_keeps_its_historical_updated_at() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        // Any call opens (and migrates) the database.
        sm.create_session(PathBuf::from("/tmp/live"), "live".into(), SessionType::User)
            .await
            .unwrap();
        let pool = sm.storage().pool().await.unwrap();

        let historical: DateTime<Utc> = "2021-03-04T05:06:07Z".parse().unwrap();
        let legacy = Session {
            id: "legacy-1".into(),
            working_dir: PathBuf::from("/tmp/legacy"),
            name: "an old chat".into(),
            user_set_name: false,
            session_type: SessionType::User,
            created_at: historical,
            updated_at: historical,
            extension_data: Default::default(),
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            accumulated_total_tokens: None,
            accumulated_input_tokens: None,
            accumulated_output_tokens: None,
            schedule_id: None,
            workflow: None,
            user_workflow_values: None,
            conversation: Some(Conversation::new_unvalidated(vec![umsg(1, "hi")])),
            message_count: 1,
            provider_name: None,
            model_config: None,
            diverged_from: None,
            branch_point_msg_uid: None,
        };
        SessionStorage::import_legacy_session(pool, &legacy)
            .await
            .unwrap();

        let imported = sm.get_session("legacy-1", true).await.unwrap();
        assert_eq!(
            imported.updated_at, historical,
            "an imported session must keep its historical mtime, not the \
             import's wall clock"
        );
        assert_eq!(imported.created_at, historical);
        // The conversation itself still landed.
        assert_eq!(imported.conversation.unwrap().messages().len(), 1);
    }

    // ── replace_conversation_preserving_tail ─────────────────────────────────

    /// Snapshot a session the way every rewrite caller must.
    async fn snapshot(sm: &SessionManager, id: &str) -> (Conversation, ConversationRevision) {
        let (session, revision) = sm.snapshot_for_rewrite(id).await.unwrap();
        (session.conversation.unwrap(), revision)
    }

    async fn stored_texts(sm: &SessionManager, id: &str) -> Vec<String> {
        sm.get_session(id, true)
            .await
            .unwrap()
            .conversation
            .unwrap()
            .messages()
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|c| match c {
                MessageContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// The no-concurrency path must be exactly the old behaviour.
    #[tokio::test]
    async fn preserving_tail_writes_verbatim_when_nothing_moved() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        sm.add_message(&id, &amsg(2, "two")).await.unwrap();

        let (known, basis) = snapshot(&sm, &id).await;
        let replacement = Conversation::new_unvalidated(vec![known.messages()[1].clone()]);

        let (outcome, stored) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(outcome, ReplaceOutcome::Replaced);
        assert_eq!(stored.messages().len(), 1);
        assert_eq!(stored_texts(&sm, &id).await, vec!["two".to_string()]);
    }

    /// THE headline case: a message appended while the caller was computing its
    /// rewrite must survive. This is BR-71's `mode: "note"`, and the shipped
    /// `biorouter term log` cross-process append.
    #[tokio::test]
    async fn preserving_tail_carries_over_a_concurrent_append() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        sm.add_message(&id, &amsg(2, "two")).await.unwrap();

        let (known, basis) = snapshot(&sm, &id).await;

        // ...another writer appends while the "summarizer" runs.
        let note_uid = sm
            .add_message(&id, &umsg(3, "NOTE from elsewhere"))
            .await
            .unwrap();

        // ...and the caller writes back a compaction of what it saw.
        let replacement = Conversation::new_unvalidated(vec![umsg(9, "summary")]);
        let (outcome, stored) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(
            outcome,
            ReplaceOutcome::ReplacedPreservingTail { preserved: 1 }
        );

        let texts = stored_texts(&sm, &id).await;
        assert_eq!(
            texts,
            vec!["summary".to_string(), "NOTE from elsewhere".to_string()],
            "the note must survive, after the compacted head"
        );
        // The returned conversation agrees with the store...
        assert_eq!(
            stored
                .messages()
                .iter()
                .filter_map(|m| m.id.clone())
                .collect::<Vec<_>>()
                .last()
                .cloned(),
            Some(note_uid.clone())
        );
        // ...and the note kept its stable id across the rewrite.
        let reloaded = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(
            reloaded.messages()[1].id.as_deref(),
            Some(note_uid.as_str())
        );
    }

    /// The discriminator against a watermark-only implementation: the writer's
    /// OWN messages, appended after it captured its basis and then deliberately
    /// compacted away, must stay gone.
    #[tokio::test]
    async fn preserving_tail_does_not_resurrect_the_writers_own_compacted_messages() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();

        let (_, basis) = snapshot(&sm, &id).await;
        // The caller appends its own messages past the watermark...
        sm.add_message(&id, &amsg(2, "mine-a")).await.unwrap();
        sm.add_message(&id, &amsg(3, "mine-b")).await.unwrap();
        // ...so its view (taken now) contains them.
        let known = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();

        let replacement = Conversation::new_unvalidated(vec![umsg(9, "summary")]);
        let (outcome, _) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(outcome, ReplaceOutcome::Replaced);
        assert_eq!(stored_texts(&sm, &id).await, vec!["summary".to_string()]);
    }

    /// The mirror discriminator, against a uid-set-only implementation: a
    /// message the snapshot saw and the compaction dropped must stay dropped.
    #[tokio::test]
    async fn preserving_tail_does_not_resurrect_messages_the_snapshot_already_dropped() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "drop me")).await.unwrap();
        sm.add_message(&id, &amsg(2, "keep me")).await.unwrap();

        let (known, basis) = snapshot(&sm, &id).await;
        let replacement = Conversation::new_unvalidated(vec![known.messages()[1].clone()]);

        let (outcome, _) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(outcome, ReplaceOutcome::Replaced);
        assert_eq!(stored_texts(&sm, &id).await, vec!["keep me".to_string()]);
    }

    #[tokio::test]
    async fn preserving_tail_reports_stale_after_a_concurrent_truncate() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        sm.add_message(&id, &amsg(2, "two")).await.unwrap();

        let (known, basis) = snapshot(&sm, &id).await;
        // A checkpoint restore / message edit removes the tail underneath us.
        sm.truncate_conversation(&id, 2).await.unwrap();
        let before = sm.conversation_revision(&id).await.unwrap();

        let replacement = Conversation::new_unvalidated(vec![umsg(9, "summary")]);
        let (outcome, returned) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(outcome, ReplaceOutcome::Stale);
        assert_eq!(
            sm.conversation_revision(&id).await.unwrap(),
            before,
            "a stale rewrite must not write anything at all"
        );
        assert_eq!(stored_texts(&sm, &id).await, vec!["one".to_string()]);
        assert_eq!(returned.messages().len(), 1, "the replacement, unchanged");
    }

    #[tokio::test]
    async fn preserving_tail_reports_stale_after_a_concurrent_wholesale_rewrite() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();

        let (known, basis) = snapshot(&sm, &id).await;
        // Another rewrite lands first: every row is renumbered above the
        // watermark, so the prefix count goes to 0.
        sm.replace_conversation(&id, &Conversation::new_unvalidated(vec![umsg(5, "theirs")]))
            .await
            .unwrap();

        let replacement = Conversation::new_unvalidated(vec![umsg(9, "summary")]);
        let (outcome, _) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(outcome, ReplaceOutcome::Stale);
        assert_eq!(stored_texts(&sm, &id).await, vec!["theirs".to_string()]);
    }

    #[tokio::test]
    async fn preserving_tail_reports_session_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        // Force the pool + schema to exist.
        let _ = revision_session(&sm).await;

        let empty = Conversation::default();
        let (outcome, _) = sm
            .replace_conversation_preserving_tail(
                "no-such-session",
                &Conversation::new_unvalidated(vec![umsg(1, "x")]),
                ConversationRevision::from_parts(0, 0, 0),
                &empty,
            )
            .await
            .unwrap();

        assert_eq!(outcome, ReplaceOutcome::SessionNotFound);
        assert_eq!(
            sm.conversation_revision("no-such-session")
                .await
                .unwrap()
                .message_count(),
            0,
            "nothing may be written for a session that does not exist"
        );
    }

    #[tokio::test]
    async fn preserving_tail_orders_recovered_messages_last_by_rowid() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();

        let (known, basis) = snapshot(&sm, &id).await;
        // Deliberately out-of-order `created` values: insertion order, not the
        // timestamp, is what the read path (`ORDER BY id`) reproduces.
        sm.add_message(&id, &umsg(50, "note-a")).await.unwrap();
        sm.add_message(&id, &umsg(40, "note-b")).await.unwrap();

        let replacement = Conversation::new_unvalidated(vec![umsg(9, "summary")]);
        let (outcome, _) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(
            outcome,
            ReplaceOutcome::ReplacedPreservingTail { preserved: 2 }
        );
        assert_eq!(
            stored_texts(&sm, &id).await,
            vec![
                "summary".to_string(),
                "note-a".to_string(),
                "note-b".to_string()
            ]
        );
    }

    /// A row a schema upgrade has not backfilled has a NULL `msg_uid`, and the
    /// read path synthesizes `msg_{session}_{idx}` for it. The recovery scan
    /// must synthesize the SAME id, or such a row would look foreign forever
    /// and be duplicated on every rewrite.
    #[tokio::test]
    async fn preserving_tail_recovers_a_legacy_null_msg_uid_row() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();

        let (known, basis) = snapshot(&sm, &id).await;

        let pool = sm.storage().pool().await.unwrap();
        sqlx::query(
            "INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json, msg_uid) \
             VALUES (?, 'user', ?, 7, NULL, NULL)",
        )
        .bind(&id)
        .bind(serde_json::to_string(&vec![MessageContent::text("legacy note")]).unwrap())
        .execute(pool)
        .await
        .unwrap();

        // The read path's synthesized id for that row (index 1 of the session).
        let read_id = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap()
            .messages()[1]
            .id
            .clone()
            .unwrap();
        assert_eq!(read_id, format!("msg_{id}_1"));

        let replacement = Conversation::new_unvalidated(vec![umsg(9, "summary")]);
        let (outcome, stored) = sm
            .replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(
            outcome,
            ReplaceOutcome::ReplacedPreservingTail { preserved: 1 }
        );
        assert_eq!(
            stored.messages()[1].id.as_deref(),
            Some(read_id.as_str()),
            "the recovery scan must reproduce the read path's synthesized id"
        );
        assert_eq!(
            stored_texts(&sm, &id).await,
            vec!["summary".to_string(), "legacy note".to_string()]
        );
    }

    /// A recovered message must be findable by chat recall — the FTS mirror is
    /// rebuilt from the merged list, not from the caller's replacement.
    #[tokio::test]
    async fn preserving_tail_indexes_recovered_messages_for_chat_recall() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "photosynthesis"))
            .await
            .unwrap();

        let (known, basis) = snapshot(&sm, &id).await;
        sm.add_message(&id, &umsg(2, "chemiosmosis in mitochondria"))
            .await
            .unwrap();

        let replacement = Conversation::new_unvalidated(vec![umsg(9, "a compaction summary")]);
        sm.replace_conversation_preserving_tail(&id, &replacement, basis, &known)
            .await
            .unwrap();

        assert_eq!(
            sm.search_chat_history("chemiosmosis", None, None, None, None)
                .await
                .unwrap()
                .results
                .len(),
            1,
            "a recovered message must be indexed for recall"
        );
        assert!(
            sm.search_chat_history("photosynthesis", None, None, None, None)
                .await
                .unwrap()
                .results
                .is_empty(),
            "a compacted-away message must drop out of the index"
        );
    }

    // ── truncate_conversation ────────────────────────────────────────────────

    async fn fts_hits(sm: &SessionManager, term: &str) -> usize {
        sm.search_chat_history(term, None, None, None, None)
            .await
            .unwrap()
            .results
            .len()
    }

    /// Rows in the recall mirror with no message behind them.
    async fn orphan_fts_rows(sm: &SessionManager, id: &str) -> i64 {
        let pool = sm.storage().pool().await.unwrap();
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM messages_fts f WHERE f.session_id = ? \
             AND NOT EXISTS (SELECT 1 FROM messages m WHERE m.id = f.message_id)",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// #51 W4: a checkpoint restore / message edit deletes message rows without
    /// touching the FTS recall mirror, so the mirror keeps a row per dropped
    /// message forever — every restore leaks more, and the only thing keeping
    /// them from surfacing as hits is an `INNER JOIN messages` in an unrelated
    /// query. The rewrite path has always kept the two in lockstep; truncation
    /// never did.
    #[tokio::test]
    async fn truncating_a_conversation_drops_its_rows_from_chat_recall() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "photosynthesis"))
            .await
            .unwrap();
        sm.add_message(&id, &amsg(2, "chemiosmosis in mitochondria"))
            .await
            .unwrap();
        assert_eq!(fts_hits(&sm, "chemiosmosis").await, 1, "seeded");

        sm.truncate_conversation(&id, 2).await.unwrap();

        assert_eq!(
            orphan_fts_rows(&sm, &id).await,
            0,
            "the recall mirror must not keep a row for a message that is gone"
        );
        assert_eq!(
            fts_hits(&sm, "chemiosmosis").await,
            0,
            "a truncated message must drop out of chat recall"
        );
        assert_eq!(
            fts_hits(&sm, "photosynthesis").await,
            1,
            "...and a surviving one must stay in it"
        );
    }

    /// #51 W4: truncation changes the session's content, so it must move
    /// `updated_at` for the same reason a rewrite must — otherwise an edited or
    /// restored session sorts as untouched in the `ORDER BY updated_at DESC`
    /// session list.
    #[tokio::test]
    async fn truncating_a_conversation_bumps_the_session_updated_at() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        sm.add_message(&id, &amsg(2, "two")).await.unwrap();

        let pool = sm.storage().pool().await.unwrap();
        sqlx::query("UPDATE sessions SET updated_at = '2000-01-01 00:00:00' WHERE id = ?")
            .bind(&id)
            .execute(pool)
            .await
            .unwrap();
        let before = sm.get_session(&id, false).await.unwrap().updated_at;

        sm.truncate_conversation(&id, 2).await.unwrap();

        let after = sm.get_session(&id, false).await.unwrap().updated_at;
        assert!(
            after > before,
            "a truncation must bump updated_at ({before} -> {after})"
        );
    }

    /// #51 W4: the deletion range is `created_timestamp >= ts`, which is open
    /// above — so a message appended after the caller decided where to cut is
    /// necessarily inside it and is destroyed, after its writer was told the
    /// append succeeded. Bounding by the rowid watermark the caller's view
    /// actually covered is what separates "the tail the user asked to drop"
    /// from "a message that arrived while we were deciding".
    #[tokio::test]
    async fn a_bounded_truncate_keeps_an_append_the_caller_never_saw() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = revision_session(&sm).await;
        sm.add_message(&id, &umsg(1, "one")).await.unwrap();
        sm.add_message(&id, &amsg(2, "two")).await.unwrap();

        // The caller reads the conversation and decides to cut at ts 2.
        let basis = sm.conversation_revision(&id).await.unwrap();

        // ...and another writer appends before the delete lands. Its timestamp
        // is necessarily >= the cut.
        sm.add_message(&id, &umsg(9, "NOTE from elsewhere"))
            .await
            .unwrap();

        assert_eq!(
            sm.truncate_conversation_bounded(&id, 2, basis)
                .await
                .unwrap(),
            TruncateOutcome::Truncated { removed: 1 }
        );

        assert_eq!(
            stored_texts(&sm, &id).await,
            vec!["one".to_string(), "NOTE from elsewhere".to_string()],
            "the cut tail goes; the append the caller never saw stays"
        );
    }

    /// A watermark from a previous incarnation of the id describes rowids that
    /// belonged to a different conversation, so it may not bound a delete in
    /// this one — the same reasoning as the guarded rewrite (#51 W3).
    #[tokio::test]
    async fn a_bounded_truncate_refuses_a_basis_from_a_previous_incarnation() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let old = revision_session(&sm).await;
        sm.add_message(&old, &umsg(1, "first incarnation"))
            .await
            .unwrap();
        let basis = sm.conversation_revision(&old).await.unwrap();

        sm.clear_all_sessions().await.unwrap();
        let new = revision_session(&sm).await;
        assert_eq!(new, old);
        sm.add_message(&new, &umsg(2, "second incarnation"))
            .await
            .unwrap();

        assert_eq!(
            sm.truncate_conversation_bounded(&new, 1, basis)
                .await
                .unwrap(),
            TruncateOutcome::Stale
        );
        assert_eq!(
            stored_texts(&sm, &new).await,
            vec!["second incarnation".to_string()],
            "a refused truncation must not delete anything"
        );
    }

    #[tokio::test]
    async fn a_bounded_truncate_reports_a_missing_session() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let _ = revision_session(&sm).await;
        let basis = sm.conversation_revision("no-such-session").await.unwrap();

        assert_eq!(
            sm.truncate_conversation_bounded("no-such-session", 0, basis)
                .await
                .unwrap(),
            TruncateOutcome::SessionNotFound
        );
    }

    /// Real overlap against a real pool and a real WAL file: 200 appends racing
    /// 60 snapshot -> "summarize" -> write-back cycles.
    ///
    /// Assertion 2 is what makes this irreplaceable. If the write-first
    /// `UPDATE sessions` at the top of the rewrite transaction is ever
    /// "simplified away" as a gratuitous write, the deferred transaction reads
    /// before it writes and the DELETE fails SQLITE_BUSY_SNAPSHOT *instantly*,
    /// bypassing the busy timeout. That surfaces here as an error, not as data
    /// loss, so a test that only checked the final message set would pass while
    /// the fix was broken.
    ///
    /// A busy *append* is a different animal from a busy *rewrite* and is
    /// tolerated here, exactly as `conversation_writeback_stress.rs` does. This
    /// workload appends with only a `yield_now` between writes against a
    /// rewriter looping on a 2 ms gap, so the rewriter re-takes the single write
    /// lock faster than a starved appender's 5 s `busy_timeout` can expire.
    /// That contention is pre-existing (it reproduces on the unguarded
    /// `replace_conversation` path), it is loud — `add_message` returns `Err`,
    /// so the caller knows — and it vanishes at realistic compaction gaps. See
    /// "What it costs" in `docs/agent-loop/conversation-writeback-freshness.md`.
    /// The invariant is unweakened: only ACKNOWLEDGED appends enter `uids`, and
    /// every one of those must still be on disk.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_appends_survive_racing_rewrites() {
        let temp = TempDir::new().unwrap();
        let sm = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let session = sm
            .create_session(PathBuf::from("/tmp"), "race".into(), SessionType::User)
            .await
            .unwrap();
        sm.add_message(&session.id, &umsg(1, "seed")).await.unwrap();

        let appender = {
            let sm = Arc::clone(&sm);
            let id = session.id.clone();
            tokio::spawn(async move {
                let mut uids = Vec::new();
                let mut busy = 0usize;
                for i in 0..200 {
                    let m = umsg(100 + i, &format!("note-{i}"));
                    match sm.add_message(&id, &m).await {
                        Ok(uid) => uids.push(uid),
                        Err(e) if e.to_string().contains("database is locked") => busy += 1,
                        Err(e) => panic!(
                            "append failed for a reason other than lock \
                                          contention: {e}"
                        ),
                    }
                    tokio::task::yield_now().await;
                }
                (uids, busy)
            })
        };
        let rewriter = {
            let sm = Arc::clone(&sm);
            let id = session.id.clone();
            tokio::spawn(async move {
                let mut outcomes = Vec::new();
                let mut errors = Vec::new();
                for i in 0..60 {
                    let (session, basis) = sm.snapshot_for_rewrite(&id).await.unwrap();
                    let known = session.conversation.unwrap();
                    // Stand-in for the summarization round-trip.
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    let mut msgs = known.messages().clone();
                    msgs.push(amsg(1_000 + i, &format!("summary-{i}")));
                    match sm
                        .replace_conversation_preserving_tail(
                            &id,
                            &Conversation::new_unvalidated(msgs),
                            basis,
                            &known,
                        )
                        .await
                    {
                        Ok((outcome, _)) => outcomes.push(outcome),
                        Err(e) => errors.push(e.to_string()),
                    }
                }
                (outcomes, errors)
            })
        };
        let (uids, rewrites) = tokio::join!(appender, rewriter);
        let (uids, busy_appends) = uids.unwrap();
        let (outcomes, errors) = rewrites.unwrap();

        // 2. The write-first lock ordering held. A busy REWRITE is always a
        // hard failure: it means the transaction read before it wrote.
        assert!(
            errors.is_empty(),
            "no rewrite may fail (a `database is locked` here means the \
             transaction read before it wrote): {errors:?}"
        );
        // The race must not have degenerated into "the appender never got in",
        // which would make every assertion below vacuous.
        assert!(
            uids.len() >= 150,
            "only {} of 200 appends were acknowledged ({busy_appends} lost the \
             write lock) — too few for this to still be testing anything",
            uids.len()
        );
        // 4. Every rewrite reported a real, non-silent outcome.
        assert_eq!(outcomes.len(), 60);

        let stored = sm
            .get_session(&session.id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        let stored_ids: Vec<String> = stored
            .messages()
            .iter()
            .filter_map(|m| m.id.clone())
            .collect();

        // 1. Nothing appended was destroyed...
        let lost: Vec<&String> = uids.iter().filter(|u| !stored_ids.contains(u)).collect();
        assert!(
            lost.is_empty(),
            "{} of {} appended messages were destroyed by racing rewrites",
            lost.len(),
            uids.len()
        );
        // 3. ...and nothing was duplicated (UNIQUE(session_id, msg_uid) held).
        let mut unique = stored_ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), stored_ids.len(), "duplicate msg_uid stored");
    }

    /// The CLI-vs-daemon shape: two INDEPENDENT stores (separate connection
    /// pools, no shared in-memory state) over one `sessions.db`, exactly as a
    /// terminal `biorouter` and the desktop `biorouterd` see it. Nothing in
    /// process memory orders these — only the guard inside the rewrite
    /// transaction does.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn appends_from_a_second_store_survive_racing_rewrites() {
        let temp = TempDir::new().unwrap();
        let daemon = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let session = daemon
            .create_session(PathBuf::from("/tmp"), "cross".into(), SessionType::User)
            .await
            .unwrap();
        daemon
            .add_message(&session.id, &umsg(1, "seed"))
            .await
            .unwrap();

        // A second store over the same file — its own pool, its own WAL reader.
        let cli = Arc::new(SessionManager::new(temp.path().to_path_buf()));

        let appender = {
            let cli = Arc::clone(&cli);
            let id = session.id.clone();
            tokio::spawn(async move {
                // Same busy-append tolerance as
                // `concurrent_appends_survive_racing_rewrites`, and for the same
                // reason — only acknowledged appends are held to the invariant.
                let mut uids = Vec::new();
                let mut busy = 0usize;
                for i in 0..80 {
                    let m = umsg(100 + i, &format!("term-log-{i}"));
                    match cli.add_message(&id, &m).await {
                        Ok(uid) => uids.push(uid),
                        Err(e) if e.to_string().contains("database is locked") => busy += 1,
                        Err(e) => panic!(
                            "cross-pool append failed for a reason other \
                                          than lock contention: {e}"
                        ),
                    }
                    tokio::task::yield_now().await;
                }
                (uids, busy)
            })
        };
        let rewriter = {
            let daemon = Arc::clone(&daemon);
            let id = session.id.clone();
            tokio::spawn(async move {
                let mut errors = Vec::new();
                for i in 0..25 {
                    let (session, basis) = daemon.snapshot_for_rewrite(&id).await.unwrap();
                    let known = session.conversation.unwrap();
                    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    let mut msgs = known.messages().clone();
                    msgs.push(amsg(1_000 + i, &format!("summary-{i}")));
                    if let Err(e) = daemon
                        .replace_conversation_preserving_tail(
                            &id,
                            &Conversation::new_unvalidated(msgs),
                            basis,
                            &known,
                        )
                        .await
                    {
                        errors.push(e.to_string());
                    }
                }
                errors
            })
        };
        let (uids, errors) = tokio::join!(appender, rewriter);
        let (uids, busy_appends) = uids.unwrap();
        let errors = errors.unwrap();

        assert!(
            errors.is_empty(),
            "cross-pool rewrites must not fail: {errors:?}"
        );
        assert!(
            uids.len() >= 60,
            "only {} of 80 cross-pool appends were acknowledged \
             ({busy_appends} lost the write lock) — too few to test anything",
            uids.len()
        );

        let stored_ids: Vec<String> = daemon
            .get_session(&session.id, true)
            .await
            .unwrap()
            .conversation
            .unwrap()
            .messages()
            .iter()
            .filter_map(|m| m.id.clone())
            .collect();
        let lost: Vec<&String> = uids.iter().filter(|u| !stored_ids.contains(u)).collect();
        assert!(
            lost.is_empty(),
            "{} of {} messages appended by the other store were destroyed",
            lost.len(),
            uids.len()
        );
    }

    /// Two racing preserving-tail rewrites must not collide on
    /// UNIQUE(session_id, msg_uid): the rewrite path has no duplicate-uid
    /// recovery of its own, unlike `add_message`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_rewrites_do_not_produce_duplicate_msg_uids() {
        let temp = TempDir::new().unwrap();
        let sm = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let session = sm
            .create_session(PathBuf::from("/tmp"), "race2".into(), SessionType::User)
            .await
            .unwrap();
        for i in 0..5 {
            sm.add_message(&session.id, &umsg(i, &format!("m{i}")))
                .await
                .unwrap();
        }

        let spawn_rewriter = |tag: &'static str| {
            let sm = Arc::clone(&sm);
            let id = session.id.clone();
            tokio::spawn(async move {
                let mut errors = Vec::new();
                for i in 0..40 {
                    let (session, basis) = sm.snapshot_for_rewrite(&id).await.unwrap();
                    let known = session.conversation.unwrap();
                    tokio::task::yield_now().await;
                    let mut msgs = known.messages().clone();
                    msgs.push(amsg(2_000 + i, &format!("{tag}-{i}")));
                    if let Err(e) = sm
                        .replace_conversation_preserving_tail(
                            &id,
                            &Conversation::new_unvalidated(msgs),
                            basis,
                            &known,
                        )
                        .await
                    {
                        errors.push(e.to_string());
                    }
                }
                errors
            })
        };
        let (a, b) = tokio::join!(spawn_rewriter("a"), spawn_rewriter("b"));
        let mut errors = a.unwrap();
        errors.extend(b.unwrap());
        assert!(
            errors.is_empty(),
            "racing rewrites must not error: {errors:?}"
        );

        let stored_ids: Vec<String> = sm
            .get_session(&session.id, true)
            .await
            .unwrap()
            .conversation
            .unwrap()
            .messages()
            .iter()
            .filter_map(|m| m.id.clone())
            .collect();
        let mut unique = stored_ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), stored_ids.len(), "duplicate msg_uid stored");
    }

    /// #41 resilience: inserting a message whose caller-supplied id already
    /// exists in the session with DIFFERENT content must NOT abort — the store
    /// re-mints the uid, keeps both rows, and RETURNS the effective uid so the
    /// caller's in-memory message can adopt it. Before this, the duplicate hit
    /// `UNIQUE(session_id, msg_uid)` (SQLite 2067) and the whole turn died;
    /// then the re-mint happened only in SQLite while the caller kept the
    /// stale id.
    #[tokio::test]
    async fn add_message_reminting_recovers_from_a_duplicate_uid() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/dup_uid"),
                "Dup uid".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let first_uid = sm
            .add_message(&session.id, &amsg(now, "first").with_id("shared-uid"))
            .await
            .unwrap();
        assert_eq!(
            first_uid, "shared-uid",
            "the happy path returns the caller-supplied uid"
        );
        // The forced duplicate: same session, same caller-supplied id,
        // different content.
        let reminted_uid = sm
            .add_message(&session.id, &amsg(now + 1, "second").with_id("shared-uid"))
            .await
            .expect("a duplicate uid must be re-minted, not abort the turn");
        assert_ne!(
            reminted_uid, "shared-uid",
            "the returned uid must be the re-minted one, not the stale caller id"
        );

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        let messages = loaded.conversation.unwrap();
        assert_eq!(messages.len(), 2, "both messages must be persisted");
        let ids: Vec<String> = messages
            .messages()
            .iter()
            .map(|m| m.id.clone().unwrap())
            .collect();
        assert_eq!(ids[0], "shared-uid", "the first insert keeps its id");
        assert_eq!(
            ids[1], reminted_uid,
            "the persisted row and the returned uid must agree"
        );
        let texts: Vec<String> = messages
            .messages()
            .iter()
            .map(Message::as_concat_text)
            .collect();
        assert_eq!(texts, vec!["first", "second"]);

        // An id duplicated across DIFFERENT sessions is fine and untouched.
        let other = sm
            .create_session(
                PathBuf::from("/tmp/dup_uid2"),
                "Other".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        sm.add_message(&other.id, &amsg(now + 2, "elsewhere").with_id("shared-uid"))
            .await
            .unwrap();
        let other_loaded = sm.get_session(&other.id, true).await.unwrap();
        assert_eq!(
            other_loaded.conversation.unwrap().messages()[0]
                .id
                .as_deref(),
            Some("shared-uid")
        );
    }

    /// #41 idempotence: re-adding the EXACT same message (same uid, identical
    /// role/content/metadata — a caller retrying a write it believes failed)
    /// is success, not a re-mint. One row, same uid back, no duplicate.
    #[tokio::test]
    async fn add_message_treats_an_exact_replay_as_success() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/replay_uid"),
                "Replay".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let message = amsg(now, "same words").with_id("replay-uid");
        let first = sm.add_message(&session.id, &message).await.unwrap();
        let second = sm
            .add_message(&session.id, &message)
            .await
            .expect("an exact replay must be idempotent success");
        assert_eq!(first, "replay-uid");
        assert_eq!(
            second, "replay-uid",
            "the replay returns the SAME uid, so the caller's in-memory id \
             stays in agreement"
        );

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        let messages = loaded.conversation.unwrap();
        assert_eq!(
            messages.len(),
            1,
            "an exact replay must not create a duplicate row"
        );
        assert_eq!(messages.messages()[0].id.as_deref(), Some("replay-uid"));
    }

    /// #41 caller contract: after adopting the re-minted uid returned by a
    /// collision, re-persisting that same in-memory message is an exact
    /// replay — success, no third row. This is the loop the agent's persist
    /// batch runs (adopt effective uid → later replays are idempotent).
    #[tokio::test]
    async fn adopted_reminted_uid_makes_later_replays_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/adopt_uid"),
                "Adopt".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        sm.add_message(&session.id, &amsg(now, "original").with_id("clash-uid"))
            .await
            .unwrap();

        // Collision with different content: the store re-mints and the caller
        // adopts the returned uid (what agent.rs does after add_message).
        let mut colliding = amsg(now + 1, "different").with_id("clash-uid");
        let effective = sm.add_message(&session.id, &colliding).await.unwrap();
        assert_ne!(effective, "clash-uid");
        colliding.id = Some(effective.clone());

        // Replaying the adopted message is idempotent success.
        let replay = sm.add_message(&session.id, &colliding).await.unwrap();
        assert_eq!(replay, effective);

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        let messages = loaded.conversation.unwrap();
        assert_eq!(messages.len(), 2, "adopt-then-replay must not add a row");
        let ids: Vec<&str> = messages
            .messages()
            .iter()
            .map(|m| m.id.as_deref().unwrap())
            .collect();
        assert_eq!(ids, vec!["clash-uid", effective.as_str()]);
    }

    /// #41: a message with NO caller id gets a minted uid, and the caller is
    /// told which one, so it can stamp its in-memory copy.
    #[tokio::test]
    async fn add_message_returns_the_minted_uid_for_idless_messages() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/minted_uid"),
                "Minted".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let minted = sm
            .add_message(&session.id, &amsg(now, "no id"))
            .await
            .unwrap();
        assert!(!minted.is_empty());

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        assert_eq!(
            loaded.conversation.unwrap().messages()[0].id.as_deref(),
            Some(minted.as_str()),
            "the persisted uid and the returned uid must agree"
        );
    }

    /// #41: the idless soft-interrupt shape — a `Message::user()` minted
    /// mid-turn, persisted, then retained in the in-memory conversation and
    /// yielded. `add_message_adopting_uid` must stamp the store's minted uid
    /// onto the in-memory copy, so a later re-persist of the retained copy is
    /// an idempotent replay instead of a duplicate row under a fresh uid.
    #[tokio::test]
    async fn adopting_uid_keeps_an_idless_soft_interrupt_in_sync_with_storage() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/soft_interrupt"),
                "SoftInterrupt".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        // The soft-interrupt message carries no id when it is persisted.
        let mut m = amsg(chrono::Utc::now().timestamp_millis(), "user correction");
        assert!(m.id.is_none());
        sm.add_message_adopting_uid(&session.id, &mut m)
            .await
            .unwrap();

        let adopted = m.id.clone().expect("the minted uid must be adopted");
        let loaded = sm.get_session(&session.id, true).await.unwrap();
        assert_eq!(
            loaded.conversation.as_ref().unwrap().messages()[0]
                .id
                .as_deref(),
            Some(adopted.as_str()),
            "memory and storage must agree on the uid"
        );

        // Re-persisting the retained copy (e.g. a later conversation write)
        // is now a replay — before adoption it minted a SECOND row.
        sm.add_message_adopting_uid(&session.id, &mut m)
            .await
            .expect("re-persisting the adopted copy must be idempotent");
        assert_eq!(m.id.as_deref(), Some(adopted.as_str()));
        let reloaded = sm.get_session(&session.id, true).await.unwrap();
        assert_eq!(
            reloaded.conversation.unwrap().len(),
            1,
            "the adopted copy must replay, not duplicate"
        );
    }

    /// #41: the replay probe must include `created_timestamp`. Two genuinely
    /// distinct messages that happen to share uid, role, content AND metadata
    /// but were created at different times are NOT replays — collapsing them
    /// would silently drop the second one.
    #[tokio::test]
    async fn same_content_different_created_timestamp_is_not_a_replay() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/created_ts"),
                "CreatedTs".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let first = sm
            .add_message(&session.id, &amsg(now, "same words").with_id("ts-uid"))
            .await
            .unwrap();
        assert_eq!(first, "ts-uid");

        // Identical role/content/metadata, later creation time: a distinct
        // message, so it must be re-minted and kept — not treated as a replay.
        let second = sm
            .add_message(&session.id, &amsg(now + 5, "same words").with_id("ts-uid"))
            .await
            .expect("a distinct message must be re-minted, not abort");
        assert_ne!(
            second, "ts-uid",
            "a different created_timestamp is a different message, not a replay"
        );

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        assert_eq!(
            loaded.conversation.unwrap().len(),
            2,
            "both distinct messages must be persisted"
        );
    }

    /// #41: `metadata_json` is nullable for rows migrated from older schemas.
    /// The replay probe must decode it as an `Option` and treat NULL as "no
    /// metadata recorded" (matching a default-metadata message) — decoding a
    /// bare String made the probe ERROR on such rows, aborting the very turn
    /// the idempotent-replay path exists to save.
    #[tokio::test]
    async fn replay_against_a_null_metadata_row_still_matches() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/null_md"),
                "NullMd".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        let message = amsg(now, "migrated row").with_id("null-md-uid");
        sm.add_message(&session.id, &message).await.unwrap();

        // Simulate a row migrated from a pre-metadata schema.
        let pool = sm.storage().pool().await.unwrap();
        sqlx::query(
            "UPDATE messages SET metadata_json = NULL WHERE session_id = ? AND msg_uid = ?",
        )
        .bind(&session.id)
        .bind("null-md-uid")
        .execute(pool)
        .await
        .unwrap();

        let replay = sm
            .add_message(&session.id, &message)
            .await
            .expect("a NULL-metadata row must not error the replay probe");
        assert_eq!(
            replay, "null-md-uid",
            "the replay must match the migrated row, not re-mint"
        );

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        assert_eq!(
            loaded.conversation.unwrap().len(),
            1,
            "the replay must not duplicate the migrated row"
        );
    }

    #[tokio::test]
    async fn conversation_rewrite_preserves_message_order_when_timestamps_tie() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/message_order"),
                "Message order".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let conversation = Conversation::new_unvalidated(vec![
            umsg(10, "question").with_id("z-user-message"),
            amsg(20, "answer").with_id("a-assistant-message"),
        ]);
        sm.replace_conversation(&session.id, &conversation)
            .await
            .unwrap();

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        let messages = loaded.conversation.unwrap();
        assert_eq!(messages.messages()[0].role, Role::User);
        assert_eq!(messages.messages()[1].role, Role::Assistant);
        let texts = messages
            .messages()
            .iter()
            .map(Message::as_concat_text)
            .collect::<Vec<_>>();

        assert_eq!(texts, vec!["question", "answer"]);
    }

    /// A row an in-flight upgrade left with a NULL `msg_uid` still loads, using
    /// the legacy positional id as a fallback (BR-45 dual-read).
    #[tokio::test]
    async fn get_conversation_falls_back_to_positional_id_for_null_uid() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                PathBuf::from("/tmp/br45null"),
                "Original".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        // Insert directly with a NULL msg_uid (mimics a not-yet-backfilled row).
        let pool = sm.storage.pool().await.unwrap();
        sqlx::query(
            "INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json, msg_uid) VALUES (?, 'user', ?, 0, '{}', NULL)",
        )
        .bind(&session.id)
        .bind(serde_json::to_string(&vec![MessageContent::text("legacy")]).unwrap())
        .execute(pool)
        .await
        .unwrap();

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        let msg = &loaded.conversation.as_ref().unwrap().messages()[0];
        assert_eq!(
            msg.id.as_deref(),
            Some(format!("msg_{}_0", session.id).as_str())
        );
    }

    /// Two messages sharing a whole-second `created` used to collapse to one
    /// anchor, so a diverge at the first silently carried the second over. The
    /// durable-id anchor keeps only the strict prefix and records the divergence point
    /// (BR-45, item 3).
    #[tokio::test]
    async fn diverge_by_uid_beats_same_second_collision() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                PathBuf::from("/tmp/br45"),
                "Original".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        // a1, q2, a2 all share created = 2000 (a single whole second).
        for m in [
            umsg(1000, "q1"),
            amsg(2000, "a1"),
            umsg(2000, "q2"),
            amsg(2000, "a2"),
        ] {
            sm.add_message(&session.id, &m).await.unwrap();
        }

        let loaded = sm.get_session(&session.id, true).await.unwrap();
        let a1_uid = loaded
            .conversation
            .as_ref()
            .unwrap()
            .messages()
            .iter()
            .find(|m| m.as_concat_text() == "a1")
            .and_then(|m| m.id.clone())
            .unwrap();

        // Anchored by durable id: keep exactly [q1, a1].
        let by_uid = sm
            .diverge_session_at(&session.id, None, None, Some(a1_uid.clone()))
            .await
            .unwrap();
        let uid_texts: Vec<String> = by_uid
            .conversation
            .as_ref()
            .unwrap()
            .messages()
            .iter()
            .map(|m| m.as_concat_text())
            .collect();
        assert_eq!(uid_texts, vec!["q1".to_string(), "a1".to_string()]);
        // The divergence point is recorded on the child branch.
        assert_eq!(
            by_uid.branch_point_msg_uid.as_deref(),
            Some(a1_uid.as_str())
        );

        // The legacy timestamp anchor (2000) cannot disambiguate and carries a2
        // over — the very over-truncation the uid anchor fixes.
        let by_ts = sm
            .diverge_session(&session.id, None, Some(2000))
            .await
            .unwrap();
        assert_eq!(by_ts.message_count, 4);
    }

    /// Migration 14 backfills `msg_uid` deterministically from the durable
    /// rowid (`m` || id) and adds the branch divergence-point column.
    #[tokio::test]
    async fn migration_14_backfills_msg_uid_from_rowid() {
        let temp_dir = TempDir::new().unwrap();
        let db = temp_dir.path().join("v13.db");
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db)
                    .create_if_missing(true),
            )
            .await
            .unwrap();

        // A minimal pre-migration (v13) shape: no msg_uid, no branch column.
        sqlx::query("CREATE TABLE sessions (id TEXT PRIMARY KEY, diverged_from TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content_json TEXT NOT NULL,
                created_timestamp INTEGER NOT NULL,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                metadata_json TEXT
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (session_id, role, content_json, created_timestamp) VALUES ('s', 'user', '[]', 0), ('s', 'assistant', '[]', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        SessionStorage::apply_migration(&pool, 14).await.unwrap();

        let uids: Vec<(i64, Option<String>)> =
            sqlx::query_as("SELECT id, msg_uid FROM messages ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(uids[0].1.as_deref(), Some("m1"));
        assert_eq!(uids[1].1.as_deref(), Some("m2"));

        // The branch divergence-point column now exists and defaults to NULL.
        let bp: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT branch_point_msg_uid FROM sessions")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(bp.is_empty());
    }

    // ---- BR-17: FTS5 relevance-ranked chat recall ----

    #[tokio::test]
    async fn chat_recall_fts_ranks_by_relevance_not_recency() {
        // The exact case the old recency `LIKE` scan got wrong: an older
        // session that matches every query term must outrank a newer session
        // that matches only one of them.
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let relevant = sm
            .create_session(
                PathBuf::from("/tmp/a"),
                "relevant".into(),
                SessionType::User,
            )
            .await
            .unwrap();
        sm.add_message(
            &relevant.id,
            &umsg(
                1,
                "quantum entanglement experiment results were significant",
            ),
        )
        .await
        .unwrap();

        // Added later, so it is the more *recent* session.
        let recent = sm
            .create_session(PathBuf::from("/tmp/b"), "recent".into(), SessionType::User)
            .await
            .unwrap();
        sm.add_message(
            &recent.id,
            &umsg(2, "quantum mechanics is a broad topic in physics"),
        )
        .await
        .unwrap();

        let res = sm
            .search_chat_history("quantum entanglement experiment", None, None, None, None)
            .await
            .unwrap();

        assert_eq!(res.results.len(), 2, "both sessions mention 'quantum'");
        assert_eq!(
            res.results[0].session_id, relevant.id,
            "the fully-matching session must rank first under bm25"
        );
    }

    #[tokio::test]
    async fn chat_recall_fts_sanitizes_operator_query() {
        // A query containing FTS operators must not raise a syntax error; it is
        // treated as literal terms.
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let s = sm
            .create_session(PathBuf::from("/tmp/a"), "s".into(), SessionType::User)
            .await
            .unwrap();
        sm.add_message(&s.id, &umsg(1, "the CFTR gene and cystic fibrosis"))
            .await
            .unwrap();

        // Would be a malformed MATCH expression if passed through unsanitized.
        let res = sm
            .search_chat_history("CFTR AND (fibrosis*", None, None, None, None)
            .await
            .unwrap();
        assert_eq!(res.results.len(), 1);
        assert_eq!(res.results[0].session_id, s.id);
    }

    #[tokio::test]
    async fn chat_recall_fts_stays_in_sync_on_replace_conversation() {
        // The compaction/edit rewrite (DELETE + reinsert) must keep the FTS
        // index consistent — the old text drops out, the new text is findable.
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let s = sm
            .create_session(PathBuf::from("/tmp/a"), "s".into(), SessionType::User)
            .await
            .unwrap();
        sm.add_message(&s.id, &umsg(1, "photosynthesis in chloroplasts"))
            .await
            .unwrap();

        assert_eq!(
            sm.search_chat_history("photosynthesis", None, None, None, None)
                .await
                .unwrap()
                .results
                .len(),
            1
        );

        // Rewrite the conversation with entirely different text.
        let convo = Conversation::new_unvalidated(vec![umsg(2, "glycolysis in the cytoplasm")]);
        sm.replace_conversation(&s.id, &convo).await.unwrap();

        // Old term is gone, new term is present — index tracked the rewrite.
        assert!(sm
            .search_chat_history("photosynthesis", None, None, None, None)
            .await
            .unwrap()
            .results
            .is_empty());
        assert_eq!(
            sm.search_chat_history("glycolysis", None, None, None, None)
                .await
                .unwrap()
                .results
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn migration_15_backfills_fts_index() {
        // Production upgrade: a pre-v15 DB with existing messages gets an FTS
        // index built by migration 15's backfill, so recall works on history
        // that predates the feature.
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();

        let content_json =
            serde_json::to_string(&vec![MessageContent::text("mitochondria powerhouse cell")])
                .unwrap();

        {
            let opts = SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();

            sqlx::query("CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)")
                .execute(&pool).await.unwrap();
            for v in 1..=7 {
                sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
                    .bind(v)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            sqlx::query(
                r#"CREATE TABLE sessions (
                    id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
                    user_set_name BOOLEAN DEFAULT FALSE, session_type TEXT NOT NULL DEFAULT 'user', working_dir TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    extension_data TEXT DEFAULT '{}', total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER,
                    accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER, accumulated_output_tokens INTEGER,
                    schedule_id TEXT, workflow_json TEXT, user_workflow_values_json TEXT, provider_name TEXT, model_config_json TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            sqlx::query(
                r#"CREATE TABLE messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL REFERENCES sessions(id),
                    role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL,
                    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP, tokens INTEGER, metadata_json TEXT
                )"#,
            ).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO sessions (id, name, working_dir) VALUES ('20240101_1', 'old', '/tmp/old')")
                .execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO messages (session_id, role, content_json, created_timestamp) VALUES ('20240101_1', 'user', ?, 1)")
                .bind(&content_json)
                .execute(&pool).await.unwrap();
            pool.close().await;
        }

        // Opening the real manager migrates 8→16, including the FTS backfill.
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let res = sm
            .search_chat_history("mitochondria", None, None, None, None)
            .await
            .unwrap();
        assert_eq!(res.results.len(), 1, "backfilled message is searchable");
        assert_eq!(res.results[0].session_id, "20240101_1");
    }

    #[tokio::test]
    async fn chat_recall_falls_back_to_like_without_fts_table() {
        // A DB lacking messages_fts (older/partial migration) must still return
        // recall results via the legacy `LIKE` scan rather than erroring.
        use crate::session::chat_history_search::ChatHistorySearch;
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("nofts.db");
        let content_json =
            serde_json::to_string(&vec![MessageContent::text("ribosome translation")]).unwrap();

        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, description TEXT DEFAULT '', working_dir TEXT DEFAULT '', created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, role TEXT, content_json TEXT, created_timestamp INTEGER, timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO sessions (id) VALUES ('s1')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO messages (session_id, role, content_json, created_timestamp) VALUES ('s1', 'user', ?, 1)")
            .bind(&content_json)
            .execute(&pool)
            .await
            .unwrap();

        let res = ChatHistorySearch::new(&pool, "ribosome", None, None, None, None)
            .execute()
            .await
            .unwrap();
        assert_eq!(
            res.results.len(),
            1,
            "LIKE fallback still finds the message"
        );
        assert_eq!(res.results[0].session_id, "s1");
    }
}

#[cfg(test)]
mod activity_tests {
    use super::*;

    fn day(n: u32) -> String {
        format!("2026-03-{n:02}")
    }

    /// Linear scaling collapses every ordinary day into the faintest shade as
    /// soon as one marathon day exists. This pins the log+quartile behaviour that
    /// replaced it: ordinary days must occupy more than one level.
    #[test]
    fn one_huge_day_does_not_flatten_the_rest() {
        let mut sessions = Vec::new();
        let mut tokens = Vec::new();
        // 12 ordinary days spanning 20k..150k tokens ...
        for i in 1..=12u32 {
            sessions.push((day(i), 1 + i64::from(i % 3)));
            tokens.push((day(i), 20_000 + i64::from(i) * 11_000, 0, 0, true));
        }
        // ... and one 1.8M-token outlier.
        sessions.push((day(13), 6));
        tokens.push((day(13), 1_800_000, 0, 0, true));

        let w = build_activity_window(day(1), day(13), &sessions, &tokens, &[]);

        assert_eq!(w.days.len(), 13);
        let outlier = w.days.iter().find(|d| d.date == day(13)).unwrap();
        assert_eq!(outlier.level, 4, "the marathon day is the darkest");

        let ordinary: std::collections::BTreeSet<u8> = w
            .days
            .iter()
            .filter(|d| d.date != day(13))
            .map(|d| d.level)
            .collect();
        assert!(
            ordinary.len() >= 3,
            "ordinary days must spread across levels, got {ordinary:?}"
        );
        assert!(!ordinary.contains(&0), "an active day is never level 0");
    }

    #[test]
    fn idle_days_are_omitted_entirely() {
        let sessions = vec![(day(1), 1)];
        let w = build_activity_window(day(1), day(5), &sessions, &[], &[]);
        assert_eq!(w.days.len(), 1);
        assert_eq!(w.days[0].date, day(1));
    }

    /// Messages alone (an edited transcript) must not light a cell — only a
    /// session started or a token spent counts as activity.
    #[test]
    fn messages_alone_do_not_create_an_active_day() {
        let messages = vec![(day(2), 40)];
        let w = build_activity_window(day(1), day(3), &[], &[], &messages);
        assert!(w.days.is_empty());
    }

    #[test]
    fn tokens_lead_sessions_break_ties() {
        // Same session count, more tokens -> strictly higher score.
        assert!(activity_score(1, 200_000) > activity_score(1, 20_000));
        // Same tokens, more sessions -> strictly higher score.
        assert!(activity_score(3, 50_000) > activity_score(1, 50_000));
        // A deep single session outranks three trivial ones.
        assert!(activity_score(1, 500_000) > activity_score(3, 1_000));
        assert_eq!(activity_score(0, 0), 0.0);
    }

    #[test]
    fn streaks_count_consecutive_active_days() {
        // active: 1,2,3   idle: 4   active: 6,7  (5 idle)
        let sessions: Vec<(String, i64)> =
            [1u32, 2, 3, 6, 7].iter().map(|i| (day(*i), 1)).collect();
        let w = build_activity_window(day(1), day(7), &sessions, &[], &[]);
        assert_eq!(w.longest_streak, 3);
        assert_eq!(w.current_streak, 2, "6th and 7th");
    }

    /// A user who has not opened the app *yet today* has not broken their streak.
    #[test]
    fn current_streak_tolerates_an_inactive_today() {
        let sessions: Vec<(String, i64)> = [4u32, 5, 6].iter().map(|i| (day(*i), 1)).collect();
        let w = build_activity_window(day(1), day(7), &sessions, &[], &[]);
        assert_eq!(w.current_streak, 3);
    }

    #[test]
    fn max_sessions_and_tokens_reported() {
        let sessions = vec![(day(1), 2), (day(2), 5)];
        let tokens = vec![(day(1), 900, 400, 500, true), (day(2), 100, 60, 40, true)];
        let w = build_activity_window(day(1), day(2), &sessions, &tokens, &[]);
        assert_eq!(w.max_sessions, 5);
        assert_eq!(w.max_tokens, 900);
        let d1 = &w.days[0];
        assert_eq!((d1.input_tokens, d1.output_tokens), (400, 500));
    }

    #[test]
    fn token_completeness_is_preserved_per_day() {
        let sessions = vec![(day(1), 1), (day(2), 1)];
        let tokens = vec![(day(1), 0, 0, 0, false), (day(2), 500, 400, 100, true)];
        let w = build_activity_window(day(1), day(2), &sessions, &tokens, &[]);

        assert!(!w.tokens_complete);
        assert!(!w.days[0].tokens_complete);
        assert!(w.days[1].tokens_complete);
    }
}

/// #51: the preservation marker must survive every way a session's history is
/// rewritten, copied or moved. A pin that is honoured by compaction but erased
/// by a fork is worse than no pin at all, because callers would trust it.
///
/// `replace_conversation` DELETEs and re-INSERTs every row, and export/import,
/// copy and diverge all funnel through it — so these are one guarantee tested
/// six ways, not six independent guarantees.
#[cfg(test)]
mod pin_persistence_tests {
    use super::*;
    use crate::conversation::message::MessageMetadata;
    use crate::conversation::Conversation;
    use tempfile::TempDir;

    const PIN_TEXT: &str = "NOTE: always cite the 2019 cohort";

    /// A session holding: a plain turn, a PINNED note, another plain turn.
    async fn seeded(sm: &SessionManager) -> String {
        let session = sm
            .create_session(
                PathBuf::from("/tmp/pin"),
                "pin-persistence".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        for (i, message) in [
            Message::user().with_text("first"),
            Message::assistant().with_text("ok"),
            Message::user().with_text(PIN_TEXT).pinned(),
            Message::user().with_text("second"),
            Message::assistant().with_text("done"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut message = message;
            message.created = 1_700_000_000 + i as i64;
            sm.add_message(&session.id, &message).await.unwrap();
        }
        session.id
    }

    async fn pinned_texts(sm: &SessionManager, session_id: &str) -> Vec<String> {
        sm.get_session(session_id, true)
            .await
            .unwrap()
            .conversation
            .unwrap()
            .messages()
            .iter()
            .filter(|m| m.is_pinned())
            .map(|m| m.as_concat_text())
            .collect()
    }

    /// The baseline: `add_message` → `get_conversation` keeps the marker.
    #[tokio::test]
    async fn a_pin_survives_the_plain_store_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        assert_eq!(pinned_texts(&sm, &id).await, vec![PIN_TEXT.to_string()]);
    }

    /// The dangerous one: the whole-history DELETE + re-INSERT that compaction,
    /// message editing and every fork path run.
    #[tokio::test]
    async fn a_pin_survives_a_whole_history_rewrite() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        let current = sm
            .get_session(&id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        sm.replace_conversation(&id, &current).await.unwrap();

        assert_eq!(pinned_texts(&sm, &id).await, vec![PIN_TEXT.to_string()]);
    }

    /// The guarded rewrite from part (a) of #51 — including the FOREIGN-TAIL
    /// path, which decodes recovered rows itself rather than reusing the read
    /// path, and so could drop the marker independently.
    #[tokio::test]
    async fn a_pin_survives_the_guarded_rewrite_and_the_recovered_tail() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        let (session, basis) = sm.snapshot_for_rewrite(&id).await.unwrap();
        let known = session.conversation.clone().unwrap();

        // A concurrent writer appends a PINNED note after the snapshot: it is
        // foreign to the rewrite and must come back through `scan_foreign_tail`
        // with its marker intact.
        sm.add_message(
            &id,
            &Message::user()
                .with_text("NOTE: appended mid-rewrite")
                .pinned(),
        )
        .await
        .unwrap();

        // The rewrite drops the tail entirely; only the guard can save the note.
        let shrunk = Conversation::new_unvalidated(known.messages()[..3].to_vec());
        let (outcome, stored) = sm
            .replace_conversation_preserving_tail(&id, &shrunk, basis, &known)
            .await
            .unwrap();
        assert!(
            outcome.stored(),
            "the guarded rewrite must land: {outcome:?}"
        );

        let stored_pins: Vec<String> = stored
            .messages()
            .iter()
            .filter(|m| m.is_pinned())
            .map(|m| m.as_concat_text())
            .collect();
        assert_eq!(
            stored_pins,
            vec![
                PIN_TEXT.to_string(),
                "NOTE: appended mid-rewrite".to_string()
            ],
            "both the kept pin and the recovered foreign pin must stay marked"
        );
        assert_eq!(pinned_texts(&sm, &id).await, stored_pins);
    }

    #[tokio::test]
    async fn a_pin_survives_a_copy() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        let copy = sm.copy_session(&id, "Copy".to_string()).await.unwrap();
        assert_eq!(
            pinned_texts(&sm, &copy.id).await,
            vec![PIN_TEXT.to_string()]
        );
        // And the parent is untouched.
        assert_eq!(pinned_texts(&sm, &id).await, vec![PIN_TEXT.to_string()]);
    }

    #[tokio::test]
    async fn a_pin_survives_a_branch() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        let branch = sm.diverge_session(&id, None, None).await.unwrap();
        assert_eq!(
            pinned_texts(&sm, &branch.id).await,
            vec![PIN_TEXT.to_string()],
            "a branch must inherit the pin, or a note vanishes at the fork"
        );
    }

    /// The edit fork truncates at a timestamp. A pin BEFORE the cut is kept;
    /// this asserts the truncation path does not launder the marker off it.
    #[tokio::test]
    async fn a_pin_survives_a_fork_for_edit() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        // Cut just after the pinned note (created = 1_700_000_002).
        let fork = sm
            .diverge_session_for_edit(&id, 1_700_000_003)
            .await
            .unwrap();
        assert_eq!(
            pinned_texts(&sm, &fork.id).await,
            vec![PIN_TEXT.to_string()]
        );
    }

    #[tokio::test]
    async fn a_pin_survives_export_and_import() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        let exported = sm.export_session(&id).await.unwrap();
        assert!(
            exported.contains("\"pinned\": true"),
            "the marker must be in the exported document, not just in memory"
        );

        let imported = sm.import_session(&exported).await.unwrap();
        assert_eq!(
            pinned_texts(&sm, &imported.id).await,
            vec![PIN_TEXT.to_string()]
        );
    }

    /// An IMPORT of a document written before the marker existed must decode,
    /// with every message unpinned and its visibility intact. This is the
    /// regression the `#[serde(default)]` on `MessageMetadata::pinned` exists
    /// for; without it the whole import fails.
    #[tokio::test]
    async fn a_legacy_export_without_the_marker_still_imports() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = seeded(&sm).await;

        let exported = sm.export_session(&id).await.unwrap();
        let legacy = exported.replace(",\n        \"pinned\": true", "");
        let legacy = legacy.replace(",\n        \"pinned\": false", "");
        assert!(!legacy.contains("\"pinned\""), "stripped the marker");

        let imported = sm.import_session(&legacy).await.unwrap();
        let conversation = sm
            .get_session(&imported.id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(conversation.messages().len(), 5);
        assert!(conversation.messages().iter().all(|m| !m.is_pinned()));
        assert!(conversation.messages().iter().all(|m| m.is_agent_visible()));
    }

    /// #41's idempotent-replay probe compares the STORED metadata json with the
    /// candidate's. A pin difference is a real difference: re-adding the same
    /// uid with the marker flipped must not be swallowed as a replay.
    #[tokio::test]
    async fn the_replay_probe_treats_a_pin_change_as_a_difference() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let session = sm
            .create_session(
                PathBuf::from("/tmp/pin"),
                "replay".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let mut plain = Message::user().with_text("note").with_id("fixed-uid");
        plain.created = 1_700_000_000;
        sm.add_message(&session.id, &plain).await.unwrap();

        // Identical apart from the marker: a distinct message, so it gets its
        // own row under a re-minted uid rather than being dropped as a replay.
        let pinned = plain
            .clone()
            .with_metadata(MessageMetadata::default().with_pinned());
        let uid = sm.add_message(&session.id, &pinned).await.unwrap();
        assert_ne!(uid, "fixed-uid", "a pin change must re-mint, not collapse");

        let conversation = sm
            .get_session(&session.id, true)
            .await
            .unwrap()
            .conversation
            .unwrap();
        assert_eq!(conversation.messages().len(), 2);
        assert_eq!(
            conversation
                .messages()
                .iter()
                .filter(|m| m.is_pinned())
                .count(),
            1
        );
    }
}
