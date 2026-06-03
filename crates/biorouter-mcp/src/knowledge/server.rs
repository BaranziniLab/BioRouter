use crate::knowledge::{
    convert::SourceInput,
    service::KnowledgeService,
    types::{ChangeKind, Manifest},
};
use anyhow::Result;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::HashSet, sync::Arc};

const SESSION_ID_META_KEY: &str = "biorouter-session-id";

/// Process-local active-KB state (one per KnowledgeServer instance).
#[derive(Clone, Default)]
pub struct ActiveKbState {
    inner: Arc<tokio::sync::Mutex<Option<String>>>,
}

impl ActiveKbState {
    pub async fn set(&self, kb_id: &str) {
        *self.inner.lock().await = Some(kb_id.to_string());
    }
    pub async fn get(&self) -> Option<String> {
        self.inner.lock().await.clone()
    }
    pub async fn clear(&self) {
        *self.inner.lock().await = None;
    }

    /// Bootstrap the in-memory state from `<knowledge-root>/.active-kb` so a
    /// freshly spawned MCP-server process picks up whatever KB the user had
    /// selected previously. Errors are intentionally swallowed — a missing or
    /// unreadable file simply yields an unset active KB.
    pub fn from_persisted(service: &KnowledgeService) -> Self {
        let initial = service.get_active_persisted().ok().flatten();
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(initial)),
        }
    }
}

