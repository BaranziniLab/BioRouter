use crate::knowledge::{convert::SourceInput, service::KnowledgeService};
use anyhow::Result;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};

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

#[tool_router(router = tool_router)]
impl KnowledgeServer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            tool_router: Self::tool_router(),
            service: KnowledgeService::new_default()?,
            instructions: include_str!("instructions.md").to_string(),
        })
    }

    #[tool(
        name = "kb_list_bases",
        description = "List all knowledge bases on this machine."
    )]
    pub async fn kb_list_bases(&self) -> Result<CallToolResult, ErrorData> {
        let bases = self.service.list_bases().map_err(into_err)?;
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
        description = "List wiki pages in a knowledge base."
    )]
    pub async fn kb_list_pages(
        &self,
        p: Parameters<ListPagesParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let pages = crate::knowledge::store::list_pages(&kb_root, p.path_prefix.as_deref())
            .map_err(into_err)?;
        ok_json(&pages)
    }

    #[tool(
        name = "kb_read_page",
        description = "Read a single wiki page by path."
    )]
    pub async fn kb_read_page(
        &self,
        p: Parameters<ReadPageParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let kb_root = crate::knowledge::paths::kb_root(self.service.root(), &p.kb_id);
        let page = crate::knowledge::store::read_page(&kb_root, &p.path).map_err(into_err)?;
        ok_json(&page)
    }

    #[tool(
        name = "kb_write_page",
        description = "Create or overwrite a wiki page and commit."
    )]
    pub async fn kb_write_page(
        &self,
        p: Parameters<WritePageParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
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
        description = "Return the cached node+edge graph for a knowledge base."
    )]
    pub async fn kb_get_graph(
        &self,
        p: Parameters<KbIdParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let g = self.service.get_graph(&p.kb_id).map_err(into_err)?;
        ok_json(&g)
    }

    #[tool(
        name = "kb_list_history",
        description = "List recent change-log entries from the git history."
    )]
    pub async fn kb_list_history(
        &self,
        p: Parameters<HistoryParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let p = p.0;
        let h = self
            .service
            .list_history(&p.kb_id, p.limit)
            .map_err(into_err)?;
        ok_json(&h)
    }

    #[tool(
        name = "kb_restore_state",
        description = "Restore the wiki to a previous commit by creating a new commit on top of HEAD."
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
