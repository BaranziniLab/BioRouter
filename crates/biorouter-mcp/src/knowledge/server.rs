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
    tool, tool_router, ErrorData, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::HashSet};

const SESSION_ID_META_KEY: &str = "biorouter-session-id";

/// Tools whose `kb_id` argument names a base the caller must be allowed to
/// reach. One list, one rule — so a twentieth `kb_*` tool is gated the day it
/// is written, and opting out means editing a list this task's test enumerates.
const KB_ID_GATED_TOOLS: &[&str] = &[
    "kb_list_pages",
    "kb_read_page",
    "kb_get_graph",
    "kb_list_history",
    "kb_search",
    "kb_search_raw_sources",
    "kb_export",
    "kb_write_page",
    "kb_add_raw_source",
    "kb_append_log",
    "kb_restore_state",
    "kb_begin_txn",
    "kb_commit_txn",
    "kb_abort_txn",
];

/// The subset that resolves an omitted `kb_id` to the session's primary (see
/// [`KnowledgeServer::kb_id_or_primary`]). For these an ABSENT id must be
/// resolved and checked too, or "just drop the kb_id" is the bypass.
const KB_PRIMARY_RESOLVING_TOOLS: &[&str] = &[
    "kb_list_pages",
    "kb_read_page",
    "kb_get_graph",
    "kb_list_history",
];

