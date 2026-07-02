//! rmcp tool server over the read-only [`DataSql`] engine — the agent-facing
//! `data_sources` / `data_query` / `data_table` tools.
//!
//! Constructed PER APP with the workspace-resolved source paths (it can't be a
//! `BUILTIN_EXTENSIONS` entry, whose servers take no per-app context), then
//! injected into the app's extension manager. Each source maps a logical name to
//! a read-only SQLite file inside the app workspace.

use std::collections::HashMap;
use std::path::PathBuf;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, ServerCapabilities,
        ServerInfo,
    },
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

use super::DataSql;

const DEFAULT_MAX_ROWS: usize = 1000;

fn invalid(msg: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, msg.into(), None)
}

#[derive(Debug, Deserialize, Serialize, rmcp::schemars::JsonSchema)]
pub struct QueryParams {
    /// The data source name (one of those listed by `data_sources`).
    pub source: String,
    /// A single read-only SQL query (SELECT / WITH / PRAGMA / EXPLAIN).
    pub sql: String,
    /// Maximum rows to return (default + hard cap 1000).
    #[serde(default)]
    pub max_rows: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, rmcp::schemars::JsonSchema)]
pub struct TableParams {
    /// The data source name.
    pub source: String,
    /// The table to describe (columns + a few sample rows).
    pub table: String,
}

/// Read-only SQL tool server for an app's declared data sources.
#[derive(Clone)]
pub struct DataSqlServer {
    tool_router: ToolRouter<Self>,
    sources: HashMap<String, PathBuf>,
}

impl DataSqlServer {
    /// `sources`: logical name → read-only SQLite file path (already
    /// workspace-jail-resolved by the caller).
    pub fn new(sources: HashMap<String, PathBuf>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            sources,
        }
    }

    fn source_path(&self, source: &str) -> Result<PathBuf, ErrorData> {
        self.sources
            .get(source)
            .cloned()
            .ok_or_else(|| invalid(format!("unknown data source '{source}'")))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for DataSqlServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-datasql".to_string(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                title: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Read-only SQL over this app's data sources. Call data_sources to list them, \
                 data_query to run a single SELECT/WITH/PRAGMA, data_table to inspect a table."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

#[tool_router(router = tool_router)]
impl DataSqlServer {
    #[tool(
        name = "data_sources",
        description = "List the read-only data sources this app may query."
    )]
    pub async fn data_sources(&self) -> Result<CallToolResult, ErrorData> {
        let mut names: Vec<&String> = self.sources.keys().collect();
        names.sort();
        let json = serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string());
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "data_query",
        description = "Run a single read-only SQL query (SELECT/WITH/PRAGMA/EXPLAIN) against a named source. Returns {columns, rows, truncated} as JSON. Mutations and multi-statement input are rejected."
    )]
    pub async fn data_query(
        &self,
        params: Parameters<QueryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let path = self.source_path(&p.source)?;
        let db = DataSql::open_readonly(&path)
            .await
            .map_err(|e| invalid(e.to_string()))?;
        let max = p.max_rows.unwrap_or(DEFAULT_MAX_ROWS).min(DEFAULT_MAX_ROWS);
        let result = db
            .query(&p.sql, max)
            .await
            .map_err(|e| invalid(e.to_string()))?;
        let json = serde_json::to_string(&result).map_err(|e| invalid(e.to_string()))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        name = "data_table",
        description = "Describe a table in a source: its column names and a few sample rows (JSON)."
    )]
    pub async fn data_table(
        &self,
        params: Parameters<TableParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = params.0;
        let path = self.source_path(&p.source)?;
        let db = DataSql::open_readonly(&path)
            .await
            .map_err(|e| invalid(e.to_string()))?;
        let result = db
            .describe_table(&p.table)
            .await
            .map_err(|e| invalid(e.to_string()))?;
        let json = serde_json::to_string(&result).map_err(|e| invalid(e.to_string()))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    async fn seeded_server() -> (tempfile::TempDir, DataSqlServer) {
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
        sqlx::query("CREATE TABLE genes (symbol TEXT, chrom TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        for (s, c) in [("CFTR", "7"), ("TP53", "17")] {
            sqlx::query("INSERT INTO genes VALUES (?, ?)")
                .bind(s)
                .bind(c)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;
        let mut sources = HashMap::new();
        sources.insert("cohort".to_string(), path);
        (dir, DataSqlServer::new(sources))
    }

    fn result_text(r: &CallToolResult) -> String {
        // Serialize the whole result and assert on the JSON payload — avoids
        // depending on rmcp's Content accessor shape across versions.
        serde_json::to_string(r).unwrap()
    }

    #[tokio::test]
    async fn data_sources_lists_declared_sources() {
        let (_d, srv) = seeded_server().await;
        let r = srv.data_sources().await.unwrap();
        assert!(result_text(&r).contains("cohort"));
    }

    #[tokio::test]
    async fn data_query_returns_rows_for_known_source() {
        let (_d, srv) = seeded_server().await;
        let r = srv
            .data_query(Parameters(QueryParams {
                source: "cohort".to_string(),
                sql: "SELECT symbol FROM genes WHERE chrom = '7'".to_string(),
                max_rows: None,
            }))
            .await
            .unwrap();
        let text = result_text(&r);
        assert!(text.contains("CFTR"), "{text}");
        assert!(!text.contains("TP53"), "filtered out: {text}");
    }

    #[tokio::test]
    async fn data_query_unknown_source_errors() {
        let (_d, srv) = seeded_server().await;
        let err = srv
            .data_query(Parameters(QueryParams {
                source: "nope".to_string(),
                sql: "SELECT 1".to_string(),
                max_rows: None,
            }))
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn data_query_rejects_mutation() {
        let (_d, srv) = seeded_server().await;
        let err = srv
            .data_query(Parameters(QueryParams {
                source: "cohort".to_string(),
                sql: "DELETE FROM genes".to_string(),
                max_rows: None,
            }))
            .await;
        assert!(err.is_err(), "mutation must be rejected");
    }

    #[tokio::test]
    async fn data_table_describes_known_table() {
        let (_d, srv) = seeded_server().await;
        let r = srv
            .data_table(Parameters(TableParams {
                source: "cohort".to_string(),
                table: "genes".to_string(),
            }))
            .await
            .unwrap();
        let text = result_text(&r);
        assert!(text.contains("symbol") && text.contains("chrom"), "{text}");
    }
}
