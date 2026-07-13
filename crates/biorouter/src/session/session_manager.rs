use crate::config::paths::Paths;
use crate::conversation::message::{new_message_id, Message, MessageContent, MessageMetadata};
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::providers::base::{Provider, MSG_COUNT_FOR_SESSION_NAME_GENERATION};
use crate::session::chat_fts;
use crate::session::extension_data::ExtensionData;
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
use tracing::{info, warn};
use utoipa::ToSchema;

pub const CURRENT_SCHEMA_VERSION: i32 = 13;
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
    /// Lifetime totals: the sum of every turn's usage, i.e. tokens actually
    /// processed and billed. They grow without bound and overflowed `i32` at
    /// ~2.1e9 — in release that wraps *negative* and then subtracts from the
    /// insights `SUM`. SQLite's INTEGER is already 64-bit.
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
    /// branched at — the fork point (BR-45). Paired with `diverged_from`
    /// (parent session), it is the edge label of the branch forest. `None` for
    /// normally-created sessions. Anchoring on this stable id instead of a
    /// whole-second timestamp is what fixes the same-second over-truncation.
    #[serde(default)]
    pub branch_point_msg_uid: Option<String>,
}

/// One turn's token usage, applied additively and atomically in SQL.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenDelta {
    pub input: Option<i32>,
    pub output: Option<i32>,
    pub total: Option<i32>,
}

