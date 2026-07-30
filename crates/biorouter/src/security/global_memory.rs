//! Consent gate for the **machine-wide (global) memory store** (issue #63).
//!
//! # The hole this closes
//!
//! Issue #58 stopped global memory *bodies* being injected into every session's
//! system prompt: the prompt now carries only the category index, and reading a
//! category takes an explicit `retrieve_memories(…, is_global=true)` call. That
//! changed the delivery but not the reachability. Nothing gated the tool call,
//! and in [`BioRouterMode::Auto`] the permission inspector approves every tool
//! deterministically — so a session could still read every global memory any
//! other session ever wrote, with no prompt and nothing shown to the user.
//! `category="*"` returned the entire store in one call.
//!
//! The write side was never gated at all. An in-process MCP server has no
//! channel to the user, so `remember_memory` could only *name* the scope in its
//! result text; the decision to write machine-wide was the model's alone.
//!
//! # The mechanism
//!
//! An **argument-aware** [`ToolInspector`], modelled on
//! [`crate::security::sensitive_ops`]: it reads the memory tools' `is_global`
//! and `category` arguments, not just the tool name, and returns a verdict the
//! escalation-only merge in [`crate::tool_inspection`] applies *over* whatever
//! the permission inspector decided. That is what makes it survive Auto mode —
//! and a user `AlwaysAllow`, and a `SmartApprove` read-only grade — because the
//! merge can raise a verdict but never lower one.
//!
//! Unlike `SensitiveOpsInspector` this gate is **not** inert outside Auto. Auto
//! is merely the loudest case. `SmartApprove` can grade `retrieve_memories` as
//! read-only from its annotations and auto-approve it, and an `AlwaysAllow` the
//! user granted for project-local recall silently covers a machine-wide read
//! too — both are the same undisclosed cross-session read. Only `Chat` is
//! skipped, and only because it dispatches no tools at all.
//!
//! Two properties the rest of the pipeline owes this verdict, each the subject
//! of its own regression suite because each was once missing:
//!
//! * **The ask cannot *lower* a denial.** The merge is a strict
//!   `Deny > RequireApproval > Allow` lattice
//!   ([`crate::tool_inspection::apply_inspection_results_to_permissions`]), so a
//!   call already refused — by a user `NeverAllow`, a managed policy, the
//!   catastrophic-command block — is not also queued for a card. It briefly was,
//!   and since "Allow once" *dispatches* the tool, that card would have run
//!   something already refused.
//! * **No automation may answer the card.** The gate's name is in
//!   [`crate::tool_inspection::NON_DELEGABLE_APPROVAL_INSPECTORS`], so a
//!   `PermissionRequest` hook returning `allow` is dropped and the user is asked
//!   anyway. A hook is one more automated grant, and it runs downstream of the
//!   merge where the lattice can no longer defend the verdict.
//!
//! # What it decides, and why the two verdicts differ
//!
//! * A global read/write/delete naming **one category** →
//!   [`InspectionAction::RequireApproval`]. The user is told which category, in
//!   which direction, and approves or denies it. The feature keeps working;
//!   consent is the point.
//! * `retrieve_memories(category="*", is_global=true)` — the whole-store read —
//!   → [`InspectionAction::Deny`], with a reason that names the itemised call to
//!   use instead.
//!
//! Refusing the bulk *read* while merely asking about the bulk *delete*
//! (`remove_memory_category(category="*", is_global=true)`) is deliberate:
//!
//! 1. **A bulk read cannot be meaningfully consented to.** The approval card
//!    can only say "every global memory"; neither it nor the user can enumerate
//!    what is about to cross, because nothing in the UI surfaces the global
//!    store's contents. Approving it is a blank cheque over text the user has
//!    not seen. A bulk *delete* is the opposite — the user knows exactly what
//!    "wipe my global memories" means without seeing them, so an ask is real
//!    consent.
//! 2. **It is never necessary.** The system prompt already carries every global
//!    category name, so a named read is always available. `"*"` buys one round
//!    trip and spends it on precisely the disclosure that ought to be itemised.
//! 3. **It is not a refusal of the feature.** Every byte remains reachable —
//!    one approved category at a time — which is exactly the bar issue #58 set
//!    and the reason the audit would not accept a blanket server-side refusal.
//!
//! # Boundary: the routes an inspector never sees
//!
//! Two production routes dispatch tool calls without going through the agent
//! loop, so no [`ToolInspector`] — this gate included — ever sees them: the
//! calls a script makes from inside `execute_code`, and `POST /agent/call_tool`.
//! A third has no agent at all: `biorouter mcp memory` serves the memory server
//! straight over stdio to any MCP client.
//!
//! The first version of this gate answered the `execute_code` case by **scanning
//! the script text** for an embedded memory call. That was the wrong shape and
//! the #63 review broke it in one line: `const flag = true;
//! retrieve_memories({category:"clinical", is_global:flag})` has no truthy
//! literal after `is_global`, so the scan sees nothing. Source text can always
//! be out-computed.
//!
//! So the decision moved to where there is nothing left to compute:
//!
//! * [`uninspected_boundary_refusal`] is applied at each of the two dispatch
//!   boundaries, reading the *dispatched* tool name and the *evaluated*
//!   arguments. Every global read, write and delete is refused there, and the
//!   model is told to make an ordinary itemised memory call instead — which is
//!   gated, and works.
//! * The memory server itself refuses global operations unless it was built by
//!   the one constructor that states a consent path
//!   (`MemoryServer::behind_consent_gate`), which closes the standalone case
//!   and makes consent a property of the store rather than of one caller.
//!
//! The script scan is **kept**, but it is no longer load-bearing: it is now a
//! warning — deliberately over-inclusive, and harmless when it is wrong because
//! the worst outcome is one approval card — and the card says plainly that the
//! script's memory calls will be refused either way.

