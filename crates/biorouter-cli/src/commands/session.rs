use crate::session::message_to_markdown;
use anyhow::{Context, Result};

use crate::commands::session_grouping::{
    group_by_parent, listed_session_types, liveness_label, render_child, Liveness, SessionRow,
};
use biorouter::privacy::declassify::DeclassifyOutcome;
use biorouter::session::{generate_diagnostics, Session, SessionManager};
use biorouter::utils::safe_truncate;
use cliclack::{confirm, multiselect, select};
use regex::Regex;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const TRUNCATED_DESC_LENGTH: usize = 60;

async fn remove_sessions(session_manager: &SessionManager, sessions: Vec<Session>) -> Result<()> {
    println!("The following sessions will be removed:");
    for session in &sessions {
        println!("- {} {}", session.id, session.name);
    }

    let should_delete = confirm("Are you sure you want to delete these sessions?")
        .initial_value(false)
        .interact()?;

    if should_delete {
        for session in sessions {
            session_manager.delete_session(&session.id).await?;
            println!("Session `{}` removed.", session.id);
        }
    } else {
        println!("Skipping deletion of the sessions.");
    }

    Ok(())
}

fn prompt_interactive_session_removal(sessions: &[Session]) -> Result<Vec<Session>> {
    if sessions.is_empty() {
        println!("No sessions to delete.");
        return Ok(vec![]);
    }

    let mut selector = multiselect(
        "Select sessions to delete (use spacebar, Enter to confirm, Ctrl+C to cancel):",
    );

    let display_map: std::collections::HashMap<String, Session> = sessions
        .iter()
        .map(|s| {
            let desc = if s.name.is_empty() {
                "(no name)"
            } else {
                &s.name
            };
            let truncated_desc = safe_truncate(desc, TRUNCATED_DESC_LENGTH);
            let display_text = format!("{} - {} ({})", s.updated_at, truncated_desc, s.id);
            (display_text, s.clone())
        })
        .collect();

    for display_text in display_map.keys() {
        selector = selector.item(display_text.clone(), display_text.clone(), "");
    }

    let selected_display_texts: Vec<String> = selector.interact()?;

    let selected_sessions: Vec<Session> = selected_display_texts
        .into_iter()
        .filter_map(|text| display_map.get(&text).cloned())
        .collect();

    Ok(selected_sessions)
}

pub async fn handle_session_remove(
    session_id: Option<String>,
    name: Option<String>,
    regex_string: Option<String>,
) -> Result<()> {
    let session_manager = SessionManager::instance();
    let all_sessions = match session_manager.list_sessions().await {
        Ok(sessions) => sessions,
        Err(e) => {
            tracing::error!("Failed to retrieve sessions: {:?}", e);
            return Err(anyhow::anyhow!("Failed to retrieve sessions"));
        }
    };

    let matched_sessions: Vec<Session>;

    if let Some(id_val) = session_id {
        if let Some(session) = all_sessions.iter().find(|s| s.id == id_val) {
            matched_sessions = vec![session.clone()];
        } else {
            return Err(anyhow::anyhow!("Session ID '{}' not found.", id_val));
        }
    } else if let Some(name_val) = name {
        if let Some(session) = all_sessions.iter().find(|s| s.name == name_val) {
            matched_sessions = vec![session.clone()];
        } else {
            return Err(anyhow::anyhow!(
                "Session with name '{}' not found.",
                name_val
            ));
        }
    } else if let Some(regex_val) = regex_string {
        let session_regex = Regex::new(&regex_val)
            .with_context(|| format!("Invalid regex pattern '{}'", regex_val))?;

        matched_sessions = all_sessions
            .into_iter()
            .filter(|session| session_regex.is_match(&session.id))
            .collect();

        if matched_sessions.is_empty() {
            println!("Regex string '{}' does not match any sessions", regex_val);
            return Ok(());
        }
    } else {
        if all_sessions.is_empty() {
            return Err(anyhow::anyhow!("No sessions found."));
        }
        matched_sessions = prompt_interactive_session_removal(&all_sessions)?;
    }

    if matched_sessions.is_empty() {
        return Ok(());
    }

    remove_sessions(&session_manager, matched_sessions).await
}

/// The rows a listing sees, for a given `--subagents`.
///
/// BR-71 Task 38b: `list_sessions()` filters `sub_agent` rows out in SQL, so the
/// flag has to widen the *query* — a display-only change would show nothing new.
/// Split out of `handle_session_list` (which is bound to the
/// `SessionManager::instance()` singleton and so untestable) purely so that
/// defect has a regression guard:
/// `the_subagents_flag_widens_the_query_not_just_the_rendering`.
///
/// `subagents == false` is the historical behaviour byte for byte:
/// `list_sessions()` IS `list_sessions_by_types(&[User, Scheduled])`.
async fn fetch_sessions(session_manager: &SessionManager, subagents: bool) -> Result<Vec<Session>> {
    session_manager
        .list_sessions_by_types(listed_session_types(subagents))
        .await
}

/// Resolve a session id from a name (or a literal id), including subagent runs.
///
/// BR-71 Task 38b fact 2: `lookup_session_id`'s name branch used
/// `list_sessions()`, so `--name` could never reach a subagent while
/// `--session-id` always could. Lives here, next to the listing that shows those
/// names, and takes the manager as an argument so it is testable.
pub async fn resolve_session_by_name(
    session_manager: &SessionManager,
    name: &str,
) -> Result<Option<String>> {
    let sessions = fetch_sessions(session_manager, true).await?;
    Ok(sessions
        .into_iter()
        .find(|s| s.name == name || s.id == name)
        .map(|s| s.id))
}

pub async fn handle_session_list(
    format: String,
    ascending: bool,
    working_dir: Option<PathBuf>,
    limit: Option<usize>,
    subagents: bool,
) -> Result<()> {
    let session_manager = SessionManager::instance();
    let mut sessions = fetch_sessions(&session_manager, subagents).await?;

    if let Some(ref pat) = working_dir {
        let pat_lower = pat.to_string_lossy().to_lowercase();
        sessions.retain(|s| {
            s.working_dir
                .to_string_lossy()
                .to_lowercase()
                .contains(&pat_lower)
        });
    }

    if ascending {
        sessions.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    } else {
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    }

    if !subagents {
        if let Some(n) = limit {
            sessions.truncate(n);
        }

        match format.as_str() {
            "json" => {
                println!("{}", serde_json::to_string(&sessions)?);
            }
            _ => {
                if sessions.is_empty() {
                    println!("No sessions found");
                    return Ok(());
                }

                println!("Available sessions:");
                for session in sessions {
                    let output =
                        format!("{} - {} - {}", session.id, session.name, session.updated_at);
                    println!("{}", output);
                }
            }
        }
        return Ok(());
    }

    print_grouped_sessions(&format, limit, &sessions).await
}