impl TokenDelta {
    fn is_empty(self) -> bool {
        self.input.is_none() && self.output.is_none() && self.total.is_none()
    }
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
    pub total_tokens: i64,
    pub sessions_last_7_days: usize,
    pub sessions_last_30_days: usize,
    pub tokens_last_7_days: i64,
    pub tokens_last_30_days: i64,
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
    pub current_streak: i64,
    pub longest_streak: i64,
    /// Only days with activity. The client fills the rest of the grid with level 0.
    pub days: Vec<DailyActivity>,
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
    token_rows: &[(String, i64, i64, i64)],
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
    }

    let mut by_day: BTreeMap<String, Day> = BTreeMap::new();

    for (day, n) in session_rows {
        by_day.entry(day.clone()).or_default().sessions += n;
    }
    for (day, tokens, input, output) in token_rows {
        let d = by_day.entry(day.clone()).or_default();
        d.tokens += tokens;
        d.input += input;
        d.output += output;
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

    pub fn working_dir(mut self, working_dir: PathBuf) -> Self {
        self.working_dir = Some(working_dir);
        self
    }

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
    /// session was branched at (BR-45 fork point).
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

    async fn apply_update_inner(&self, builder: SessionUpdateBuilder<'_>) -> Result<()> {
        self.storage.apply_update(builder).await
    }

    pub async fn add_message(&self, id: &str, message: &Message) -> Result<()> {
        self.storage.add_message(id, message).await
    }

    pub async fn replace_conversation(&self, id: &str, conversation: &Conversation) -> Result<()> {
        self.storage.replace_conversation(id, conversation).await
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.storage.list_sessions().await
    }

    pub async fn list_sessions_by_types(&self, types: &[SessionType]) -> Result<Vec<Session>> {
        self.storage.list_sessions_by_types(types).await
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        self.storage.delete_session(id).await
    }

    pub async fn get_insights(&self) -> Result<SessionInsights> {
        self.storage.get_insights().await
    }

    /// Per-day usage for the Home heatmap, over the last `days` calendar days.
    pub async fn get_activity(&self, days: i64) -> Result<ActivityWindow> {
        self.storage.get_activity(days).await
    }

    /// Append one turn's usage to the per-turn token ledger.
    pub async fn record_token_event(
        &self,
        session_id: &str,
        input: Option<i32>,
        output: Option<i32>,
        total: i32,
    ) -> Result<()> {
        self.storage
            .record_token_event(session_id, input, output, total)
            .await
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

    /// Diverge anchored by a durable message id (`anchor_uid`), the BR-45 fork
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

    pub async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.storage
            .truncate_conversation(session_id, timestamp)
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
                branch_point_msg_uid TEXT
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

        // Append-only per-turn token accounting. See migration 10 for why this is
        // a side table rather than `messages.tokens`.
        sqlx::query(
            r#"
            CREATE TABLE token_events (
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
        sqlx::query("CREATE INDEX idx_token_events_session ON token_events(session_id, ts)")
            .execute(pool)
            .await?;

        // BR-43 shadow-git checkpoints (migration 11), created inline for fresh DBs.
        Self::create_checkpoints_table(pool).await?;

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

        // FTS5 index for relevance-ranked chat recall (BR-17). See migration 13
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
            provider_name, model_config_json, diverged_from
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            Self::replace_conversation_inner(pool, &session.id, conversation).await?;
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
                // BR-43 shadow-git checkpoints. Additive side table keyed by the
                // turn's anchor `created_timestamp` (NOT the positional message
                // id) so checkpoints survive the future stable-UUID migration.
                Self::create_checkpoints_table(pool).await?;
            }
            12 => {
                // BR-45: stable, durable per-message id (`msg_uid`) that survives
                // history rewrites, plus a branch fork-point anchored on that id.
                sqlx::query("ALTER TABLE messages ADD COLUMN msg_uid TEXT")
                    .execute(pool)
                    .await?;
                // Deterministic backfill from the durable rowid. Stable
                // thereafter because every rewrite (compaction/edit/diverge)
                // preserves the carried `msg_uid`.
                sqlx::query("UPDATE messages SET msg_uid = 'm' || id WHERE msg_uid IS NULL")
                    .execute(pool)
                    .await?;
                // Unique per-session (ids are intentionally carried into a
                // diverged child, so uniqueness is not global).
                sqlx::query(
                    "CREATE UNIQUE INDEX idx_messages_uid ON messages(session_id, msg_uid)",
                )
                .execute(pool)
                .await?;
                // The exact parent message a branch was cut at (replaces the
                // fuzzy whole-second timestamp).
                sqlx::query("ALTER TABLE sessions ADD COLUMN branch_point_msg_uid TEXT")
                    .execute(pool)
                    .await?;
            }
            13 => {
                // FTS5 full-text index over user-visible message text for
                // relevance-ranked (bm25) chat recall, replacing the old
                // substring `LIKE` scan (BR-17). Additive and idempotent; an
                // older binary opening a v13 DB simply ignores messages_fts and
                // recall degrades to the `LIKE` fallback.
                sqlx::query(MESSAGES_FTS_DDL).execute(pool).await?;

                // One-time backfill from existing messages. O(n) once, in the
                // spirit of migration 10's token_events backfill.
                let rows = sqlx::query_as::<_, (i64, String, String, Option<String>)>(
                    "SELECT id, session_id, content_json, metadata_json FROM messages",
                )
                .fetch_all(pool)
                .await?;

                for (id, session_id, content_json, metadata_json) in rows {
                    if !message_is_user_visible(metadata_json.as_deref()) {
                        continue;
                    }
                    let Ok(content) = serde_json::from_str::<Vec<MessageContent>>(&content_json)
                    else {
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
            }
            _ => {
                anyhow::bail!("Unknown migration version: {}", version);
            }
        }

        Ok(())
    }

    /// The BR-43 `checkpoints` side table (migration 11 + fresh-DB schema).
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
                INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data)
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
                    '{}'
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
                q = q.bind(i64::from(value));
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
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>)>(
            "SELECT role, content_json, created_timestamp, metadata_json, msg_uid FROM messages WHERE session_id = ? ORDER BY timestamp",
        )
            .bind(session_id)
            .fetch_all(pool)
            .await?;

        let mut messages = Vec::new();
        for (idx, (role_str, content_json, created_timestamp, metadata_json, msg_uid)) in
            rows.into_iter().enumerate()
        {
            let role = match role_str.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => continue,
            };

            let content = serde_json::from_str(&content_json)?;
            let metadata = metadata_json
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            let mut message = Message::new(role, created_timestamp, content);
            message.metadata = metadata;
            // Dual-read: prefer the durable `msg_uid`; fall back to the legacy
            // positional id only for a row an in-flight upgrade hasn't
            // backfilled yet (migration 12 backfills all existing rows).
            let id = msg_uid.unwrap_or_else(|| format!("msg_{}_{}", session_id, idx));
            message = message.with_id(id);
            messages.push(message);
        }

        Ok(Conversation::new_unvalidated(messages))
    }

    async fn add_message(&self, session_id: &str, message: &Message) -> Result<()> {
        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;

        let metadata_json = serde_json::to_string(&message.metadata)?;
        // Persist the message's stable id, minting a fresh UUIDv7 when the
        // caller didn't supply one (BR-45).
        let msg_uid = message.id.clone().unwrap_or_else(new_message_id);

        let insert = sqlx::query(
            r#"
            INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json, msg_uid)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(session_id)
        .bind(role_to_string(&message.role))
        .bind(serde_json::to_string(&message.content)?)
        .bind(message.created)
        .bind(metadata_json)
        .bind(msg_uid)
        .execute(&mut *tx)
        .await?;

        // Keep the FTS recall index in sync with the new row (BR-17).
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

    /// True when the FTS5 mirror table exists (created by schema migration 11).
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
    ) -> Result<()> {
        let mut tx = pool.begin().await?;

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

        for message in conversation.messages() {
            let metadata_json = serde_json::to_string(&message.metadata)?;
            // PRESERVE each kept message's stable id across the rewrite (this is
            // the exact op — DELETE + re-INSERT — that used to renumber ids).
            // Only a newly-minted message (e.g. a compaction summary) with no id
            // gets a fresh one (BR-45).
            let msg_uid = message.id.clone().unwrap_or_else(new_message_id);

            let insert = sqlx::query(
                r#"
            INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json, msg_uid)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
            )
            .bind(session_id)
            .bind(role_to_string(&message.role))
            .bind(serde_json::to_string(&message.content)?)
            .bind(message.created)
            .bind(metadata_json)
            .bind(msg_uid)
            .execute(&mut *tx)
            .await?;

            Self::index_message_fts(
                &mut tx,
                session_id,
                insert.last_insert_rowid(),
                message,
                fts_available,
            )
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_conversation(
        &self,
        session_id: &str,
        conversation: &Conversation,
    ) -> Result<()> {
        let pool = self.pool().await?;
        Self::replace_conversation_inner(pool, session_id, conversation).await
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

        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_insights(&self) -> Result<SessionInsights> {
        let pool = self.pool().await?;

        // Sessions: totals plus 7d/30d windows.
        //
        // The session windows key on `updated_at` deliberately — an active
        // session counts as recent even if it was started earlier. Only
        // user-facing session types are counted, so these tiles agree with the
        // session list rendered beneath them.
        let sessions = sqlx::query_as::<_, (i64, Option<i64>, i64, i64)>(
            r#"
            SELECT
              COUNT(*) AS total_sessions,
              COALESCE(SUM(COALESCE(accumulated_total_tokens, total_tokens, 0)), 0) AS total_tokens,
              SUM(CASE WHEN updated_at >= datetime('now', '-7 days') THEN 1 ELSE 0 END) AS sessions_7d,
              SUM(CASE WHEN updated_at >= datetime('now', '-30 days') THEN 1 ELSE 0 END) AS sessions_30d
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
        let tokens = sqlx::query_as::<_, (Option<i64>, Option<i64>)>(
            r#"
            SELECT
              COALESCE(SUM(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-7 days') AS INTEGER)
                THEN te.total_tokens ELSE 0 END), 0) AS tokens_7d,
              COALESCE(SUM(CASE WHEN te.ts >= CAST(strftime('%s', 'now', '-30 days') AS INTEGER)
                THEN te.total_tokens ELSE 0 END), 0) AS tokens_30d
            FROM token_events te
            JOIN sessions s ON s.id = te.session_id
            WHERE s.session_type IN ('user', 'scheduled')
            "#,
        )
        .fetch_one(pool)
        .await?;

        Ok(SessionInsights {
            total_sessions: sessions.0 as usize,
            total_tokens: sessions.1.unwrap_or(0),
            sessions_last_7_days: sessions.2.max(0) as usize,
            sessions_last_30_days: sessions.3.max(0) as usize,
            tokens_last_7_days: tokens.0.unwrap_or(0),
            tokens_last_30_days: tokens.1.unwrap_or(0),
        })
    }

    /// Record one turn's usage. Append-only; never updated, never deleted.
    async fn record_token_event(
        &self,
        session_id: &str,
        input: Option<i32>,
        output: Option<i32>,
        total: i32,
    ) -> Result<()> {
        let pool = self.pool().await?;
        sqlx::query(
            r#"
            INSERT INTO token_events (session_id, ts, input_tokens, output_tokens, total_tokens)
            VALUES (?, CAST(strftime('%s', 'now') AS INTEGER), ?, ?, ?)
            "#,
        )
        .bind(session_id)
        .bind(input.map(i64::from))
        .bind(output.map(i64::from))
        .bind(i64::from(total))
        .execute(pool)
        .await?;
        Ok(())
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

        let token_rows = sqlx::query_as::<_, (String, i64, i64, i64)>(
            r#"
            SELECT date(te.ts, 'unixepoch', 'localtime') AS day,
                   COALESCE(SUM(te.total_tokens), 0)  AS tokens,
                   COALESCE(SUM(te.input_tokens), 0)  AS input_tokens,
                   COALESCE(SUM(te.output_tokens), 0) AS output_tokens
            FROM token_events te
            JOIN sessions s ON s.id = te.session_id
            WHERE s.session_type IN ('user', 'scheduled')
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
        // fork at one of two same-second messages does not over-truncate.
        let branch_conversation = original
            .conversation
            .as_ref()
            .map(|c| trim_to_last_complete_answer_at(c, anchor_uid.as_deref(), anchor_ms))
            .unwrap_or_default();

        // Record the fork point: the explicit anchor id when supplied, else the
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
            // the branch marker) and record the lineage pointer + fork point.
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

    async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        let pool = self.pool().await?;
        sqlx::query("DELETE FROM messages WHERE session_id = ? AND created_timestamp >= ?")
            .bind(session_id)
            .bind(timestamp)
            .execute(pool)
            .await?;

        Ok(())
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
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent};
    use tempfile::TempDir;

    const NUM_CONCURRENT_SESSIONS: i32 = 10;

    #[tokio::test]
    async fn checkpoints_table_crud_roundtrip() {
        // A fresh DB (create_schema path) must carry the migration-11 `checkpoints`
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
        assert_eq!(insights.total_tokens, expected_tokens as i64);
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
    // `copy_session` is the engine behind both the edit-fork path and the
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

        let insights = sm.get_insights().await.unwrap();
        assert_eq!(
            insights.total_tokens, beyond_i32,
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
            sm.update(&s.id)
                .accumulated_total_tokens(Some(tokens))
                .apply()
                .await
                .unwrap();
        }

        let insights = sm.get_insights().await.unwrap();
        assert_eq!(insights.total_sessions, 2, "user + scheduled only");
        assert_eq!(insights.total_tokens, 1_500);
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
        assert_eq!(insights.total_tokens, 50_000);
        assert_eq!(
            insights.tokens_last_7_days, 0,
            "a lifetime total is not a 7-day total"
        );

        sm.record_token_event(&session.id, Some(80), Some(20), 100)
            .await
            .unwrap();
        let insights = sm.get_insights().await.unwrap();
        assert_eq!(insights.tokens_last_7_days, 100);
        assert_eq!(insights.tokens_last_30_days, 100);

        let activity = sm.get_activity(30).await.unwrap();
        assert_eq!(activity.days.len(), 1);
        assert_eq!(activity.days[0].tokens, 100);
        assert_eq!(activity.days[0].sessions, 1);
        assert!(activity.days[0].level >= 1);
        assert_eq!(activity.current_streak, 1);
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

    // ── BR-45: stable per-message ids + branch fork point ───────────────────

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
    /// durable-id anchor keeps only the strict prefix and records the fork point
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
        // The fork point is recorded on the child branch.
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

    /// Migration 12 backfills `msg_uid` deterministically from the durable
    /// rowid (`m` || id) and adds the branch fork-point column.
    #[tokio::test]
    async fn migration_12_backfills_msg_uid_from_rowid() {
        let temp_dir = TempDir::new().unwrap();
        let db = temp_dir.path().join("v11.db");
        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db)
                    .create_if_missing(true),
            )
            .await
            .unwrap();

        // A minimal pre-migration (v11) shape: no msg_uid, no branch column.
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

        SessionStorage::apply_migration(&pool, 12).await.unwrap();

        let uids: Vec<(i64, Option<String>)> =
            sqlx::query_as("SELECT id, msg_uid FROM messages ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(uids[0].1.as_deref(), Some("m1"));
        assert_eq!(uids[1].1.as_deref(), Some("m2"));

        // The branch fork-point column now exists and defaults to NULL.
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
    async fn migration_11_backfills_fts_index() {
        // Production upgrade: a pre-v11 DB with existing messages gets an FTS
        // index built by migration 11's backfill, so recall works on history
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

        // Opening the real manager migrates 8→11, building messages_fts.
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
            tokens.push((day(i), 20_000 + i64::from(i) * 11_000, 0, 0));
        }
        // ... and one 1.8M-token outlier.
        sessions.push((day(13), 6));
        tokens.push((day(13), 1_800_000, 0, 0));

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
        let tokens = vec![(day(1), 900, 400, 500), (day(2), 100, 60, 40)];
        let w = build_activity_window(day(1), day(2), &sessions, &tokens, &[]);
        assert_eq!(w.max_sessions, 5);
        assert_eq!(w.max_tokens, 900);
        let d1 = &w.days[0];
        assert_eq!((d1.input_tokens, d1.output_tokens), (400, 500));
    }
}
