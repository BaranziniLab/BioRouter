//! Whether the coding-agent CLIs are installed and signed in.
//!
//! The settings card needs three facts per provider — is the binary there, which
//! version, and is the user signed in with a *subscription* rather than an API key —
//! and none of them can be answered from configuration alone. Each requires
//! spawning the CLI or reading its credential store, which is exactly why they are
//! not answered by the provider's `from_env`: `GET /config/providers` constructs
//! every configured provider under a three-second timeout to sample its tier, and a
//! probe there would stall the whole settings page.
//!
//! Nothing here reads or returns a credential. The probe asks the CLI what it thinks
//! its own state is; the token stays between the user and the vendor.

use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use biorouter::providers::coding_agent::discovery::{self, AgentAvailability};

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CodingAgentStatusResponse {
    pub agents: Vec<AgentAvailability>,
}

#[utoipa::path(
    get,
    path = "/coding_agents/status",
    responses(
        (status = 200, description = "Install and sign-in state for each coding-agent CLI", body = CodingAgentStatusResponse)
    ),
)]
async fn coding_agents_status() -> Json<CodingAgentStatusResponse> {
    Json(CodingAgentStatusResponse {
        agents: discovery::probe_all().await,
    })
}

pub fn routes(state: std::sync::Arc<crate::state::AppState>) -> Router {
    Router::new()
        .route("/coding_agents/status", get(coding_agents_status))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    /// The route answers on a machine with neither CLI installed, and answers for
    /// **both** providers rather than omitting the missing one — the card needs a
    /// row to render "not installed" into.
    #[tokio::test]
    async fn status_reports_every_agent_even_when_none_is_installed() {
        let app = Router::new().route("/coding_agents/status", get(coding_agents_status));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/coding_agents/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let agents = value["agents"].as_array().expect("agents is an array");
        assert_eq!(agents.len(), 2, "both providers must always be reported");

        let ids: Vec<&str> = agents
            .iter()
            .map(|a| a["providerId"].as_str().unwrap_or_default())
            .collect();
        assert!(
            ids.contains(&"claude_code") && ids.contains(&"codex"),
            "{ids:?}"
        );

        for agent in agents {
            // The card switches on this, so it must always be present and tagged.
            assert!(
                agent["auth"]["state"].is_string(),
                "every agent needs an auth state: {agent}"
            );
            // The two remediation strings are what the card renders; an empty one
            // would leave the user with nothing to do.
            assert!(!agent["loginCommand"]
                .as_str()
                .unwrap_or_default()
                .is_empty());
            assert!(!agent["installHint"].as_str().unwrap_or_default().is_empty());
        }
    }

    /// No field of the response may carry a credential. Asserted on the serialised
    /// body rather than by reading the struct, because the wire form is what leaves
    /// the process.
    #[tokio::test]
    async fn the_response_never_carries_a_token() {
        let agents = discovery::probe_all().await;
        let body = serde_json::to_string(&CodingAgentStatusResponse { agents }).unwrap();
        let lowered = body.to_lowercase();
        for forbidden in [
            "access_token",
            "refresh_token",
            "id_token",
            "bearer",
            "api_key",
        ] {
            assert!(
                !lowered.contains(forbidden),
                "the status response must never carry `{forbidden}`: {body}"
            );
        }
    }
}
