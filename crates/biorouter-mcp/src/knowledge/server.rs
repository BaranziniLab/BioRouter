use crate::knowledge::{
    convert::SourceInput,
    service::KnowledgeService,
    store::SearchScope,
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
use std::{cmp::Ordering, collections::HashSet};

const SESSION_ID_META_KEY: &str = "biorouter-session-id";

#[derive(Clone)]
pub struct KnowledgeServer {
    tool_router: ToolRouter<Self>,
    service: KnowledgeService,
    instructions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateBaseParams {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportArchiveParams {
    /// Knowledge base id to export.
    pub kb_id: String,
    /// Absolute file path to write the `.brkb` archive to. If omitted, a file
    /// named `<kb_id>.brkb` is written to the system temp directory.
    #[serde(default)]
    pub dest_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportArchiveParams {
    /// Absolute path to the `.brkb` archive file to import.
    pub src_path: String,
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
    #[serde(default)]
    pub include_raw_sources: bool,
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
        Ok(Self {
            tool_router: Self::tool_router(),
            service: KnowledgeService::new_default()?,
            instructions: include_str!("instructions.md").to_string(),
        })
    }

    fn session_id_from_context(context: &RequestContext<RoleServer>) -> Option<&str> {
        context.meta.0.get(SESSION_ID_META_KEY)?.as_str()
    }

    fn session_id(context: Option<&RequestContext<RoleServer>>) -> Option<&str> {
        context.and_then(Self::session_id_from_context)
    }

    /// Issue #56. The capability the daemon admitted this call on, PUBLIC unless
    /// the request meta says otherwise. It *delegates* to
    /// [`crate::knowledge::tier::caller_is_private`] rather than re-reading the
    /// key, so CP1 here and CP4 in `agent_drafter` cannot drift.
    ///
    /// Consumed by Task 10C's hand-written `call_tool` (CP1); nothing reads it
    /// yet, which is the same Phase-1 separation O1 uses.
    #[allow(dead_code)]
    fn caller_is_private(context: Option<&RequestContext<RoleServer>>) -> bool {
        context
            .map(|c| crate::knowledge::tier::caller_is_private(&c.meta))
            .unwrap_or(false)
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
        scope: SearchScope,
    ) -> Result<Vec<SearchHitWithKb>, ErrorData> {
        let mut hits = Vec::new();
        for base in self.visible_bases_for_session(session_id)? {
            let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &base.id);
            let kb_hits = crate::knowledge::store::search_with_scope(&kb_root, query, limit, scope)
                .map_err(into_err)?;
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

    /// This session's primary knowledge base — the write target for KB-less
    /// mutating calls and the default subject for single-base reads. Resolved
    /// from disk on every call: session file → machine file, returned only
    /// while it names a member of the session's set.
    fn primary_kb_for_session(
        &self,
        session_id: Option<&str>,
    ) -> Result<Option<String>, ErrorData> {
        self.service
            .primary_for_session(session_id)
            .map_err(into_err)
    }

    fn primary_kb_for_context(
        &self,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<Option<String>, ErrorData> {
        self.primary_kb_for_session(Self::session_id(context))
    }

    /// Resolve `supplied` kb_id, else this session's primary.
    ///
    /// An explicit `kb_id` always wins and is never filtered against the
    /// session's set — that is how a hidden base (Soul) stays reachable.
    fn kb_id_or_primary(
        &self,
        supplied: Option<String>,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<String, ErrorData> {
        if let Some(id) = supplied {
            return Ok(id);
        }
        if let Some(primary) = self.primary_kb_for_context(context)? {
            return Ok(primary);
        }
        let ids = self
            .service
            .session_kb_ids(Self::session_id(context))
            .map_err(into_err)?;
        Err(ErrorData::invalid_params(
            if ids.is_empty() {
                "this session has no knowledge bases, so there is nothing to read. \
                 Create one with kb_create_base."
                    .to_string()
            } else {
                format!(
                    "kb_id not supplied and this session has no primary knowledge base. \
                     Pass kb_id explicitly (one of: {}), or call kb_set_active to make one \
                     the primary — that is also where KB-less writes go.",
                    ids.join(", ")
                )
            },
            None,
        ))
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
        description = "List knowledge pages in a knowledge base. Omit kb_id to use this session's primary knowledge base. To read a different base, pass its kb_id — you never need to change the primary to read."
    )]
    pub async fn kb_list_pages(
        &self,
        p: Parameters<ListPagesOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_primary(p.kb_id, Some(&context))?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &kb_id);
        let pages = crate::knowledge::store::list_pages(&kb_root, p.path_prefix.as_deref())
            .map_err(into_err)?;
        ok_json(&pages)
    }

    #[tool(
        name = "kb_read_page",
        description = "Read a single knowledge page by path. Omit kb_id to use this session's primary knowledge base. To read a different base, pass its kb_id — you never need to change the primary to read."
    )]
    pub async fn kb_read_page(
        &self,
        p: Parameters<ReadPageOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_primary(p.kb_id, Some(&context))?;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &kb_id);
        let page = crate::knowledge::store::read_page(&kb_root, &p.path).map_err(into_err)?;
        ok_json(&page)
    }

    #[tool(
        name = "kb_write_page",
        description = "Create or overwrite a knowledge page and commit. The path must be under \
                       knowledge/ (e.g. knowledge/<topic>.md) or be index.md/schema.md/log.md; \
                       raw/ holds immutable ingested sources and is read-only — to add or update \
                       a source, use kb_add_raw_source or re-ingest it."
    )]
    pub async fn kb_write_page(
        &self,
        p: Parameters<WritePageParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        // Issue #26: reject contract violations as INVALID_PARAMS (the error
        // taxonomy reads that as invalid_args — "fix the call itself") instead
        // of letting them flow through into_err as an opaque internal error.
        if !crate::knowledge::store::is_writable_page_path(&p.path) {
            return Err(ErrorData::invalid_params(
                format!(
                    "invalid write path {:?}: must start with knowledge/ or be \
                     index.md/schema.md/log.md. {}",
                    p.path,
                    crate::knowledge::store::WRITE_PATH_RECOVERY
                ),
                None,
            ));
        }
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
        // Keep the derived graph cache in sync after a page write. Without
        // this, pages authored from chat (via this tool) never appear in the
        // Knowledge graph view: get_graph returns the empty cache written at
        // create time, and the "Refresh graph" button only re-reads that
        // cache. add_raw_source already rebuilds for the GUI ingest path; do
        // the same here so chat-curated KBs visualize their pages/links.
        self.service
            .rebuild_graph_cache(&p.kb_id)
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
        description = "Return the cached node+edge graph for a knowledge base. Omit kb_id to use this session's primary knowledge base. To read a different base, pass its kb_id — you never need to change the primary to read."
    )]
    pub async fn kb_get_graph(
        &self,
        p: Parameters<KbIdOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_primary(p.kb_id, Some(&context))?;
        let g = self.service.get_graph(&kb_id).map_err(into_err)?;
        ok_json(&g)
    }

    #[tool(
        name = "kb_list_history",
        description = "List recent change-log entries from the git history. Omit kb_id to use this session's primary knowledge base. To read a different base, pass its kb_id — you never need to change the primary to read."
    )]
    pub async fn kb_list_history(
        &self,
        p: Parameters<HistoryOptParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_id = self.kb_id_or_primary(p.kb_id, Some(&context))?;
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
        description = "BM25 full-text search over curated knowledge pages. Omit kb_id to search all visible knowledge bases. Set include_raw_sources=true only when the user explicitly asks to inspect/search original raw sources."
    )]
    pub async fn kb_search(
        &self,
        p: Parameters<SearchParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let scope = if p.include_raw_sources {
            SearchScope::All
        } else {
            SearchScope::Knowledge
        };
        let hits = if let Some(kb_id) = p.kb_id {
            let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &kb_id);
            crate::knowledge::store::search_with_scope(&kb_root, &p.query, p.limit, scope)
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
            self.search_visible_bases(&p.query, p.limit, Self::session_id(Some(&context)), scope)?
        };
        ok_json(&hits)
    }

    #[tool(
        name = "kb_search_raw_sources",
        description = "BM25 full-text search over original raw source markdown only. Use this rarely, when the user specifically asks for raw/original/source-document evidence instead of the curated knowledge graph."
    )]
    pub async fn kb_search_raw_sources(
        &self,
        p: Parameters<SearchParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let hits = if let Some(kb_id) = p.kb_id {
            let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &kb_id);
            crate::knowledge::store::search_with_scope(
                &kb_root,
                &p.query,
                p.limit,
                SearchScope::RawSources,
            )
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
            self.search_visible_bases(
                &p.query,
                p.limit,
                Self::session_id(Some(&context)),
                SearchScope::RawSources,
            )?
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

    // ── The session's knowledge-base set and its primary ──────────────────────

    /// Body of `kb_set_active`, split out so it can be unit-tested without
    /// fabricating a `RequestContext`.
    fn set_primary_json(
        &self,
        session_id: Option<&str>,
        kb_id: &str,
    ) -> Result<serde_json::Value, ErrorData> {
        crate::knowledge::paths::validate_kb_id(kb_id)
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
        let selection = self
            .service
            .set_selection(
                session_id,
                None,
                crate::knowledge::service::PrimaryUpdate::Set(kb_id),
            )
            .map_err(|e| ErrorData::invalid_params(format!("{e:#}"), None))?;
        Ok(Self::selection_value(&selection, true))
    }

    /// Body of `kb_get_active`.
    fn selection_json(&self, session_id: Option<&str>) -> Result<serde_json::Value, ErrorData> {
        let selection = self.service.selection(session_id).map_err(into_err)?;
        Ok(Self::selection_value(&selection, false))
    }

    fn selection_value(
        selection: &crate::knowledge::service::KbSelection,
        ok: bool,
    ) -> serde_json::Value {
        let mut v = serde_json::json!({
            "primary_kb": selection.primary_kb,
            "knowledge_bases": selection.kb_ids,
            // Deprecated mirror of `primary_kb`, kept for one release so
            // anything that learned the old key keeps working.
            "active_kb": selection.primary_kb,
        });
        if ok {
            v["ok"] = serde_json::Value::Bool(true);
        }
        v
    }

    #[tool(
        name = "kb_set_active",
        description = "Make one knowledge base this session's primary: the base that KB-less writes land in and that single-base reads default to. It does not change what you can search — kb_search with no kb_id already covers every knowledge base in this session, tagging each hit with its kb_id. To read or write another base, pass its kb_id; do not switch the primary to get at it. The base must be one of this session's knowledge bases."
    )]
    pub async fn kb_set_active(
        &self,
        p: Parameters<SetActiveParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let v = self.set_primary_json(Self::session_id(Some(&context)), &p.0.kb_id)?;
        ok_json(&v)
    }

    #[tool(
        name = "kb_get_active",
        description = "Return this session's knowledge bases and which one is the primary (the KB-less write target)."
    )]
    pub async fn kb_get_active(
        &self,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let v = self.selection_json(Self::session_id(Some(&context)))?;
        ok_json(&v)
    }

    #[tool(
        name = "kb_export",
        description = "Export a knowledge base to a .brkb archive file on disk. Returns the absolute path written."
    )]
    pub async fn kb_export(
        &self,
        p: Parameters<ExportArchiveParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let _lock = self.service.lock_kb(&p.kb_id).await.map_err(into_err)?;
        let bytes = self.service.export_brkb(&p.kb_id).map_err(into_err)?;
        // Issue #56, decision (2b). A MODEL's export of a PRIVATE base may not
        // be aimed anywhere it asks: a `.brkb` is a zip, so an archive dropped
        // outside the knowledge tree is readable by the same session's shell
        // with `unzip -p`, no import and no marker-stripping required. Forcing
        // it into `<knowledge-root>/.exports/` keeps the artifact beside the base
        // it came from. The directory name is a DOTFILE on purpose: see
        // `paths::MODEL_EXPORT_DIR` — a plain `exports/` is a legal kb id, so a
        // session could create the base `exports` and collect every private
        // archive inside a public base's own tree.
        //
        // Scoped to PRIVATE bases on purpose: relocating every model export
        // would break `kb_export` as a feature. And it lives HERE rather than in
        // `KnowledgeService::export_brkb`, because that function also serves the
        // user's own download from the Knowledge view, which this rule must not
        // touch.
        //
        // The tier is read BEFORE `dest_path` is honoured — a write-then-move
        // would leave a complete copy of a private knowledge base at a
        // public-readable path for the length of the copy.
        let dest = if crate::knowledge::tier::is_private(self.service.root(), &p.kb_id) {
            crate::knowledge::paths::model_export_dir(self.service.root())
                .join(format!("{}.brkb", p.kb_id))
        } else {
            match p.dest_path {
                Some(path) => std::path::PathBuf::from(path),
                None => std::env::temp_dir().join(format!("{}.brkb", p.kb_id)),
            }
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| into_err(anyhow::anyhow!("create export dir: {e}")))?;
        }
        std::fs::write(&dest, &bytes).map_err(|e| into_err(anyhow::anyhow!("write .brkb: {e}")))?;
        ok_json(&serde_json::json!({
            "kb_id": p.kb_id,
            "path": dest.to_string_lossy(),
            "bytes": bytes.len(),
        }))
    }

    #[tool(
        name = "kb_import",
        description = "Import a .brkb archive file from disk as a new knowledge base. Returns the new knowledge base id."
    )]
    pub async fn kb_import(
        &self,
        p: Parameters<ImportArchiveParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let bytes = std::fs::read(&p.src_path)
            .map_err(|e| into_err(anyhow::anyhow!("read .brkb '{}': {e}", p.src_path)))?;
        // Issue #56. `kb_import` takes no `RequestContext`, so the importer's
        // own tier is not knowable here — the archive's marker still applies as
        // a floor. Task 10B stamps the caller's tier after the import returns
        // (safe because `brkb::import`'s collision loop always lands on a fresh
        // id, so the stamp can never hit an existing base).
        let new_id = self
            .service
            .import_brkb(&bytes, /* importer_is_private */ false)
            .map_err(into_err)?;
        ok_json(&serde_json::json!({ "imported_kb_id": new_id }))
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
    use std::path::{Path, PathBuf};

    /// The knowledge instructions must teach the agent to consult the built-in
    /// Soul KB for personal context and that a hidden KB (which Soul may be) is
    /// reachable only by explicit kb_id — otherwise the agent never personalises
    /// from Soul because the default cross-base search skips hidden bases.
    #[test]
    fn instructions_cover_soul_and_hidden_kb_access() {
        let instructions = include_str!("instructions.md");
        assert!(
            instructions.contains("Soul") && instructions.contains("kb_id=\"soul\""),
            "instructions must name the Soul KB and how to search it"
        );
        assert!(
            instructions.contains("hidden") && instructions.to_lowercase().contains("explicit"),
            "instructions must explain searching a hidden KB by explicit kb_id"
        );
    }

    fn server_with_root(root: std::path::PathBuf) -> KnowledgeServer {
        let service = KnowledgeService::new(root);
        KnowledgeServer {
            tool_router: KnowledgeServer::tool_router(),
            service,
            instructions: String::new(),
        }
    }

    /// Regression (pre-existing, not introduced by the merge): the deleted
    /// process-local active-KB cache was one `Option<String>` for the **whole
    /// KnowledgeServer process**. `kb_set_active` wrote it alongside the
    /// session file and `active_kb_for_context` consulted it for any session
    /// that had no file of its own — so one chat's choice silently became
    /// every other chat's write target inside one daemon, and it was never
    /// invalidated on rename or delete.
    #[test]
    fn one_sessions_primary_does_not_leak_into_another() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let server = server_with_root(tmp.path().to_path_buf());
        server.service.create_base("alpha", "Alpha", None)?;
        server.service.create_base("beta", "Beta", None)?;

        server
            .service
            .set_primary_for_session("session-a", Some("beta"))?;

        assert_eq!(
            server.primary_kb_for_session(Some("session-a"))?.as_deref(),
            Some("beta")
        );
        assert_eq!(
            server.primary_kb_for_session(Some("session-b"))?,
            None,
            "session-b never chose a primary; session-a's choice must not become its write target"
        );
        Ok(())
    }

    /// The guard against re-introducing the cache. Primary resolution must be
    /// a pure function of (session id, on-disk state) — any in-process slot
    /// re-opens the cross-session leak and the stale-after-rename bug, and
    /// neither has a cheap behavioural test because both need a live
    /// `RequestContext`.
    #[test]
    fn knowledge_server_keeps_no_in_process_primary_cache() {
        let src = include_str!("server.rs");
        // Assembled at runtime: spelling the identifier literally anywhere in
        // this file — including in this test — would make the guard pass
        // vacuously the moment somebody re-introduced the struct.
        let banned = concat!("Active", "KbState");
        assert!(
            !src.contains(banned),
            "primary resolution must read the service, not a process-local cache"
        );
    }

    /// The hinge of the whole change. With no `kb_id` and no primary, the
    /// error is the only instruction the model gets — it must name the
    /// candidates and the exact recovery, never guess a base.
    #[test]
    fn kb_id_or_primary_errors_with_the_candidate_list() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let server = server_with_root(tmp.path().to_path_buf());
        server.service.create_base("alpha", "Alpha", None)?;
        server.service.create_base("beta", "Beta", None)?;

        let err = server
            .kb_id_or_primary(None, None)
            .expect_err("no primary chosen");
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("alpha, beta") && err.message.contains("kb_set_active"),
            "the error must list the candidates and the fix, got: {}",
            err.message
        );

        server.service.set_primary_persisted(Some("beta"))?;
        assert_eq!(server.kb_id_or_primary(None, None)?, "beta");
        assert_eq!(
            server.kb_id_or_primary(Some("alpha".to_string()), None)?,
            "alpha",
            "an explicit kb_id always wins — that is how a base outside the set is reached"
        );
        Ok(())
    }

    /// The four read tools that fall back to the primary must say so, in the
    /// new vocabulary — the model's mental model is built from these strings,
    /// and "the active KB" is what makes it switch instead of passing kb_id.
    #[test]
    fn read_tool_descriptions_teach_the_primary_not_the_active_kb() {
        let tools = KnowledgeServer::tool_router().list_all();
        for name in [
            "kb_list_pages",
            "kb_read_page",
            "kb_get_graph",
            "kb_list_history",
        ] {
            let desc = tools
                .iter()
                .find(|t| t.name == name)
                .and_then(|t| t.description.clone())
                .unwrap_or_else(|| panic!("{name} has a description"));
            assert!(
                desc.contains("primary knowledge base"),
                "{name} must name the primary, got: {desc}"
            );
            assert!(
                !desc.contains("active KB"),
                "{name} must not keep teaching the single-active model, got: {desc}"
            );
        }
    }

    /// `kb_set_active` used to validate the id's *format* only — it would
    /// happily point the session at a base that does not exist, and with a
    /// KB-less write behind it that is a lost write. It now validates
    /// membership, and reports the whole selection back so the model does not
    /// need a second round-trip to see its bases.
    #[test]
    fn set_primary_validates_membership_and_reports_the_set() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let server = server_with_root(tmp.path().to_path_buf());
        for id in ["alpha", "beta", "gamma"] {
            server.service.create_base(id, id, None)?;
        }
        server
            .service
            .set_hidden_for_session("session-a", &["gamma".to_string()])?;

        let v = server.set_primary_json(Some("session-a"), "beta")?;
        assert_eq!(v["primary_kb"], serde_json::json!("beta"));
        assert_eq!(
            v["active_kb"],
            serde_json::json!("beta"),
            "the deprecated mirror must track the primary for one release"
        );
        assert_eq!(
            v["knowledge_bases"],
            serde_json::json!(["alpha", "beta"]),
            "the set comes back with the primary, so discovery is one call"
        );

        let err = server
            .set_primary_json(Some("session-a"), "gamma")
            .expect_err("gamma is not in this session");
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("gamma") && err.message.contains("alpha, beta"),
            "got: {}",
            err.message
        );

        let err = server
            .set_primary_json(Some("session-a"), "no-such-kb")
            .expect_err("a base that does not exist can never be primary");
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);

        assert_eq!(
            server.selection_json(Some("session-a"))?["primary_kb"],
            serde_json::json!("beta")
        );
        Ok(())
    }

    #[test]
    fn state_tool_descriptions_teach_the_merged_model() {
        let tools = KnowledgeServer::tool_router().list_all();
        let desc = tools
            .iter()
            .find(|t| t.name == "kb_set_active")
            .and_then(|t| t.description.clone())
            .expect("kb_set_active has a description");
        assert!(
            desc.contains("primary") && desc.contains("does not change what you can search"),
            "kb_set_active must stop implying that activating narrows search, got: {desc}"
        );
    }

    /// Prose is behaviour here. Pin the sentences the model needs: that every
    /// base in the session is already in play, that one of them is the primary
    /// write target, and — the load-bearing one — that reading another base
    /// means passing kb_id, not switching the primary.
    #[test]
    fn instructions_teach_the_session_set_and_the_primary() {
        let instructions = include_str!("instructions.md");
        assert!(
            instructions.contains("primary") && instructions.contains("kb_get_active"),
            "instructions must name the primary and how to read it"
        );
        assert!(
            instructions.contains("Do not switch the primary"),
            "instructions must forbid switching the primary just to read another base"
        );
        assert!(
            instructions.contains("every knowledge base in this session"),
            "instructions must state that a kb_id-less kb_search already covers the whole set"
        );
        assert!(
            instructions.contains("kb_set_active"),
            "instructions must name the recovery when there is no primary"
        );
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

        let hits = server.search_visible_bases(
            "shared topic",
            10,
            Some("session-a"),
            SearchScope::Knowledge,
        )?;
        let kb_ids = hits.into_iter().map(|hit| hit.kb_id).collect::<Vec<_>>();
        assert!(kb_ids.contains(&"alpha".to_string()));
        assert!(kb_ids.contains(&"beta".to_string()));
        assert!(!kb_ids.contains(&"hidden".to_string()));
        Ok(())
    }

    /// Issue #26: a raw/ write is a contract violation the caller can fix, so
    /// it must surface as INVALID_PARAMS (taxonomy: invalid_args — "fix the
    /// call itself") carrying the recovery path, not as an opaque internal
    /// error classified tool_failure.
    #[tokio::test]
    async fn kb_write_page_rejects_raw_paths_as_invalid_params() -> anyhow::Result<()> {
        let tmp = tempfile::TempDir::new()?;
        let server = server_with_root(tmp.path().to_path_buf());
        server.service.create_base("kb", "KB", None)?;

        let err = server
            .kb_write_page(rmcp::handler::server::wrapper::Parameters(
                WritePageParams {
                    kb_id: "kb".to_string(),
                    path: "raw/x/source.md".to_string(),
                    content: "body".to_string(),
                    commit_message: "try to edit a raw source".to_string(),
                },
            ))
            .await
            .expect_err("raw/ writes must be rejected");

        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("knowledge/") && err.message.contains("kb_add_raw_source"),
            "rejection must state the contract and the recovery, got: {}",
            err.message
        );

        // The description itself must teach the path contract up front.
        let desc = KnowledgeServer::tool_router()
            .list_all()
            .into_iter()
            .find(|t| t.name == "kb_write_page")
            .and_then(|t| t.description.clone())
            .expect("kb_write_page has a description");
        assert!(
            desc.contains("knowledge/") && desc.contains("read-only"),
            "kb_write_page description must state the path contract, got: {desc}"
        );
        Ok(())
    }

    // ---- Issue #56, decision (2)(b): where a MODEL's export comes to rest ----
    //
    // ⚠ DEVIATION from the task text, recorded rather than hidden. The task
    // drives these through `call_tool_as(&srv, tool, args, tier)` — CP1's
    // harness, which Task 10C creates. There is no such seam in this task: the
    // generated `<KnowledgeServer as ServerHandler>::call_tool` demands an
    // `rmcp::RequestContext`, and building one needs a live `Peer` (see
    // `developer/rmcp_developer.rs`'s `serve_directly` fixtures). The tools are
    // therefore invoked directly, which is the same production function body the
    // router would reach. Nothing is lost: the location rule keys on the
    // **base's** tier, not the caller's — a public caller never gets this far,
    // because Task 10C's barrier refuses it outright — so the `tier` argument
    // would have been inert here anyway.

    fn migrated_server_with_base(id: &str) -> (KnowledgeServer, tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let server = server_with_root(root.clone());
        server.service.create_base(id, id, None).unwrap();
        (server, tmp, root)
    }

    fn seed_page(root: &Path, kb_id: &str, rel: &str, body: &str) {
        let p = root.join(kb_id).join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn reported_export_path(out: &CallToolResult) -> PathBuf {
        let text = out
            .content
            .iter()
            .find_map(|c| c.as_text())
            .expect("kb_export returns a text payload");
        let v: serde_json::Value = serde_json::from_str(&text.text).expect("valid json");
        PathBuf::from(v["path"].as_str().expect("a reported path"))
    }

    fn zip_names(bytes: &[u8]) -> Vec<String> {
        let mut a = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
        (0..a.len())
            .map(|i| a.by_index(i).unwrap().name().to_string())
            .collect()
    }

    async fn kb_export_via_tool(
        srv: &KnowledgeServer,
        kb_id: &str,
        dest_path: Option<String>,
    ) -> Result<CallToolResult, ErrorData> {
        srv.kb_export(Parameters(ExportArchiveParams {
            kb_id: kb_id.to_string(),
            dest_path,
        }))
        .await
    }

    #[tokio::test]
    async fn a_models_export_of_a_private_base_lands_inside_the_knowledge_root() {
        // Decision (2)(b) as behaviour. The exporter is PRIVATE — a public one is
        // refused outright by Task 10C's barrier — so this is the caller the
        // location rule exists for: permitted to export, not permitted to choose
        // where the bytes come to rest.
        let (srv, _tmp, root) = migrated_server_with_base("omop");
        crate::knowledge::tier::raise_unlocked(&root, "omop", true).unwrap();
        seed_page(&root, "omop", "knowledge/x.md", "SENTINEL-COHORT-N-412");
        let elsewhere = tempfile::tempdir().unwrap();
        let asked = elsewhere.path().join("omop.brkb");
        // ⚠ READ-ONLY, and this is the assertion — not decoration. A
        // `!asked.exists()` at the END passes "write the archive outside, then
        // move it inside before returning", which opens a real public-read window
        // for however long the copy takes. A final-state check cannot see a
        // transient file and no amount of polling makes it deterministic. Making
        // the directory unwritable turns the timing question into an ERROR: the
        // write-then-move implementation gets EACCES and fails the export; the
        // correct one never touches this directory and is unaffected.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(elsewhere.path(), std::fs::Permissions::from_mode(0o555))
                .unwrap();
            // ⚠ SELF-CHECK ON THE FIXTURE, and it is part of the gate. Under root
            // the mode bits are ignored and the `chmod` silently becomes a no-op,
            // which turns this whole test back into the final-state check it
            // replaces. Assert the property directly rather than proxying it
            // through a euid comparison.
            assert!(
                std::fs::write(elsewhere.path().join(".probe"), b"x").is_err(),
                "the read-only fixture did not take (running as root?) — this test \
                 would silently degrade to the assertion it was written to replace"
            );
        }

        let out = kb_export_via_tool(&srv, "omop", Some(asked.display().to_string()))
            .await
            .unwrap();

        // (a) nothing was written where the model aimed it — at any point, not
        //     just at the end.
        assert!(
            !asked.exists(),
            "a private base was exported outside the deny root"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // so TempDir can clean up
            std::fs::set_permissions(elsewhere.path(), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0);
        // (b) the tool REPORTED the real location, and it is under <root>/.exports/.
        let written = reported_export_path(&out);
        assert!(
            written.starts_with(crate::knowledge::paths::model_export_dir(&root)),
            "reported {}, which is not inside the knowledge root",
            written.display()
        );
        assert!(written.exists());
        // (c) …and it is the archive, not an empty file that satisfies (a) and (b).
        //     Without this, "write nothing anywhere" passes.
        assert!(zip_names(&std::fs::read(&written).unwrap())
            .iter()
            .any(|n| n.ends_with("knowledge/x.md")));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn a_models_export_of_a_PUBLIC_base_still_honours_dest_path() {
        // The mirror, and the reason the rule is scoped to private bases: forcing
        // the location for EVERY model export breaks `kb_export` as a feature, and
        // whoever hits that next will "fix" it by deleting the rule.
        let (srv, _tmp, root) = migrated_server_with_base("notes"); // registers public
        let elsewhere = tempfile::tempdir().unwrap();
        let asked = elsewhere.path().join("notes.brkb");
        let out = kb_export_via_tool(&srv, "notes", Some(asked.display().to_string()))
            .await
            .unwrap();
        assert!(asked.exists(), "a public base's export was relocated");
        assert_eq!(reported_export_path(&out), asked);
        assert!(!crate::knowledge::paths::model_export_dir(&root)
            .join("notes.brkb")
            .exists());
    }

    #[tokio::test]
    async fn a_private_export_cannot_be_collected_inside_a_public_base() {
        // Issue #56. The export directory is a sibling of the bases, never one
        // of them. If its name validated as a kb id, a session could create that
        // base first and every private archive would land inside a PUBLIC base's
        // own tree — `brkb::walk` packs whatever it finds — so exporting that
        // base would hand out every private one. The name is `.exports`, which
        // `validate_kb_id` rejects, so `create_base` cannot reach it at all.
        let (srv, _tmp, root) = migrated_server_with_base("omop");
        crate::knowledge::tier::raise_unlocked(&root, "omop", true).unwrap();
        seed_page(&root, "omop", "knowledge/x.md", "SENTINEL-COHORT-N-412");

        let dir = crate::knowledge::paths::MODEL_EXPORT_DIR;
        assert!(
            crate::knowledge::paths::validate_kb_id(dir).is_err(),
            "the export directory {dir} is a legal kb id, so a base can be created over it"
        );
        assert!(srv.service.create_base(dir, "collector", None).is_err());

        let written =
            reported_export_path(&kb_export_via_tool(&srv, "omop", None).await.unwrap());
        assert_eq!(
            written.parent().unwrap(),
            crate::knowledge::paths::model_export_dir(&root)
        );
        // …and the archive is not inside any knowledge base's directory.
        for entry in std::fs::read_dir(&root).unwrap() {
            let e = entry.unwrap();
            let name = e.file_name().to_string_lossy().to_string();
            if crate::knowledge::paths::validate_kb_id(&name).is_ok() {
                assert!(
                    !written.starts_with(e.path()),
                    "the export landed inside the knowledge base {name}"
                );
            }
        }
    }
}