/// Content-bearing writes by a model: the base takes the caller's tier BEFORE
/// the write runs (issue #56).
const KB_RATCHETING_TOOLS: &[&str] = &["kb_write_page", "kb_add_raw_source", "kb_append_log"];

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
    /// Consumed by the hand-written `call_tool` below (CP1) and by
    /// `kb_create_base` / `kb_import`, the two tools whose subject id does not
    /// exist before the call.
    fn caller_is_private(context: Option<&RequestContext<RoleServer>>) -> bool {
        context
            .map(|c| crate::knowledge::tier::caller_is_private(&c.meta))
            .unwrap_or(false)
    }

    /// The base this call names, or `None` when it names none (issue #56).
    fn gated_kb_id(
        &self,
        tool: &str,
        args: Option<&rmcp::model::JsonObject>,
        context: Option<&RequestContext<RoleServer>>,
    ) -> Result<Option<String>, ErrorData> {
        if !KB_ID_GATED_TOOLS.contains(&tool) {
            return Ok(None);
        }
        if let Some(id) = args
            .and_then(|a| a.get("kb_id"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(Some(id.to_string()));
        }
        if !KB_PRIMARY_RESOLVING_TOOLS.contains(&tool) {
            // `kb_search` / `kb_search_raw_sources` with no kb_id fan out over
            // the visible set and filter per base (`search_visible_bases`) —
            // Task 10C's fan-out check is per-hit, not all-or-nothing.
            // `kb_export` and the writes REQUIRE kb_id, so an absent one is the
            // tool's own 400 and not ours to pre-empt.
            return Ok(None);
        }
        // Resolve exactly as the tool will (`kb_id_or_primary`), so omitting the
        // kb_id is not the bypass. Its error case — no id and no primary — is
        // the tool's own message and must NOT become a privacy refusal, so
        // `None` falls through and the tool answers.
        self.primary_kb_for_context(context)
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
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        // Issue #56. One of exactly TWO tools that take a `RequestContext` for
        // the ratchet, because their subject id is not knowable before the
        // call. Not a raise *before* the write and not one *after* it either:
        // `create_base_as` stamps the tier inside the same root-lock
        // transaction that creates the directory, so there is no window in
        // which a private session's brand-new base reads PUBLIC and no way for
        // a failing stamp to leave a PUBLIC base behind an `Err`. Same shape as
        // `import_brkb`, whose stamp rides in its single store write.
        let m = self
            .service
            .create_base_as(
                &p.id,
                &p.name,
                p.color.as_deref(),
                Self::caller_is_private(Some(&context)),
            )
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
        // Issue #56, decision (2b). A MODEL's export of a PRIVATE base is not
        // written where the model asked; it goes to `<knowledge-root>/.exports/`.
        //
        // ⚠ WHAT THIS IS AND IS NOT, because the plan's own wording has been
        // amended and the stale half is the appealing one. It is NOT a barrier.
        // The original argument — "`.exports/` is inside DR-14 deny root #2, so
        // the same kernel deny that hides the base hides the artifact" — depends
        // on a read-deny that DR-17 **descoped for v1**; AR-8 says so in as many
        // words ("Withdrawn: the claim that a model's export of a private base
        // lands somewhere a public session cannot read... Task 10A still forces
        // the export location, and it is now a provenance control rather than a
        // barrier"), and DR-17's accepted risk 4 names exports specifically. In
        // v1 nothing stops a public session's shell from reading this file, and
        // the tool reports the path it wrote.
        //
        // What it DOES buy, and why the rule stays: every model-made archive of
        // a private base lands in one known place, beside the base it came from
        // and inside the tree the user already treats as their knowledge store,
        // instead of scattered wherever a model chose. That is what makes the
        // whole set of them findable — by the user today, and by the read-deny
        // if DR-14 is ever un-descoped, with no change here. Keeping it also
        // keeps `.brkb` archives from being the one artifact whose location a
        // model picks, which is the shape the laundering path used.
        //
        // The directory name is a DOTFILE on purpose: see
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
        // would leave a complete copy of a private knowledge base at the path
        // the model chose for however long the copy takes.
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
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let bytes = std::fs::read(&p.src_path)
            .map_err(|e| into_err(anyhow::anyhow!("read .brkb '{}': {e}", p.src_path)))?;
        // Issue #56. The second of exactly TWO tools that take a
        // `RequestContext`: the new base's id is chosen by `brkb::import`'s
        // collision loop, so it is not knowable before the call. The importer's
        // tier and the archive's marker are a disjunction inside `import_brkb`
        // — the marker can only raise, never lower — and because the loop
        // always lands on a FRESH id, classifying there can never re-tier an
        // existing base.
        let new_id = self
            .service
            .import_brkb(&bytes, Self::caller_is_private(Some(&context)))
            .map_err(into_err)?;
        ok_json(&serde_json::json!({ "imported_kb_id": new_id }))
    }
}

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

    /// Verbatim what `#[tool_handler]` generated
    /// (`rmcp-macros-0.14.0/src/tool_handler.rs`); re-check that file when
    /// bumping rmcp.
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    /// Issue #56, design §9.3 B4 as ruled. ONE seam for all nineteen `kb_*`
    /// tools, including the EIGHT that take no `RequestContext` and therefore
    /// cannot learn the caller's capability inside their own body — among them
    /// `kb_write_page`, `kb_add_raw_source` and `kb_append_log`, i.e. every
    /// content-bearing write there is.
    ///
    /// This is `#[tool_handler]`'s generated body plus the gate: the last two
    /// statements are exactly what the macro emitted.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let caller_private = Self::caller_is_private(Some(&context));
        let name = request.name.to_string();

        if let Some(kb_id) = self.gated_kb_id(&name, request.arguments.as_ref(), Some(&context))? {
            // Task 10C adds `self.assert_kb_reachable(&kb_id, caller_private)?;`
            // HERE, on the line above the raise.
            if KB_RATCHETING_TOOLS.contains(&name.as_str()) {
                // BEFORE the write: a raise that only lands on success leaves
                // content in a base whose tier never moved if the write panics
                // or the process dies mid-commit. The failure direction of an
                // over-raise is a badge the user can see; the failure direction
                // of an under-raise is silent.
                self.service
                    .raise_tier(&kb_id, caller_private)
                    .map_err(into_err)?;
            }
        }

        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
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

        let written = reported_export_path(&kb_export_via_tool(&srv, "omop", None).await.unwrap());
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

    // ── Issue #56, Task 10B: the ratchet at CP1 ──────────────────────────────

    /// The capability a test drives a tool call with. An enum rather than a
    /// `bool` so the call sites read as `Private` / `Public` and cannot be
    /// transposed silently.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Caller {
        Public,
        Private,
    }
    use Caller::{Private, Public};

    impl Caller {
        fn is_private(self) -> bool {
            matches!(self, Caller::Private)
        }
    }

    fn migrated_server_with_bases(ids: &[&str]) -> (KnowledgeServer, tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let server = server_with_root(root.clone());
        for id in ids {
            server.service.create_base(id, id, None).unwrap();
        }
        (server, tmp, root)
    }

    /// Drive `KnowledgeServer::call_tool` BY NAME with a request whose meta
    /// carries the caller's capability — the only way to express "as a private
    /// caller" for the eight `kb_*` tools that take no `RequestContext` at all,
    /// and therefore the whole reason CP1 is a hand-written `call_tool` rather
    /// than a per-tool argument.
    ///
    /// A `RequestContext` needs a live `Peer`, which only `serve_directly`
    /// mints; the duplex transport is drained and dropped with the call. This
    /// mirrors `developer/rmcp_developer.rs`'s `create_test_transport`.
    async fn call_tool_as(
        srv: &KnowledgeServer,
        name: &str,
        args: serde_json::Value,
        caller: Caller,
    ) -> Result<CallToolResult, ErrorData> {
        use tokio::io::AsyncReadExt as _;

        let (mut client, server_side) = tokio::io::duplex(64 * 1024);
        tokio::spawn(async move {
            let mut buffer = [0_u8; 8192];
            while client.read(&mut buffer).await.unwrap_or(0) != 0 {}
        });
        let running = rmcp::service::serve_directly(srv.clone(), server_side, None);
        let mut meta = rmcp::model::Meta::new();
        meta.0.insert(
            crate::knowledge::tier::CAPABILITY_TIER_META_KEY.to_string(),
            serde_json::Value::String(
                crate::knowledge::tier::capability_meta_value(caller.is_private()).to_string(),
            ),
        );
        let context = RequestContext {
            ct: Default::default(),
            id: rmcp::model::NumberOrString::Number(1),
            meta,
            extensions: Default::default(),
            peer: running.peer().clone(),
        };
        let request = rmcp::model::CallToolRequestParams {
            name: name.to_string().into(),
            arguments: args.as_object().cloned(),
            task: None,
            meta: None,
        };
        let out = ServerHandler::call_tool(srv, request, context).await;
        drop(running);
        out
    }

    /// A `.brkb` archive of a base whose tier is `tier`, written to a file and
    /// returned with the `TempDir` that owns it — bind BOTH or the path is
    /// unlinked before the import reads it.
    fn brkb_fixture(tier: Caller) -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let src_root = tmp.path().join("src-root");
        std::fs::create_dir_all(&src_root).unwrap();
        let svc = KnowledgeService::new(src_root.clone());
        svc.create_base("shipped", "Shipped", None).unwrap();
        seed_page(&src_root, "shipped", "knowledge/x.md", "SENTINEL");
        if tier.is_private() {
            crate::knowledge::tier::raise_unlocked(&src_root, "shipped", true).unwrap();
        }
        let bytes = svc.export_brkb("shipped").unwrap();
        let path = tmp.path().join("shipped.brkb");
        std::fs::write(&path, &bytes).unwrap();
        (tmp, path.to_string_lossy().to_string())
    }

    fn imported_kb_id(out: &CallToolResult) -> String {
        let text = out
            .content
            .iter()
            .find_map(|c| c.as_text())
            .expect("kb_import returns a text payload");
        let v: serde_json::Value = serde_json::from_str(&text.text).expect("valid json");
        v["imported_kb_id"]
            .as_str()
            .expect("an imported id")
            .to_string()
    }

    #[tokio::test]
    async fn a_private_session_writing_one_page_ratchets_the_whole_base() {
        // THE test for the ruling: one page from one private chat privatises the
        // machine-wide base.
        let (srv, _tmp, root) = migrated_server_with_bases(&["default"]);
        call_tool_as(
            &srv,
            "kb_write_page",
            serde_json::json!({
                "kb_id": "default", "path": "knowledge/omop.md",
                "content": "n=412 T2D patients", "commit_message": "x"
            }),
            Private,
        )
        .await
        .unwrap();
        assert!(crate::knowledge::tier::is_private(&root, "default"));
    }

    #[tokio::test]
    async fn a_public_session_writing_never_lowers_a_ratcheted_base() {
        let (srv, _tmp, root) = migrated_server_with_bases(&["default"]);
        crate::knowledge::tier::raise_unlocked(&root, "default", true).unwrap();
        // Task 10C has not landed, so this write still SUCCEEDS. What must not
        // happen is the tier moving.
        call_tool_as(
            &srv,
            "kb_append_log",
            serde_json::json!({ "kb_id": "default", "kind": "manual", "summary": "hi" }),
            Public,
        )
        .await
        .unwrap();
        assert!(
            crate::knowledge::tier::is_private(&root, "default"),
            "a public write lowered the tier"
        );
    }

    /// name, arguments against the base "default", and whether the call must
    /// leave "default" private when made by a private caller.
    struct ToolProbe {
        name: &'static str,
        args: fn() -> serde_json::Value,
        ratchets: bool,
    }

    /// All nineteen `kb_*` tools. The exclusion list as data, reviewable in one
    /// place:
    ///   ratchets "default":      kb_write_page, kb_add_raw_source, kb_append_log
    ///   ratchets its OWN new id: kb_create_base, kb_import
    ///   does not ratchet:        the other fourteen
    const KB_TOOL_PROBES: &[ToolProbe] = &[
        ToolProbe {
            name: "kb_list_bases",
            args: || serde_json::json!({}),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_list_pages",
            args: || serde_json::json!({ "kb_id": "default" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_read_page",
            args: || serde_json::json!({ "kb_id": "default", "path": "index.md" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_write_page",
            args: || {
                serde_json::json!({
                    "kb_id": "default", "path": "knowledge/p.md",
                    "content": "body", "commit_message": "m"
                })
            },
            ratchets: true,
        },
        ToolProbe {
            name: "kb_add_raw_source",
            args: || {
                serde_json::json!({
                    "kb_id": "default",
                    "source": { "kind": "text", "text": "n=412", "title": "note" }
                })
            },
            ratchets: true,
        },
        ToolProbe {
            name: "kb_append_log",
            args: || serde_json::json!({ "kb_id": "default", "kind": "manual", "summary": "s" }),
            ratchets: true,
        },
        ToolProbe {
            name: "kb_get_graph",
            args: || serde_json::json!({ "kb_id": "default" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_list_history",
            args: || serde_json::json!({ "kb_id": "default" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_restore_state",
            args: || serde_json::json!({ "kb_id": "default", "commit_sha": "HEAD" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_begin_txn",
            args: || serde_json::json!({ "kb_id": "default", "label": "t" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_commit_txn",
            args: || {
                serde_json::json!({
                    "kb_id": "default", "txn": "txn/t", "kind": "manual", "summary": "s"
                })
            },
            ratchets: false,
        },
        ToolProbe {
            name: "kb_abort_txn",
            args: || serde_json::json!({ "kb_id": "default", "txn": "txn/t" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_search",
            args: || serde_json::json!({ "kb_id": "default", "query": "n" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_search_raw_sources",
            args: || serde_json::json!({ "kb_id": "default", "query": "n" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_set_active",
            args: || serde_json::json!({ "kb_id": "default" }),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_get_active",
            args: || serde_json::json!({}),
            ratchets: false,
        },
        ToolProbe {
            name: "kb_export",
            args: || serde_json::json!({ "kb_id": "default" }),
            ratchets: false,
        },
    ];

    #[tokio::test]
    async fn every_tool_that_writes_content_ratchets_and_the_plumbing_ones_do_not() {
        // Parameterised over the seventeen `default`-addressing tools, driven
        // through `call_tool` BY NAME — which is the point of CP1: eight of them
        // take no `RequestContext`, so a test that calls the `#[tool]` fn
        // directly cannot express "as a private caller" for them at all. A test
        // on kb_write_page alone passes an implementation that misses
        // kb_add_raw_source — the tool the GUI ingest panel and the `ingest`
        // macro actually call — so the whole ingest path would launder.
        //
        // `kb_create_base` and `kb_import` are the other two of the nineteen;
        // they ratchet their OWN new id and have their own tests below.
        for probe in KB_TOOL_PROBES {
            let (srv, _tmp, root) = migrated_server_with_bases(&["default"]);
            let _ = call_tool_as(&srv, probe.name, (probe.args)(), Private).await;
            assert_eq!(
                crate::knowledge::tier::is_private(&root, "default"),
                probe.ratchets,
                "{} ratchets={} but the store says otherwise",
                probe.name,
                probe.ratchets
            );
        }
    }

    #[tokio::test]
    async fn a_base_created_from_a_private_chat_is_born_private() {
        let (srv, _tmp, root) = migrated_server_with_bases(&["default"]);
        call_tool_as(
            &srv,
            "kb_create_base",
            serde_json::json!({ "id": "omop", "name": "OMOP" }),
            Private,
        )
        .await
        .unwrap();
        assert!(crate::knowledge::tier::is_private(&root, "omop"));
        assert!(
            !crate::knowledge::tier::is_private(&root, "default"),
            "creating one base moved another"
        );
    }

    #[tokio::test]
    async fn a_public_chat_can_still_create_and_import_a_knowledge_base() {
        // The regression the sixteen-site enumeration encoded, as a test. A
        // public session must be able to make its own base; `assert_reachable`
        // permits a kb id with no directory on disk (Task 10A, decision 3).
        let (srv, _tmp, root) = migrated_server_with_bases(&["default"]);
        call_tool_as(
            &srv,
            "kb_create_base",
            serde_json::json!({ "id": "notes", "name": "Notes" }),
            Public,
        )
        .await
        .unwrap();
        assert!(!crate::knowledge::tier::is_private(&root, "notes"));

        let (_fx, path) = brkb_fixture(Public);
        let out = call_tool_as(
            &srv,
            "kb_import",
            serde_json::json!({ "src_path": path }),
            Public,
        )
        .await
        .unwrap();
        assert!(!crate::knowledge::tier::is_private(
            &root,
            &imported_kb_id(&out)
        ));
    }

    #[tokio::test]
    async fn an_imported_base_takes_the_importing_sessions_tier_or_the_archives_floor() {
        // `brkb::import` resolves collisions by suffixing, so an import always
        // lands on a FRESH id — which is what makes stamping after the call safe.
        let (srv, _tmp, root) = migrated_server_with_bases(&["default"]);
        let (_fx, public_path) = brkb_fixture(Public);
        let out = call_tool_as(
            &srv,
            "kb_import",
            serde_json::json!({ "src_path": public_path }),
            Private,
        )
        .await
        .unwrap();
        assert!(crate::knowledge::tier::is_private(
            &root,
            &imported_kb_id(&out)
        ));

        // ⚠ The line above is the SAFE direction and, on its own, it is what let
        // export-private / import-public through a whole review round: a private
        // importer privatising what it imports proves nothing about a public one.
        // The unsafe direction is Task 10A's
        // `a_private_export_cannot_be_laundered_by_importing_it_into_a_public_chat`;
        // this is its tool-level twin, so the bypass is closed at the surface a
        // model actually calls and not only in the store.
        let (_fx2, private_path) = brkb_fixture(Private);
        let out = call_tool_as(
            &srv,
            "kb_import",
            serde_json::json!({ "src_path": private_path }),
            Public,
        )
        .await
        .unwrap();
        assert!(
            crate::knowledge::tier::is_private(&root, &imported_kb_id(&out)),
            "a public chat imported a private base's archive and got a public base"
        );
    }
}
