//! End-to-end tests for reaching `chatrecall` **through the `code_execution` JS
//! sandbox** — the only route the model has once `code_execution` is enabled
//! (`reply_parts::survives_code_execution_filter` strips every other tool from
//! the model's list, and `code_execution` is `default_enabled: true`).
//!
//! Issue #93: `chatrecall` is the one built-in whose extension key equals its
//! own tool name, so `create_server_module`'s server-name export landed on the
//! same Boa module binding as the tool export and overwrote the function with a
//! plain object. Every documented call form then threw
//! `TypeError: not a callable function`, and the tool's Rust handler was never
//! entered — a total outage for chat recall in the shipped default config.
//!
//! These tests deliberately go through `dispatch_tool_call` with a *real*
//! `ExtensionManager`, a *real* `SessionManager` and *real* seeded transcripts,
//! so they fail if any layer between the JS binding and the SQLite FTS index is
//! wrong — not just the binding.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, RawContent};
use rmcp::object;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use biorouter::agents::extension::ExtensionConfig;
use biorouter::agents::extension_manager::ExtensionManager;
use biorouter::conversation::message::Message;
use biorouter::session::{SessionManager, SessionType};

const CALLER: &str = "chatrecall-ce-caller";

struct Harness {
    _temp: tempfile::TempDir,
    sm: Arc<SessionManager>,
    manager: Arc<ExtensionManager>,
}