/// The `--subagents` half of [`handle_session_list`]: resolve liveness, nest
/// children under their parent, and print.
///
/// Split out so `handle_session_list` stays under the
/// `clippy::too_many_lines` baseline. The cut is the function's own
/// `if !subagents` fork, so each half is one whole output mode rather than an
/// arbitrary slice — and `sessions` arrives already filtered and sorted,
/// because both modes share that work.
async fn print_grouped_sessions(
    format: &str,
    limit: Option<usize>,
    sessions: &[Session],
) -> Result<()> {
    // The daemon owns liveness. With none reachable, say "unknown" rather than
    // printing "done" over a run that is still going.
    //
    // ⚠ The reason is reported ONCE on stderr rather than guessed at per row.
    // "no daemon" is frequently the wrong diagnosis: an agent-spawned shell has
    // `BIOROUTER_SERVER__SECRET_KEY` stripped by `strip_daemon_private_env`, so
    // this fails with a daemon running perfectly well, and a row that blamed a
    // missing daemon would send someone hunting a problem that does not exist.
    // stderr, so a `--format json` consumer reading stdout is unaffected.
    let live: Option<std::collections::HashSet<String>> =
        match crate::commands::session_watch::running_session_ids().await {
            Ok(ids) => Some(ids),
            Err(err) => {
                eprintln!("note: subagent liveness is unknown — {err}");
                None
            }
        };

    let rows: Vec<SessionRow> = sessions
        .iter()
        .map(|s| SessionRow {
            id: s.id.clone(),
            name: s.name.clone(),
            session_type: s.session_type,
            parent_session_id: s.parent_session_id.clone(),
            updated_at: s.updated_at,
            message_count: s.message_count,
        })
        .collect();
    let mut groups = group_by_parent(rows);
    // ⚠ `limit` caps the TOP-LEVEL rows, after grouping. Truncating the flat row
    // list first would let one parent's six children consume a `--limit 5`.
    if let Some(n) = limit {
        groups.truncate(n);
    }

    let liveness_of = |id: &str| match &live {
        None => Liveness::Unknown,
        Some(ids) if ids.contains(id) => Liveness::Running,
        Some(_) => Liveness::Finished,
    };

    // `SessionRow` is a projection for the pure helpers and deliberately carries
    // no `working_dir`. The JSON arm still has to emit one — the flat
    // (`--subagents`-less) arm serialises whole `Session`s and includes it, and
    // it is the field the sibling `--working-dir` filter matches on, so a script
    // that adds `--subagents` must not silently lose it.
    let working_dirs: std::collections::HashMap<&str, &std::path::Path> = sessions
        .iter()
        .map(|s| (s.id.as_str(), s.working_dir.as_path()))
        .collect();
    let as_json = |row: &SessionRow| {
        serde_json::json!({
            "id": row.id,
            "name": row.name,
            "session_type": row.session_type.to_string(),
            "parent_session_id": row.parent_session_id,
            "working_dir": working_dirs.get(row.id.as_str()),
            "updated_at": row.updated_at,
            "message_count": row.message_count,
            "live": liveness_label(liveness_of(&row.id)),
        })
    };

    match format {
        "json" => {
            let payload: Vec<serde_json::Value> = groups
                .iter()
                .map(|group| {
                    serde_json::json!({
                        "session": as_json(&group.session),
                        "children": group.children.iter().map(&as_json).collect::<Vec<_>>(),
                        // The group-level `live` is the group session's, repeated
                        // here so a consumer can read a group's state without
                        // descending into it. Children carry their own inline.
                        "live": liveness_label(liveness_of(&group.session.id)),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string(&payload)?);
        }
        _ => {
            if groups.is_empty() {
                println!("No sessions found");
                return Ok(());
            }

            println!("Available sessions:");
            for group in groups {
                println!(
                    "{} - {} - {}",
                    group.session.id, group.session.name, group.session.updated_at
                );
                for child in &group.children {
                    println!("{}", render_child(child, liveness_of(&child.id)));
                }
            }
        }
    }
    Ok(())
}

pub async fn handle_session_export(
    session_id: String,
    output_path: Option<PathBuf>,
    format: String,
) -> Result<()> {
    let session_manager = SessionManager::instance();
    let session = match session_manager.get_session(&session_id, true).await {
        Ok(session) => session,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Session '{}' not found or failed to read: {}",
                session_id,
                e
            ));
        }
    };

    let output = match format.as_str() {
        "json" => serde_json::to_string_pretty(&session)?,
        "yaml" => serde_yaml::to_string(&session)?,
        "markdown" => {
            let conversation = session
                .conversation
                .ok_or_else(|| anyhow::anyhow!("Session has no messages"))?;
            export_session_to_markdown(conversation.messages().to_vec(), &session.name)
        }
        _ => return Err(anyhow::anyhow!("Unsupported format: {}", format)),
    };

    if let Some(output_path) = output_path {
        fs::write(&output_path, output).with_context(|| {
            format!("Failed to write to output file: {}", output_path.display())
        })?;
        println!("Session exported to {}", output_path.display());
    } else {
        println!("{}", output);
    }

    Ok(())
}

/// Maximum session name length, mirroring the server route
/// (`routes/session.rs` MAX_NAME_LENGTH) so the CLI and daemon agree.
const MAX_SESSION_NAME_LENGTH: usize = 200;

pub async fn handle_session_rename(session_id: &str, new_name: String) -> Result<()> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("A session name cannot be empty."));
    }
    if trimmed.chars().count() > MAX_SESSION_NAME_LENGTH {
        return Err(anyhow::anyhow!(
            "Session name is too long ({} chars); the maximum is {}.",
            trimmed.chars().count(),
            MAX_SESSION_NAME_LENGTH
        ));
    }

    let session_manager = SessionManager::instance();
    // Confirm the session exists so we report a clear error rather than silently
    // creating/updating a non-existent record.
    session_manager
        .get_session(session_id, false)
        .await
        .map_err(|e| anyhow::anyhow!("Session '{}' not found: {}", session_id, e))?;

    session_manager
        .update(session_id)
        .user_provided_name(trimmed)
        .apply()
        .await
        .with_context(|| format!("Failed to rename session '{}'", session_id))?;

    println!("Renamed session {} → {}", session_id, trimmed);
    Ok(())
}

