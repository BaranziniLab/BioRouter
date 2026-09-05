//! `platform__report_bug` — the agent files a Biorouter bug for the user.
//!
//! ## The shape, and why it is two calls and not one
//!
//! "Report a bug" is not one action. The model has to find out what went
//! wrong before it can write about it, and the two halves want different
//! answers when they fail:
//!
//! * [`Action::Analyze`] reads the session's own failure record and hands back
//!   a digest. It **files nothing** and needs no approval. When it cannot tell
//!   what went wrong and the user has not said, it answers with a question and
//!   an instruction not to file — the push-back.
//! * [`Action::File`] takes the report the model wrote, scrubs it, checks it
//!   against the harness, parks a proof-backed approval showing the exact body,
//!   and only then posts.
//!
//! A single call would have to guess, and the guess it would make is the one
//! that files. The action is inferred when the model does not say — a call
//! carrying a title and a description means "file", anything else means
//! "analyze" — because a model that omits an enum should land on the half that
//! cannot publish.
//!
//! ## What the user is agreeing to
//!
//! The approval card carries the **rendered body, verbatim**, in its preview,
//! and its prompt names the destination repository and says whether pressing
//! the button publishes immediately or opens a page the user still has to
//! submit. Those two facts are the whole consent: a card that said "file a bug
//! report?" would be asking about a category, not about the paragraph that is
//! going to be world-readable.
//!
//! It is modelled on `install_extension` (`extension_manager_extension.rs`),
//! the one other tool here that gathers local state, blocks on a proof-backed
//! card and then takes an outward-facing action — including its ordering:
//! preflight, await approval, **re-check that nothing changed between the card
//! and the click**, then act.
//!
//! ## The privacy ruling
//!
//! A GitHub issue is world-readable and permanent. A session classified
//! `Private` has touched a private model or a private data source, and the
//! report is written *from* that session by a model reading it — so nothing
//! here can certify the distillation carries none of it. The tool therefore
//! **refuses to post from a private session** and hands the finished report
//! back instead, naming what it refused and what to do with it.
//!
//! ⚠ It refuses rather than warns, and the asymmetry is deliberate: a user who
//! insists can still file the report themselves, with their own eyes on it.
//! What must not happen is the *agent* publishing private material because a
//! card was clicked.
//!
//! ⚠ It honours the DR-15 master switch, like every other gate. A switch that
//! some gates ignore is not a switch, and the card names the session's
//! classification either way, so the fact is in front of the user regardless.

pub mod evidence;
pub mod issue;
pub mod redact;

use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{Content, ErrorCode, ErrorData};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::conversation::Conversation;
use crate::mcp_utils::ToolResult;
use crate::pending_user_action::{
    PendingUserActions, ToolApprovalRequest, UserActionOutcome, UserActionRequest,
};
use crate::permission::tool_risk::ToolRisk;
use crate::privacy::SessionClassification;
use crate::session::session_manager::Session;
use crate::session::SessionManager;

use evidence::Evidence;
use issue::{Draft, Filer};

pub const REPORT_BUG_TOOL_NAME: &str = "platform__report_bug";

/// How long the approval card stands. Long enough for a user to read a whole
/// issue body — which is the point of showing it — and to fetch a colleague.
const APPROVAL_TTL: Duration = Duration::from_secs(15 * 60);

/// Which half of the tool was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Analyze,
    File,
}

impl Action {
    /// ⚠ Inference lands on `Analyze`, always. A model that omits the argument
    /// must not thereby publish something.
    fn infer(arguments: &Value) -> Self {
        match arguments.get("action").and_then(Value::as_str) {
            Some("file") => Self::File,
            Some(_) => Self::Analyze,
            None => {
                let has = |key: &str| {
                    arguments
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.trim().is_empty())
                };
                if has("title") && has("description") {
                    Self::File
                } else {
                    Self::Analyze
                }
            }
        }
    }
}

fn invalid_params(message: impl std::fmt::Display) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, message.to_string(), None)
}

fn text(body: String) -> ToolResult<Vec<Content>> {
    Ok(vec![Content::text(body)])
}