use serde_json::{Map, Value};

use crate::config::BioRouterMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};
use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

/// This inspector's name, as it appears on an [`InspectionResult`]. Also the key
/// [`crate::agents::Agent::handle_denied_tools`] matches on so a refusal reaches
/// the model as its real reason instead of "the user declined".
pub const GLOBAL_MEMORY_INSPECTOR_NAME: &str = "global_memory";

/// The verdict for one tool call against the global memory store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMemoryGate {
    /// Route to the standard approval flow, showing this message.
    Ask(String),
    /// Refuse outright, telling the model this reason (and how to proceed).
    Refuse(String),
}

/// The `biorouter-mcp` memory server's four tools, named as the *tool* names
/// they are declared with. The `<extension>__` prefix the extension manager adds
/// is configuration (an operator can mount the server under any name), so it is
/// deliberately not part of the match.
const RETRIEVE_MEMORIES: &str = "retrieve_memories";
const REMEMBER_MEMORY: &str = "remember_memory";
const REMOVE_MEMORY_CATEGORY: &str = "remove_memory_category";
const REMOVE_SPECIFIC_MEMORY: &str = "remove_specific_memory";

const MEMORY_TOOL_NAMES: &[&str] = &[
    RETRIEVE_MEMORIES,
    REMEMBER_MEMORY,
    REMOVE_MEMORY_CATEGORY,
    REMOVE_SPECIFIC_MEMORY,
];

/// The documented "every category" sentinel, matched exactly as the memory
/// server dispatches it (`params.category == "*"`). Issue #73 keeps `*` a legal
/// plain category name as well, so a padded `" * "` is an ordinary category and
/// must not be reported to the user as a whole-store operation.
const ALL_CATEGORIES: &str = "*";

/// Which memory tool this is, from the tool name as the model called it.
fn memory_tool(tool_name: &str) -> Option<&'static str> {
    // `a__b` → `b`; a bare `b` → `b`.
    let base = tool_name
        .rsplit("__")
        .next()
        .unwrap_or(tool_name)
        .to_ascii_lowercase();
    MEMORY_TOOL_NAMES.iter().copied().find(|n| *n == base)
}

/// Whether a JSON value reads as `true`.
///
/// `is_global` is declared `bool`, but a model that sends `"true"` would
/// otherwise slip past a strict `as_bool()` and reach the store ungated — and
/// `serde` would still accept it in some shapes. Leniency here only ever *adds*
/// a consent prompt. An absent or unrecognised flag is **not** treated as
/// global: local is the tool's documented default, so failing that way turns a
/// malformed call into a local one, never into a silent machine-wide one.
fn reads_as_true(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::String(s) => matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "yes" | "1"),
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

fn targets_global_store(args: &Map<String, Value>) -> bool {
    args.get("is_global").is_some_and(reads_as_true)
}

fn category_of(args: &Map<String, Value>) -> Option<&str> {
    args.get("category").and_then(Value::as_str)
}

/// True for the code-execution tool, whose JS body carries the real (inner) tool
/// calls. Those are dispatched straight through the extension manager and never
/// reach an agent-layer inspector, so the body itself has to be inspected.
fn is_execute_code(tool_name: &str) -> bool {
    tool_name.to_ascii_lowercase().contains("execute_code")
}

