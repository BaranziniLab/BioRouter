//! **The first-crossing disclosure, end to end** (issue #56, design §7's `✓!`
//! cells): a private-capability conversation writing into a PUBLIC one shows the
//! user the exact payload before it is sent, once per (caller, target) pair.
//!
//! # Why this is an integration binary and not a unit test
//!
//! Two of the three inputs are process-global. `WorkspaceCrossingInspector`
//! resolves the target through `SessionManager::instance()` — the real store,
//! which a unit test in `workspace_extension`'s own module deliberately does not
//! use — and `privacy::crossing`'s ledger is a process-global set. So the test
//! needs a whole process it can point at a temp directory, which is what a
//! separate test binary is.
//!
//! ⚠ **`BIOROUTER_PATH_ROOT` must be set before ANYTHING touches the store.**
//! `SESSION_STORAGE` is a `LazyLock` over `Paths::data_dir()`; once it has been
//! forced, this test would be reading and writing the developer's own
//! `sessions.db`. It is set at the top of the one test in this file, and the
//! file has exactly one test for that reason.

use std::sync::Arc;

use async_trait::async_trait;
use biorouter::agents::workspace_inspector::WorkspaceCrossingInspector;
use biorouter::config::BioRouterMode;
use biorouter::conversation::message::{Message, ToolRequest};
use biorouter::model::ModelConfig;
use biorouter::privacy::{ProviderTier, SessionClassification};
use biorouter::providers::base::{Provider, ProviderMetadata, ProviderUsage};
use biorouter::providers::errors::ProviderError;
use biorouter::session::session_manager::{SessionManager, SessionType};
use biorouter::tool_inspection::{InspectionAction, ToolInspector};
use rmcp::model::Tool;

/// A provider that exists only to answer `tier()`. Named after a real private
/// provider so nothing downstream can decide it is public by name.
struct InstitutionalModel;

#[async_trait]
impl Provider for InstitutionalModel {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::empty()
    }
    fn get_name(&self) -> &str {
        "versa_azure"
    }
    fn tier(&self) -> ProviderTier {
        ProviderTier::Private
    }
    fn get_model_config(&self) -> ModelConfig {
        ModelConfig::new_or_fail("test-model")
    }
    async fn complete_with_model(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        unreachable!("this fixture exists only to be asked its tier")
    }
}

fn send_prompt_request(id: &str, target: &str, text: &str) -> ToolRequest {
    ToolRequest {
        id: id.to_string(),
        tool_call: Ok(rmcp::model::CallToolRequestParams {
            name: "workspace_send_prompt".into(),
            arguments: Some(
                serde_json::json!({ "session_id": target, "mode": "note", "text": text })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            meta: None,
            task: None,
        }),
        metadata: Default::default(),
        tool_meta: Default::default(),
    }
}

#[tokio::test]
async fn a_private_chat_writing_into_a_public_one_discloses_its_payload_exactly_once() {
    let root = tempfile::TempDir::new().unwrap();
    // SAFETY: single-threaded prelude of a test binary whose only test this is;
    // nothing else in this process has read an env var yet.
    unsafe {
        std::env::set_var("BIOROUTER_PATH_ROOT", root.path());
    }

    const PAYLOAD: &str = "the MS cohort's 2019 relapse counts, verbatim";

    let sm = SessionManager::instance();
    let caller = sm
        .create_session(
            root.path().to_path_buf(),
            "private lead".into(),
            SessionType::User,
        )
        .await
        .unwrap();
    let public_target = sm
        .create_session(
            root.path().to_path_buf(),
            "public worker".into(),
            SessionType::User,
        )
        .await
        .unwrap();
    let private_target = sm
        .create_session(
            root.path().to_path_buf(),
            "private peer".into(),
            SessionType::User,
        )
        .await
        .unwrap();
    sm.update(&private_target.id)
        .raise_privacy(
            SessionClassification::Private,
            "test:workspace-crossing-disclosure",
        )
        .apply()
        .await
        .unwrap();
    // The ratchet really fired. Without this the "same tier, no disclosure"
    // assertion below would pass against a target that is merely public.
    assert_eq!(
        sm.get_session(&private_target.id, false)
            .await
            .unwrap()
            .privacy_tier,
        SessionClassification::Private,
    );

    let provider: biorouter::agents::types::SharedProvider =
        Arc::new(tokio::sync::Mutex::new(Some(Arc::new(InstitutionalModel))));
    let inspector = WorkspaceCrossingInspector::new(provider);

    let caller_session = sm.get_session(&caller.id, false).await.unwrap();
    let inspect = |request: ToolRequest| {
        let inspector = &inspector;
        let session = caller_session.clone();
        async move {
            inspector
                .inspect(&[request], &[], BioRouterMode::Auto, &session)
                .await
                .unwrap()
        }
    };

    // 1. The crossing. Private caller, public target, first time.
    let results = inspect(send_prompt_request("req-1", &public_target.id, PAYLOAD)).await;
    assert_eq!(results.len(), 1, "no disclosure was raised: {results:?}");
    let prompt = match &results[0].action {
        InspectionAction::RequireApproval(Some(prompt)) => prompt.clone(),
        other => panic!("the disclosure must be a RequireApproval carrying text, got {other:?}"),
    };
    // The payload, VERBATIM. A card that summarised what was about to be sent
    // would leave the user approving something they cannot check — and the whole
    // point of this gate is that they can.
    assert!(
        prompt.contains(PAYLOAD),
        "the approval did not show the payload: {prompt}"
    );
    assert!(
        prompt.contains(&public_target.id),
        "the approval did not name the target: {prompt}"
    );

    // 2. Asking again — with no write having landed — asks again. This is the
    //    denial case: a user who says no must be asked on the retry, not
    //    silently obeyed.
    let again = inspect(send_prompt_request("req-2", &public_target.id, PAYLOAD)).await;
    assert_eq!(
        again.len(),
        1,
        "a second attempt was let through, so denying the first would have bought \
         silence for it"
    );

    // 3. Once the write has landed, the pair has crossed and stops asking.
    biorouter::privacy::crossing::record(&caller.id, &public_target.id);
    let after = inspect(send_prompt_request("req-3", &public_target.id, PAYLOAD)).await;
    assert!(
        after.is_empty(),
        "the disclosure repeated for a pair that has already crossed: {after:?}"
    );

    // 4. A DIFFERENT public target is a different crossing. Keying the ledger on
    //    the caller alone would let one approval cover every public conversation
    //    on the machine.
    let other_public = sm
        .create_session(
            root.path().to_path_buf(),
            "another worker".into(),
            SessionType::User,
        )
        .await
        .unwrap();
    let second = inspect(send_prompt_request("req-4", &other_public.id, PAYLOAD)).await;
    assert_eq!(second.len(), 1, "a second public target was not disclosed");

    // 5. A same-tier write crosses nothing, so it discloses nothing. This is the
    //    control that keeps the assertions above from being "the inspector fires
    //    for everything".
    let same_tier = inspect(send_prompt_request("req-5", &private_target.id, PAYLOAD)).await;
    assert!(
        same_tier.is_empty(),
        "a private→private write raised a crossing disclosure: {same_tier:?}"
    );
}
