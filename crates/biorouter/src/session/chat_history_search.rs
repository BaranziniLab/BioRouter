use crate::conversation::message::MessageContent;
use crate::session::chat_fts;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct ChatRecallResult {
    pub session_id: String,
    pub session_description: String,
    pub session_working_dir: String,
    pub last_activity: DateTime<Utc>,
    pub total_messages_in_session: usize,
    pub messages: Vec<ChatRecallMessage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRecallMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ChatRecallResults {
    pub results: Vec<ChatRecallResult>,
    pub total_matches: usize,
}

type SqlQueryRow = (
    String,
    String,
    String,
    DateTime<Utc>,
    String,
    String,
    DateTime<Utc>,
);

type SessionMessageGroup = (
    String,
    String,
    DateTime<Utc>,
    Vec<(String, String, DateTime<Utc>)>,
);

pub struct ChatHistorySearch<'a> {
    pool: &'a Pool<Sqlite>,
    query: &'a str,
    limit: usize,
    after_date: Option<DateTime<Utc>>,
    before_date: Option<DateTime<Utc>>,
    exclude_session_id: Option<String>,
}

impl<'a> ChatHistorySearch<'a> {
    pub fn new(
        pool: &'a Pool<Sqlite>,
        query: &'a str,
        limit: Option<usize>,
        after_date: Option<DateTime<Utc>>,
        before_date: Option<DateTime<Utc>>,
        exclude_session_id: Option<String>,
    ) -> Self {
        Self {
            pool,
            query,
            limit: limit.unwrap_or(10),
            after_date,
            before_date,
            exclude_session_id,
        }
    }

    pub async fn execute(self) -> Result<ChatRecallResults> {
        let empty = || ChatRecallResults {
            results: vec![],
            total_matches: 0,
        };

        // Prefer the FTS5 index (relevance-ranked via bm25) when it exists;
        // fall back to the legacy substring `LIKE` scan for an un-migrated DB
        // so recall never errors, it only degrades (BR-17).
        let rows = if self.fts_available().await {
            let match_expr = chat_fts::sanitize_fts_query(self.query);
            if match_expr.is_empty() {
                return Ok(empty());
            }
            self.fetch_rows_fts(&match_expr).await?
        } else {
            let keywords = self.parse_keywords();
            if keywords.is_empty() {
                return Ok(empty());
            }
            self.fetch_rows_like(&keywords).await?
        };

        let (session_messages, order) = self.process_rows(rows);
        let session_totals = self.get_session_totals(&session_messages).await?;
        let results = self.convert_to_results(session_messages, order, session_totals);

        Ok(results)
    }