/// Does this script set `is_global` to something truthy anywhere?
///
/// Deliberately crude and deliberately over-inclusive: the cost of a false
/// positive is one approval prompt on a script that also mentions memory, and
/// the cost of a false negative is the ungated cross-session read this whole
/// module exists to stop.
fn sets_global_true(lowercased: &str) -> bool {
    // `split` + `skip(1)` yields the text following each `is_global` occurrence,
    // without indexing into the string by byte offset.
    lowercased.split("is_global").skip(1).any(|after| {
        let value = after.trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ':' | '=' | '"' | '\'' | '`')
        });
        value.starts_with("true") || value.starts_with("yes") || value.starts_with('1')
    })
}

/// A global-memory call embedded in an `execute_code` body.
fn code_touches_global_memory(code: &str) -> bool {
    let lowercased = code.to_ascii_lowercase();
    MEMORY_TOOL_NAMES
        .iter()
        .any(|name| lowercased.contains(name))
        && sets_global_true(&lowercased)
}

/// Classify a tool call against the global memory store.
///
/// `None` means "this call does not touch the machine-wide store" — a local
/// memory call, or any other tool at all.
pub fn global_memory_gate(tool_name: &str, args: &Map<String, Value>) -> Option<GlobalMemoryGate> {
    if is_execute_code(tool_name) {
        let code = args.get("code").and_then(Value::as_str)?;
        return code_touches_global_memory(code).then(|| {
            GlobalMemoryGate::Ask(
                "🔒 This script looks like it wants cross-session memory.\n\
                 It appears to read or write the global memory store — the machine-wide store \
                 shared by every Biorouter session on this computer, in every project. Tool \
                 calls made from inside a script are not itemised, so there is nothing here for \
                 you to approve one by one, and Biorouter therefore refuses global memory \
                 operations attempted from inside a script whatever you decide now.\n\
                 So this asks about the rest of what the script does. Approve it to run it — its \
                 global memory calls will fail with an explanation — or deny it and ask for the \
                 memory calls to be made directly, where each one is shown to you by category."
                    .to_string(),
            )
        });
    }

    let tool = memory_tool(tool_name)?;
    if !targets_global_store(args) {
        // Project-local memory lives under the working directory the user
        // opened, so it crosses no session boundary and is left alone.
        return None;
    }
    let category = category_of(args)?;
    let all_categories = category == ALL_CATEGORIES;

    Some(match (tool, all_categories) {
        // The whole-store read: refused, never asked about. See the module docs
        // — a disclosure neither the card nor the user can enumerate is not
        // something consent can be given for, and it is never necessary.
        (RETRIEVE_MEMORIES, true) => GlobalMemoryGate::Refuse(
            "Refused: retrieve_memories(category=\"*\", is_global=true) would disclose the entire \
             machine-wide memory store — every global memory written by every other session on \
             this computer — in one call, and the user cannot be shown what that is in order to \
             consent to it. Read one category at a time instead: \
             retrieve_memories(category=\"<name>\", is_global=true), which asks the user about \
             that category by name. The global category names are listed in your system prompt. \
             Local bulk retrieval (is_global=false) is unaffected."
                .to_string(),
        ),
        (RETRIEVE_MEMORIES, false) => GlobalMemoryGate::Ask(format!(
            "🔒 Cross-session memory read.\n\
             This conversation is asking to read the global memory category \"{category}\" — the \
             machine-wide store that every Biorouter session on this computer shares, including \
             sessions in other projects. Its contents were written by other conversations and are \
             not otherwise part of this one.\n\
             Approve it to disclose that category here, or deny it. To read \"{category}\" \
             yourself first — or delete what is in it — open Settings → Chat → Memory. \
             Project-local memories (.biorouter/memory) are unaffected and never prompt."
        )),
        (REMEMBER_MEMORY, _) => GlobalMemoryGate::Ask(format!(
            "🔒 Cross-session memory write.\n\
             This conversation is asking to save a memory to the global category \"{category}\" — \
             the machine-wide store, readable from now on by every Biorouter session on this \
             computer, in every project.\n\
             Approve it to let this note follow you across projects, or deny it and ask for a \
             project-local memory instead."
        )),
        // Clearing the whole store is asked about rather than refused: unlike a
        // bulk read, "wipe my global memories" is something a user can consent
        // to without being shown the contents.
        (REMOVE_MEMORY_CATEGORY, true) => GlobalMemoryGate::Ask(
            "🔒 Deletes every global memory.\n\
             This conversation is asking to clear the entire machine-wide memory store — every \
             global category any session on this computer ever saved. This cannot be undone.\n\
             Approve it only if you asked for your global memories to be wiped. To see what \
             would be lost, or to delete categories one at a time instead, open \
             Settings → Chat → Memory. Project-local memories (.biorouter/memory) are \
             unaffected."
                .to_string(),
        ),
        (REMOVE_MEMORY_CATEGORY | REMOVE_SPECIFIC_MEMORY, _) => GlobalMemoryGate::Ask(format!(
            "🔒 Cross-session memory change.\n\
             This conversation is asking to delete from the global memory category \
             \"{category}\" — the machine-wide store shared by every Biorouter session on this \
             computer. Removing it here removes it for every project.\n\
             Approve it, or deny it."
        )),
        // `memory_tool` returns one of the four constants above; a new tool
        // added to the server without a rule here fails closed.
        _ => GlobalMemoryGate::Ask(format!(
            "🔒 Cross-session memory access.\n\
             This conversation is asking to use the global memory category \"{category}\" — the \
             machine-wide store shared by every Biorouter session on this computer, in every \
             project.\n\
             Approve it, or deny it."
        )),
    })
}