pub async fn handle_session_diverge(session_id: &str, name: Option<String>) -> Result<()> {
    let session_manager = SessionManager::instance();
    let branched = session_manager
        .diverge_session(session_id, name, None)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to diverge session '{}': {}", session_id, e))?;

    // Machine-readable id on stdout (scriptable), human hint on stderr.
    println!("{}", branched.id);
    eprintln!(
        "Diverged '{}' → new session '{}' (\"{}\"). Resume it with: biorouter session --resume --session-id {}",
        session_id, branched.id, branched.name, branched.id
    );
    Ok(())
}

/// Issue #56 Task 31 / §12.4 — declassify a chat from the terminal, by id.
///
/// **Why the CLI needs its own door.** `list_sessions` filters to (`user`,
/// `scheduled`), so a private `Hidden`, `SubAgent` or `Terminal` chat has no GUI
/// declassification surface at all: History cannot show it, and a control that
/// cannot be selected is not a control. The obvious fix — a "System sessions"
/// filter in History — surfaces 511 hidden sessions on this machine into a
/// user-facing list, which is a regression traded for an edge case. So this
/// works by **id** and consults no session type at all; a Step 5 gate greps this
/// function's body for `SessionType` and expects none.
///
/// ⚠ **This is the second place in the tree that mints
/// `privacy::declassify::UserConfirmation`**, and that is a deliberate widening
/// of a claim `declassify.rs`'s audit used to make with one member. What the
/// audit still guarantees is that the set is *closed and named*: adding a third
/// door turns the build red. What it no longer says is "the only door is an HTTP
/// route behind the user-action header". The honest statement of the residual is
/// that an agent holding `developer__shell` can drive this command — and that
/// same agent can already `sqlite3` the classification column directly, so the
/// store was never protected from the shell in the first place. What the shell
/// cannot do through this door is declassify *silently*: the ledger row
/// [`biorouter::privacy::declassify::declassify`] writes is identical whichever
/// door was used.
pub async fn declassify_command(session_id: &str) -> Result<()> {
    let session_manager = SessionManager::instance();
    let outcome = declassify_by_id(&session_manager, session_id, &mut TerminalPrompt).await?;
    println!("{}", render_declassify_outcome(session_id, outcome));
    Ok(())
}

/// How the terminal asks §12.4's graded confirmation. A trait so the whole of
/// [`declassify_by_id`] — the grading, the escalation, the writing — is testable
/// without a tty, which is the only way the "a refused prompt writes nothing"
/// assertion can exist at all.
pub(crate) trait DeclassifyPrompt {
    /// §12.4's weak control: one yes/no, for a chat that merely ran a turn
    /// against a private endpoint.
    fn confirm_single_click(&mut self, session_id: &str) -> Result<bool>;

    /// §12.4's strong control: retype `phrase`. `None` means the user backed
    /// out.
    ///
    /// `notice` is the already-rendered sentence saying WHY this chat is on the
    /// strong control, printed verbatim. It is passed in rather than composed
    /// here because [`TerminalPrompt`] is the one implementation a test cannot
    /// drive, and the wording is the thing under test: see
    /// [`render_declassify_prompt_notice`] and
    /// [`DECLASSIFY_ESCALATION_NOTICE`], which are pure and are asserted per
    /// provenance.
    fn ask_phrase(
        &mut self,
        session_id: &str,
        phrase: &str,
        notice: &str,
    ) -> Result<Option<String>>;
}

/// Why this chat is being asked for the typed phrase, as one sentence.
///
/// ⚠ **It does not say "reached a private data source" unless the chat did.**
/// That sentence shipped for every provenance, and it is false for the two that
/// dominate day one: the one-time migration marks a chat `backfill:<provider>`
/// from the model it was last bound to, having observed nothing it reached, and
/// an `imported` chat arrived already marked. The per-provenance clause lives in
/// `biorouter::privacy::declassify::strong_confirmation_reason`, beside the
/// grading it must agree with, and is shared with the daemon and the desktop
/// dialog.
pub(crate) fn render_declassify_prompt_notice(
    session_id: &str,
    privacy_reason: Option<&str>,
) -> Option<String> {
    biorouter::privacy::declassify::strong_confirmation_reason(privacy_reason)
        .map(|why| format!("Session {session_id} {why}, so declassifying it needs confirmation."))
}

/// What the terminal says when the grade moved between the read and the write —
/// the escalation arm of [`declassify_by_id`].
///
/// ⚠ **Deliberately not [`render_declassify_prompt_notice`]'s sentence.** The
/// provenance this process read is by definition the stale one, so any clause
/// derived from it is a claim about the conversation that the refusal did not
/// establish. The desktop dialog steps around the same trap in its `escalated`
/// branch; this is the terminal's copy of that reasoning.
pub(crate) const DECLASSIFY_ESCALATION_NOTICE: &str =
    "That request was refused: this chat's record has changed since it was read, so it now takes \
     the typed confirmation.";

/// The real one.
struct TerminalPrompt;

impl DeclassifyPrompt for TerminalPrompt {
    fn confirm_single_click(&mut self, session_id: &str) -> Result<bool> {
        Ok(confirm(format!(
            "Declassify session {session_id}? It will no longer be restricted to private models."
        ))
        .initial_value(false)
        .interact()?)
    }

    fn ask_phrase(
        &mut self,
        _session_id: &str,
        phrase: &str,
        notice: &str,
    ) -> Result<Option<String>> {
        println!("{notice}");
        let typed: String = cliclack::input(format!(
            "Type the last six characters of the session id ({phrase}) to confirm, or leave \
             blank to cancel"
        ))
        .required(false)
        .interact()?;
        Ok(if typed.trim().is_empty() {
            None
        } else {
            Some(typed)
        })
    }
}

