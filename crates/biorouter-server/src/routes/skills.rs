//! `/skills/*` — the canonical skill catalog, and per-conversation skill state.
//!
//! # Why these routes exist (#113)
//!
//! The composer's skill menu used to answer both questions itself. It walked
//! the filesystem over its own list of three roots, and on a per-chat toggle it
//! wrote a `Map` in React state and raised a toast saying the skill was
//! "enabled for this chat". Nothing left the renderer: no request, no
//! `extension_data` write, no catalog refresh, no live agent. The switch moved,
//! the toast was green, and the next turn saw exactly the skills it saw before.
//!
//! A success toast that reports intent rather than confirmed state is worse
//! than no control at all, so the composer now goes through here, and the
//! handler's answer — not the click — is what moves the switch.
//!
//! # The three properties these handlers are built around
//!
//! **One catalog, served, not re-derived.** [`biorouter::agents::skill_catalog`]
//! enumerates the roots (all seven kinds, including
//! `~/.config/biorouter/extensions/<name>/skills`, which the renderer's own
//! scanner omitted), discovers the skills, and composes machine-wide with
//! per-session state. This route hands that over verbatim. There is no second
//! interpretation on the interface side; that separation was root cause 2.
//!
//! **A mutation is persisted before it is reported.**
//! [`biorouter::agents::session_skills::apply`] writes
//! `workspace_skills/v1` inside one transaction, and only then is a catalog
//! read back and returned. A failed write is a non-2xx with a message, and the
//! interface restores the switch from it — see `useSkillCatalog.ts`.
//!
//! **The write is never `skills-config.json`.** That file is the machine-wide
//! preference shared with `biorouter skill enable/disable` and every other
//! window; a per-chat toggle that touched it would change every other
//! conversation. The scoping rule is stated once, in `session_skills.rs`, and
//! this route is bound by it.
//!
//! # Why there is no `X-User-Action` gate
//!
//! Proof-of-user exists for raises the *model* must not perform on its own
//! (privacy Gate C's cross-affiliation grant). Enabling a skill in your own
//! chat is not one: it grants nothing the machine-wide catalog did not already
//! contain, it is scoped to a single conversation, and the model already has a
//! sanctioned route to the same state through `workspace_set_tools`. Requiring
//! the proof would also break browser access outright — `biorouter serve`
//! spawns the daemon with `Stdio::null()`, so no digest is ever installed
//! (`docs/deployment/serve-decisions.md`, SD-1). The secret-key middleware
//! guards these routes exactly as it guards `/config` and `/sessions`.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use biorouter::agents::session_skills::{self, SessionSkillOverride};
use biorouter::agents::skill_catalog::{self, CatalogView};
use biorouter::session::SessionManager;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SkillCatalogQuery {
    /// Compose the machine-wide catalog with this conversation's override.
    /// Omit for the machine-wide view — what a new chat would start with.
    pub session_id: Option<String>,
    /// Rescan the filesystem before answering, instead of reusing the cached
    /// snapshot. The interface sets this after an install it did not perform
    /// itself (a marketplace click, a `.brxt` drop) and after a `CatalogChanged`
    /// notice, since a change made by another process may land inside the
    /// snapshot's one-second mtime window.
    #[serde(default)]
    pub refresh: bool,
}

/// Add and remove are applied to **this** conversation only.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionSkillsRequest {
    pub session_id: String,
    /// Skill (or bundle) names to enable for this conversation, even where the
    /// machine-wide preference has them off.
    #[serde(default)]
    pub add: Vec<String>,
    /// Skill (or bundle) names to disable for this conversation, even where the
    /// machine-wide preference has them on.
    #[serde(default)]
    pub remove: Vec<String>,
}

/// The catalog after the write, plus the override that produced it.
///
/// Returning the whole catalog rather than an acknowledgement is deliberate: it
/// is what lets the interface render confirmed state instead of the optimistic
/// state it just guessed, and it collapses the toggle-then-refetch race that
/// two concurrent toggles would otherwise lose.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionSkillsResponse {
    pub catalog: CatalogView,
    /// The persisted `workspace_skills/v1` value, echoed so a caller can tell a
    /// per-chat deviation from a machine-wide default without re-deriving it.
    pub session_add: Vec<String>,
    pub session_remove: Vec<String>,
}

async fn override_for(
    session_manager: &SessionManager,
    session_id: Option<&str>,
) -> Result<SessionSkillOverride, (StatusCode, String)> {
    let Some(session_id) = session_id else {
        return Ok(SessionSkillOverride::default());
    };
    session_skills::for_session(session_manager, session_id)
        .await
        .map_err(|e| {
            // Fail CLOSED, exactly as the extension's dispatch does: answering
            // from the machine-wide view would show every skill this
            // conversation had revoked as enabled.
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not read this conversation's skill state: {e:#}"),
            )
        })
}

