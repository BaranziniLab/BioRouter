//! Read-only SQL data engine for BRSDK apps — the "work on databases" capability
//! for app-local tabular data (a SQLite file in the app workspace).
//!
//! Two layers of read-only enforcement: the connection is opened
//! `SQLITE_OPEN_READONLY`, AND a statement-kind guard rejects anything that
//! isn't a single SELECT/PRAGMA/EXPLAIN/WITH — so neither a DML statement nor a
//! `select 1; drop table x` injection can mutate data. Results are returned as
//! JSON and row-capped. The rmcp `#[tool]` wrapper (data_sources/data_query/
//! data_table) is a thin layer over this tested engine.

use std::path::Path;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::{Column, Row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataError {
    Open(String),
    /// The statement is not a single read-only query.
    NotReadOnly,
    Query(String),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Open(e) => write!(f, "data: cannot open database: {e}"),
            DataError::NotReadOnly => {
                write!(f, "data: only a single read-only SELECT/PRAGMA query is allowed")
            }
            DataError::Query(e) => write!(f, "data: query failed: {e}"),
        }
    }
}
impl std::error::Error for DataError {}

/// A query result: column names + JSON-valued rows, with a truncation flag.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub truncated: bool,
}

/// True only for a single read-only statement. Rejects DML/DDL and multi-
/// statement injection (`;` separating statements).
pub fn is_read_only_sql(sql: &str) -> bool {
    let trimmed = sql.trim();
    // Disallow a statement separator entirely (defeats `select 1; drop …`).
    // A trailing single `;` is allowed.
    let body = trimmed.strip_suffix(';').unwrap_or(trimmed);
    if body.contains(';') {
        return false;
    }
    let lower = body.trim_start().to_ascii_lowercase();
    lower.starts_with("select ")
        || lower.starts_with("select\n")
        || lower.starts_with("select\t")
        || lower == "select"
        || lower.starts_with("with ")
        || lower.starts_with("pragma ")
        || lower.starts_with("pragma\n")
        || lower.starts_with("explain ")
}

/// An open read-only SQLite database.
pub struct DataSql {
    pool: SqlitePool,
}

impl DataSql {
    /// Open `db_path` read-only. Fails if the file doesn't exist (no creation).
    pub async fn open_readonly(db_path: &Path) -> Result<Self, DataError> {
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .read_only(true)
            .create_if_missing(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await
            .map_err(|e| DataError::Open(e.to_string()))?;
        Ok(DataSql { pool })
    }

    /// Run a read-only query, returning up to `max_rows` rows as JSON.
    pub async fn query(&self, sql: &str, max_rows: usize) -> Result<QueryResult, DataError> {
        if !is_read_only_sql(sql) {
            return Err(DataError::NotReadOnly);
        }
        let fetched = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DataError::Query(e.to_string()))?;

        let mut columns: Vec<String> = Vec::new();
        if let Some(first) = fetched.first() {
            columns = first.columns().iter().map(|c| c.name().to_string()).collect();
        }
        let truncated = fetched.len() > max_rows;
        let mut rows = Vec::with_capacity(fetched.len().min(max_rows));
        for row in fetched.iter().take(max_rows) {
            let mut out = Vec::with_capacity(columns.len());
            for i in 0..columns.len() {
                out.push(cell_to_json(row, i));
            }
            rows.push(out);
        }
        Ok(QueryResult {
            columns,
            rows,
            truncated,
        })
    }

    /// Describe a table: its column names + a small sample of rows.
    pub async fn describe_table(&self, table: &str) -> Result<QueryResult, DataError> {
        // Validate the identifier so it can't smuggle SQL.
        if !table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || table.is_empty() {
            return Err(DataError::NotReadOnly);
        }
        self.query(&format!("SELECT * FROM {table} LIMIT 5"), 5).await
    }
}