fn string_arg(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// `steps` may arrive as a list or as one newline-separated string; models
/// produce both and neither is wrong.
fn steps_arg(arguments: &Value) -> Vec<String> {
    match arguments.get("steps") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(|step| step.trim().to_string())
            .filter(|step| !step.is_empty())
            .collect(),
        Some(Value::String(value)) => value
            .lines()
            .map(|line| line.trim().trim_start_matches(['-', '*', '•']).trim())
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Is this session's content barred from a public destination?
///
/// ⚠ Pure, taking the master switch as an ARGUMENT rather than reading it. The
/// switch is a process-global atomic, and a test that flipped it to exercise
/// the second arm broke six unrelated privacy tests running concurrently in the
/// same binary — a failure that reads as a privacy hole in code nobody touched.
/// The same shape as `CallCapability`: sample once, thread the value, and the
/// decision becomes testable without a global to fight over.
fn refuses_public_disclosure(tier: SessionClassification, tiers_enabled: bool) -> bool {
    tiers_enabled && tier == SessionClassification::Private
}

/// The conversation to read the failures out of.
///
/// ⚠ **Read from the store, not from `session.conversation`**, even though the
/// row handed to `dispatch_tool_call` usually carries one. That copy is the
/// snapshot `RewriteBasis::read_with_session` took at the TOP of this turn, so
/// it is missing the current user's own message ("report a bug") and every tool
/// call this turn has already made — which, for a report raised the moment
/// something failed, is precisely the evidence being asked about. Preferring it
/// to save a query is the shape of a bug that would only appear in the case the
/// tool exists for.
///
/// The store read is a superset, so the turn-start snapshot is kept only as a
/// fallback for a read that fails outright: a degraded report beats none.
async fn conversation_for(
    session: &Session,
    session_manager: &SessionManager,
) -> ToolResult<Conversation> {
    match session_manager.get_session(&session.id, true).await {
        Ok(loaded) => Ok(loaded
            .conversation
            .or_else(|| session.conversation.clone())
            .unwrap_or_default()),
        Err(error) => session.conversation.clone().ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("could not read this chat's history: {error}"),
                None,
            )
        }),
    }
}

/// The digest handed back by [`Action::Analyze`], and spliced into the file
/// path's receipt so the two can never describe the same session differently.
fn render_digest(evidence: &Evidence, scrubbed_working_dir: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Biorouter v{} on {} {} ({}), provider {} / model {}, working directory {}.\n",
        evidence.app_version,
        evidence.os,
        evidence.os_version,
        evidence.architecture,
        evidence.provider.as_deref().unwrap_or("not set"),
        evidence.model.as_deref().unwrap_or("not set"),
        scrubbed_working_dir,
    ));
    out.push_str(&format!(
        "Tool calls in this chat: {} total, {} failed.\n",
        evidence.total_tool_calls, evidence.total_failed_calls
    ));

    if evidence.failures.is_empty() {
        out.push_str("\nNo failed tool calls are recorded in this chat.\n");
        return out;
    }

    out.push_str("\nFailures, most repeated first:\n");
    for failure in &evidence.failures {
        out.push_str(&failure.to_line());
        out.push('\n');
        if let Some(arguments) = &failure.arguments {
            out.push_str(&format!("    arguments: {arguments}\n"));
        }
    }
    if evidence.failures.iter().any(|f| f.looks_deliberate) {
        out.push_str(
            "\nAt least one of those is Biorouter refusing ON PURPOSE — a privacy or \
             permission boundary doing its job. Do not file that as a defect unless the \
             user says the WRONG thing was refused.\n",
        );
    }
    out
}

/// Read the session and report what is there — or push back.
async fn analyze(
    evidence: &Evidence,
    user_description: Option<&str>,
    scrubbed_working_dir: &str,
) -> ToolResult<Vec<Content>> {
    let digest = render_digest(evidence, scrubbed_working_dir);

    // ⚠ The push-back. Nothing here is an error: an error invites a retry, and
    // a retry cannot produce information the model does not have. It has to go
    // and ask.
    if user_description.is_none() && !evidence.is_conclusive() {
        return text(format!(
            "I could not tell what went wrong from this chat on its own, so nothing has \
             been filed and nothing will be until you say what to report.\n\n\
             {digest}\n\
             ASK THE USER what they want to report, and be specific — name what you can \
             see above and ask whether that is the problem, or whether it is something \
             else (something looked wrong on screen, an answer was incorrect, the app was \
             slow, a control did nothing). Then call this tool again with \
             `action: \"file\"`, a `title`, a `description`, and `steps` if they gave \
             them.\n\n\
             Do NOT call `action: \"file\"` before they have answered, and do not invent \
             a report from the failures above."
        ));
    }

    let lead = match (user_description, evidence.headline()) {
        (Some(description), _) => format!("The user is reporting: {description}\n\n"),
        (None, Some(headline)) => {
            format!("The clearest failure in this chat is: {headline}\n\n")
        }
        (None, None) => String::new(),
    };

    text(format!(
        "{lead}{digest}\n\
         Write the report now and call this tool again with `action: \"file\"`. Give a \
         `title` a maintainer can recognise in a list, a `description` that says what \
         happened and why it is wrong, `steps` to reproduce it if you can honestly state \
         them, and `expected`. Say what you actually observed — do not pad the report \
         with guesses about the cause.\n\n\
         The environment, the version and the failure list above are added automatically; \
         do not repeat them. Home paths, usernames and anything credential-shaped are \
         removed before posting, and the user must approve the exact text before it goes \
         anywhere."
    ))
}