    /// True when the FTS5 mirror table exists (created by schema migration 11).
    async fn fts_available(&self) -> bool {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='messages_fts'",
        )
        .fetch_one(self.pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false)
    }

    /// Relevance-ranked recall over the FTS5 index. Rows come back best-first
    /// (`bm25` ascending) and, because the index only stores the flattened
    /// text, this joins back to `messages`/`sessions` so the downstream row
    /// shape and rendering stay identical to the `LIKE` path.
    async fn fetch_rows_fts(&self, match_expr: &str) -> Result<Vec<SqlQueryRow>> {
        let mut sql = String::from(
            r#"
            SELECT
                s.id as session_id,
                s.description as session_description,
                s.working_dir as session_working_dir,
                s.created_at as session_created_at,
                m.role,
                m.content_json,
                m.timestamp
            FROM messages_fts f
            INNER JOIN messages m ON m.id = f.message_id
            INNER JOIN sessions s ON m.session_id = s.id
            WHERE messages_fts MATCH ?
        "#,
        );

        if self.exclude_session_id.is_some() {
            sql.push_str(" AND s.id != ?");
        }
        if self.after_date.is_some() {
            sql.push_str(" AND m.timestamp >= ?");
        }
        if self.before_date.is_some() {
            sql.push_str(" AND m.timestamp <= ?");
        }

        sql.push_str(" ORDER BY bm25(messages_fts) ASC LIMIT ?");

        let mut query_builder = sqlx::query_as::<_, SqlQueryRow>(&sql).bind(match_expr);

        if let Some(exclude_id) = &self.exclude_session_id {
            query_builder = query_builder.bind(exclude_id);
        }
        if let Some(after) = self.after_date {
            query_builder = query_builder.bind(after);
        }
        if let Some(before) = self.before_date {
            query_builder = query_builder.bind(before);
        }
        query_builder = query_builder.bind(self.limit as i64);

        Ok(query_builder.fetch_all(self.pool).await?)
    }

    async fn fetch_rows_like(&self, keywords: &[String]) -> Result<Vec<SqlQueryRow>> {
        let sql = self.build_sql(keywords);
        let mut query_builder = sqlx::query_as::<_, SqlQueryRow>(&sql);

        for keyword in keywords {
            query_builder = query_builder.bind(keyword);
        }

        if let Some(exclude_id) = &self.exclude_session_id {
            query_builder = query_builder.bind(exclude_id);
        }

        if let Some(after) = self.after_date {
            query_builder = query_builder.bind(after);
        }
        if let Some(before) = self.before_date {
            query_builder = query_builder.bind(before);
        }

        query_builder = query_builder.bind(self.limit as i64);

        Ok(query_builder.fetch_all(self.pool).await?)
    }

    fn parse_keywords(&self) -> Vec<String> {
        self.query
            .split_whitespace()
            .map(|word| format!("%{}%", word.to_lowercase()))
            .collect()
    }

    fn build_sql(&self, keywords: &[String]) -> String {
        let mut sql = String::from(
            r#"
            SELECT 
                s.id as session_id,
                s.description as session_description,
                s.working_dir as session_working_dir,
                s.created_at as session_created_at,
                m.role,
                m.content_json,
                m.timestamp
            FROM messages m
            INNER JOIN sessions s ON m.session_id = s.id
            WHERE EXISTS (
                SELECT 1 FROM json_each(m.content_json) 
                WHERE json_extract(value, '$.type') = 'text' 
                AND (
        "#,
        );

        for (i, _) in keywords.iter().enumerate() {
            if i > 0 {
                sql.push_str(" OR ");
            }
            sql.push_str("LOWER(json_extract(value, '$.text')) LIKE ?");
        }

        sql.push_str(
            r#"
                )
            )
        "#,
        );

        if self.exclude_session_id.is_some() {
            sql.push_str(" AND s.id != ?");
        }

        if self.after_date.is_some() {
            sql.push_str(" AND m.timestamp >= ?");
        }
        if self.before_date.is_some() {
            sql.push_str(" AND m.timestamp <= ?");
        }

        sql.push_str(" ORDER BY m.timestamp DESC LIMIT ?");

        sql
    }

    /// Group matched rows by session, returning the sessions in the order they
    /// were first seen. Rows arrive ranked (bm25-best-first for FTS,
    /// most-recent-first for the `LIKE` fallback), so first-seen order carries
    /// the ranking down to the session level.
    fn process_rows(
        &self,
        rows: Vec<SqlQueryRow>,
    ) -> (HashMap<String, SessionMessageGroup>, Vec<String>) {
        let mut session_messages: HashMap<String, SessionMessageGroup> = HashMap::new();
        let mut order: Vec<String> = Vec::new();

        for (
            session_id,
            session_description,
            session_working_dir,
            session_created_at,
            role,
            content_json,
            timestamp,
        ) in rows
        {
            if let Ok(content_vec) = serde_json::from_str::<Vec<MessageContent>>(&content_json) {
                let text_parts = chat_fts::searchable_parts(&content_vec);

                if !text_parts.is_empty() {
                    if !session_messages.contains_key(&session_id) {
                        order.push(session_id.clone());
                    }
                    let entry = session_messages.entry(session_id.clone()).or_insert((
                        session_description.clone(),
                        session_working_dir.clone(),
                        session_created_at,
                        Vec::new(),
                    ));
                    entry
                        .3
                        .push((role.clone(), text_parts.join("\n"), timestamp));
                }
            }
        }

        (session_messages, order)
    }

    async fn get_session_totals(
        &self,
        session_messages: &HashMap<String, SessionMessageGroup>,
    ) -> Result<HashMap<String, usize>> {
        let mut session_totals: HashMap<String, usize> = HashMap::new();
        if session_messages.is_empty() {
            return Ok(session_totals);
        }

        // Single grouped query instead of one COUNT(*) round-trip per session
        // (the previous N+1 issued a separate query for every matched session).
        let ids: Vec<&String> = session_messages.keys().collect();
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT session_id, COUNT(*) FROM messages WHERE session_id IN ({placeholders}) GROUP BY session_id"
        );
        let mut query = sqlx::query_as::<_, (String, i64)>(&sql);
        for id in &ids {
            query = query.bind(*id);
        }
        let rows = query.fetch_all(self.pool).await.unwrap_or_default();
        for (session_id, count) in rows {
            session_totals.insert(session_id, count as usize);
        }
        Ok(session_totals)
    }

    fn convert_to_results(
        &self,
        mut session_messages: HashMap<String, SessionMessageGroup>,
        order: Vec<String>,
        session_totals: HashMap<String, usize>,
    ) -> ChatRecallResults {
        // Emit sessions in ranked order (best first) rather than re-sorting by
        // recency, so bm25 relevance is what the caller sees.
        let mut results: Vec<ChatRecallResult> = Vec::with_capacity(order.len());
        for session_id in order {
            let Some((description, working_dir, _created_at, messages)) =
                session_messages.remove(&session_id)
            else {
                continue;
            };

            let message_vec: Vec<ChatRecallMessage> = messages
                .into_iter()
                .map(|(role, content, timestamp)| ChatRecallMessage {
                    role,
                    content,
                    timestamp,
                })
                .collect();

            let last_activity = message_vec
                .iter()
                .map(|m| m.timestamp)
                .max()
                .unwrap_or_else(chrono::Utc::now);

            let total_messages_in_session = session_totals.get(&session_id).copied().unwrap_or(0);

            results.push(ChatRecallResult {
                session_id,
                session_description: description,
                session_working_dir: working_dir,
                last_activity,
                total_messages_in_session,
                messages: message_vec,
            });
        }

        let total_matches = results.iter().map(|r| r.messages.len()).sum();
        ChatRecallResults {
            results,
            total_matches,
        }
    }
}