/// A production route that dispatches tool calls without passing them through
/// the agent loop — and therefore without passing them through any
/// [`ToolInspector`], this gate included (issue #63 review, finding 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninspectedBoundary {
    /// The tool calls a script makes from inside `execute_code`. The JS host
    /// hands them straight to `ExtensionManager::dispatch_tool_call`.
    ExecuteCodeScript,
    /// `POST /agent/call_tool`, which does the same from an HTTP handler.
    AgentCallToolRoute,
}

impl UninspectedBoundary {
    /// How the refusal names where the call came from, and what to do instead.
    fn remedy(self) -> &'static str {
        match self {
            Self::ExecuteCodeScript => {
                "Tool calls made from inside a script are not shown to the user one by one, so \
                 there is nothing for them to approve. Make the memory call directly, outside \
                 execute_code — retrieve_memories / remember_memory / remove_memory_category / \
                 remove_specific_memory with is_global=true — and the user will be shown that \
                 exact operation and asked to approve it."
            }
            Self::AgentCallToolRoute => {
                "This route dispatches a tool without the agent loop, so nothing here can put \
                 the operation to the user. Issue the memory call in a conversation instead, \
                 where it is inspected and approved."
            }
        }
    }
}

/// Is this a memory-tool call against the machine-wide store?
///
/// Unlike [`code_touches_global_memory`], which reads *source text* and can
/// therefore always be out-computed, this reads the dispatched call: the real
/// tool name and the real, already-evaluated arguments. There is nothing left to
/// hide behind — `is_global: flag` and `is_global: true` arrive here identically.
fn is_global_store_call(tool_name: &str, args: &Map<String, Value>) -> bool {
    memory_tool(tool_name).is_some() && targets_global_store(args)
}

/// The refusal a dispatch boundary that cannot ask the user must return for a
/// machine-wide memory operation, or `None` if the call is not one.
///
/// This is the answer to "the static `execute_code` scan can be evaded". It is
/// not a better scan; it is the check moved to where evasion is impossible.
/// Every global read, write and delete is refused at these boundaries — a
/// boundary that cannot obtain consent does not act without it — and the model
/// is told to make an ordinary itemised memory call, which *is* gated and does
/// work. Local memory is untouched.
pub fn uninspected_boundary_refusal(
    tool_name: &str,
    args: Option<&Map<String, Value>>,
    boundary: UninspectedBoundary,
) -> Option<String> {
    if !args.is_some_and(|a| is_global_store_call(tool_name, a)) {
        return None;
    }
    tracing::warn!(
        counter.biorouter.global_memory_uninspected_refused = 1,
        tool_name = %tool_name,
        boundary = ?boundary,
        "Refused a machine-wide memory operation at a boundary that cannot ask the user"
    );
    Some(format!(
        "Refused: this call would read, write or delete the global memory store — the \
         machine-wide store shared by every Biorouter session on this computer, in every \
         project — and every such operation has to be shown to the user and approved first. \
         {}",
        boundary.remedy()
    ))
}

/// Inspector that routes global-memory access through the user, in every mode
/// that runs tools. See the module docs for the policy.
pub struct GlobalMemoryInspector;