/// The refusal a private session gets, with the finished report attached so the
/// work is not thrown away.
fn private_session_refusal(title: &str, body: &str, repo: &str) -> String {
    format!(
        "This chat is classified PRIVATE, so I have not posted anything and I will not: \
         a GitHub issue on github.com/{repo} is public and permanent, and this report was \
         written from a conversation that has touched a private model or a private data \
         source. I cannot certify that the summary carries none of it, and that is not a \
         judgement to make on your behalf.\n\n\
         The report is written and checked — here it is, so you can read it and decide:\n\n\
         ----- title -----\n{title}\n\n----- body -----\n{body}\n----- end -----\n\n\
         If you want it filed, file it yourself once you have read it. For a private \
         report, prefer a private channel over the public tracker. A full diagnostics \
         bundle is under Chat summary → Diagnostics → Generate diagnostics; it contains \
         the transcript, so treat it the same way."
    )
}

/// Build the approval card.
///
/// The body rides in `arguments` rather than in `preview`'s structured shape:
/// `ToolPreview::Arguments` renders the arguments as pretty JSON, which is
/// exactly the right frame for "here is the text that will be published", and
/// it is the one variant every surface already knows how to draw.
fn approval_request(
    title: &str,
    body: &str,
    repo: &str,
    filer: &Filer,
    classification: SessionClassification,
) -> UserActionRequest {
    let arguments = serde_json::json!({
        "repository": format!("github.com/{repo}"),
        "title": title,
        "body": body,
        "labels": [issue::LABEL],
        "chatPrivacyTier": match classification {
            SessionClassification::Private => "private",
            SessionClassification::Public => "public",
        },
    })
    .as_object()
    .expect("the approval arguments are an object")
    .clone();

    let redirected = if repo == issue::DEFAULT_REPO {
        String::new()
    } else {
        format!(
            " ⚠ This is NOT the Biorouter project's own tracker (github.com/{}); it has \
             been redirected to github.com/{repo}.",
            issue::DEFAULT_REPO
        )
    };

    UserActionRequest::ToolApproval(ToolApprovalRequest {
        tool_name: REPORT_BUG_TOOL_NAME.to_string(),
        preview: crate::conversation::tool_preview::ToolPreview::for_tool_call(
            REPORT_BUG_TOOL_NAME,
            &arguments,
        ),
        arguments,
        prompt: Some(format!(
            "Publish this bug report to the public issue tracker? {}{redirected} Read the \
             body below: everything in it becomes world-readable and permanent.",
            filer.describe(repo)
        )),
        // High, not Medium: the act is irreversible and public. `install_extension`
        // is graded the same way for a change that is at least undoable.
        risk: Some(ToolRisk::High),
        requires_user_proof: true,
    })
}

/// The whole tool.
///
/// `session_manager` is a parameter rather than reached through an `Agent`, so
/// every branch below is exercisable without one.
pub async fn handle_report_bug(
    arguments: Value,
    session: &Session,
    session_manager: Arc<SessionManager>,
    cancel: Option<CancellationToken>,
) -> ToolResult<Vec<Content>> {
    let home = dirs::home_dir();
    let home = home.as_deref();
    let conversation = conversation_for(session, session_manager.as_ref()).await?;
    let evidence = evidence::collect(session, &conversation);
    let scrubbed_working_dir = redact::scrub(&evidence.working_dir, home).text;
    let user_description = string_arg(&arguments, "description")
        .or_else(|| string_arg(&arguments, "user_description"));

    if Action::infer(&arguments) == Action::Analyze {
        return analyze(
            &evidence,
            user_description.as_deref(),
            &scrubbed_working_dir,
        )
        .await;
    }
    file_report(
        &arguments,
        session,
        evidence,
        user_description,
        &scrubbed_working_dir,
        home,
        cancel,
    )
    .await
}