#[derive(Clone)]
pub struct KnowledgeServer {
    tool_router: ToolRouter<Self>,
    service: KnowledgeService,
    instructions: String,
    active: ActiveKbState,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateBaseParams {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KbIdParams {
    pub kb_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListPagesParams {
    pub kb_id: String,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadPageParams {
    pub kb_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WritePageParams {
    pub kb_id: String,
    pub path: String,
    pub content: String,
    pub commit_message: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddRawSourceParams {
    pub kb_id: String,
    pub source: RawSourceInput,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RawSourceInput {
    Url {
        url: String,
    },
    Text {
        text: String,
        #[serde(default)]
        title: Option<String>,
    },
    // File uploads via MCP are out of scope; the HTTP layer (Plan 3) handles them.
}

impl From<RawSourceInput> for SourceInput {
    fn from(r: RawSourceInput) -> Self {
        match r {
            RawSourceInput::Url { url } => SourceInput::Url(url),
            RawSourceInput::Text { text, title } => SourceInput::Text { text, title },
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryParams {
    pub kb_id: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RestoreParams {
    pub kb_id: String,
    pub commit_sha: String,
}

// ── Task 4: Transaction tools ─────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BeginTxnParams {
    pub kb_id: String,
    pub label: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CommitTxnParams {
    pub kb_id: String,
    pub txn: String,
    pub summary: String,
    pub kind: ChangeKind,
    #[serde(default)]
    pub delta: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AbortTxnParams {
    pub kb_id: String,
    pub txn: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    pub kb_id: Option<String>,
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: usize,
}

fn default_search_limit() -> usize {
    5
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppendLogParams {
    pub kb_id: String,
    pub kind: ChangeKind,
    pub summary: String,
    #[serde(default)]
    pub delta: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct SearchHitWithKb {
    pub kb_id: String,
    pub path: String,
    pub score: f32,
    pub snippet: String,
}

// ── Task 5: Active-KB tools ───────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetActiveParams {
    pub kb_id: String,
}

// ── Task 5: Optional-kb_id variants of read-only params ─────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListPagesOptParams {
    pub kb_id: Option<String>,
    #[serde(default)]
    pub path_prefix: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadPageOptParams {
    pub kb_id: Option<String>,
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KbIdOptParams {
    pub kb_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryOptParams {
    pub kb_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[tool_router(router = tool_router)]
impl KnowledgeServer {
    pub fn new() -> Result<Self> {
        let service = KnowledgeService::new_default()?;
        let active = ActiveKbState::from_persisted(&service);
        Ok(Self {
            tool_router: Self::tool_router(),
            service,
            instructions: include_str!("instructions.md").to_string(),
            active,
        })
    }

    fn session_id_from_context(context: &RequestContext<RoleServer>) -> Option<&str> {
        context.meta.0.get(SESSION_ID_META_KEY)?.as_str()
    }

    fn session_id(context: Option<&RequestContext<RoleServer>>) -> Option<&str> {
        context.and_then(Self::session_id_from_context)
    }

    fn hidden_kbs_for_session(&self, session_id: Option<&str>) -> Result<Vec<String>, ErrorData> {
        match session_id {
            Some(session_id) => self
                .service
                .get_hidden_for_session_or_persisted(session_id)
                .map_err(into_err),
            None => self.service.get_hidden_persisted().map_err(into_err),
        }
    }

    fn visible_bases_for_session(
        &self,
        session_id: Option<&str>,
    ) -> Result<Vec<Manifest>, ErrorData> {
        let hidden = self.hidden_kbs_for_session(session_id)?;
        let hidden = hidden.into_iter().collect::<HashSet<_>>();
        let mut bases = self.service.list_bases().map_err(into_err)?;
        bases.retain(|base| !hidden.contains(&base.id));
        Ok(bases)
    }

    fn visible_bases_for_context(
        &self,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<Vec<Manifest>, ErrorData> {
        self.visible_bases_for_session(Self::session_id(context))
    }

    fn search_visible_bases(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
    ) -> Result<Vec<SearchHitWithKb>, ErrorData> {
        let mut hits = Vec::new();
        for base in self.visible_bases_for_session(session_id)? {
            let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &base.id);
            let kb_hits =
                crate::knowledge::store::search(&kb_root, query, limit).map_err(into_err)?;
            hits.extend(kb_hits.into_iter().map(|hit| SearchHitWithKb {
                kb_id: base.id.clone(),
                path: hit.path,
                score: hit.score,
                snippet: hit.snippet,
            }));
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.kb_id.cmp(&b.kb_id))
                .then_with(|| a.path.cmp(&b.path))
        });
        hits.truncate(limit);
        Ok(hits)
    }

    async fn active_kb_for_context(
        &self,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<Option<String>, ErrorData> {
        if let Some(context) = context {
            if let Some(session_id) = Self::session_id_from_context(context) {
                if let Some(session_active) = self
                    .service
                    .get_active_for_session(session_id)
                    .map_err(into_err)?
                {
                    return Ok(Some(session_active));
                }
            }
        }

        if let Some(active) = self.active.get().await {
            return Ok(Some(active));
        }

        self.service.get_active_persisted().map_err(into_err)
    }

    /// Resolve `supplied` kb_id or fall back to the active KB for this request context.
    async fn kb_id_or_active(
        &self,
        supplied: Option<String>,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<String, ErrorData> {
        if let Some(id) = supplied {
            return Ok(id);
        }
        self.active_kb_for_context(context).await?.ok_or_else(|| {
            ErrorData::invalid_params(
                "kb_id not supplied and no active knowledge base is set. Call kb_set_active first.",
                None,
            )
        })
    }

    #[tool(
        name = "kb_list_bases",
        description = "List knowledge bases visible to this session. Hidden knowledge bases are omitted from discovery."
    )]
    pub async fn kb_list_bases(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let bases = self.visible_bases_for_context(Some(&context))?;
        ok_json(&bases)
    }

    #[tool(name = "kb_create_base", description = "Create a new knowledge base.")]
    pub async fn kb_create_base(
        &self,
        p: Parameters<CreateBaseParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let m = self
            .service
            .create_base(&p.id, &p.name, p.color.as_deref())
            .map_err(into_err)?;
        ok_json(&m)
    }

    #[tool(
        name = "kb_list_pages",
        description = "List knowledge pages in a knowledge base. Omit kb_id to use the active KB."
    )]
    pub async fn kb_list_pages(
        &self,
        p: Parameters<ListPagesOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_active(p.kb_id, Some(&context)).await?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &kb_id);
        let pages = crate::knowledge::store::list_pages(&kb_root, p.path_prefix.as_deref())
            .map_err(into_err)?;
        ok_json(&pages)
    }

    #[tool(
        name = "kb_read_page",
        description = "Read a single knowledge page by path. Omit kb_id to use the active KB."
    )]
    pub async fn kb_read_page(
        &self,
        p: Parameters<ReadPageOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_active(p.kb_id, Some(&context)).await?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &kb_id);
        let page = crate::knowledge::store::read_page(&kb_root, &p.path).map_err(into_err)?;
        ok_json(&page)
    }

    #[tool(
        name = "kb_write_page",
        description = "Create or overwrite a knowledge page and commit."
    )]
    pub async fn kb_write_page(
        &self,
        p: Parameters<WritePageParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let sha = crate::knowledge::store::write_page(
            &kb_root,
            &p.path,
            &p.content,
            &p.commit_message,
            None,
        )
        .map_err(into_err)?;
        ok_json(&serde_json::json!({ "commit_sha": sha }))
    }

    #[tool(
        name = "kb_add_raw_source",
        description = "Add a raw source (URL or pasted text), convert to markdown, and classify credibility."
    )]
    pub async fn kb_add_raw_source(
        &self,
        p: Parameters<AddRawSourceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        let res = self
            .service
            .add_raw_source(&p.kb_id, p.source.into(), None)
            .await
            .map_err(into_err)?;
        ok_json(&serde_json::json!({
            "source_id": res.source_id,
            "source_md_path": res.source_md_path,
            "meta_path": res.meta_path,
        }))
    }

    #[tool(
        name = "kb_get_graph",
        description = "Return the cached node+edge graph for a knowledge base. Omit kb_id to use the active KB."
    )]
    pub async fn kb_get_graph(
        &self,
        p: Parameters<KbIdOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_active(p.kb_id, Some(&context)).await?;
        let g = self.service.get_graph(&kb_id).map_err(into_err)?;
        ok_json(&g)
    }

    #[tool(
        name = "kb_list_history",
        description = "List recent change-log entries from the git history. Omit kb_id to use the active KB."
    )]
    pub async fn kb_list_history(
        &self,
        p: Parameters<HistoryOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_active(p.kb_id, Some(&context)).await?;
        let h = self
            .service
            .list_history(&kb_id, p.limit)
            .map_err(into_err)?;
        ok_json(&h)
    }