#[async_trait]
impl ToolInspector for GlobalMemoryInspector {
    fn name(&self) -> &'static str {
        GLOBAL_MEMORY_INSPECTOR_NAME
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        biorouter_mode: BioRouterMode,
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
            let Some(gate) = global_memory_gate(&tool_call.name, args) else {
                continue;
            };
            let (action, reason) = match gate {
                GlobalMemoryGate::Ask(message) => (
                    InspectionAction::RequireApproval(Some(message.clone())),
                    message,
                ),
                GlobalMemoryGate::Refuse(reason) => (InspectionAction::Deny, reason),
            };
            tracing::warn!(
                counter.biorouter.global_memory_gated = 1,
                tool_name = %tool_call.name,
                tool_request_id = %request.id,
                denied = matches!(action, InspectionAction::Deny),
                "Global memory access routed through the user"
            );
            results.push(InspectionResult {
                tool_request_id: request.id.clone(),
                action,
                reason,
                confidence: 1.0,
                inspector_name: self.name().to_string(),
                finding_id: Some(format!("GMEM-{}", Uuid::new_v4().simple())),
            });
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolRequestParams;
    use serde_json::json;

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    // --- classification: the reads ----------------------------------------

    /// The issue's exact demonstration: `category="*"` + `is_global=true` hands
    /// back the entire machine-wide store in one call. It is refused, not asked
    /// about — see the module docs for why a bulk read cannot be consented to.
    #[test]
    fn the_whole_store_read_is_refused() {
        let gate = global_memory_gate(
            "memory__retrieve_memories",
            &args(json!({"category": "*", "is_global": true})),
        );
        let Some(GlobalMemoryGate::Refuse(reason)) = gate else {
            panic!("retrieve_memories(\"*\", is_global=true) must be refused, got {gate:?}");
        };
        assert!(
            reason.contains("retrieve_memories") && reason.contains("category"),
            "the refusal must name the itemised call that still works, or the \
             feature is dead: {reason}"
        );
    }

    /// A named global category is the shape the feature is *for*. It is asked
    /// about, never refused, and the card names the category.
    #[test]
    fn a_named_global_read_is_asked_about() {
        let gate = global_memory_gate(
            "memory__retrieve_memories",
            &args(json!({"category": "clinical", "is_global": true})),
        );
        let Some(GlobalMemoryGate::Ask(message)) = gate else {
            panic!("a named global read must ask, got {gate:?}");
        };
        assert!(
            message.contains("clinical"),
            "the approval card must name the category being disclosed: {message}"
        );
    }

    /// Naming the category is only half an answer. The user's actual question
    /// on seeing this card is "what is *in* `clinical`?", and there is now a
    /// place that answers it — so the card has to say where, or the approval is
    /// still over text the user has never seen.
    ///
    /// The two cards that need it are the ones whose subject is *content the
    /// user cannot see*: the read (what am I disclosing?) and the whole-store
    /// wipe (what am I destroying?). The write card carries its own subject —
    /// the model just supplied the text — and a card per call would make the
    /// pointer noise rather than help.
    #[test]
    fn the_cards_about_unseen_content_say_where_to_see_it() {
        for (tool, arguments) in [
            (
                "memory__retrieve_memories",
                json!({"category": "clinical", "is_global": true}),
            ),
            (
                "memory__remove_memory_category",
                json!({"category": "*", "is_global": true}),
            ),
        ] {
            let gate = global_memory_gate(tool, &args(arguments));
            let Some(GlobalMemoryGate::Ask(message)) = gate else {
                panic!("{tool} must ask, got {gate:?}");
            };
            assert!(
                message.contains("Settings")
                    && message.contains("Chat")
                    && message.contains("Memory"),
                "{tool}'s card must point at the surface that lists the store, \
                 by the path the user would actually follow: {message}"
            );
        }
    }

    // --- classification: the writes ---------------------------------------

    /// CLAUDE.md records the write as the gap #58 could not close from inside an
    /// MCP server ("a real confirmation needs the permission path"). This is it.
    #[test]
    fn a_global_write_is_asked_about() {
        let gate = global_memory_gate(
            "memory__remember_memory",
            &args(json!({
                "category": "clinical",
                "data": "cohort 4217 had 12 responders",
                "is_global": true
            })),
        );
        let Some(GlobalMemoryGate::Ask(message)) = gate else {
            panic!("a global write must ask, got {gate:?}");
        };
        assert!(
            message.contains("clinical"),
            "the card must name the category being written: {message}"
        );
        assert!(
            !message.contains("cohort 4217"),
            "the card is a consent prompt, not a second copy of the payload: {message}"
        );
    }