impl Harness {
    /// `chatrecall` + `code_execution`, both as platform extensions, over one
    /// isolated session store. Deliberately does NOT add `developer`: this must
    /// pass with chatrecall as the only non-code_execution extension.
    async fn new() -> Self {
        let _temp = tempfile::TempDir::new().unwrap();
        let sm = Arc::new(SessionManager::new(_temp.path().to_path_buf()));
        let manager = Arc::new(ExtensionManager::new(
            Arc::new(Mutex::new(None)),
            Arc::clone(&sm),
        ));

        manager
            .add_extension(ExtensionConfig::Platform {
                name: "chatrecall".to_string(),
                description: "Search past conversations".to_string(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .expect("add chatrecall");

        manager
            .add_extension(ExtensionConfig::Platform {
                name: "code_execution".to_string(),
                description: "Execute JavaScript code in a sandboxed environment".to_string(),
                bundled: Some(true),
                available_tools: vec![],
            })
            .await
            .expect("add code_execution");

        Self { _temp, sm, manager }
    }

    /// Seed one past conversation the search is expected to find.
    async fn seed(&self, name: &str, texts: &[&str]) -> String {
        let s = self
            .sm
            .create_session(
                PathBuf::from("/tmp/seeded"),
                name.to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        for t in texts {
            self.sm
                .add_message(&s.id, &Message::user().with_text(*t))
                .await
                .unwrap();
        }
        s.id
    }

    /// Run `execute_code` exactly as the agent loop does and return
    /// `(is_error, joined_text)`.
    async fn exec(&self, code: &str) -> (bool, String) {
        let call = CallToolRequestParams {
            task: None,
            meta: None,
            name: "code_execution__execute_code".into(),
            arguments: Some(object!({ "code": code })),
        };
        let dispatched = self
            .manager
            .dispatch_tool_call(
                CALLER,
                call,
                biorouter::privacy::CallCapability::public_enforced(),
                CancellationToken::new(),
            )
            .await
            .expect("dispatch");
        let result = dispatched.result.await.expect("tool result");
        let text = result
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        (result.is_error.unwrap_or(false), text)
    }

    /// Assert a script ran and did not hit the collision bug.
    async fn exec_ok(&self, code: &str) -> String {
        let (is_error, text) = self.exec(code).await;
        assert!(
            !text.contains("not a callable function"),
            "issue #93 regression: the chatrecall module export is not callable.\n{text}"
        );
        assert!(!is_error, "execute_code reported an error: {text}");
        text
    }
}

/// The exact call form `code_execution__read_module("chatrecall")` prints to the
/// model — `chatrecall["chatrecall"]({...})`. This is the form the reported
/// session used, and the one that threw.
#[tokio::test]
async fn read_modules_documented_call_form_reaches_chatrecall() {
    let h = Harness::new().await;
    h.seed(
        "SPOKE graph work",
        &["We queried the SPOKE biomedical knowledge graph for MS genes"],
    )
    .await;

    let text = h
        .exec_ok(
            r#"import * as chatrecall from "chatrecall";
record_result(chatrecall["chatrecall"]({ query: "SPOKE", limit: 25 }));"#,
        )
        .await;

    assert!(
        text.contains("SPOKE"),
        "the search ran but did not return the seeded SPOKE conversation:\n{text}"
    );
}

/// Every import form `read_module` / the system prompt tell the model to use
/// must reach the tool, not just the one form.
#[tokio::test]
async fn every_documented_import_form_reaches_chatrecall() {
    let h = Harness::new().await;
    h.seed("SPOKE graph work", &["SPOKE knowledge graph traversal"])
        .await;

    for (label, code) in [
        (
            "namespace bracket",
            r#"import * as chatrecall from "chatrecall";
record_result(chatrecall["chatrecall"]({ query: "SPOKE" }));"#,
        ),
        (
            "namespace dot",
            r#"import * as chatrecall from "chatrecall";
record_result(chatrecall.chatrecall({ query: "SPOKE" }));"#,
        ),
        (
            "named import",
            r#"import { chatrecall } from "chatrecall";
record_result(chatrecall({ query: "SPOKE" }));"#,
        ),
        (
            "server-named import",
            r#"import { chatrecall as srv } from "chatrecall";
record_result(srv.chatrecall({ query: "SPOKE" }));"#,
        ),
    ] {
        let text = h.exec_ok(code).await;
        assert!(
            text.contains("SPOKE"),
            "import form `{label}` did not return the seeded conversation:\n{text}"
        );
    }
}

/// `typeof` must be `function`. This is the single value that was wrong — the
/// server-name export made it `object` — and it is the cheapest possible guard.
#[tokio::test]
async fn the_chatrecall_export_is_a_function_not_an_object() {
    let h = Harness::new().await;
    let text = h
        .exec_ok(
            r#"import * as ns from "chatrecall";
record_result({ typeofExport: typeof ns.chatrecall, typeofBracket: typeof ns["chatrecall"] });"#,
        )
        .await;
    assert!(
        text.contains("\"typeofExport\": \"function\"")
            || text.contains("\"typeofExport\":\"function\""),
        "the module export must be callable, got:\n{text}"
    );
}

/// The tool's Rust handler must actually be entered and must actually search:
/// a query that matches nothing returns the "No results" answer rather than a
/// JS error, and a query that matches returns the session by name.
#[tokio::test]
async fn search_mode_returns_real_hits_and_real_misses() {
    let h = Harness::new().await;
    h.seed("Diabetes cohort", &["We built an OMOP diabetes cohort"])
        .await;
    h.seed("SPOKE graph work", &["SPOKE knowledge graph traversal"])
        .await;

    let hit = h
        .exec_ok(
            r#"import { chatrecall } from "chatrecall";
record_result(chatrecall({ query: "SPOKE" }));"#,
        )
        .await;
    assert!(
        hit.contains("SPOKE graph work"),
        "search did not name the matching session:\n{hit}"
    );
    assert!(
        !hit.contains("Diabetes cohort"),
        "search returned an unrelated session:\n{hit}"
    );

    let miss = h
        .exec_ok(
            r#"import { chatrecall } from "chatrecall";
record_result(chatrecall({ query: "zzzznotarealtermzzzz" }));"#,
        )
        .await;
    assert!(
        miss.contains("No results found"),
        "a miss must be a clean empty answer, not an error:\n{miss}"
    );
}

/// LOAD mode (`session_id`) is the tool's other half and travels the same
/// broken binding, so it needs its own coverage.
#[tokio::test]
async fn load_mode_returns_a_session_summary() {
    let h = Harness::new().await;
    let id = h
        .seed(
            "SPOKE graph work",
            &[
                "first message about SPOKE",
                "second message",
                "third message",
            ],
        )
        .await;

    let code = format!(
        r#"import {{ chatrecall }} from "chatrecall";
record_result(chatrecall({{ session_id: "{id}" }}));"#
    );
    let text = h.exec_ok(&code).await;

    assert!(
        text.contains("SPOKE graph work") && text.contains(&id),
        "load mode did not return the session summary:\n{text}"
    );
    assert!(
        text.contains("first message about SPOKE"),
        "load mode did not include the session's messages:\n{text}"
    );
}

/// The result must be usable *as data* inside the script — the model's whole
/// reason for being in the sandbox. A tool result that cannot be inspected is
/// only half fixed.
#[tokio::test]
async fn the_result_can_be_processed_inside_the_script() {
    let h = Harness::new().await;
    h.seed("SPOKE graph work", &["SPOKE knowledge graph traversal"])
        .await;

    let text = h
        .exec_ok(
            r#"import { chatrecall } from "chatrecall";
const r = chatrecall({ query: "SPOKE" });
const s = typeof r === "string" ? r : JSON.stringify(r);
record_result({ kind: typeof r, mentionsSpoke: s.includes("SPOKE"), length: s.length });"#,
        )
        .await;

    assert!(
        text.contains("\"mentionsSpoke\": true") || text.contains("\"mentionsSpoke\":true"),
        "the script could not inspect the chatrecall result:\n{text}"
    );
}

/// `read_module("chatrecall")` must print a call form that actually works.
/// Before the fix it printed `chatrecall["chatrecall"]({...})`, which threw —
/// the tool catalogue was actively teaching the model a broken incantation.
#[tokio::test]
async fn read_module_prints_a_call_form_that_executes() {
    let h = Harness::new().await;
    h.seed("SPOKE graph work", &["SPOKE knowledge graph traversal"])
        .await;

    let call = CallToolRequestParams {
        task: None,
        meta: None,
        name: "code_execution__read_module".into(),
        arguments: Some(object!({ "module_path": "chatrecall" })),
    };
    let dispatched = h
        .manager
        .dispatch_tool_call(
            CALLER,
            call,
            biorouter::privacy::CallCapability::public_enforced(),
            CancellationToken::new(),
        )
        .await
        .expect("dispatch read_module");
    let result = dispatched.result.await.expect("read_module result");
    let doc = result
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        doc.contains(r#"import * as chatrecall from "chatrecall""#),
        "read_module did not print the import line:\n{doc}"
    );
    assert!(
        doc.contains(r#"chatrecall["chatrecall"]"#),
        "read_module no longer prints the bracket call form; update this test:\n{doc}"
    );

    // Now run precisely what it told the model to run.
    let text = h
        .exec_ok(
            r#"import * as chatrecall from "chatrecall";
record_result(chatrecall["chatrecall"]({ query: "SPOKE" }));"#,
        )
        .await;
    assert!(
        text.contains("SPOKE"),
        "the printed form ran but found nothing:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// Defects found while unblocking the tool. Each one was measured against a real
// 11,672-session store before it was fixed, and each had ZERO coverage.
// ---------------------------------------------------------------------------

impl Harness {
    /// A session whose messages are supplied as whole `Message` values, so a
    /// test can seed tool traffic and thinking blocks rather than only prose.
    async fn seed_messages(&self, name: &str, messages: Vec<Message>) -> String {
        let s = self
            .sm
            .create_session(
                PathBuf::from("/tmp/seeded"),
                name.to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        for m in &messages {
            self.sm.add_message(&s.id, m).await.unwrap();
        }
        s.id
    }
}

/// SEARCH must name the session. `sessions.description` has been a dead column
/// since the title moved to `sessions.name`; reading it alone rendered every hit
/// as `Session:  (ID: …)`. Measured on a real store: 11,672 sessions, 0 with a
/// description, 11,672 with a name.
#[tokio::test]
async fn search_results_carry_the_session_title() {
    let h = Harness::new().await;
    h.seed("OMOP diabetes cohort", &["cohort characterisation notes"])
        .await;

    let text = h
        .exec_ok(
            r#"import { chatrecall } from "chatrecall";
record_result(chatrecall({ query: "cohort" }));"#,
        )
        .await;

    assert!(
        text.contains("OMOP diabetes cohort"),
        "the hit must be named, not rendered as `Session:  (ID: …)`:\n{text}"
    );
    assert!(
        !text.contains("Session:  ("),
        "an empty session title leaked through:\n{text}"
    );
}

/// `after_date` is inclusive of its own day.
///
/// `messages.timestamp` is TEXT in `%F %T` form; sqlx encodes a `DateTime` as
/// RFC3339, whose 'T' sorts after the space, so binding one silently discarded
/// the whole boundary day. The reporter's `daily-meditation.yaml` scheduled job
/// searches "with a recent date range", so this quietly dropped the most recent
/// day's chats — the ones it most wanted.
#[tokio::test]
async fn date_filters_include_their_own_boundary_day() {
    let h = Harness::new().await;
    h.seed("SPOKE graph work", &["SPOKE knowledge graph traversal"])
        .await;

    // Today, at midnight: the message was written seconds ago, so an
    // `after_date` of today 00:00:00 must still find it.
    let today = chrono::Utc::now().format("%Y-%m-%dT00:00:00Z").to_string();
    let code = format!(
        r#"import {{ chatrecall }} from "chatrecall";
record_result(chatrecall({{ query: "SPOKE", after_date: "{today}" }}));"#
    );
    let text = h.exec_ok(&code).await;

    assert!(
        text.contains("SPOKE graph work"),
        "an after_date of today 00:00 dropped a message written today:\n{text}"
    );

    // And the filter must still filter: tomorrow excludes it.
    let tomorrow = (chrono::Utc::now() + chrono::Duration::days(1))
        .format("%Y-%m-%dT00:00:00Z")
        .to_string();
    let code = format!(
        r#"import {{ chatrecall }} from "chatrecall";
record_result(chatrecall({{ query: "SPOKE", after_date: "{tomorrow}" }}));"#
    );
    let text = h.exec_ok(&code).await;
    assert!(
        text.contains("No results found"),
        "an after_date in the future must exclude everything:\n{text}"
    );
}

/// LOAD's first/last windows must not overlap. At 4 and 5 messages the old
/// arithmetic printed the middle ones twice, under two different numbers — the
/// only two sizes where the windows meet, which is why 3 and 6 looked fine.
#[test_case::test_case(4; "four_messages")]
#[test_case::test_case(5; "five_messages")]
#[test_case::test_case(6; "six_messages")]
#[tokio::test]
async fn load_mode_never_prints_a_message_twice(total: usize) {
    let h = Harness::new().await;
    let bodies: Vec<String> = (1..=total).map(|i| format!("unique-body-{i}")).collect();
    let id = h
        .seed_messages(
            "Overlap check",
            bodies
                .iter()
                .map(|b| Message::user().with_text(b.clone()))
                .collect(),
        )
        .await;

    let code = format!(
        r#"import {{ chatrecall }} from "chatrecall";
record_result(chatrecall({{ session_id: "{id}" }}));"#
    );
    let text = h.exec_ok(&code).await;

    for body in &bodies {
        // `unique-body-1` is a prefix of nothing else, but `unique-body-1` vs
        // `unique-body-10` would collide at total >= 10; the fixtures stop at 6.
        let seen = text.matches(body.as_str()).count();
        assert!(
            seen <= 1,
            "`{body}` was printed {seen} times at total={total} — the first/last \
             windows overlap:\n{text}"
        );
    }
}

/// LOAD must render tool traffic. `MessageContent::as_text()` returns `None` for
/// a tool call, a tool response and a thinking block, so a message carrying only
/// those printed its header and an empty body — 62% of messages in a real store.
#[tokio::test]
async fn load_mode_renders_messages_that_carry_no_plain_text() {
    let h = Harness::new().await;
    let tool_call = rmcp::model::CallToolRequestParams {
        task: None,
        meta: None,
        name: "developer__shell".into(),
        arguments: Some(object!({ "command": "rg SPOKE" })),
    };
    let id = h
        .seed_messages(
            "Tool-only session",
            vec![
                Message::user().with_text("find SPOKE"),
                Message::assistant().with_tool_request("call_1", Ok(tool_call)),
            ],
        )
        .await;

    let code = format!(
        r#"import {{ chatrecall }} from "chatrecall";
record_result(chatrecall({{ session_id: "{id}" }}));"#
    );
    let text = h.exec_ok(&code).await;

    assert!(
        text.contains("developer__shell") || text.contains("[Tool:"),
        "a tool-call message rendered as an empty body:\n{text}"
    );
    assert!(
        !text.contains("[no renderable content]"),
        "a tool call should render as tool traffic, not as a fallback:\n{text}"
    );
}

/// SEARCH clips each message. Unbounded bodies produced ~195k tokens for one
/// ordinary query at `limit: 50` — about 8x the inline cap — which pushes the
/// whole answer into a file the model then has to grep.
#[tokio::test]
async fn search_clips_long_messages_and_says_so() {
    let h = Harness::new().await;
    let long = format!("SPOKE {}", "x".repeat(20_000));
    h.seed("Long message", &[long.as_str()]).await;

    let text = h
        .exec_ok(
            r#"import { chatrecall } from "chatrecall";
record_result(chatrecall({ query: "SPOKE" }));"#,
        )
        .await;

    assert!(
        text.contains("truncated"),
        "a 20k-character message was rendered without a truncation marker:\n{}",
        text.chars().take(600).collect::<String>()
    );
    assert!(
        text.len() < 8_000,
        "the clipped result is still {} chars — the excerpt cap is not applied",
        text.len()
    );
}

/// When the search hits its own `limit`, the headline must not present the
/// capped count as the total. It used to say "Found 2 matching message(s)" when
/// 2 was simply the cap.
#[tokio::test]
async fn search_discloses_when_the_limit_truncated_the_answer() {
    let h = Harness::new().await;
    for i in 0..5 {
        h.seed(&format!("SPOKE session {i}"), &["SPOKE knowledge graph"])
            .await;
    }

    let text = h
        .exec_ok(
            r#"import { chatrecall } from "chatrecall";
record_result(chatrecall({ query: "SPOKE", limit: 2 }));"#,
        )
        .await;

    assert!(
        text.contains("at least"),
        "a capped search must not report its cap as the total:\n{text}"
    );
    assert!(
        text.contains("limit"),
        "a capped search must name the lever that widens it:\n{text}"
    );

    // And an uncapped search must NOT carry the disclosure.
    let uncapped = h
        .exec_ok(
            r#"import { chatrecall } from "chatrecall";
record_result(chatrecall({ query: "zzzunmatchedzzz OR SPOKE", limit: 50 }));"#,
        )
        .await;
    assert!(
        !uncapped.contains("at least"),
        "an uncapped search must not claim it was truncated:\n{uncapped}"
    );
}

/// LOAD must be bounded too. Making LOAD render tool traffic (so a tool-only
/// message stops printing an empty body) also made it render the tool's
/// ARGUMENTS, and a `text_editor` write carries a whole file in those. Without a
/// cap, "show me the first few messages" can return a hundred kilobytes.
#[tokio::test]
async fn load_mode_bounds_a_message_that_carries_a_huge_tool_payload() {
    let h = Harness::new().await;
    let huge = "y".repeat(60_000);
    let tool_call = rmcp::model::CallToolRequestParams {
        task: None,
        meta: None,
        name: "developer__text_editor".into(),
        arguments: Some(object!({ "command": "write", "file_text": huge })),
    };
    let id = h
        .seed_messages(
            "Big write",
            vec![
                Message::user().with_text("write the file"),
                Message::assistant().with_tool_request("call_1", Ok(tool_call)),
            ],
        )
        .await;

    let code = format!(
        r#"import {{ chatrecall }} from "chatrecall";
record_result(chatrecall({{ session_id: "{id}" }}));"#
    );
    let text = h.exec_ok(&code).await;

    assert!(
        text.contains("truncated"),
        "a 60k-character tool payload was rendered without a truncation marker"
    );
    assert!(
        text.len() < 20_000,
        "LOAD returned {} chars for a 2-message session — the cap is not applied",
        text.len()
    );
    // Still useful: the tool is named even though its payload was clipped.
    assert!(
        text.contains("text_editor"),
        "clipping must not hide WHICH tool ran:\n{}",
        text.chars().take(500).collect::<String>()
    );
}
