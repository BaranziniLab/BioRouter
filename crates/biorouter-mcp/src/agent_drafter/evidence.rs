//! `report_evidence` — a worker's verdict, made machine-readable.
//!
//! The worst failure in the 100-app test drive: a worker profile ("Fine Mapper")
//! reported that posterior inclusion probabilities *were not defensible without
//! summary statistics, an LD reference, and harmonization*. The main agent read
//! that, and then **invented five PIPs that summed to 1.0** and shaded them onto
//! the page as a credible set. They rendered identically to computed ones.
//!
//! Nothing in the platform could have stopped it:
//!
//!   * `consult` returns free prose, so to the server that refusal was an ordinary
//!     paragraph — there was no representation of "the evidence is missing";
//!   * `app_call` validated arguments **shape-only**, and five plausible floats
//!     satisfy any schema an author would realistically write; and
//!   * no field anywhere recorded where a number came from, so the UI could not
//!     tell a computed credible set from an invented one.
//!
//! The model *read* the refusal and proceeded anyway. That is the strongest
//! possible evidence that no amount of prompt text fixes this.
//!
//! This module supplies the missing representation. Workers — and **only**
//! workers — carry `report_evidence`; the main agent cannot write its own alibi.
//! The verdict lands in the bridge's per-turn evidence ledger, which `app_call`
//! consults before letting an action that declares `requires_evidence` publish
//! anything. A synthetic demo is still allowed, but it is labelled.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ErrorCode, ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::control::{EvidenceEntry, EvidenceStatus, UiBridge};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReportEvidenceParams {
    /// `"ok"` — you had the inputs you needed.
    /// `"insufficient_data"` — you did NOT, and any quantitative answer would be
    /// guesswork. `"error"` — you failed for some other reason.
    pub status: String,
    /// The named inputs you did not have, e.g. `["sumstats", "ld_reference"]`.
    /// Required when `status` is `insufficient_data`: the main agent's action may
    /// declare that it depends on exactly these, and naming them is what blocks it
    /// from publishing invented values in their place.
    #[serde(default)]
    pub missing: Vec<String>,
    /// What you concluded, in prose. Returned to the main agent as usual.
    #[serde(default)]
    pub findings: String,
}

/// An in-process MCP server carrying the single `report_evidence` tool, injected
/// into every WORKER agent.
#[derive(Clone)]
pub struct EvidenceServer {
    tool_router: ToolRouter<Self>,
    bridge: UiBridge,
    /// The manifest key of the profile this server belongs to. Stamped onto every
    /// verdict, so the main agent's refusal can name who said the data was missing.
    profile: String,
}

#[tool_router(router = tool_router)]
impl EvidenceServer {
    pub fn new(bridge: UiBridge, profile: impl Into<String>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            bridge,
            profile: profile.into(),
        }
    }

    #[tool(
        name = "report_evidence",
        description = "Record — as a MACHINE-READABLE verdict, not just prose — whether you \
                       actually had the inputs needed to answer. Call this BEFORE you return, \
                       every time. If the honest answer is that the data was insufficient, say \
                       so here with `status: \"insufficient_data\"` and name what was missing. \
                       This is load-bearing: the main agent's page-publishing actions can declare \
                       that they depend on those inputs, and your verdict is what stops it from \
                       inventing numbers to fill the gap. Prose alone cannot do that — it has \
                       been read and ignored."
    )]
    pub async fn report_evidence(
        &self,
        Parameters(p): Parameters<ReportEvidenceParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let status = EvidenceStatus::parse(&p.status).ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "status must be \"ok\", \"insufficient_data\", or \"error\"; got \"{}\"",
                    p.status
                ),
                None,
            )
        })?;

        let missing: Vec<String> = p
            .missing
            .into_iter()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect();

        if status == EvidenceStatus::InsufficientData && missing.is_empty() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "reporting `insufficient_data` requires naming what was missing (e.g. \
                 `missing: [\"sumstats\", \"ld_reference\"]`). The names are what the main \
                 agent's actions check against — an unnamed gap blocks nothing."
                    .to_string(),
                None,
            ));
        }

        self.bridge.record_evidence(EvidenceEntry {
            profile: self.profile.clone(),
            status,
            missing: missing.clone(),
        });

        let note = match status {
            EvidenceStatus::Ok => "Recorded: you had the inputs you needed.".to_string(),
            EvidenceStatus::InsufficientData => format!(
                "Recorded: insufficient data (missing: {}). Any action the main agent declares as \
                 depending on these will now REFUSE to publish computed-looking values.",
                missing.join(", ")
            ),
            EvidenceStatus::Error => "Recorded: you failed for another reason.".to_string(),
        };

        Ok(CallToolResult::success(vec![Content::text(note)]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EvidenceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Before you return your answer, call `report_evidence` to state whether you \
                 actually had the inputs you needed. If you did not, say so and name what was \
                 missing — that verdict is what prevents the main agent from inventing the \
                 numbers you declined to produce."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