/// The testable core of [`declassify_command`].
///
/// The read here decides which control to **show**; the writer decides, inside
/// its own transaction, whether that was the right one — the check-then-act
/// `privacy::declassify` documents. So a weak prompt that comes back
/// `ConfirmationRequired` is not a bug and not a loop: the chat reached a
/// private data source between the two, and the answer is to escalate to the
/// strong control once, exactly as the desktop dialog does.
///
/// ⚠ The proof-of-user is minted in **one** place inside this function, before
/// the loop. Two call sites (one per grade) would read more naturally and would
/// break `the_proof_of_user_is_constructed_in_exactly_two_places`, whose per-file
/// count is what stops a second, unguarded mint from hiding in a file that is
/// already a permitted member of the set.
///
/// ⚠ **DR-20's system authentication is raised here too, and it is the LAST
/// thing before the write** (Task 55). The loop escalates at most twice — once
/// to the typed phrase, once to the operating system — and each escalation is
/// guarded by its own flag, so a user who says no is not asked again. A `turn:*`
/// chat reaches neither escalation and keeps its single click.
pub(crate) async fn declassify_by_id(
    session_manager: &SessionManager,
    session_id: &str,
    prompt: &mut dyn DeclassifyPrompt,
) -> Result<DeclassifyOutcome> {
    use biorouter::privacy::declassify::{
        authenticate_declassification, confirmation_phrase, declassify,
        requires_typed_confirmation, SystemAuthorization, UserConfirmation,
    };
    use biorouter::privacy::system_auth::AuthOutcome;
    use biorouter::privacy::SessionClassification;

    let session = session_manager
        .get_session(session_id, false)
        .await
        .map_err(|e| anyhow::anyhow!("Session '{}' not found: {}", session_id, e))?;

    // Answered before anything is asked: there is nothing to confirm about a
    // no-op, and after a successful declassification the provenance reads
    // `declassified_by_user`, which grades onto the STRONG control — so a second
    // run would otherwise demand a phrase the first run never showed.
    if session.privacy_tier == SessionClassification::Public {
        return Ok(DeclassifyOutcome::AlreadyPublic);
    }

    let phrase = confirmation_phrase(session_id);
    // `Some` exactly when the strong control applies, by construction — the same
    // predicate decides both — so this match cannot show the strong copy to a
    // chat that is getting the single click.
    let notice = render_declassify_prompt_notice(session_id, session.privacy_reason.as_deref());
    let mut typed: Option<String> = if let Some(notice) = notice.as_deref() {
        debug_assert!(requires_typed_confirmation(
            session.privacy_reason.as_deref()
        ));
        match prompt.ask_phrase(session_id, &phrase, notice)? {
            Some(typed) => Some(typed),
            None => return Ok(DeclassifyOutcome::ConfirmationRequired),
        }
    } else if prompt.confirm_single_click(session_id)? {
        None
    } else {
        return Ok(DeclassifyOutcome::ConfirmationRequired);
    };

    let ok = UserConfirmation::from_typed_confirmation();
    let named = [session_id.to_string()];
    let mut escalated = false;
    let mut authorization: Option<SystemAuthorization> = None;
    loop {
        let outcome = declassify(
            session_manager,
            session_id,
            typed.as_deref(),
            authorization.as_ref(),
            &ok,
        )
        .await?;
        match outcome {
            // The grade moved under us. Ask once for the control it moved to; a
            // second refusal is the user's answer, not a reason to ask again.
            DeclassifyOutcome::ConfirmationRequired if !escalated => {
                escalated = true;
                // NOT `notice`: the provenance this function read is the stale
                // one — that is what "the grade moved" means — so a clause
                // derived from it would be a claim about the conversation that
                // nothing here established.
                typed = prompt.ask_phrase(session_id, &phrase, DECLASSIFY_ESCALATION_NOTICE)?;
                if typed.is_none() {
                    return Ok(outcome);
                }
            }
            // DR-20. Everything else has passed — the row is private, the grade
            // demands the strong control, the phrase matched — so this is the
            // moment to ask the operating system, and no earlier.
            DeclassifyOutcome::SystemAuthenticationRequired if authorization.is_none() => {
                match authenticate_declassification(&named).await {
                    Ok(granted) => authorization = Some(granted),
                    // "This machine cannot raise the prompt" is not the user's
                    // answer, and reporting it as one would tell a Linux user
                    // with no polkit that they declined something they were
                    // never shown. It carries the platform's own advice, so it
                    // surfaces as an error rather than as an outcome.
                    Err(refusal) if refusal.outcome == AuthOutcome::Unavailable => {
                        return Err(anyhow::anyhow!("{refusal}"));
                    }
                    // A refusal IS the user's answer, and it reads like every
                    // other refusal at this terminal: nothing changed.
                    Err(_) => return Ok(outcome),
                }
            }
            _ => return Ok(outcome),
        }
    }
}

/// What to print. Separated from the work so the wording is testable and so the
/// three non-writing outcomes cannot be reported as a success.
pub(crate) fn render_declassify_outcome(session_id: &str, outcome: DeclassifyOutcome) -> String {
    match outcome {
        DeclassifyOutcome::Declassified => format!(
            "Session {session_id} is now public. It may run on any model, and the change is \
             recorded in the classification ledger."
        ),
        DeclassifyOutcome::AlreadyPublic => {
            format!("Session {session_id} is already public. Nothing changed.")
        }
        DeclassifyOutcome::ConfirmationRequired => format!(
            "Session {session_id} was NOT declassified: the confirmation was not given. The chat \
             is unchanged."
        ),
        DeclassifyOutcome::SystemAuthenticationRequired => format!(
            "Session {session_id} was NOT declassified: the system authentication was not \
             completed. The chat is unchanged."
        ),
        DeclassifyOutcome::SessionNotFound => {
            format!("Session {session_id} no longer exists. Nothing changed.")
        }
    }
}

pub async fn handle_diagnostics(session_id: &str, output_path: Option<PathBuf>) -> Result<()> {
    println!(
        "Generating diagnostics bundle for session '{}'...",
        session_id
    );

    let session_manager = SessionManager::instance();
    let diagnostics_data = generate_diagnostics(&session_manager, session_id)
        .await
        .with_context(|| {
            format!(
                "Failed to write to generate diagnostics bundle for session '{}'",
                session_id
            )
        })?;

    let output_file = if let Some(path) = output_path {
        path.clone()
    } else {
        PathBuf::from(format!("diagnostics_{}.zip", session_id))
    };

    let mut file = fs::File::create(&output_file).context(format!(
        "Failed to create output file: {}",
        output_file.display()
    ))?;

    file.write_all(&diagnostics_data)
        .context("Failed to write diagnostics data")?;

    println!("Diagnostics bundle saved to: {}", output_file.display());

    Ok(())
}