    #[test]
    fn a_global_delete_is_asked_about() {
        for (tool, arguments) in [
            (
                "memory__remove_memory_category",
                json!({"category": "clinical", "is_global": true}),
            ),
            (
                "memory__remove_specific_memory",
                json!({"category": "clinical", "memory_content": "x", "is_global": true}),
            ),
        ] {
            assert!(
                matches!(
                    global_memory_gate(tool, &args(arguments)),
                    Some(GlobalMemoryGate::Ask(_))
                ),
                "{tool} against the global store must ask"
            );
        }
    }

    /// Wiping the whole global store is asked about rather than refused: unlike a
    /// bulk read, a user knows exactly what they are consenting to without being
    /// shown the contents.
    #[test]
    fn the_whole_store_delete_is_asked_about_not_refused() {
        let gate = global_memory_gate(
            "memory__remove_memory_category",
            &args(json!({"category": "*", "is_global": true})),
        );
        let Some(GlobalMemoryGate::Ask(message)) = gate else {
            panic!("clearing the whole global store must ask, got {gate:?}");
        };
        assert!(
            message.contains("every"),
            "the card has to say the blast radius is every global category: {message}"
        );
    }

    // --- the feature is not disabled --------------------------------------

    /// Local memory is untouched by this gate — including the bulk read, which
    /// crosses no session boundary because `.biorouter/memory` lives under the
    /// working directory the user opened.
    #[test]
    fn local_memory_is_never_gated() {
        for (tool, arguments) in [
            (
                "memory__retrieve_memories",
                json!({"category": "*", "is_global": false}),
            ),
            (
                "memory__retrieve_memories",
                json!({"category": "development", "is_global": false}),
            ),
            (
                "memory__remember_memory",
                json!({"category": "development", "data": "x", "is_global": false}),
            ),
            (
                "memory__remove_memory_category",
                json!({"category": "*", "is_global": false}),
            ),
        ] {
            assert!(
                global_memory_gate(tool, &args(arguments)).is_none(),
                "{tool} on the local store must not be gated"
            );
        }
    }

    /// Nothing else in the tool surface is touched.
    #[test]
    fn unrelated_tools_are_not_gated() {
        for (tool, arguments) in [
            ("developer__shell", json!({"command": "ls"})),
            (
                "developer__text_editor",
                json!({"command": "write", "path": "/tmp/x"}),
            ),
            // A different extension that happens to carry an `is_global` flag.
            (
                "workflow__save_workflow",
                json!({"name": "x", "is_global": true}),
            ),
        ] {
            assert!(
                global_memory_gate(tool, &args(arguments)).is_none(),
                "{tool} is not a memory tool and must not be gated"
            );
        }
    }

    // --- robustness of the argument read ----------------------------------

    /// The extension prefix is configuration, not a contract: the gate keys off
    /// the tool's own name so a memory server mounted under another name is
    /// still gated.
    #[test]
    fn the_extension_prefix_is_not_load_bearing() {
        for tool in [
            "retrieve_memories",
            "memory__retrieve_memories",
            "personal_memory__retrieve_memories",
        ] {
            assert!(
                global_memory_gate(tool, &args(json!({"category": "c", "is_global": true})))
                    .is_some(),
                "{tool} must be recognised as a memory retrieval"
            );
        }
    }

    /// Models send booleans as strings often enough that a strict `as_bool`
    /// would be a silent bypass. Anything that reads as true counts as global;
    /// a missing flag does not (the tool's own default, and its schema, is
    /// local).
    #[test]
    fn a_stringly_typed_global_flag_still_gates() {
        for flag in [json!(true), json!("true"), json!("True"), json!(1)] {
            assert!(
                global_memory_gate(
                    "memory__retrieve_memories",
                    &args(json!({"category": "c", "is_global": flag}))
                )
                .is_some(),
                "is_global={flag} must gate"
            );
        }
        for flag in [json!(false), json!("false"), json!(0), json!(null)] {
            assert!(
                global_memory_gate(
                    "memory__retrieve_memories",
                    &args(json!({"category": "c", "is_global": flag}))
                )
                .is_none(),
                "is_global={flag} is not a global call"
            );
        }
        assert!(
            global_memory_gate("memory__retrieve_memories", &args(json!({"category": "c"})))
                .is_none(),
            "an absent is_global is a local call"
        );
    }

    /// `"*"` is the sentinel only when it is exactly `"*"` — that is how the
    /// memory server dispatches it (issue #73 keeps it a legal plain name too),
    /// so a padded `" * "` is an ordinary category and must be asked about, not
    /// reported to the user as a whole-store read.
    #[test]
    fn only_the_exact_star_is_the_bulk_sentinel() {
        assert!(
            matches!(
                global_memory_gate(
                    "memory__retrieve_memories",
                    &args(json!({"category": " * ", "is_global": true}))
                ),
                Some(GlobalMemoryGate::Ask(_))
            ),
            "a padded star is a plain category name, not the sentinel"
        );
    }