/// The canonical catalog: every root, every skill, every bundle, and the
/// composed enablement for one conversation.
#[utoipa::path(
    get,
    path = "/skills/catalog",
    params(SkillCatalogQuery),
    responses(
        (status = 200, description = "The catalog", body = CatalogView),
        (status = 401, description = "Unauthorized - invalid or missing secret key"),
        (status = 500, description = "This conversation's skill state is unreadable"),
    ),
    security(("api_key" = [])),
    tag = "skills",
)]
pub async fn skill_catalog_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SkillCatalogQuery>,
) -> Result<Json<CatalogView>, (StatusCode, String)> {
    let over = override_for(state.session_manager(), query.session_id.as_deref()).await?;
    let catalog = if query.refresh {
        skill_catalog::refresh()
    } else {
        skill_catalog::current()
    };
    Ok(Json(catalog.view(&over)))
}

/// Enable or disable skills for one conversation, and hand back what the model
/// will see on its next turn.
#[utoipa::path(
    post,
    path = "/skills/session",
    request_body = SessionSkillsRequest,
    responses(
        (status = 200, description = "Applied", body = SessionSkillsResponse),
        (status = 401, description = "Unauthorized - invalid or missing secret key"),
        (status = 404, description = "No such conversation"),
        (status = 500, description = "The override could not be persisted"),
    ),
    security(("api_key" = [])),
    tag = "skills",
)]
pub async fn set_session_skills(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SessionSkillsRequest>,
) -> Result<Json<SessionSkillsResponse>, (StatusCode, String)> {
    if request.add.is_empty() && request.remove.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "name at least one skill to add or remove".to_string(),
        ));
    }

    let over = session_skills::apply(
        state.session_manager(),
        &request.session_id,
        &request.add,
        &request.remove,
    )
    .await
    .map_err(|e| {
        // `apply` reports a missing session as an error rather than a silent
        // grant, and that case is the caller's mistake, not the daemon's.
        let missing = format!("{e:#}").contains("not found");
        let status = if missing {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, format!("{e:#}"))
    })?;

    // Read the catalog only after the write committed, so what comes back is
    // confirmed state rather than the optimistic answer the caller already has.
    let catalog = skill_catalog::current();
    Ok(Json(SessionSkillsResponse {
        catalog: catalog.view(&over),
        session_add: over.add,
        session_remove: over.remove,
    }))
}

/// Rescan the filesystem and publish a new catalog to every live conversation.
///
/// The endpoint an install calls — a marketplace skill, a dropped `.zip`, a
/// `.brxt` extension whose `skills/` subdirectory becomes a new root, or
/// worktree 4's `CatalogChanged` notice. It rescans unconditionally rather than
/// consulting mtimes, because the caller already knows something changed.
#[utoipa::path(
    post,
    path = "/skills/refresh",
    params(SkillCatalogQuery),
    responses(
        (status = 200, description = "The freshly scanned catalog", body = CatalogView),
        (status = 401, description = "Unauthorized - invalid or missing secret key"),
        (status = 500, description = "This conversation's skill state is unreadable"),
    ),
    security(("api_key" = [])),
    tag = "skills",
)]
pub async fn refresh_skill_catalog(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SkillCatalogQuery>,
) -> Result<Json<CatalogView>, (StatusCode, String)> {
    let over = override_for(state.session_manager(), query.session_id.as_deref()).await?;
    Ok(Json(skill_catalog::refresh().view(&over)))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/skills/catalog", get(skill_catalog_handler))
        .route("/skills/session", post(set_session_skills))
        .route("/skills/refresh", post(refresh_skill_catalog))
        .with_state(state)
}

#[cfg(test)]
mod tests {

    /// The per-chat write must never reach the machine-wide preference file.
    /// Stated as a source assertion because the handler's own body is where the
    /// mistake would be made, and a behavioural test would need the developer's
    /// real `~/.config/biorouter` to prove a negative about it.
    #[test]
    fn the_session_handler_writes_only_the_session_row() {
        let src = include_str!("skills.rs");
        let body = crate::routes::body_of(
            src,
            "async fn set_session_skills(\n    State(state): State<Arc<AppState>>,",
        );
        assert!(
            body.contains("session_skills::apply("),
            "the persisted write is `session_skills::apply`"
        );
        assert!(
            !body.contains("skills-config.json"),
            "a per-chat toggle must not touch the machine-wide preference file"
        );
        // Negative control: the extractor stopped inside this handler and did
        // not run on to the refresh handler next door.
        assert!(!body.contains("fn refresh_skill_catalog"));
    }

    /// Reading the catalog back *after* the write is what makes the response
    /// confirmed state rather than the caller's optimistic guess.
    #[test]
    fn the_catalog_is_read_after_the_write_not_before() {
        let src = include_str!("skills.rs");
        let body = crate::routes::body_of(
            src,
            "async fn set_session_skills(\n    State(state): State<Arc<AppState>>,",
        );
        let write = body.find("session_skills::apply(").expect("write present");
        let read = body
            .find("let catalog = skill_catalog::current();")
            .expect("read present");
        assert!(write < read, "the write must precede the read");
    }

    /// `refresh` rescans; `catalog` may reuse the cached snapshot. Swapping
    /// them would make the post-install refresh a no-op that looks like one.
    #[test]
    fn refresh_rescans_and_the_plain_read_does_not_have_to() {
        let src = include_str!("skills.rs");
        let refresh = crate::routes::body_of(
            src,
            "async fn refresh_skill_catalog(\n    State(state): State<Arc<AppState>>,",
        );
        assert!(refresh.contains("skill_catalog::refresh()"));
        assert!(!refresh.contains("skill_catalog::current()"));
    }
}