fn export_session_to_markdown(
    messages: Vec<biorouter::conversation::message::Message>,
    session_name: &String,
) -> String {
    let mut markdown_output = String::new();

    markdown_output.push_str(&format!("# Session Export: {}\n\n", session_name));

    if messages.is_empty() {
        markdown_output.push_str("*(This session has no messages)*\n");
        return markdown_output;
    }

    markdown_output.push_str(&format!("*Total messages: {}*\n\n---\n\n", messages.len()));

    // Track if the last message had tool requests to properly handle tool responses
    let mut skip_next_if_tool_response = false;

    for message in &messages {
        // Check if this is a User message containing only ToolResponses
        let is_only_tool_response = message.role == rmcp::model::Role::User
            && message.content.iter().all(|content| {
                matches!(
                    content,
                    biorouter::conversation::message::MessageContent::ToolResponse(_)
                )
            });

        // If the previous message had tool requests and this one is just tool responses,
        // don't create a new User section - we'll attach the responses to the tool calls
        if skip_next_if_tool_response && is_only_tool_response {
            // Export the tool responses without a User heading
            markdown_output.push_str(&message_to_markdown(message, false));
            markdown_output.push_str("\n\n---\n\n");
            skip_next_if_tool_response = false;
            continue;
        }

        // Reset the skip flag - we'll update it below if needed
        skip_next_if_tool_response = false;

        // Output the role prefix except for tool response-only messages
        if !is_only_tool_response {
            let role_prefix = match message.role {
                rmcp::model::Role::User => "### User:\n",
                rmcp::model::Role::Assistant => "### Assistant:\n",
            };
            markdown_output.push_str(role_prefix);
        }

        // Add the message content
        markdown_output.push_str(&message_to_markdown(message, false));
        markdown_output.push_str("\n\n---\n\n");

        // Check if this message has any tool requests, to handle the next message differently
        if message.content.iter().any(|content| {
            matches!(
                content,
                biorouter::conversation::message::MessageContent::ToolRequest(_)
            )
        }) {
            skip_next_if_tool_response = true;
        }
    }

    markdown_output
}