    // --- the execute_code bypass ------------------------------------------

    /// `execute_code`'s inner tool calls are dispatched straight through the
    /// extension manager and never reach an inspector, so a global read hidden
    /// in the script would otherwise run unseen (the same gap `sensitive_ops`
    /// documents as R2-01).
    #[test]
    fn a_global_read_hidden_in_execute_code_is_escalated() {
        let code = r#"import { retrieve_memories } from "memory";
const all = retrieve_memories({ category: "*", is_global: true });
record_result(all);"#;
        assert!(
            matches!(
                global_memory_gate("code_execution__execute_code", &args(json!({"code": code}))),
                Some(GlobalMemoryGate::Ask(_))
            ),
            "an embedded global memory read must escalate the execute_code call"
        );
    }

    // --- the uninspected dispatch boundaries ------------------------------

    /// #63 review, finding 3. The scan above reads *source text*, so it can
    /// always be out-computed — the review's one-liner is `const flag = true;
    /// retrieve_memories({category:"clinical", is_global:flag})`, which has no
    /// truthy literal after `is_global`. The boundary check reads the
    /// *dispatched call*, where `flag` has already become `true`, so the shape
    /// of the source is irrelevant to it.
    #[test]
    fn a_boundary_refusal_does_not_depend_on_how_the_call_was_written() {
        for boundary in [
            UninspectedBoundary::ExecuteCodeScript,
            UninspectedBoundary::AgentCallToolRoute,
        ] {
            for (tool, arguments) in [
                (
                    "memory__retrieve_memories",
                    json!({"category": "clinical", "is_global": true}),
                ),
                (
                    "memory__retrieve_memories",
                    json!({"category": "*", "is_global": true}),
                ),
                (
                    "memory__remember_memory",
                    json!({"category": "clinical", "data": "x", "is_global": true}),
                ),
                (
                    "memory__remove_memory_category",
                    json!({"category": "clinical", "is_global": true}),
                ),
                (
                    "memory__remove_memory_category",
                    json!({"category": "*", "is_global": true}),
                ),
                (
                    "memory__remove_specific_memory",
                    json!({"category": "clinical", "memory_content": "x", "is_global": true}),
                ),
                // Mounted under another name, and stringly-typed flag: the same
                // leniencies the gate itself applies.
                (
                    "personal_memory__retrieve_memories",
                    json!({"category": "clinical", "is_global": "true"}),
                ),
            ] {
                let refusal =
                    uninspected_boundary_refusal(tool, Some(&args(arguments.clone())), boundary);
                let message = refusal.unwrap_or_else(|| {
                    panic!("{boundary:?} let {tool} {arguments} through unasked")
                });
                assert!(
                    message.contains("machine-wide"),
                    "the refusal has to say what it protects: {message}"
                );
            }
        }
    }

    /// The boundary refuses machine-wide memory, not memory and not the
    /// boundary. Everything else keeps working, or the fix is an outage.
    #[test]
    fn a_boundary_leaves_local_memory_and_every_other_tool_alone() {
        for (tool, arguments) in [
            (
                "memory__retrieve_memories",
                json!({"category": "*", "is_global": false}),
            ),
            (
                "memory__remember_memory",
                json!({"category": "dev", "data": "x", "is_global": false}),
            ),
            (
                "memory__remove_memory_category",
                json!({"category": "*", "is_global": false}),
            ),
            // No flag at all is a local call, exactly as the gate reads it.
            ("memory__retrieve_memories", json!({"category": "dev"})),
            ("developer__shell", json!({"command": "ls"})),
            (
                "workflow__save_workflow",
                json!({"name": "x", "is_global": true}),
            ),
        ] {
            assert!(
                uninspected_boundary_refusal(
                    tool,
                    Some(&args(arguments.clone())),
                    UninspectedBoundary::ExecuteCodeScript
                )
                .is_none(),
                "{tool} {arguments} must pass the boundary"
            );
        }
        assert!(
            uninspected_boundary_refusal(
                "memory__retrieve_memories",
                None,
                UninspectedBoundary::ExecuteCodeScript
            )
            .is_none(),
            "a call with no arguments cannot be a global one"
        );
    }