    #[tool(
        name = "kb_restore_state",
        description = "Restore the knowledge folder to a previous commit by creating a new commit on top of HEAD."
    )]
    pub async fn kb_restore_state(
        &self,
        p: Parameters<RestoreParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let sha = self
            .service
            .restore_state(&p.kb_id, &p.commit_sha)
            .map_err(into_err)?;
        ok_json(&serde_json::json!({ "ok": true, "new_commit_sha": sha }))
    }

    // ── Task 4: Transaction MCP tools ─────────────────────────────────────────

    #[tool(
        name = "kb_begin_txn",
        description = "Open a transactional working branch on a knowledge base. Returns the txn handle (branch name) for use with subsequent mutating primitives."
    )]
    pub async fn kb_begin_txn(
        &self,
        p: Parameters<BeginTxnParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let repo = crate::knowledge::git::GitRepo::open(&kb_root).map_err(into_err)?;
        let txn = repo.begin_txn(&p.label).map_err(into_err)?;
        ok_json(&serde_json::json!({ "txn": txn.branch }))
    }

    #[tool(
        name = "kb_commit_txn",
        description = "Squash-merge a transaction branch onto the main history with the given kind/summary/delta."
    )]
    pub async fn kb_commit_txn(
        &self,
        p: Parameters<CommitTxnParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let repo = crate::knowledge::git::GitRepo::open(&kb_root).map_err(into_err)?;
        let txn = crate::knowledge::git::Txn { branch: p.txn };
        let sha = repo
            .commit_txn(&txn, p.kind, &p.summary, p.delta.as_deref())
            .map_err(into_err)?;
        ok_json(&serde_json::json!({ "commit_sha": sha }))
    }

    #[tool(
        name = "kb_abort_txn",
        description = "Discard a transaction branch and restore the working tree to main."
    )]
    pub async fn kb_abort_txn(
        &self,
        p: Parameters<AbortTxnParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let repo = crate::knowledge::git::GitRepo::open(&kb_root).map_err(into_err)?;
        let txn = crate::knowledge::git::Txn { branch: p.txn };
        repo.abort_txn(&txn).map_err(into_err)?;
        ok_json(&serde_json::json!({ "ok": true }))
    }

    #[tool(
        name = "kb_search",
        description = "BM25 full-text search over knowledge pages and raw source documents. Omit kb_id to search all knowledge bases visible to this session."
    )]
    pub async fn kb_search(
        &self,
        p: Parameters<SearchParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let hits = if let Some(kb_id) = p.kb_id {
            let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &kb_id);
            crate::knowledge::store::search(&kb_root, &p.query, p.limit)
                .map_err(into_err)?
                .into_iter()
                .map(|hit| SearchHitWithKb {
                    kb_id: kb_id.clone(),
                    path: hit.path,
                    score: hit.score,
                    snippet: hit.snippet,
                })
                .collect::<Vec<_>>()
        } else {
            self.search_visible_bases(&p.query, p.limit, Self::session_id(Some(&context)))?
        };
        ok_json(&hits)
    }

    #[tool(
        name = "kb_append_log",
        description = "Append a structured entry to the KB change log and commit it."
    )]
    pub async fn kb_append_log(
        &self,
        p: Parameters<AppendLogParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let sha =
            crate::knowledge::log::append(&kb_root, p.kind, &p.summary, p.delta.as_deref(), None)
                .map_err(into_err)?;
        ok_json(&serde_json::json!({ "ok": true, "commit_sha": sha }))
    }

    // ── Task 5: Active-KB MCP tools ───────────────────────────────────────────

    #[tool(
        name = "kb_set_active",
        description = "Set the active knowledge base for this session. Subsequent kb_* tool calls that omit kb_id will default to this one."
    )]
    pub async fn kb_set_active(
        &self,
        p: Parameters<SetActiveParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        crate::knowledge::paths::validate_kb_id(&p.0.kb_id).map_err(|e| into_err(e.into()))?;
        self.active.set(&p.0.kb_id).await;
        if let Some(session_id) = Self::session_id_from_context(&context) {
            self.service
                .set_active_for_session(session_id, Some(&p.0.kb_id))
                .map_err(into_err)?;
        } else {
            self.service
                .set_active_persisted(Some(&p.0.kb_id))
                .map_err(into_err)?;
        }
        ok_json(&serde_json::json!({ "ok": true, "active_kb": p.0.kb_id }))
    }

    #[tool(
        name = "kb_get_active",
        description = "Return the currently active knowledge base id (if any)."
    )]
    pub async fn kb_get_active(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let v = self.active_kb_for_context(Some(&context)).await?;
        ok_json(&serde_json::json!({ "active_kb": v }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for KnowledgeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "biorouter-knowledge".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(self.instructions.clone()),
            ..Default::default()
        }
    }
}

