//! Consent gate for **deleting a knowledge base** (`kb_delete_base`).
//!
//! # Why the tool cannot ask for itself
//!
//! `kb_delete_base` lives in `biorouter-mcp`, which sits *below* this crate in
//! the dependency graph and therefore cannot reach [`crate::pending_user_action`]
//! — the channel `remove_extension` and `removeSkillPackage` use to put a
//! destructive operation to the user and wait for an answer. An in-process MCP
//! server has no way to raise a card; the most it can do is name the
//! consequences in its result text, which is a description of what already
//! happened.
//!
//! So the ask lives here, as a [`ToolInspector`], for the same reason
//! [`crate::security::global_memory`]'s does: the escalation-only merge in
//! [`crate::tool_inspection`] applies this verdict *over* whatever the
//! permission inspector decided, and the merge can raise a verdict but never
//! lower one. That is what makes it survive
//! [`BioRouterMode::Auto`](crate::config::BioRouterMode::Auto) — where the
//! permission inspector approves every tool deterministically — and a user
//! `AlwaysAllow`, and a `SmartApprove` read-only grade.
//!
//! # Why it asks in every mode
//!
//! The tool is annotated `destructive_hint: true`, so
//! [`crate::permission::tool_risk`] already grades it `High` and `SmartApprove`
//! would confirm it. That grade is not enough on its own, and the gap is the
//! reason this module exists rather than a line in the annotations:
//!
//! * **Auto approves everything.** `Auto` is the mode the gap was found in — an
//!   agent asked to "create a KB, confirm it, then delete it" would otherwise
//!   destroy it with nothing shown to the user at all.
//! * **An `AlwaysAllow` is permanent and untargeted.** A user who approves one
//!   cleanup of a scratch base has, on the tool-name grant, approved the
//!   deletion of every base they will ever own. `remove_extension` refuses that
//!   shape too (`requires_user_proof: true`), and an extension can be
//!   reinstalled.
//!
//! What is *not* claimed: this is not a security barrier. An agent with a shell
//! can `rm -rf` the same directory, and issue #56's general filesystem
//! read-deny is deferred. The barrier stops a mistake — the model reaching for
//! a destructive tool the user did not ask for — which is precisely what a
//! disclosure is for.
//!
//! # Ask, never refuse
//!
//! Unlike the whole-store memory read, deletion has no narrower substitute and
//! is routinely the thing the user actually asked for. So the verdict is always
//! [`InspectionAction::RequireApproval`] and never
//! [`InspectionAction::Deny`]: the card names the base, its page count and what
//! goes with it, and the user decides. A gate that refused would not be safer,
//! it would just move the deletion to `rm -rf` in the shell, where nothing
//! describes what is about to be lost.
//!
//! The one thing the card must get right is the *size* of the loss, so
//! [`deletion_card`] reads the base off disk — its display name and how many
//! curated pages it holds — rather than echoing the id the model supplied. A
//! card that says "delete kb-smoke-test" and a card that says "delete Patient
//! Cohort 2024 — 312 pages, and its entire history" are different decisions.
//!
//! # The doors no inspector sees
//!
//! Two production routes dispatch tool calls outside the agent loop
//! ([`crate::security::UninspectedBoundary`]), and neither can show a card and
//! wait. [`uninspected_boundary_refusal`] refuses the delete at both, naming the
//! two doors that do work: an ordinary tool call in a conversation, or
//! `DELETE /knowledge/bases/{id}`, which is what the Knowledge view's own
//! delete button calls and where the human is the one clicking.
//!
//! ⚠ **The `DELETE /knowledge/bases/{id}` route is deliberately left alone**,
//! and so is `KnowledgeService::delete_base`. The refusal belongs on the
//! *model's* surface: a user who insists proceeds past a warning — the
//! Knowledge view already shows one — and an agent never does the same thing
//! automatically. Pushing this gate down into the service would also break
//! `routes::reset::reset_knowledge`, which deletes every base including Soul.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

