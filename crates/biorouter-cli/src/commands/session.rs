use crate::session::message_to_markdown;
use anyhow::{Context, Result};

use crate::commands::session_grouping::{
    group_by_parent, listed_session_types, liveness_label, render_child, Liveness, SessionRow,
};
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
