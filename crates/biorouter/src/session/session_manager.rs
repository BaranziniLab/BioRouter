use crate::config::paths::Paths;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::providers::base::{Provider, MSG_COUNT_FOR_SESSION_NAME_GENERATION};
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

pub const CURRENT_SCHEMA_VERSION: i32 = 9;
pub const SESSIONS_FOLDER: &str = "sessions";
pub const DB_NAME: &str = "sessions.db";

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
    pub total_tokens: Option<i32>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub accumulated_total_tokens: Option<i32>,
    pub accumulated_input_tokens: Option<i32>,
    pub accumulated_output_tokens: Option<i32>,
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
    accumulated_total_tokens: Option<Option<i32>>,
    accumulated_input_tokens: Option<Option<i32>>,
    accumulated_output_tokens: Option<Option<i32>>,
    schedule_id: Option<Option<String>>,
    workflow: Option<Option<Workflow>>,
    user_workflow_values: Option<Option<HashMap<String, String>>>,
    provider_name: Option<Option<String>>,
    model_config: Option<Option<ModelConfig>>,
    diverged_from: Option<Option<String>>,
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
            schedule_id: None,
            workflow: None,
            user_workflow_values: None,
            provider_name: None,
            model_config: None,
            diverged_from: None,
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

    pub fn accumulated_total_tokens(mut self, tokens: Option<i32>) -> Self {
        self.accumulated_total_tokens = Some(tokens);
        self
    }

    pub fn accumulated_input_tokens(mut self, tokens: Option<i32>) -> Self {
        self.accumulated_input_tokens = Some(tokens);
        self
    }

    pub fn accumulated_output_tokens(mut self, tokens: Option<i32>) -> Self {
        self.accumulated_output_tokens = Some(tokens);
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
}

