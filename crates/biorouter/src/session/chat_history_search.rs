use crate::conversation::message::MessageContent;
use crate::privacy::ProviderTier;
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
    /// Issue #56 Gate D. The reach this search runs with. `Public` filters
    /// private sessions out **in SQL**; `Private` is full reach.
    ///
    /// Deliberately a bare [`ProviderTier`] and not a `CallCapability`: this
    /// type sits in the session layer and is constructed from
    /// `crates/biorouter/tests/`, where `CallCapability`'s test constructor is
    /// not reachable. The one caller that holds a capability collapses it to a
    /// reach — see `chatrecall_extension.rs`'s SEARCH arm, which is also where
    /// DR-15's master opt-out is applied.
    caller_capability: ProviderTier,
}

impl<'a> ChatHistorySearch<'a> {
    pub fn new(
        pool: &'a Pool<Sqlite>,
        query: &'a str,
        limit: Option<usize>,
        after_date: Option<DateTime<Utc>>,
        before_date: Option<DateTime<Utc>>,
        exclude_session_id: Option<String>,
        caller_capability: ProviderTier,
    ) -> Self {
        Self {
            pool,
            query,
            limit: limit.unwrap_or(10),
            after_date,
            before_date,
            exclude_session_id,
            caller_capability,
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

        // Issue #56 Gate D. `sessions s` is already joined above, so this is one
        // clause. A SQL LITERAL, never a `?`: both builders bind strictly
        // positionally with the optional clauses in a fixed order, so an
        // inserted placeholder shifts every later ordinal and mis-binds
        // SILENTLY — no error, wrong results. The literal is a compile-time
        // constant of the code path, not user input.
        //
        // It sits BEFORE the `LIMIT ?` on purpose: SQLite applies the limit to
        // the filtered set, so a public caller gets its full quota of public
        // rows even when private rows would have outranked them. A Rust-side
        // post-filter after `execute()` returns is the same one-liner and is
        // wrong for exactly that reason.
        if self.caller_capability == ProviderTier::Public {
            sql.push_str(" AND s.privacy_tier = 'public'");
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

        // Issue #56 Gate D — the same clause on the `LIKE` fallback. `execute`
        // branches on a `sqlite_master` probe for `messages_fts`, so filtering
        // only the FTS builder leaks on every un-migrated profile.
        if self.caller_capability == ProviderTier::Public {
            sql.push_str(" AND s.privacy_tier = 'public'");
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

#[cfg(test)]
mod tests {
    //! Issue #56 Gate D (SEARCH).
    //!
    //! This file had no `mod tests` before this task, so the pre-count of the
    //! `session::chat_history_search` filter is zero — "0 passed, exits 0" is
    //! the baseline, and the only meaningful number is the count of tests added
    //! here.

    use super::*;
    use crate::privacy::ProviderTier;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    /// `execute` picks its builder by probing `sqlite_master` for
    /// `messages_fts`, so the only honest way to exercise the `LIKE` fallback
    /// is to seed a database that genuinely lacks the table.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum QueryPath {
        Fts,
        LikeFallback,
    }

    #[derive(Clone)]
    struct Chat {
        description: String,
        working_dir: String,
        private: bool,
        text: String,
    }

    fn chat(private: bool, text: &str) -> Chat {
        Chat {
            description: if private {
                "private chat".to_string()
            } else {
                "public chat".to_string()
            },
            working_dir: "/tmp/w".to_string(),
            private,
            text: text.to_string(),
        }
    }

    fn private_chat_containing(term: &str) -> Chat {
        chat(true, &format!("we discussed the {term} at length"))
    }

    fn public_chat_containing(term: &str) -> Chat {
        chat(false, &format!("we discussed the {term} at length"))
    }

    /// A short document with one hit: best bm25 (`ORDER BY bm25 ASC`). These are
    /// seeded first, and `seeded` gives earlier rows the newer timestamp, so the
    /// `LIKE` fallback's `ORDER BY m.timestamp DESC` reproduces the same ranking.
    fn private_chat_ranking_high(term: &str) -> Chat {
        chat(true, term)
    }

    /// The same term buried in a long document: worst bm25, and seeded last so
    /// it also has the oldest timestamp.
    fn public_chat_ranking_low(term: &str) -> Chat {
        let filler = "filler ".repeat(200);
        chat(false, &format!("{filler}{term} {filler}"))
    }

    fn private_chat_titled(description: &str, working_dir: &str, term: &str) -> Chat {
        Chat {
            description: description.to_string(),
            working_dir: working_dir.to_string(),
            private: true,
            text: format!("notes about the {term}"),
        }
    }

    fn vec_of(n: usize, c: Chat) -> Vec<Chat> {
        std::iter::repeat_n(c, n).collect()
    }

    /// The plan spelled this `vec_of(..).chain(vec_of(..))`; `Vec` is not an
    /// `Iterator`, so the concatenation is a free function.
    fn chain(a: Vec<Chat>, b: Vec<Chat>) -> Vec<Chat> {
        a.into_iter().chain(b).collect()
    }

    struct Db {
        _temp: tempfile::TempDir,
        pool: Pool<Sqlite>,
    }

    async fn seeded(path: QueryPath, chats: &[Chat]) -> Db {
        let _temp = tempfile::TempDir::new().unwrap();
        let opts = SqliteConnectOptions::new()
            .filename(_temp.path().join("recall.db"))
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();

        sqlx::query(
            "CREATE TABLE sessions (id TEXT PRIMARY KEY, description TEXT NOT NULL DEFAULT '', \
             working_dir TEXT NOT NULL DEFAULT '', created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, \
             privacy_tier TEXT NOT NULL DEFAULT 'public')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE messages (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, \
             role TEXT NOT NULL, content_json TEXT NOT NULL, created_timestamp INTEGER NOT NULL, \
             timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .unwrap();
        if path == QueryPath::Fts {
            sqlx::query(
                "CREATE VIRTUAL TABLE messages_fts USING fts5(text, session_id UNINDEXED, \
                 message_id UNINDEXED, tokenize = 'porter unicode61')",
            )
            .execute(&pool)
            .await
            .unwrap();
        }

        let base = chrono::Utc::now();
        for (i, c) in chats.iter().enumerate() {
            let sid = format!("s{i}");
            let ts = base - chrono::Duration::seconds(i as i64);
            sqlx::query(
                "INSERT INTO sessions (id, description, working_dir, privacy_tier) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&sid)
            .bind(&c.description)
            .bind(&c.working_dir)
            .bind(if c.private { "private" } else { "public" })
            .execute(&pool)
            .await
            .unwrap();

            let content_json =
                serde_json::to_string(&vec![MessageContent::text(c.text.clone())]).unwrap();
            let inserted = sqlx::query(
                "INSERT INTO messages (session_id, role, content_json, created_timestamp, timestamp) \
                 VALUES (?, 'user', ?, ?, ?)",
            )
            .bind(&sid)
            .bind(&content_json)
            .bind(i as i64)
            .bind(ts)
            .execute(&pool)
            .await
            .unwrap();

            if path == QueryPath::Fts {
                sqlx::query(
                    "INSERT INTO messages_fts (text, session_id, message_id) VALUES (?, ?, ?)",
                )
                .bind(&c.text)
                .bind(&sid)
                .bind(inserted.last_insert_rowid())
                .execute(&pool)
                .await
                .unwrap();
            }
        }

        Db { _temp, pool }
    }

    async fn search_with_limit(
        tier: ProviderTier,
        db: &Db,
        query: &str,
        limit: usize,
    ) -> ChatRecallResults {
        ChatHistorySearch::new(&db.pool, query, Some(limit), None, None, None, tier)
            .execute()
            .await
            .unwrap()
    }

    async fn search_as(tier: ProviderTier, db: &Db, query: &str) -> ChatRecallResults {
        ChatHistorySearch::new(&db.pool, query, None, None, None, None, tier)
            .execute()
            .await
            .unwrap()
    }

    /// Everything that reaches the model. Stricter than chatrecall's prose
    /// formatting: it covers every field of every result, not just the ones the
    /// current renderer happens to print.
    fn render_for_model(r: &ChatRecallResults) -> String {
        serde_json::to_string(r).unwrap()
    }

    #[tokio::test]
    async fn both_query_paths_filter_private_rows() {
        for path in [QueryPath::Fts, QueryPath::LikeFallback] {
            let db = seeded(
                path,
                &[
                    private_chat_containing("cohort"),
                    public_chat_containing("cohort"),
                ],
            )
            .await;
            assert_eq!(
                search_as(ProviderTier::Public, &db, "cohort")
                    .await
                    .results
                    .len(),
                1,
                "{path:?}"
            );
            assert_eq!(
                search_as(ProviderTier::Private, &db, "cohort")
                    .await
                    .results
                    .len(),
                2,
                "{path:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_limit_is_applied_after_the_filter_not_before() {
        // THE test. 10 private rows ranking above 3 public ones with limit=5: a
        // public caller must get all 3 public rows, not 0. A Rust-side post-filter
        // — the obvious implementation, and the one that needs no SQL change —
        // returns 0 here, silently and non-deterministically, with no error.
        // SQLite applies the `LIMIT ?` at the end of each builder.
        for path in [QueryPath::Fts, QueryPath::LikeFallback] {
            let db = seeded(
                path,
                &chain(
                    vec_of(10, private_chat_ranking_high("cohort")),
                    vec_of(3, public_chat_ranking_low("cohort")),
                ),
            )
            .await;
            let r = search_with_limit(ProviderTier::Public, &db, "cohort", 5).await;
            assert_eq!(
                r.results.len(),
                3,
                "post-filtered in Rust instead of in SQL ({path:?})"
            );
        }
    }

    #[tokio::test]
    async fn no_content_field_of_a_private_row_survives() {
        // §11.4: session_description is the LLM-generated title, produced FROM the
        // conversation, and is the field most likely to be mislabelled as metadata.
        let db = seeded(
            QueryPath::Fts,
            &[private_chat_titled(
                "PHI cohort characterisation",
                "/data/phi/x",
                "cohort",
            )],
        )
        .await;
        let r = search_as(ProviderTier::Public, &db, "cohort").await;
        let rendered = render_for_model(&r);
        for leak in [
            "PHI cohort characterisation",
            "/data/phi",
            "cohort characterisation",
        ] {
            assert!(!rendered.contains(leak), "{leak} survived: {rendered}");
        }
    }
}