fn ok_json<T: Serialize>(v: &T) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string_pretty(v)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

fn into_err(e: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(format!("{e:#}"), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server_with_root(root: std::path::PathBuf) -> KnowledgeServer {
        let service = KnowledgeService::new(root);
        KnowledgeServer {
            tool_router: KnowledgeServer::tool_router(),
            service,
            instructions: String::new(),
            active: ActiveKbState::default(),
        }
    }

    #[test]
    fn list_bases_hides_session_hidden_kbs() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let server = server_with_root(tmp.path().to_path_buf());
        server.service.create_base("visible", "Visible", None)?;
        server.service.create_base("hidden", "Hidden", None)?;
        server
            .service
            .set_hidden_for_session("session-a", &["hidden".to_string()])?;

        let visible = server.visible_bases_for_session(Some("session-a"))?;
        let ids = visible.into_iter().map(|base| base.id).collect::<Vec<_>>();
        assert_eq!(ids, vec!["visible".to_string()]);

        let all_visible = server.visible_bases_for_session(Some("session-b"))?;
        assert_eq!(all_visible.len(), 2);
        Ok(())
    }

    #[test]
    fn search_without_kb_id_spans_all_visible_bases() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let server = server_with_root(tmp.path().to_path_buf());
        server.service.create_base("alpha", "Alpha", None)?;
        server.service.create_base("beta", "Beta", None)?;
        server.service.create_base("hidden", "Hidden", None)?;

        crate::knowledge::store::write_page(
            &crate::knowledge::paths::kb_root(server.service.root(), "alpha"),
            "knowledge/notes/a.md",
            "# Shared topic\n\nalpha content",
            "alpha page",
            None,
        )?;
        crate::knowledge::store::write_page(
            &crate::knowledge::paths::kb_root(server.service.root(), "beta"),
            "knowledge/notes/b.md",
            "# Shared topic\n\nbeta content",
            "beta page",
            None,
        )?;
        crate::knowledge::store::write_page(
            &crate::knowledge::paths::kb_root(server.service.root(), "hidden"),
            "knowledge/notes/c.md",
            "# Shared topic\n\nhidden content",
            "hidden page",
            None,
        )?;
        server
            .service
            .set_hidden_for_session("session-a", &["hidden".to_string()])?;

        let hits = server.search_visible_bases("shared topic", 10, Some("session-a"))?;
        let kb_ids = hits.into_iter().map(|hit| hit.kb_id).collect::<Vec<_>>();
        assert!(kb_ids.contains(&"alpha".to_string()));
        assert!(kb_ids.contains(&"beta".to_string()));
        assert!(!kb_ids.contains(&"hidden".to_string()));
        Ok(())
    }
}