/// The six token counters stored on a session row. Fetched cheaply on the
/// streaming hot path without the surrounding metadata or message count.
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct SessionTokenCounts {
    pub total_tokens: Option<i32>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub accumulated_total_tokens: Option<i32>,
    pub accumulated_input_tokens: Option<i32>,
    pub accumulated_output_tokens: Option<i32>,
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
    /// assistant answer (see `trim_to_last_complete_answer`), so a diverge
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
        self.storage
            .diverge_session(self, session_id, custom_name, anchor_ms)
            .await
    }

    pub async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.storage
            .truncate_conversation(session_id, timestamp)
            .await
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
        let inner = &trimmed[idx + " (branch ".len()..];
        if let Some(digits) = inner.strip_suffix(')') {
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                return trimmed[..idx].trim_end();
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
pub(crate) fn trim_to_last_complete_answer(
    conversation: &Conversation,
    anchor_ms: Option<i64>,
) -> Conversation {
    let kept: Vec<&Message> = conversation
        .messages()
        .iter()
        .filter(|m| anchor_ms.is_none_or(|ts| m.created <= ts))
        .collect();

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
                external_key TEXT
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
                metadata_json TEXT
            )
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX idx_messages_session ON messages(session_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_messages_timestamp ON messages(timestamp)")
            .execute(pool)
            .await?;
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
            _ => {
                anyhow::bail!("Unknown migration version: {}", version);
            }
        }

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
        if let Some(id) = sqlx::query_scalar::<_, String>(
            "SELECT id FROM sessions WHERE external_key = ?",
        )
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
               provider_name, model_config_json, diverged_from
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
        add_update!(builder.schedule_id, "schedule_id");
        add_update!(builder.workflow, "workflow_json");
        add_update!(builder.user_workflow_values, "user_workflow_values_json");
        add_update!(builder.provider_name, "provider_name");
        add_update!(builder.model_config, "model_config_json");
        add_update!(builder.diverged_from, "diverged_from");

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

        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;
        q = q.bind(&builder.session_id);
        q.execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_conversation(&self, session_id: &str) -> Result<Conversation> {
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, (String, String, i64, Option<String>)>(
            "SELECT role, content_json, created_timestamp, metadata_json FROM messages WHERE session_id = ? ORDER BY timestamp",
        )
            .bind(session_id)
            .fetch_all(pool)
            .await?;

        let mut messages = Vec::new();
        for (idx, (role_str, content_json, created_timestamp, metadata_json)) in
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
            message = message.with_id(format!("msg_{}_{}", session_id, idx));
            messages.push(message);
        }

        Ok(Conversation::new_unvalidated(messages))
    }

    async fn add_message(&self, session_id: &str, message: &Message) -> Result<()> {
        let pool = self.pool().await?;
        let mut tx = pool.begin().await?;

        let metadata_json = serde_json::to_string(&message.metadata)?;

        sqlx::query(
            r#"
            INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json)
            VALUES (?, ?, ?, ?, ?)
        "#,
        )
        .bind(session_id)
        .bind(role_to_string(&message.role))
        .bind(serde_json::to_string(&message.content)?)
        .bind(message.created)
        .bind(metadata_json)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE sessions SET updated_at = datetime('now') WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
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

        for message in conversation.messages() {
            let metadata_json = serde_json::to_string(&message.metadata)?;

            sqlx::query(
                r#"
            INSERT INTO messages (session_id, role, content_json, created_timestamp, metadata_json)
            VALUES (?, ?, ?, ?, ?)
        "#,
            )
            .bind(session_id)
            .bind(role_to_string(&message.role))
            .bind(serde_json::to_string(&message.content)?)
            .bind(message.created)
            .bind(metadata_json)
            .execute(&mut *tx)
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
        // Single aggregate over sessions: totals plus 7d/30d windows.
        // Window uses updated_at (already indexed) so an active session
        // counts toward the recent window even if it was started earlier.
        let row = sqlx::query_as::<_, (i64, Option<i64>, i64, i64, Option<i64>, Option<i64>)>(
            r#"
            SELECT
              COUNT(*) AS total_sessions,
              COALESCE(SUM(COALESCE(accumulated_total_tokens, total_tokens, 0)), 0) AS total_tokens,
              SUM(CASE WHEN updated_at >= datetime('now', '-7 days') THEN 1 ELSE 0 END) AS sessions_7d,
              SUM(CASE WHEN updated_at >= datetime('now', '-30 days') THEN 1 ELSE 0 END) AS sessions_30d,
              COALESCE(SUM(CASE WHEN updated_at >= datetime('now', '-7 days')
                THEN COALESCE(accumulated_total_tokens, total_tokens, 0) ELSE 0 END), 0) AS tokens_7d,
              COALESCE(SUM(CASE WHEN updated_at >= datetime('now', '-30 days')
                THEN COALESCE(accumulated_total_tokens, total_tokens, 0) ELSE 0 END), 0) AS tokens_30d
            FROM sessions
            "#,
        )
            .fetch_one(pool)
            .await?;

        Ok(SessionInsights {
            total_sessions: row.0 as usize,
            total_tokens: row.1.unwrap_or(0),
            sessions_last_7_days: row.2.max(0) as usize,
            sessions_last_30_days: row.3.max(0) as usize,
            tokens_last_7_days: row.4.unwrap_or(0),
            tokens_last_30_days: row.5.unwrap_or(0),
        })
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
        // carries over an unanswered question or a dangling tool call).
        let branch_conversation = original
            .conversation
            .as_ref()
            .map(|c| trim_to_last_complete_answer(c, anchor_ms))
            .unwrap_or_default();

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
            // the branch marker) and record the lineage pointer.
            .user_provided_name(new_name)
            .diverged_from(Some(session_id.to_string()))
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
        const ACCUMULATED_TOKENS: i32 = 1000;
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
        let convo = s2.conversation.expect("resumed session carries conversation");
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
        let db_path = temp_dir
            .path()
            .join(SESSIONS_FOLDER)
            .join(DB_NAME);
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
                    .bind(v).execute(&pool).await.unwrap();
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
            sm.get_session("20240101_1", true).await.unwrap().message_count,
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
            .create_session(PathBuf::from("/tmp"), "plain".to_string(), SessionType::User)
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
        let t = trim_to_last_complete_answer(&conv, None);
        assert_eq!(t.messages().len(), 4);
        assert_eq!(t.messages().last().unwrap().as_concat_text(), "a2");
    }

    #[test]
    fn test_trim_drops_trailing_unanswered_question() {
        // The reported bug: diverge fired while the agent was still generating
        // the answer to q2, so the DB has q2 persisted with no answer yet.
        let conv = Conversation::new_unvalidated(vec![umsg(1, "q1"), amsg(2, "a1"), umsg(3, "q2")]);
        let t = trim_to_last_complete_answer(&conv, None);
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
        let t = trim_to_last_complete_answer(&conv, None);
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
        let t = trim_to_last_complete_answer(&conv, Some(20));
        assert_eq!(t.messages().len(), 2);
        assert_eq!(t.messages().last().unwrap().as_concat_text(), "a1");
    }

    #[test]
    fn test_trim_empty_when_no_complete_answer() {
        // Diverged before the first reply landed: only an unanswered question.
        let conv = Conversation::new_unvalidated(vec![umsg(1, "q1")]);
        let t = trim_to_last_complete_answer(&conv, None);
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
}