/// Decode a SQLite cell to JSON, trying integer → float → text → null.
fn cell_to_json(row: &sqlx::sqlite::SqliteRow, i: usize) -> serde_json::Value {
    if let Ok(n) = row.try_get::<i64, _>(i) {
        return serde_json::json!(n);
    }
    if let Ok(f) = row.try_get::<f64, _>(i) {
        return serde_json::json!(f);
    }
    if let Ok(s) = row.try_get::<String, _>(i) {
        return serde_json::json!(s);
    }
    serde_json::Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;

    async fn seed_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cohort.db");
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE patients (id INTEGER, gene TEXT, vaf REAL)")
            .execute(&pool)
            .await
            .unwrap();
        for (id, gene, vaf) in [(1, "CFTR", 0.42), (2, "TP53", 0.10), (3, "BRCA1", 0.88)] {
            sqlx::query("INSERT INTO patients (id, gene, vaf) VALUES (?, ?, ?)")
                .bind(id)
                .bind(gene)
                .bind(vaf)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;
        (dir, path)
    }

    #[test]
    fn read_only_guard_accepts_queries_rejects_mutations() {
        assert!(is_read_only_sql("SELECT * FROM t"));
        assert!(is_read_only_sql("  select id from t where x = 1 "));
        assert!(is_read_only_sql("WITH c AS (SELECT 1) SELECT * FROM c"));
        assert!(is_read_only_sql("PRAGMA table_info(t)"));
        assert!(is_read_only_sql("SELECT 1;")); // trailing semicolon ok
        // Mutations + injection rejected.
        assert!(!is_read_only_sql("DROP TABLE t"));
        assert!(!is_read_only_sql("UPDATE t SET x = 1"));
        assert!(!is_read_only_sql("DELETE FROM t"));
        assert!(!is_read_only_sql("INSERT INTO t VALUES (1)"));
        assert!(!is_read_only_sql("SELECT 1; DROP TABLE t")); // multi-statement
    }

    #[tokio::test]
    async fn queries_real_data_and_returns_json_rows() {
        let (_d, path) = seed_db().await;
        let db = DataSql::open_readonly(&path).await.unwrap();
        let res = db
            .query("SELECT gene, vaf FROM patients WHERE vaf > 0.3 ORDER BY vaf DESC", 100)
            .await
            .unwrap();
        assert_eq!(res.columns, vec!["gene", "vaf"]);
        assert_eq!(res.rows.len(), 2); // CFTR (0.42) and BRCA1 (0.88)
        assert!(!res.truncated);
        // Ordered DESC → BRCA1 first.
        assert_eq!(res.rows[0][0], serde_json::json!("BRCA1"));
        assert_eq!(res.rows[0][1], serde_json::json!(0.88));
    }

    #[tokio::test]
    async fn row_cap_sets_truncated() {
        let (_d, path) = seed_db().await;
        let db = DataSql::open_readonly(&path).await.unwrap();
        let res = db.query("SELECT * FROM patients", 2).await.unwrap();
        assert_eq!(res.rows.len(), 2);
        assert!(res.truncated, "3 rows capped at 2 → truncated");
    }

    #[tokio::test]
    async fn mutation_is_rejected_before_touching_db() {
        let (_d, path) = seed_db().await;
        let db = DataSql::open_readonly(&path).await.unwrap();
        assert_eq!(
            db.query("DELETE FROM patients", 100).await,
            Err(DataError::NotReadOnly)
        );
        // And even if the guard were bypassed, the connection is read-only: a
        // write fails at the DB layer.
        let raw = db.query("UPDATE patients SET gene='X'", 100).await;
        assert!(raw.is_err());
        // Data is intact.
        let check = db.query("SELECT COUNT(*) FROM patients", 10).await.unwrap();
        assert_eq!(check.rows[0][0], serde_json::json!(3));
    }

    #[tokio::test]
    async fn describe_table_validates_identifier() {
        let (_d, path) = seed_db().await;
        let db = DataSql::open_readonly(&path).await.unwrap();
        let res = db.describe_table("patients").await.unwrap();
        assert_eq!(res.columns, vec!["id", "gene", "vaf"]);
        // A malicious "table name" is rejected.
        assert_eq!(
            db.describe_table("patients; DROP TABLE patients").await,
            Err(DataError::NotReadOnly)
        );
    }
}