/// Registered in [`crate::tool_inspection::NON_DELEGABLE_APPROVAL_INSPECTORS`],
/// so a `PermissionRequest` hook returning `allow` is dropped and the user is
/// asked anyway. A hook is one more automated grant, and it runs downstream of
/// the escalation-only merge where the lattice can no longer defend the verdict.
pub const KNOWLEDGE_DELETE_INSPECTOR_NAME: &str = "knowledge_delete";

/// The MCP tool this gate is about, as the knowledge server names it.
const DELETE_TOOL: &str = "kb_delete_base";

/// Is this call `kb_delete_base`, however the extension prefixed it?
///
/// `a__b` → `b`, a bare `b` → `b` — the same normalisation
/// [`crate::security::global_memory`] uses, because the dispatched name carries
/// the extension key (`knowledge__kb_delete_base`) and the model may write
/// either shape.
///
/// ⚠ **Public because Code Execution mode has to ask the same question twice,
/// in two crates' worth of unrelated code, and the two answers must be the
/// same.** `ExtensionManager::get_prefixed_tools_excluding` asks it to keep the
/// tool out of the importable catalogue; `reply_parts::survives_code_execution_filter`
/// asks it to keep the tool directly callable. A tool dropped from the first
/// without being kept by the second reaches NOWHERE — which is what this
/// module's boundary refusal did to `kb_delete_base` in its first live run: the
/// script path refused it and the direct path had never been offered, so the
/// model correctly reported that deleting a base was impossible. One predicate,
/// two callers, no second spelling.
pub fn is_knowledge_delete_tool(tool_name: &str) -> bool {
    is_delete_call(tool_name)
}

fn is_delete_call(tool_name: &str) -> bool {
    tool_name
        .rsplit("__")
        .next()
        .unwrap_or(tool_name)
        .eq_ignore_ascii_case(DELETE_TOOL)
}