/// Scrub the model's prose, render the body, and run the harness over it.
///
/// Returns the draft, the rendered body and the title's own scrub result — the
/// last so the receipt can say what was removed without re-running anything.
///
/// Split out of [`file_report`] to stay under the `too_many_lines` baseline,
/// and because everything here is pure: no approval, no network, no clock.
fn prepare_report(
    arguments: &Value,
    user_description: Option<String>,
    evidence: Evidence,
    scrubbed_working_dir: &str,
    home: Option<&std::path::Path>,
) -> Result<(Draft, String, redact::Scrubbed), ErrorData> {
    let title = string_arg(arguments, "title").ok_or_else(|| {
        invalid_params(
            "`title` is required to file. Call this tool with `action: \"analyze\"` first \
             if you do not yet know what the report should say.",
        )
    })?;
    let description = user_description.ok_or_else(|| {
        invalid_params(
            "`description` is required to file: it becomes the report's \"Describe the \
             bug\" section. Ask the user what went wrong rather than inventing one.",
        )
    })?;

    // ⚠ Scrub the model's own prose FIRST. It writes the report from the
    // transcript, so it quotes the error it is reporting -- and a real error
    // message is where a home path, a bearer token or a signed URL lives.
    let title_scrub = redact::scrub(&title, home);
    let draft = Draft {
        title: title_scrub.text.clone(),
        description: redact::scrub(&description, home).text,
        steps: steps_arg(arguments)
            .iter()
            .map(|step| redact::scrub(step, home).text)
            .collect(),
        expected: string_arg(arguments, "expected")
            .map(|expected| redact::scrub(&expected, home).text)
            .unwrap_or_default(),
        additional: string_arg(arguments, "additional")
            .map(|additional| redact::scrub(&additional, home).text),
    };

    // The environment block is assembled from the evidence, whose working
    // directory is a real path; scrub the evidence too, not only the prose.
    let evidence = Evidence {
        working_dir: scrubbed_working_dir.to_string(),
        failures: evidence
            .failures
            .iter()
            .map(|failure| evidence::ToolFailure {
                message: redact::scrub(&failure.message, home).text,
                arguments: failure
                    .arguments
                    .as_ref()
                    .map(|args| redact::scrub(args, home).text),
                ..failure.clone()
            })
            .collect(),
        ..evidence
    };

    let body = issue::render_body(&draft, &evidence);

    // ⚠ The harness. It re-runs the scrub and refuses what it still finds,
    // rather than trusting the passes above to have been complete. A refusal
    // here is returned to the model with the reasons, so it can fix and retry —
    // this is the one failure in the tool that a retry can actually resolve.
    let violations = redact::validate_issue(&draft.title, &body, home);
    if !violations.is_empty() {
        return Err(invalid_params(format!(
            "The report was NOT filed: it did not pass Biorouter's own checks.\n{}\n\n\
             Fix these and call the tool again. If the problem is that identifying \
             material survived redaction, rewrite the offending text rather than \
             quoting it.",
            violations
                .iter()
                .map(|violation| format!("  - {violation}"))
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }

    Ok((draft, body, title_scrub))
}

/// The `file` half: write the report, check it, ask, post.
///
/// Split from [`handle_report_bug`] so that neither half is over the
/// `too_many_lines` baseline, and because the boundary is a real one — nothing
/// below here runs for an `analyze` call.
async fn file_report(
    arguments: &Value,
    session: &Session,
    evidence: Evidence,
    user_description: Option<String>,
    scrubbed_working_dir: &str,
    home: Option<&std::path::Path>,
    cancel: Option<CancellationToken>,
) -> ToolResult<Vec<Content>> {
    let (draft, body, title_scrub) = prepare_report(
        arguments,
        user_description,
        evidence,
        scrubbed_working_dir,
        home,
    )?;
    let repo = issue::repo();

    // Privacy: decided AFTER the report is written, deliberately. The work is
    // the expensive part and it is not wasted — the refusal hands it back.
    if refuses_public_disclosure(
        session.privacy_tier,
        crate::privacy::privacy_tiers_enabled(),
    ) {
        return text(private_session_refusal(&draft.title, &body, &repo));
    }

    let filer = if issue::gh_ready().await {
        Filer::GhCli
    } else {
        match issue::compose_url(&repo, &draft.title, &body) {
            Some(url) => Filer::ComposeUrl(url),
            None => Filer::Manual,
        }
    };

    if session.id.is_empty() {
        return Err(invalid_params(
            "Filing a bug report needs a visible chat, because the user has to approve \
             the exact text before it is published.",
        ));
    }

    let parked = PendingUserActions::global().park(
        Some(&session.id),
        None,
        approval_request(&draft.title, &body, &repo, &filer, session.privacy_tier),
    );
    match parked.wait(APPROVAL_TTL, cancel.as_ref()).await {
        UserActionOutcome::Approved { .. } => {}
        outcome => {
            return text(format!(
                "Nothing was filed: the approval {}. The report is written and ready — \
                 say the word and I will ask again.",
                outcome.refusal_detail()
            ));
        }
    }

    // `install_extension`'s re-check, for the same reason: approval binds the
    // exact thing shown, not merely the intention. A destination that changed
    // between the card and the click gets a new card.
    if issue::repo() != repo {
        return Err(invalid_params(format!(
            "The destination repository changed after you approved (it was \
             github.com/{repo}). Nothing was filed."
        )));
    }
    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return text(
            "The turn was cancelled after you approved, so nothing was filed.".to_string(),
        );
    }

    post_report(filer, &repo, &draft, &body, &title_scrub).await
}

/// Everything after the approval: do the thing, and say what happened.
///
/// Every arm returns an Ok result rather than an error, including the ones that
/// could not post. The user approved this exact text; handing it back to be
/// pasted is a worse outcome than an issue URL and a much better one than an
/// error that loses the report.
async fn post_report(
    filer: Filer,
    repo: &str,
    draft: &Draft,
    body: &str,
    title_scrub: &redact::Scrubbed,
) -> ToolResult<Vec<Content>> {
    let attach = "\n\nTo attach the full diagnostics bundle — transcript, redacted config, \
                  logs — open Chat summary → Diagnostics → Generate diagnostics and drag the \
                  zip onto the issue. Read it first: it contains the whole conversation.";

    match filer {
        Filer::GhCli => {
            let body_file = std::env::temp_dir()
                .join(format!("biorouter-bug-report-{}.md", uuid::Uuid::new_v4()));
            match issue::file_with_gh(repo, &draft.title, body, &body_file).await {
                Ok(url) => text(format!(
                    "Filed: {url}\n\nTitle: {}\nRedacted before posting: {}.{attach}",
                    draft.title,
                    title_scrub.summary()
                )),
                Err(error) => {
                    // Falling back rather than failing: the user approved this
                    // exact text, and a `gh` that broke is not a reason to make
                    // them start over.
                    let fallback = issue::compose_url(repo, &draft.title, body);
                    text(match fallback {
                        Some(url) => format!(
                            "The GitHub CLI could not create the issue ({error}), so nothing \
                             was posted. Everything else is ready — open this prefilled page \
                             and press Submit:\n\n{url}{attach}"
                        ),
                        None => format!(
                            "The GitHub CLI could not create the issue ({error}), so nothing \
                             was posted, and the report is too large for a prefilled link. \
                             Open https://github.com/{repo}/issues/new and paste this:\n\n\
                             ----- title -----\n{}\n\n----- body -----\n{body}",
                            draft.title
                        ),
                    })
                }
            }
        }
        Filer::ComposeUrl(url) => text(format!(
            "Nothing is posted yet. Open this prefilled page and press Submit to file \
             it:\n\n{url}\n\nRedacted before posting: {}.{attach}",
            title_scrub.summary()
        )),
        Filer::Manual => text(format!(
            "The report is ready but too large for a prefilled link, and the GitHub CLI \
             is not signed in on this machine. Open https://github.com/{repo}/issues/new \
             and paste this:\n\n----- title -----\n{}\n\n----- body -----\n{body}",
            draft.title
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent, ToolRequest, ToolResponse};
    use crate::pending_user_action::{DecisionAuthority, ResolveOutcome};
    use crate::permission::Permission;
    use crate::session::session_manager::SessionType;
    use rmcp::model::{CallToolRequestParams, CallToolResult, Content};
    use std::borrow::Cow;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn args(value: serde_json::Value) -> Value {
        value
    }

    /// A session with `n` hard failures already in its transcript, over an
    /// isolated store. The `TempDir` is returned because dropping it deletes the
    /// SQLite file the manager still holds.
    async fn session_with_failures(count: usize) -> (TempDir, Arc<SessionManager>, Session) {
        let dir = TempDir::new().unwrap();
        let manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
        let mut session = manager
            .create_session(
                PathBuf::from("/workspace/demo"),
                "bug-report".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();

        let mut content = Vec::new();
        for index in 0..count {
            let id = format!("call-{index}");
            content.push(MessageContent::ToolRequest(ToolRequest {
                id: id.clone(),
                tool_call: Ok(CallToolRequestParams {
                    task: None,
                    name: Cow::from("developer__shell"),
                    arguments: serde_json::json!({ "command": format!("build {index}") })
                        .as_object()
                        .cloned(),
                    meta: None,
                }),
                metadata: None,
                tool_meta: None,
            }));
            content.push(MessageContent::ToolResponse(ToolResponse {
                id,
                tool_result: Ok(CallToolResult {
                    content: vec![Content::text(format!(
                        "the panel rendered blank (failure {index})"
                    ))],
                    structured_content: None,
                    is_error: Some(true),
                    meta: None,
                }),
                metadata: None,
            }));
        }
        // ⚠ PERSISTED, not merely attached to the in-memory row. The handler
        // reads the store, because the row `dispatch_tool_call` is handed
        // carries a snapshot from the top of the turn and is missing everything
        // the turn has done since. A fixture that only set `session.conversation`
        // would exercise a path production never takes — and it did, until this
        // comment's change caught it.
        let message = content
            .into_iter()
            .fold(Message::assistant(), Message::with_content);
        manager.add_message(&session.id, &message).await.unwrap();
        session.conversation = Some(Conversation::new_unvalidated(vec![message]));
        (dir, manager, session)
    }

    fn body_of(result: &ToolResult<Vec<Content>>) -> String {
        result
            .as_ref()
            .expect("an Ok result")
            .iter()
            .filter_map(|content| content.as_text().map(|text| text.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn file_args() -> Value {
        args(serde_json::json!({
            "action": "file",
            "title": "Chart panel renders blank for a single-row dataset",
            "description": "The Auto Visualiser panel is empty when the dataset has one row.",
            "steps": ["Open a chat", "Ask for a chart of one row"],
            "expected": "The chart renders."
        }))
    }

    /// ⚠ The failure the report is about must be FOUND, even when the row the
    /// agent hands over does not know about it.
    ///
    /// `dispatch_tool_call` receives the `Session` that
    /// `RewriteBasis::read_with_session` snapshotted at the TOP of the turn, so
    /// its `conversation` is missing the current user's message and every tool
    /// call this turn has already made. Reading it "because it is already in
    /// hand" saves a query and loses exactly the evidence a report raised the
    /// moment something failed is about — a bug that appears only in the case
    /// the tool exists for, and in no test that builds its fixture by hand.
    ///
    /// This fixture reproduces the divergence deliberately: the store holds a
    /// hard failure, and the in-memory row holds an EMPTY conversation, which is
    /// what a turn-start snapshot looks like on the first turn.
    #[tokio::test]
    async fn the_store_is_read_rather_than_the_turn_start_snapshot() {
        let (_dir, manager, mut session) = session_with_failures(2).await;
        session.conversation = Some(Conversation::default());

        let result = handle_report_bug(
            args(serde_json::json!({"action": "analyze"})),
            &session,
            manager,
            None,
        )
        .await;
        let body = body_of(&result);
        assert!(
            body.contains("2 total, 2 failed"),
            "the turn-start snapshot was read instead of the store, so the failure \
             being reported is invisible: {body}"
        );
        assert!(!body.contains("ASK THE USER"), "{body}");
    }

    /// ⚠ The push-back. An empty session plus no user description must produce
    /// a QUESTION, not an issue — and not an error either, because an error
    /// invites a retry and a retry cannot produce information the model does
    /// not have.
    #[tokio::test]
    async fn an_inconclusive_session_with_no_description_asks_instead_of_filing() {
        let (_dir, manager, session) = session_with_failures(0).await;
        let result = handle_report_bug(args(serde_json::json!({})), &session, manager, None).await;
        let body = body_of(&result);
        assert!(body.contains("nothing has been filed"), "{body}");
        assert!(body.contains("ASK THE USER"), "{body}");
        assert!(
            body.contains("Do NOT call `action: \"file\"` before they have answered"),
            "{body}"
        );
    }

    /// The same empty session, but the user said what is wrong. There is
    /// nothing left to ask, so the tool gets on with it.
    #[tokio::test]
    async fn a_user_supplied_description_removes_the_need_to_push_back() {
        let (_dir, manager, session) = session_with_failures(0).await;
        let result = handle_report_bug(
            args(serde_json::json!({
                "action": "analyze",
                "description": "the window will not resize below 1200px"
            })),
            &session,
            manager,
            None,
        )
        .await;
        let body = body_of(&result);
        assert!(body.contains("The user is reporting"), "{body}");
        assert!(!body.contains("ASK THE USER"), "{body}");
    }

    /// Conclusive evidence stands in for a description.
    #[tokio::test]
    async fn a_conclusive_session_is_reported_without_pushing_back() {
        let (_dir, manager, session) = session_with_failures(3).await;
        let result = handle_report_bug(
            args(serde_json::json!({"action": "analyze"})),
            &session,
            manager,
            None,
        )
        .await;
        let body = body_of(&result);
        assert!(
            body.contains("The clearest failure in this chat is"),
            "{body}"
        );
        assert!(body.contains("`developer__shell`"), "{body}");
        assert!(!body.contains("ASK THE USER"), "{body}");
    }

    /// ⚠ Nothing is filed without an approval, and a refusal is reported
    /// honestly rather than as a success.
    ///
    /// `without_human_surface` is the production statement of "there is nobody
    /// to answer this": `park` registers nothing and the handle answers
    /// `Cancelled` at once, which is the same outcome a dismissal produces.
    #[tokio::test]
    async fn a_refused_approval_files_nothing_and_says_so() {
        let (_dir, manager, session) = session_with_failures(2).await;
        let result = crate::user_surface::without_human_surface(handle_report_bug(
            file_args(),
            &session,
            manager,
            None,
        ))
        .await;
        let body = body_of(&result);
        assert!(body.contains("Nothing was filed"), "{body}");
        assert!(
            body.contains("was cancelled before anyone answered it"),
            "{body}"
        );
    }

    /// The card carries the exact body, names the destination, and demands
    /// proof of a person.
    ///
    /// ⚠ All three are the consent. A card that said "file a bug report?" would
    /// be asking about a category; the user is agreeing to a specific paragraph
    /// becoming world-readable at a specific address.
    /// ⚠ These tests probe the real `gh` on the machine they run on, so the
    /// filer they pick differs between a developer's laptop and CI. Every
    /// assertion below is therefore branch-INDEPENDENT; do not add one that
    /// reads `Filer::GhCli`'s wording without pinning the branch first. Nothing
    /// here can post: `issue::file_with_gh` refuses outright under `cfg!(test)`,
    /// and this test denies the card in any case.
    #[tokio::test]
    async fn the_approval_card_shows_the_exact_body_and_the_destination() {
        let (_dir, manager, session) = session_with_failures(2).await;
        let session_id = session.id.clone();

        let running = tokio::spawn({
            let manager = Arc::clone(&manager);
            let session = session.clone();
            async move { handle_report_bug(file_args(), &session, manager, None).await }
        });

        let registry = PendingUserActions::global();
        let (id, request) = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let pending: Vec<_> = registry
                    .pending_cards_for_session(&session_id)
                    .into_iter()
                    .collect();
                if let Some(message) = pending.into_iter().next() {
                    if let Some(MessageContent::ActionRequired(action)) =
                        message.content.into_iter().next()
                    {
                        if let crate::conversation::message::ActionRequiredData::ToolConfirmation {
                            id,
                            tool_name,
                            arguments,
                            prompt,
                            ..
                        } = action.data
                        {
                            return (id, (tool_name, arguments, prompt));
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the file half must raise an approval card");

        let (tool_name, arguments, prompt) = request;
        assert_eq!(tool_name, REPORT_BUG_TOOL_NAME);
        assert!(
            registry.requires_user_proof_in_session(&session_id, &id),
            "publishing must not be approvable through daemon HTTP by the model"
        );

        let card_body = arguments
            .get("body")
            .and_then(Value::as_str)
            .expect("the card carries the body verbatim");
        assert!(card_body.contains("**Describe the bug**"), "{card_body}");
        assert!(
            card_body.contains("The Auto Visualiser panel is empty"),
            "{card_body}"
        );
        assert_eq!(
            arguments.get("repository").and_then(Value::as_str),
            Some(format!("github.com/{}", issue::repo()).as_str())
        );
        let prompt = prompt.unwrap_or_default();
        assert!(prompt.contains("public issue tracker"), "{prompt}");
        assert!(
            prompt.contains("world-readable and permanent"),
            "the card must say what publishing means: {prompt}"
        );

        // Deny it, and the tool must file nothing.
        assert_eq!(
            registry.resolve_in_session(
                &session_id,
                &id,
                UserActionOutcome::Denied {
                    permission: Permission::DenyOnce
                },
                DecisionAuthority::for_test_proven(),
            ),
            ResolveOutcome::Delivered
        );
        let body = body_of(&running.await.unwrap());
        assert!(body.contains("was refused by the user"), "{body}");
        assert!(!body.contains("Filed:"), "{body}");
    }

    /// ⚠ A private chat is not filed from, whatever the user clicks — and the
    /// refusal comes with the finished report, because the analysis is the
    /// expensive half and throwing it away would just push the user to paste
    /// the transcript somewhere worse.
    /// ⚠ The private-session rule, tested as the pure predicate it is.
    ///
    /// It used to be tested by flipping the process-global master switch, which
    /// broke six unrelated privacy tests in the same binary — a race that reads
    /// as a privacy hole in code nobody had touched. Nothing here mutates any
    /// global, so nothing can.
    #[test]
    fn a_private_chat_is_barred_from_a_public_destination() {
        assert!(refuses_public_disclosure(
            SessionClassification::Private,
            true
        ));
        assert!(!refuses_public_disclosure(
            SessionClassification::Public,
            true
        ));
    }

    /// ⚠ The DR-15 master switch turns this gate off with all the others.
    ///
    /// A deliberate ruling, not an oversight: a switch some gates ignore is not
    /// a switch, and AR-7 is explicit that the toggle stops the classification
    /// ratchet along with the gates — so with it off, a session marked
    /// `Private` was marked by machinery the user has disabled. The
    /// classification is still named on the approval card either way, so the
    /// fact stays in front of the person deciding.
    #[test]
    fn the_master_switch_turns_this_gate_off_with_every_other_one() {
        assert!(!refuses_public_disclosure(
            SessionClassification::Private,
            false
        ));
    }

    /// The refusal reaches the caller with the finished report attached, so the
    /// analysis is not thrown away and the user is not pushed into pasting the
    /// transcript somewhere worse.
    ///
    /// Reads the ambient switch rather than setting it, and SKIPS if it is off.
    /// A skip is honest; flipping it is the race above.
    #[tokio::test]
    async fn a_private_chat_gets_the_refusal_and_its_report_back() {
        if !crate::privacy::privacy_tiers_enabled() {
            return;
        }
        let (_dir, manager, mut session) = session_with_failures(2).await;
        session.privacy_tier = SessionClassification::Private;

        // `without_human_surface` proves the refusal is not merely an
        // unanswerable card: a card raised here would answer `Cancelled`, whose
        // text is different.
        let result = crate::user_surface::without_human_surface(handle_report_bug(
            file_args(),
            &session,
            manager,
            None,
        ))
        .await;
        let body = body_of(&result);
        assert!(body.contains("classified PRIVATE"), "{body}");
        assert!(body.contains("I have not posted anything"), "{body}");
        assert!(
            body.contains("----- body -----"),
            "the finished report must come back with the refusal: {body}"
        );
        assert!(!body.contains("Nothing was filed:"), "{body}");
    }

    /// ⚠ The harness refuses rather than posting. This is the one failure in
    /// the tool a retry CAN fix, so it is an error carrying the reasons.
    #[tokio::test]
    async fn a_report_that_fails_the_harness_is_refused_with_its_reasons() {
        let (_dir, manager, session) = session_with_failures(1).await;
        let result = handle_report_bug(
            args(serde_json::json!({
                "action": "file",
                "title": "bug",
                "description": "it broke"
            })),
            &session,
            manager,
            None,
        )
        .await;
        let error = result.expect_err("an unusable title must not reach a card");
        assert!(error.message.contains("was NOT filed"), "{error:?}");
        assert!(error.message.contains("title"), "{error:?}");
    }

    #[tokio::test]
    async fn filing_without_a_title_or_description_says_which_is_missing() {
        let (_dir, manager, session) = session_with_failures(1).await;
        let error = handle_report_bug(
            args(serde_json::json!({"action": "file", "description": "it broke"})),
            &session,
            Arc::clone(&manager),
            None,
        )
        .await
        .expect_err("filing needs a title");
        assert!(error.message.contains("`title` is required"), "{error:?}");

        let error = handle_report_bug(
            args(serde_json::json!({"action": "file", "title": "A believable title here"})),
            &session,
            manager,
            None,
        )
        .await
        .expect_err("filing needs a description");
        assert!(
            error.message.contains("`description` is required"),
            "{error:?}"
        );
    }

    /// ⚠ A model that omits `action` must land on the half that cannot publish.
    #[test]
    fn an_ambiguous_call_analyses_rather_than_files() {
        assert_eq!(Action::infer(&args(serde_json::json!({}))), Action::Analyze);
        assert_eq!(
            Action::infer(&args(serde_json::json!({"title": "something broke"}))),
            Action::Analyze,
            "a title alone is a draft, not an instruction to publish"
        );
        assert_eq!(
            Action::infer(&args(serde_json::json!({"description": "it broke"}))),
            Action::Analyze
        );
        assert_eq!(
            Action::infer(&args(serde_json::json!({"action": "nonsense"}))),
            Action::Analyze,
            "an unrecognised action must not fall through to filing"
        );
        assert_eq!(
            Action::infer(&args(serde_json::json!({"action": "Analyze"}))),
            Action::Analyze,
            "matching is exact; anything unrecognised is safe"
        );
    }

    #[test]
    fn a_complete_draft_or_an_explicit_action_files() {
        assert_eq!(
            Action::infer(&args(serde_json::json!({
                "title": "Charts render blank",
                "description": "The panel is empty."
            }))),
            Action::File
        );
        assert_eq!(
            Action::infer(&args(serde_json::json!({"action": "file"}))),
            Action::File
        );
    }

    /// Blank strings are not a draft.
    #[test]
    fn whitespace_arguments_do_not_count_as_a_draft() {
        assert_eq!(
            Action::infer(&args(
                serde_json::json!({"title": "  ", "description": "\n"})
            )),
            Action::Analyze
        );
    }

    #[test]
    fn steps_are_accepted_as_a_list_or_as_prose() {
        assert_eq!(
            steps_arg(&args(serde_json::json!({"steps": ["one", " two "]}))),
            vec!["one", "two"]
        );
        assert_eq!(
            steps_arg(&args(serde_json::json!({"steps": "- one\n- two\n\n"}))),
            vec!["one", "two"]
        );
        assert!(steps_arg(&args(serde_json::json!({}))).is_empty());
    }
}