    /// A refusal that only says no is a dead end: the model retries, or tells
    /// the user the feature is broken. Each boundary names the route that does
    /// work.
    #[test]
    fn each_boundary_names_the_way_through() {
        let global = args(json!({"category": "clinical", "is_global": true}));

        let script = uninspected_boundary_refusal(
            "memory__retrieve_memories",
            Some(&global),
            UninspectedBoundary::ExecuteCodeScript,
        )
        .unwrap();
        assert!(
            script.contains("execute_code") && script.contains("retrieve_memories"),
            "the script refusal must say to make the call directly instead: {script}"
        );

        let route = uninspected_boundary_refusal(
            "memory__retrieve_memories",
            Some(&global),
            UninspectedBoundary::AgentCallToolRoute,
        )
        .unwrap();
        assert!(
            route.contains("conversation"),
            "the route refusal must point at the inspected path: {route}"
        );
    }

    #[test]
    fn execute_code_without_global_memory_is_not_escalated() {
        for code in [
            // Local memory work inside a script is ordinary.
            r#"import { retrieve_memories } from "memory";
retrieve_memories({ category: "*", is_global: false });"#,
            // No memory at all.
            r#"import { shell } from "developer";
shell({ command: "ls -la /tmp" });"#,
            // `is_global` belonging to something that is not a memory call.
            r#"import { save_workflow } from "workflow";
save_workflow({ name: "x", is_global: true });"#,
        ] {
            assert!(
                global_memory_gate("code_execution__execute_code", &args(json!({"code": code})))
                    .is_none(),
                "must not escalate:\n{code}"
            );
        }
    }

    // --- the inspector, end to end ----------------------------------------

    fn request(id: &str, name: &str, arguments: Value) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams {
                task: None,
                name: name.to_string().into(),
                arguments: Some(args(arguments)),
                meta: None,
            }),
            metadata: None,
            tool_meta: None,
        }
    }

    /// Auto mode is the whole point: the permission inspector approves every
    /// tool there deterministically, so the gate has to fire anyway.
    #[tokio::test]
    async fn auto_mode_still_gates_a_global_read() {
        let results = GlobalMemoryInspector
            .inspect(
                &[request(
                    "r1",
                    "memory__retrieve_memories",
                    json!({"category": "clinical", "is_global": true}),
                )],
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();
        let r = results
            .first()
            .expect("a global read must be gated in Auto");
        assert!(matches!(
            r.action,
            InspectionAction::RequireApproval(Some(_))
        ));
        assert_eq!(r.inspector_name, GLOBAL_MEMORY_INSPECTOR_NAME);
        assert!(r.finding_id.as_deref().unwrap_or("").starts_with("GMEM-"));
    }

    /// Unlike `sensitive_ops`, this gate is **not** inert outside Auto: a
    /// `SmartApprove` read-only grade would otherwise auto-approve the very same
    /// undisclosed cross-session read.
    #[tokio::test]
    async fn every_tool_running_mode_gates_a_global_read() {
        for mode in [
            BioRouterMode::Auto,
            BioRouterMode::Approve,
            BioRouterMode::SmartApprove,
        ] {
            let results = GlobalMemoryInspector
                .inspect(
                    &[request(
                        "r1",
                        "memory__retrieve_memories",
                        json!({"category": "clinical", "is_global": true}),
                    )],
                    &[],
                    mode,
                    &crate::session::Session::default(),
                )
                .await
                .unwrap();
            assert_eq!(results.len(), 1, "{mode:?} must gate the global read");
        }
    }

    #[tokio::test]
    async fn chat_mode_is_inert() {
        let results = GlobalMemoryInspector
            .inspect(
                &[request(
                    "r1",
                    "memory__retrieve_memories",
                    json!({"category": "clinical", "is_global": true}),
                )],
                &[],
                BioRouterMode::Chat,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();
        assert!(results.is_empty(), "Chat dispatches no tools at all");
    }

    #[tokio::test]
    async fn the_whole_store_read_reaches_the_pipeline_as_a_deny() {
        let results = GlobalMemoryInspector
            .inspect(
                &[request(
                    "r1",
                    "memory__retrieve_memories",
                    json!({"category": "*", "is_global": true}),
                )],
                &[],
                BioRouterMode::Auto,
                &crate::session::Session::default(),
            )
            .await
            .unwrap();
        let r = results.first().expect("the bulk read must be gated");
        assert_eq!(r.action, InspectionAction::Deny);
        assert!(
            !r.reason.trim().is_empty(),
            "a refusal with no reason reaches the model as \"the user declined\", \
             which is both untrue and unactionable"
        );
    }
}