/// The `kb_id` this call names, trimmed, or `None` if it names none.
///
/// An absent or blank id is **not** escalated: `kb_delete_base` requires the
/// argument and rejects the call on its own terms, and a card for a deletion
/// that cannot happen teaches the user to click through them.
fn target_kb_id(args: &Map<String, Value>) -> Option<String> {
    let id = args.get("kb_id")?.as_str()?.trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// What the base holds, read off disk so the card states the real loss.
///
/// Both fields fail soft, and in the direction that keeps the card honest: an
/// unreadable manifest falls back to the id (never to a friendlier name the
/// user would not recognise), and an unreadable tree reports the page count as
/// unknown rather than as zero. "0 pages" on a base that could not be walked
/// reads as "nothing to lose", which is the one wrong thing this card could say.
fn describe_base(kb_id: &str) -> (String, Option<usize>) {
    let Ok(root) = crate::knowledge::paths::knowledge_root() else {
        return (kb_id.to_string(), None);
    };
    let kb_root = crate::knowledge::paths::kb_root(&root, kb_id);
    let name = crate::knowledge::manifest::load(&kb_root)
        .ok()
        .map(|manifest| manifest.name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| kb_id.to_string());
    let pages = crate::knowledge::store::list_pages(&kb_root, None)
        .ok()
        .map(|pages| pages.len());
    (name, pages)
}

/// The sentence the user reads, or `None` when this call is not a deletion.
pub fn deletion_card(tool_name: &str, args: &Map<String, Value>) -> Option<String> {
    if !is_delete_call(tool_name) {
        return None;
    }
    let kb_id = target_kb_id(args)?;
    let (name, pages) = describe_base(&kb_id);
    let held = match pages {
        Some(1) => "1 knowledge page".to_string(),
        Some(n) => format!("{n} knowledge pages"),
        // See `describe_base`: silence beats an unearned "0".
        None => "its knowledge pages".to_string(),
    };
    Some(format!(
        "🔒 An agent is permanently deleting a knowledge base.\n\
         Knowledge base: {name} ({kb_id})\n\
         This removes {held}, every raw source filed in it, and its entire git \
         history. There is no undo — restoring a commit works inside a base, not \
         on one that is gone.\n\
         This confirmation appears in every permission mode, including Fully \
         Automatic."
    ))
}

/// What the Code Execution catalogue says when a script tries to import the
/// delete tool.
///
/// The tool is stripped from the importable catalogue
/// (`ExtensionManager::get_prefixed_tools_excluding`) and kept on the model's
/// direct roster (`reply_parts::survives_code_execution_filter`), so the remedy
/// really does work — but only if the message says what it is. A bare "Tool not
/// found" reads as a spelling mistake and gets retried with a different
/// spelling, which is the failure issue #141 recorded for the `platform` family
/// and which was measured again here: with no explanation, GPT-5.5 spent two
/// extra tool calls rediscovering the direct name.
pub fn sandbox_import_refusal(named: &str) -> String {
    format!(
        "`{named}` cannot be imported or called from a script. Deleting a knowledge base \
         destroys its pages, its raw sources and its whole git history, so Biorouter shows \
         the user an approval naming the base first — and this sandbox dispatches tool calls \
         without the inspector chain that raises that approval, so nobody would be asked. \
         `{DELETE_TOOL}` is on your tool list as an ordinary tool: call it directly instead. \
         Every other knowledge tool is importable as usual."
    )
}

/// The refusal for the two doors that dispatch a tool call without passing it
/// through any [`ToolInspector`], and so cannot show [`deletion_card`] and wait.
///
/// One decision for both, and `boundary` is therefore not read by it — only
/// logged, so an operator can tell which door a refusal came from. Two spellings
/// of one refusal is how a rule becomes two slightly-different rules.
pub fn uninspected_boundary_refusal(
    tool_name: &str,
    args: Option<&Map<String, Value>>,
    boundary: crate::security::UninspectedBoundary,
) -> Option<String> {
    if !is_delete_call(tool_name) {
        return None;
    }
    // Deliberately independent of `target_kb_id`: a delete call arriving here at
    // all is refused, whatever it names or fails to name. A gate that only fires
    // when it can parse the argument is a gate with a parse bug for a bypass.
    tracing::warn!(
        counter.biorouter.knowledge_delete_uninspected_refused = 1,
        tool_name = %tool_name,
        boundary = ?boundary,
        kb_id = ?args.and_then(target_kb_id),
        "Refused a knowledge-base deletion at a boundary that cannot ask the user"
    );
    Some(format!(
        "{DELETE_TOOL} is refused here. Deleting a knowledge base destroys its pages, \
         its raw sources and its whole git history, so it is shown to the user for \
         approval first — and this entry point dispatches tool calls without the \
         inspector chain that raises that approval, so nobody would be asked. Call \
         {DELETE_TOOL} as an ordinary tool call in a conversation, where the approval \
         is shown; a person deleting a base themselves can use the Knowledge view, or \
         DELETE /knowledge/bases/{{id}}."
    ))
}

/// Inspector that routes every model-initiated knowledge-base deletion through
/// the user, in every mode that runs tools. See the module docs for the policy.
pub struct KnowledgeDeleteInspector;

#[async_trait]
impl ToolInspector for KnowledgeDeleteInspector {
    fn name(&self) -> &'static str {
        KNOWLEDGE_DELETE_INSPECTOR_NAME
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        biorouter_mode: BioRouterMode,
        // Deliberately unread. A base is put to the user because deleting it is
        // irreversible, not because of who is asking — see
        // `a_private_capability_caller_is_asked_too`. The tier axis decides
        // whether this caller may reach the base at all, and it has already
        // decided that, at the knowledge server's own CP1 seam.
        _session: &crate::session::Session,
    ) -> Result<Vec<InspectionResult>> {
        // Chat dispatches no tools; the agent splices a canned response.
        if biorouter_mode == BioRouterMode::Chat {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        for request in tool_requests {
            let Ok(tool_call) = &request.tool_call else {
                continue;
            };
            let Some(args) = tool_call.arguments.as_ref() else {
                continue;
            };
            let Some(card) = deletion_card(&tool_call.name, args) else {
                continue;
            };
            tracing::warn!(
                counter.biorouter.knowledge_delete_gated = 1,
                tool_name = %tool_call.name,
                tool_request_id = %request.id,
                "Knowledge-base deletion routed through the user"
            );
            results.push(InspectionResult {
                tool_request_id: request.id.clone(),
                action: InspectionAction::RequireApproval(Some(card.clone())),
                reason: card,
                confidence: 1.0,
                inspector_name: self.name().to_string(),
                finding_id: Some(format!("KBDEL-{}", Uuid::new_v4().simple())),
            });
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    // --- what counts as a deletion ----------------------------------------

    /// The dispatched name carries the extension key, so a gate that matched
    /// the bare tool name only would be inert in the app and pass every test
    /// written against the bare name.
    #[test]
    fn the_prefixed_name_the_agent_actually_dispatches_is_recognised() {
        for name in [
            "kb_delete_base",
            "knowledge__kb_delete_base",
            "mcp__biorouter__kb_delete_base",
        ] {
            assert!(
                deletion_card(name, &args(json!({"kb_id": "scratch"}))).is_some(),
                "{name} must be recognised as a deletion"
            );
        }
    }

    /// Every other knowledge tool is untouched: this gate must not turn reading
    /// a page into an approval prompt.
    #[test]
    fn no_other_knowledge_tool_is_gated() {
        for name in [
            "knowledge__kb_create_base",
            "knowledge__kb_write_page",
            "knowledge__kb_list_bases",
            "knowledge__kb_merge",
            "knowledge__kb_export",
            "developer__shell",
        ] {
            assert!(
                deletion_card(name, &args(json!({"kb_id": "scratch"}))).is_none(),
                "{name} must not raise a deletion card"
            );
        }
    }

    /// A call with no usable `kb_id` cannot delete anything — the tool requires
    /// the argument — so escalating it would teach the user to click through
    /// cards for operations that never happen.
    #[test]
    fn a_call_that_names_no_base_raises_no_card() {
        for payload in [json!({}), json!({"kb_id": ""}), json!({"kb_id": "   "})] {
            assert!(
                deletion_card("knowledge__kb_delete_base", &args(payload.clone())).is_none(),
                "{payload} should not raise a card"
            );
        }
    }

    // --- what the card says -----------------------------------------------

    /// The card must name the base and say what goes with it. A card that says
    /// only "delete a knowledge base" is a card the user cannot weigh.
    #[test]
    fn the_card_names_the_base_and_the_loss() {
        let card = deletion_card("knowledge__kb_delete_base", &args(json!({"kb_id": "omop"})))
            .expect("a deletion must raise a card");
        assert!(card.contains("omop"), "{card}");
        assert!(
            card.contains("history"),
            "the history is part of the loss: {card}"
        );
        assert!(card.contains("no undo"), "{card}");
        assert!(
            card.contains("every permission mode"),
            "the user should know an Auto-mode agent still cannot do this silently: {card}"
        );
    }

    /// ⚠ A base whose tree cannot be walked must NOT be described as holding
    /// zero pages. "0 pages" reads as "nothing to lose", which is the one wrong
    /// thing this card could say — and it is the reading an unreadable or
    /// nonexistent base would produce from a `unwrap_or(0)`.
    #[test]
    fn an_unreadable_base_is_not_reported_as_empty() {
        // No such base on disk, so `list_pages` fails and the count is unknown.
        let card = deletion_card(
            "knowledge__kb_delete_base",
            &args(json!({"kb_id": "no-such-base-anywhere"})),
        )
        .expect("a deletion must raise a card even when the base cannot be read");
        assert!(
            !card.contains("0 knowledge page"),
            "an unreadable base was reported as empty: {card}"
        );
        assert!(card.contains("its knowledge pages"), "{card}");
    }

    // --- the boundaries ---------------------------------------------------

    /// Neither uninspected door can show the card, so neither may dispatch the
    /// delete. The refusal has to name a door that works, or the model retries
    /// the same call forever.
    #[test]
    fn both_uninspected_boundaries_refuse_and_name_a_route_that_works() {
        for boundary in [
            crate::security::UninspectedBoundary::ExecuteCodeScript,
            crate::security::UninspectedBoundary::AgentCallToolRoute,
        ] {
            let refusal = uninspected_boundary_refusal(
                "knowledge__kb_delete_base",
                Some(&args(json!({"kb_id": "scratch"}))),
                boundary,
            )
            .unwrap_or_else(|| panic!("{boundary:?} must refuse a deletion"));
            assert!(refusal.contains("kb_delete_base"), "{refusal}");
            assert!(
                refusal.contains("/knowledge/bases/"),
                "the refusal must name the human route: {refusal}"
            );
        }
    }

    /// ⚠ The boundary refuses on the tool NAME alone, with no argument parsing
    /// in the path. A gate that only fires once it has parsed `kb_id` has its
    /// own parse bug for a bypass — and at these doors the arguments are already
    /// evaluated, so there is nothing an inspector-style leniency would buy.
    #[test]
    fn the_boundary_refuses_even_a_call_it_cannot_parse() {
        for payload in [None, Some(args(json!({}))), Some(args(json!({"kb_id": 7})))] {
            assert!(
                uninspected_boundary_refusal(
                    "kb_delete_base",
                    payload.as_ref(),
                    crate::security::UninspectedBoundary::ExecuteCodeScript,
                )
                .is_some(),
                "an unparseable delete still reached an uninspected door"
            );
        }
    }

    /// …and nothing else is refused there. This gate must not become a second,
    /// broader boundary rule by accident.
    #[test]
    fn the_boundary_leaves_every_other_tool_alone() {
        for name in [
            "knowledge__kb_write_page",
            "knowledge__kb_export",
            "developer__shell",
        ] {
            assert!(
                uninspected_boundary_refusal(
                    name,
                    Some(&args(json!({"kb_id": "scratch"}))),
                    crate::security::UninspectedBoundary::AgentCallToolRoute,
                )
                .is_none(),
                "{name} was refused at a boundary it has nothing to do with"
            );
        }
    }

    // --- the inspector, end to end ----------------------------------------

    fn request(name: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            id: "r1".to_string(),
            tool_call: Ok(rmcp::model::CallToolRequestParams {
                task: None,
                name: name.to_string().into(),
                arguments: Some(args(arguments)),
                meta: None,
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    async fn inspect_one(
        name: &str,
        arguments: Value,
        mode: BioRouterMode,
    ) -> Vec<InspectionResult> {
        KnowledgeDeleteInspector
            .inspect(
                &[request(name, arguments)],
                &[],
                mode,
                &crate::session::Session::default(),
            )
            .await
            .unwrap()
    }

    /// ⚠ **The mode loop is the test.** `Auto` is the mode the gap was found
    /// in — the permission inspector approves every tool there deterministically
    /// — but a gate that fired only in `Auto` would still let a `SmartApprove`
    /// grade or a user `AlwaysAllow` delete a base with nothing shown, and those
    /// are the same silent destruction. `Chat` is the only exclusion, and only
    /// because it dispatches no tools at all.
    #[tokio::test]
    async fn every_tool_running_mode_asks_before_a_base_is_deleted() {
        for mode in [
            BioRouterMode::Auto,
            BioRouterMode::Approve,
            BioRouterMode::SmartApprove,
        ] {
            let results = inspect_one(
                "knowledge__kb_delete_base",
                json!({"kb_id": "scratch"}),
                mode,
            )
            .await;
            let result = results
                .first()
                .unwrap_or_else(|| panic!("{mode:?} did not ask before deleting a base"));
            assert!(
                matches!(result.action, InspectionAction::RequireApproval(Some(_))),
                "{mode:?} produced {:?} rather than an approval carrying the card",
                result.action
            );
            assert_eq!(result.inspector_name, KNOWLEDGE_DELETE_INSPECTOR_NAME);
            assert!(result
                .finding_id
                .as_deref()
                .unwrap_or_default()
                .starts_with("KBDEL-"));
        }

        assert!(
            inspect_one(
                "knowledge__kb_delete_base",
                json!({"kb_id": "scratch"}),
                BioRouterMode::Chat,
            )
            .await
            .is_empty(),
            "Chat dispatches no tools; a card there is noise"
        );
    }

    /// It ASKS, and must never DENY. Deletion is routinely the thing the user
    /// asked for, and a refusal would only move it to `rm -rf` in the shell,
    /// where nothing describes what is about to be lost.
    #[tokio::test]
    async fn the_verdict_is_an_approval_and_never_a_refusal() {
        let results = inspect_one(
            "knowledge__kb_delete_base",
            json!({"kb_id": "scratch"}),
            BioRouterMode::Auto,
        )
        .await;
        assert!(
            !results
                .iter()
                .any(|r| matches!(r.action, InspectionAction::Deny)),
            "this gate denied a deletion: {results:?}"
        );
    }

    /// ⚠ **The regression this test exists for was found in a live run, and it
    /// made the tool useless rather than merely awkward.**
    ///
    /// With `code_execution` enabled — which is the default — the model's
    /// roster does not carry ordinary extension tools; they are reached by
    /// importing them in a script. Refusing the delete at the script boundary
    /// without also keeping it on the roster left it reachable from NOWHERE, and
    /// GPT-5.5 said so in as many words: "no direct Knowledge tool is
    /// exposed — Knowledge is only available through code_execution, where
    /// deletion is refused."
    ///
    /// The two halves are one predicate for that reason. This asserts the
    /// implication in the direction that bites: anything this module refuses at
    /// the script boundary must survive the Code Execution filter.
    #[test]
    fn what_the_script_boundary_refuses_stays_directly_callable() {
        let name = "knowledge__kb_delete_base";
        assert!(
            uninspected_boundary_refusal(
                name,
                Some(&args(json!({"kb_id": "scratch"}))),
                crate::security::UninspectedBoundary::ExecuteCodeScript,
            )
            .is_some(),
            "precondition: the script boundary must refuse this tool"
        );
        assert!(
            crate::agents::reply_parts::survives_code_execution_filter(
                name,
                "code_execution__",
                &std::collections::HashSet::new(),
            ),
            "`{name}` is refused in the sandbox and dropped from the roster: it reaches \
             nowhere, which is the state this pairing exists to prevent"
        );
        // …and the exemption is not a blanket one: an ordinary knowledge tool
        // is still reached by writing code, which is what Code Execution mode is
        // for.
        assert!(
            !crate::agents::reply_parts::survives_code_execution_filter(
                "knowledge__kb_write_page",
                "code_execution__",
                &std::collections::HashSet::new(),
            ),
            "the exemption widened to every knowledge tool"
        );
    }

    /// The import refusal has to name the remedy, because the remedy exists.
    /// "Tool not found" is the dead end this replaces.
    #[test]
    fn the_import_refusal_points_at_the_direct_call() {
        let refusal = sandbox_import_refusal("knowledge/kb_delete_base");
        assert!(refusal.contains("knowledge/kb_delete_base"), "{refusal}");
        assert!(refusal.contains("call it directly"), "{refusal}");
        assert!(
            refusal.contains("approval"),
            "the model should learn WHY, or it will look for a way round: {refusal}"
        );
    }

    /// The card cannot be answered by a `PermissionRequest` hook. A hook is one
    /// more automated grant, and it runs downstream of the escalation-only
    /// merge where the lattice can no longer defend the verdict — so a hook that
    /// could answer this is a hook that can delete a user's curated knowledge
    /// without them ever seeing it.
    #[tokio::test]
    async fn only_a_human_may_answer_the_deletion_card() {
        let results = inspect_one(
            "knowledge__kb_delete_base",
            json!({"kb_id": "scratch"}),
            BioRouterMode::Auto,
        )
        .await;
        assert!(
            crate::tool_inspection::approval_requires_a_human("r1", &results),
            "an automated grant could answer a permanent deletion"
        );
    }
}