/// Prompt the user to interactively select a session
///
/// Shows a list of available sessions and lets the user select one
pub async fn prompt_interactive_session_selection(
    session_manager: &SessionManager,
) -> Result<String> {
    let sessions = session_manager.list_sessions().await?;

    if sessions.is_empty() {
        return Err(anyhow::anyhow!("No sessions found"));
    }

    // Build the selection prompt
    let mut selector = select("Select a session to export:");

    // Map to display text
    let display_map: std::collections::HashMap<String, Session> = sessions
        .iter()
        .map(|s| {
            let desc = if s.name.is_empty() {
                "(no name)"
            } else {
                &s.name
            };
            let truncated_desc = safe_truncate(desc, TRUNCATED_DESC_LENGTH);

            let display_text = format!("{} - {} ({})", s.updated_at, truncated_desc, s.id);
            (display_text, s.clone())
        })
        .collect();

    // Add each session as an option
    for display_text in display_map.keys() {
        selector = selector.item(display_text.clone(), display_text.clone(), "");
    }

    // Add a cancel option
    let cancel_value = String::from("cancel");
    selector = selector.item(cancel_value, "Cancel", "Cancel export");

    // Get user selection
    let selected_display_text: String = selector.interact()?;

    if selected_display_text == "cancel" {
        return Err(anyhow::anyhow!("Export canceled"));
    }

    // Retrieve the selected session
    if let Some(session) = display_map.get(&selected_display_text) {
        Ok(session.id.clone())
    } else {
        Err(anyhow::anyhow!("Invalid selection"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use biorouter::conversation::message::Message;
    use biorouter::session::session_manager::SessionType;
    use tempfile::TempDir;

    /// A `SessionManager` over a throwaway directory, plus one `User` row and
    /// one `SubAgent` row that are identical in every way except their type.
    ///
    /// ⚠ Each row gets a message. `list_sessions_by_types` INNER JOINs
    /// `messages`, so a session with none is invisible whatever its type — a
    /// fixture without this passes the "subagent is hidden" half for entirely
    /// the wrong reason and then fails the other half.
    async fn store_with_a_user_and_a_subagent_session(
        dir: &TempDir,
    ) -> (SessionManager, String, String) {
        let sm = SessionManager::new(dir.path().to_path_buf());
        let parent = sm
            .create_session(
                dir.path().to_path_buf(),
                "Migration review".to_string(),
                SessionType::User,
            )
            .await
            .unwrap();
        let child = sm
            .create_session(
                dir.path().to_path_buf(),
                "Subagent: audit the migration".to_string(),
                SessionType::SubAgent,
            )
            .await
            .unwrap();
        for id in [&parent.id, &child.id] {
            sm.add_message(id, &Message::user().with_text("hello"))
                .await
                .unwrap();
        }
        (sm, parent.id, child.id)
    }

    /// BR-71 Task 38b, fact 1 — the defect this task exists to fix. A subagent
    /// row is filtered out **in SQL**: `list_sessions()` is exactly
    /// `list_sessions_by_types(&[User, Scheduled])`, so no amount of formatting
    /// makes one appear. `--subagents` therefore has to widen the QUERY.
    ///
    /// This is the assertion that fails if `fetch_sessions` is ever reverted to
    /// `list_sessions()`; the pure grouping tests cannot see that regression at
    /// all, because grouping is only ever handed rows the query already
    /// returned.
    #[tokio::test]
    async fn the_subagents_flag_widens_the_query_not_just_the_rendering() {
        let dir = TempDir::new().unwrap();
        let (sm, parent_id, child_id) = store_with_a_user_and_a_subagent_session(&dir).await;

        let narrow = fetch_sessions(&sm, false).await.unwrap();
        assert!(
            narrow.iter().any(|s| s.id == parent_id),
            "the default listing still shows user sessions"
        );
        assert!(
            !narrow.iter().any(|s| s.id == child_id),
            "without the flag a subagent row is invisible — this is the defect, \
             and it lives in the SQL type filter"
        );

        let wide = fetch_sessions(&sm, true).await.unwrap();
        assert!(
            wide.iter().any(|s| s.id == child_id),
            "--subagents must widen the query; a rendering-only change shows nothing new"
        );
        assert!(
            wide.iter().any(|s| s.id == parent_id),
            "widening must ADD a type, not swap one out"
        );
    }

    /// BR-71 Task 38b, fact 2 — the same hole in `--name`. `--session-id`
    /// always worked, which is why this one is easy to miss: by-id resolves,
    /// by-name silently does not. A user cannot attach to a run they cannot
    /// name.
    #[tokio::test]
    async fn a_subagent_run_is_addressable_by_name() {
        let dir = TempDir::new().unwrap();
        let (sm, parent_id, child_id) = store_with_a_user_and_a_subagent_session(&dir).await;

        assert_eq!(
            resolve_session_by_name(&sm, "Subagent: audit the migration")
                .await
                .unwrap(),
            Some(child_id),
            "a subagent must be resolvable by name, not only by id"
        );
        assert_eq!(
            resolve_session_by_name(&sm, "Migration review")
                .await
                .unwrap(),
            Some(parent_id),
            "widening the lookup must not break the existing by-name path"
        );
        assert_eq!(
            resolve_session_by_name(&sm, "no such session")
                .await
                .unwrap(),
            None,
            "an unknown name is still not found"
        );
    }

    /// Arm DR-20's system-authentication seam to approve the next prompt.
    ///
    /// ⚠ **This compiles only because `biorouter` is a `[dev-dependency]` of
    /// this crate with `privacy-test-auth` on.** That is deliberate: if the
    /// feature is ever moved to `[dependencies]` — which would ship the bypass —
    /// nothing here changes, but
    /// `privacy::system_auth::tests::the_test_seam_cannot_be_compiled_into_a_shipped_profile`
    /// turns red. And if the dev-dependency is dropped, this line stops
    /// compiling, which is a loud failure rather than a test suite that starts
    /// asking the developer for their password.
    fn approve_the_next_system_prompt() {
        biorouter::privacy::system_auth_seam::reset();
        biorouter::privacy::system_auth_seam::answer_next_prompt(
            biorouter::privacy::system_auth::AuthOutcome::Approved,
        );
    }

    /// Issue #56 Task 31. A prompt that always gives the strongest answer the
    /// terminal could give: yes to the single click, and the phrase when one is
    /// asked for. It records what it was asked, so a test can tell the two
    /// controls apart.
    #[derive(Default)]
    struct AlwaysConfirms {
        single_clicks: usize,
        phrases_asked: usize,
        /// Every sentence the user was shown before being asked to retype, in
        /// order. Recorded so the WORDING is testable and not just the count —
        /// the shipped string claimed a private data source for every chat.
        notices: Vec<String>,
    }

    impl DeclassifyPrompt for AlwaysConfirms {
        fn confirm_single_click(&mut self, _session_id: &str) -> Result<bool> {
            self.single_clicks += 1;
            Ok(true)
        }

        fn ask_phrase(
            &mut self,
            _session_id: &str,
            phrase: &str,
            notice: &str,
        ) -> Result<Option<String>> {
            self.phrases_asked += 1;
            self.notices.push(notice.to_string());
            Ok(Some(phrase.to_string()))
        }
    }

    /// A prompt nobody answers: the user hit Ctrl-C, or said no.
    struct Refuses;

    impl DeclassifyPrompt for Refuses {
        fn confirm_single_click(&mut self, _session_id: &str) -> Result<bool> {
            Ok(false)
        }

        fn ask_phrase(
            &mut self,
            _session_id: &str,
            _phrase: &str,
            _notice: &str,
        ) -> Result<Option<String>> {
            Ok(None)
        }
    }

    /// A private session of `kind`, carrying `reason` as its provenance.
    async fn private_session_of_type(
        sm: &SessionManager,
        dir: &TempDir,
        kind: SessionType,
        reason: &str,
    ) -> String {
        let s = sm
            .create_session(dir.path().to_path_buf(), "a cohort chat".to_string(), kind)
            .await
            .unwrap();
        sm.add_message(&s.id, &Message::user().with_text("patient MRN 12345"))
            .await
            .unwrap();
        sm.update(&s.id)
            .raise_privacy(biorouter::privacy::SessionClassification::Private, reason)
            .apply()
            .await
            .unwrap();
        s.id
    }

    /// Issue #56 Task 31. `list_sessions` filters to (`user`, `scheduled`), so a
    /// private `Hidden`, `SubAgent` or `Terminal` chat has NO GUI
    /// declassification surface at all — the History list it would have to be
    /// selected from cannot show it.
    ///
    /// The obvious fix is a "System sessions" filter in History, and it is the
    /// wrong one: on this machine that surfaces 511 hidden sessions into a
    /// user-facing list, a regression traded for an edge case. The CLI escape
    /// hatch works by **id**, which is exactly why it does not need one.
    ///
    /// ⚠ `#[serial]` on the seam's own key, because it arms DR-20's
    /// **process-global** one-shot (`system_auth_seam::{NEXT, LAST}`) and so
    /// does its sibling below. Measured, not predicted: run just these two on
    /// two threads and 34 of 40 runs fail — this one seeing its arming consumed
    /// by the sibling's `reset()`, and the sibling seeing THIS one's `Approved`
    /// satisfy a prompt it had armed to `Denied`. The second direction is the
    /// one that matters: it makes a discriminating assertion pass for a reason
    /// that has nothing to do with the code under test.
    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn declassify_works_by_id_regardless_of_session_type() {
        use biorouter::privacy::declassify::DeclassifyOutcome;
        use biorouter::privacy::SessionClassification;

        for kind in [
            SessionType::Hidden,
            SessionType::SubAgent,
            SessionType::Terminal,
            SessionType::User,
        ] {
            let dir = TempDir::new().unwrap();
            let sm = SessionManager::new(dir.path().to_path_buf());
            let id = private_session_of_type(&sm, &dir, kind, "mcp:ucsfomopagent").await;

            // `mcp:*` grades onto the strong control, which since Task 55 also
            // means DR-20's system authentication. One arming per chat, because
            // the seam is one-shot for the same reason DR-20 admits no cached
            // grant.
            approve_the_next_system_prompt();
            let mut prompt = AlwaysConfirms::default();
            assert_eq!(
                declassify_by_id(&sm, &id, &mut prompt).await.unwrap(),
                DeclassifyOutcome::Declassified,
                "a private {kind:?} chat must be declassifiable by id"
            );
            assert_eq!(
                sm.get_session(&id, false).await.unwrap().privacy_tier,
                SessionClassification::Public
            );
            // `mcp:*` provenance grades onto §12.4's STRONG control, whatever
            // the session's type is.
            assert_eq!(prompt.phrases_asked, 1, "{kind:?}");
            assert_eq!(prompt.single_clicks, 0, "{kind:?}");
        }
    }

    /// …and the reason the escape hatch has to exist: three of those four types
    /// are invisible to every listing the GUI builds its History from.
    #[tokio::test]
    async fn three_of_those_four_types_have_no_listing_to_be_selected_from() {
        let dir = TempDir::new().unwrap();
        let sm = SessionManager::new(dir.path().to_path_buf());
        let mut hidden = vec![];
        for kind in [
            SessionType::Hidden,
            SessionType::SubAgent,
            SessionType::Terminal,
        ] {
            hidden.push(private_session_of_type(&sm, &dir, kind, "mcp:x").await);
        }
        let visible = private_session_of_type(&sm, &dir, SessionType::User, "mcp:x").await;

        let listed = sm.list_sessions().await.unwrap();
        assert!(listed.iter().any(|s| s.id == visible));
        for id in &hidden {
            assert!(
                !listed.iter().any(|s| &s.id == id),
                "{id} is listed after all — this test's premise is gone"
            );
        }
    }

    /// §12.4's grading, at the terminal. A chat that merely ran a turn gets the
    /// single click; everything else gets the typed phrase.
    #[tokio::test]
    async fn the_terminal_shows_the_control_the_provenance_grades_onto() {
        use biorouter::privacy::declassify::DeclassifyOutcome;

        let dir = TempDir::new().unwrap();
        let sm = SessionManager::new(dir.path().to_path_buf());
        let weak = private_session_of_type(&sm, &dir, SessionType::User, "turn:versa_azure").await;
        let mut prompt = AlwaysConfirms::default();
        assert_eq!(
            declassify_by_id(&sm, &weak, &mut prompt).await.unwrap(),
            DeclassifyOutcome::Declassified
        );
        assert_eq!(prompt.single_clicks, 1);
        assert_eq!(prompt.phrases_asked, 0);

        // A second call on the now-public row is a no-op and asks nothing.
        let mut again = AlwaysConfirms::default();
        assert_eq!(
            declassify_by_id(&sm, &weak, &mut again).await.unwrap(),
            DeclassifyOutcome::AlreadyPublic
        );
        assert_eq!(again.single_clicks, 0);
        assert_eq!(again.phrases_asked, 0);
    }

    /// The sentence the terminal prints above the phrase field, **asserted per
    /// provenance**, because it shipped as one sentence — "reached a private
    /// data source" — for all of them.
    ///
    /// That is false for `backfill:*` and `imported`, and those are not an edge
    /// case: the one-time migration marks a chat by the model it was last bound
    /// to, so on a machine with history `backfill:*` is most of the private rows
    /// a user meets this control on. A single assertion on one provenance is
    /// exactly what let it ship, so this walks the vocabulary.
    #[test]
    fn each_provenance_is_given_the_reason_that_is_true_of_it() {
        let id = "20260101_120000";
        let cases: [(Option<&str>, &str); 8] = [
            (
                Some("mcp:ucsfomopagent"),
                "Session 20260101_120000 reached a private data source, so declassifying it needs \
                 confirmation.",
            ),
            (
                Some("inherited:20251231_090000"),
                "Session 20260101_120000 was created inside a private chat, so declassifying it \
                 needs confirmation.",
            ),
            (
                Some("diverged:20251231_090000"),
                "Session 20260101_120000 was branched out of a private chat, so declassifying it \
                 needs confirmation.",
            ),
            (
                Some("backfill:versa_azure"),
                "Session 20260101_120000 was marked private by the one-time migration, from the \
                 model it was last using rather than from anything it reached, so declassifying \
                 it needs confirmation.",
            ),
            (
                Some("imported"),
                "Session 20260101_120000 was imported already marked private, so declassifying it \
                 needs confirmation.",
            ),
            (
                Some("something_new"),
                "Session 20260101_120000 does not record an observed turn on a private model as \
                 the reason it is private, so declassifying it needs confirmation.",
            ),
            (
                Some(""),
                "Session 20260101_120000 does not record an observed turn on a private model as \
                 the reason it is private, so declassifying it needs confirmation.",
            ),
            (
                None,
                "Session 20260101_120000 does not record an observed turn on a private model as \
                 the reason it is private, so declassifying it needs confirmation.",
            ),
        ];
        for (reason, expected) in cases {
            assert_eq!(
                render_declassify_prompt_notice(id, reason).as_deref(),
                Some(expected),
                "the sentence a {reason:?} chat is shown"
            );
        }

        // A `turn:*` chat never sees this control at all, so it has no sentence
        // to be given a wrong one.
        assert_eq!(
            render_declassify_prompt_notice(id, Some("turn:versa_azure")),
            None
        );

        // And the escalation arm does not borrow any of them: the provenance it
        // would derive from is the stale one by definition.
        assert!(!DECLASSIFY_ESCALATION_NOTICE.contains("reached a private data source"));
        assert!(DECLASSIFY_ESCALATION_NOTICE.contains("has changed"));
    }

    /// …and the sentence above is the one `declassify_by_id` actually hands the
    /// prompt, for each provenance, through the real read of the stored row.
    ///
    /// The pure test cannot catch a call site that passes the wrong string (the
    /// escalation notice, a hardcoded sentence, the id twice); this walks the
    /// same vocabulary through the writer.
    ///
    /// ⚠ `#[serial]` on the seam's own key: the strong control now owes DR-20's
    /// system authentication, whose arming is process-global.
    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn the_reason_reaches_the_prompt_through_the_real_read() {
        use biorouter::privacy::declassify::DeclassifyOutcome;

        for (reason, must_say) in [
            ("mcp:ucsfomopagent", "reached a private data source"),
            (
                "inherited:20251231_090000",
                "was created inside a private chat",
            ),
            (
                "diverged:20251231_090000",
                "was branched out of a private chat",
            ),
            (
                "backfill:versa_azure",
                "was marked private by the one-time migration",
            ),
            ("imported", "was imported already marked private"),
            (
                "something_new",
                "does not record an observed turn on a private model",
            ),
        ] {
            let dir = TempDir::new().unwrap();
            let sm = SessionManager::new(dir.path().to_path_buf());
            let id = private_session_of_type(&sm, &dir, SessionType::User, reason).await;

            approve_the_next_system_prompt();
            let mut prompt = AlwaysConfirms::default();
            assert_eq!(
                declassify_by_id(&sm, &id, &mut prompt).await.unwrap(),
                DeclassifyOutcome::Declassified,
                "{reason}"
            );
            assert_eq!(prompt.notices.len(), 1, "{reason}");
            let said = &prompt.notices[0];
            assert!(
                said.contains(must_say),
                "a {reason} chat was told {said:?}, which does not say {must_say:?}"
            );
            assert!(
                said.contains(&id),
                "{reason}: {said:?} does not name the chat"
            );
            if reason != "mcp:ucsfomopagent" {
                assert!(
                    !said.contains("reached a private data source"),
                    "a {reason} chat was told it reached a private data source: {said:?}"
                );
            }
        }
    }

    /// A refusal at the prompt writes nothing. Both controls, because a "no"
    /// that declassified anyway is the one failure this whole surface exists to
    /// prevent.
    #[tokio::test]
    async fn a_prompt_nobody_answers_leaves_the_chat_private() {
        use biorouter::privacy::declassify::DeclassifyOutcome;
        use biorouter::privacy::SessionClassification;

        let dir = TempDir::new().unwrap();
        let sm = SessionManager::new(dir.path().to_path_buf());
        for reason in ["turn:versa_azure", "mcp:ucsfomopagent"] {
            let id = private_session_of_type(&sm, &dir, SessionType::User, reason).await;
            assert_eq!(
                declassify_by_id(&sm, &id, &mut Refuses).await.unwrap(),
                DeclassifyOutcome::ConfirmationRequired
            );
            assert_eq!(
                sm.get_session(&id, false).await.unwrap().privacy_tier,
                SessionClassification::Private,
                "a refused confirmation must leave the chat exactly as it was"
            );
        }
    }

    /// Issue #56 DR-20 / Task 55. The terminal door asks the operating system
    /// too, and a refusal there leaves the chat exactly as it was.
    ///
    /// ⚠ **The `turn:*` half is the discriminating one.** The seam is armed only
    /// for the `mcp:*` chat; an unarmed seam refuses by default, so if the weak
    /// control had gained a password prompt this test would fail on the
    /// `turn:*` chat rather than pass quietly.
    ///
    /// ⚠ That last sentence is only true with `BIOROUTER_PRIVACY_TEST_AUTH`
    /// **unset**, which is why the guard below is not decoration. `env_answer`
    /// is consulted whenever the one-shot arming is absent, so a developer or a
    /// CI runner with `BIOROUTER_PRIVACY_TEST_AUTH=approve` exported turns the
    /// unarmed seam into an approving one. Measured: with
    /// `requires_system_authentication` mutated to `true` — exactly the
    /// regression this test names — the assertion below fails with the variable
    /// unset and **passes** with it set to `approve`. The lock closes that.
    ///
    /// ⚠ `#[serial]` on the seam's own key — see
    /// `declassify_works_by_id_regardless_of_session_type` for the measurement.
    /// Without it, that test's `Approved` arming answers the `Denied` prompt
    /// this one raises, and the first assertion below passes reading
    /// `Declassified` — the discriminating test losing its power to a race.
    #[tokio::test]
    #[serial_test::serial(privacy_test_auth_seam)]
    async fn the_terminal_asks_the_operating_system_for_the_strong_control_only() {
        use biorouter::privacy::declassify::DeclassifyOutcome;
        use biorouter::privacy::system_auth::AuthOutcome;
        use biorouter::privacy::SessionClassification;

        let _env = env_lock::lock_env([("BIOROUTER_PRIVACY_TEST_AUTH", None::<&str>)]);

        let dir = TempDir::new().unwrap();
        let sm = SessionManager::new(dir.path().to_path_buf());

        // A denied prompt: the phrase was typed and matched, and the chat is
        // still private with nothing written.
        let strong = private_session_of_type(&sm, &dir, SessionType::User, "mcp:x").await;
        biorouter::privacy::system_auth_seam::reset();
        biorouter::privacy::system_auth_seam::answer_next_prompt(AuthOutcome::Denied);
        let mut prompt = AlwaysConfirms::default();
        assert_eq!(
            declassify_by_id(&sm, &strong, &mut prompt).await.unwrap(),
            DeclassifyOutcome::SystemAuthenticationRequired
        );
        assert_eq!(
            prompt.phrases_asked, 1,
            "the typed phrase must still be asked"
        );
        assert_eq!(
            sm.get_session(&strong, false).await.unwrap().privacy_tier,
            SessionClassification::Private,
            "a refused system authentication declassified the chat anyway"
        );

        // A platform with no prompter is an ERROR carrying the platform's own
        // advice, not the user's answer — telling a Linux user with no polkit
        // that they declined something they were never shown would be a lie.
        biorouter::privacy::system_auth_seam::reset();
        biorouter::privacy::system_auth_seam::answer_next_prompt(AuthOutcome::Unavailable);
        let err = declassify_by_id(&sm, &strong, &mut AlwaysConfirms::default())
            .await
            .expect_err("an unavailable prompter must not read as a refusal by the user");
        assert!(!err.to_string().is_empty(), "{err}");
        assert_eq!(
            sm.get_session(&strong, false).await.unwrap().privacy_tier,
            SessionClassification::Private
        );

        // Approved: both proofs given, and only then.
        approve_the_next_system_prompt();
        assert_eq!(
            declassify_by_id(&sm, &strong, &mut AlwaysConfirms::default())
                .await
                .unwrap(),
            DeclassifyOutcome::Declassified
        );

        // …and the weak control raises no prompt at all. The seam is left
        // UNARMED here on purpose: it defaults to refusing, so a `turn:*` chat
        // that asked for a password would come back
        // `SystemAuthenticationRequired` instead of `Declassified`.
        biorouter::privacy::system_auth_seam::reset();
        let weak = private_session_of_type(&sm, &dir, SessionType::User, "turn:versa_azure").await;
        assert_eq!(
            declassify_by_id(&sm, &weak, &mut AlwaysConfirms::default())
                .await
                .unwrap(),
            DeclassifyOutcome::Declassified,
            "the single-click control gained a password prompt it never shows the user"
        );
    }

    /// The four non-writing outcomes must not read as success. A user who is
    /// told "declassified" and finds the chat still refusing has been lied to by
    /// the one surface whose whole job is to be believed.
    #[test]
    fn only_the_writing_outcome_reports_a_declassification() {
        assert!(
            render_declassify_outcome("20260801_7", DeclassifyOutcome::Declassified)
                .contains("now public")
        );
        for outcome in [
            DeclassifyOutcome::AlreadyPublic,
            DeclassifyOutcome::ConfirmationRequired,
            DeclassifyOutcome::SystemAuthenticationRequired,
            DeclassifyOutcome::SessionNotFound,
        ] {
            let text = render_declassify_outcome("20260801_7", outcome);
            assert!(
                text.contains("Nothing changed") || text.contains("unchanged"),
                "{outcome:?} reported as a change: {text}"
            );
            assert!(!text.contains("is now public"), "{outcome:?}: {text}");
        }
    }

    /// An id that names no row is reported, not silently reported as success.
    #[tokio::test]
    async fn an_unknown_id_is_an_error_and_not_a_declassification() {
        let dir = TempDir::new().unwrap();
        let sm = SessionManager::new(dir.path().to_path_buf());
        let err = declassify_by_id(&sm, "29990101_000000", &mut AlwaysConfirms::default())
            .await
            .expect_err("an unknown id must not read as a successful declassification");
        assert!(err.to_string().contains("29990101_000000"), "{err}");
    }

    /// The rule lives in exactly one place, so the listing and the `--name`
    /// lookup cannot drift apart.
    #[test]
    fn the_widened_type_list_adds_subagents_and_removes_nothing() {
        let narrow = listed_session_types(false);
        let wide = listed_session_types(true);
        assert!(!narrow.contains(&SessionType::SubAgent));
        assert!(wide.contains(&SessionType::SubAgent));
        for kind in narrow {
            assert!(wide.contains(kind), "{kind:?} must survive the widening");
        }
    }
}
