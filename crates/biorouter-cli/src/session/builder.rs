use super::output;
use super::CliSession;
use biorouter::agents::{Agent, AgentConfig};
use biorouter::config::get_enabled_extensions;
use biorouter::config::resolve_extensions_for_new_session;
use biorouter::config::{
    extensions::get_extension_by_name, get_all_extensions, BioRouterMode, Config, ExtensionConfig,
    PermissionManager,
};
use biorouter::providers::create;
use biorouter::session::session_manager::SessionType;
use biorouter::session::{EnabledExtensionsState, ExtensionState, SessionManager};
use biorouter::workflow::Workflow;
use console::style;
use rustyline::EditMode;
use std::collections::BTreeSet;
use std::process;
use std::sync::Arc;
use tokio::task::JoinSet;

const EXTENSION_HINT_MAX_LEN: usize = 5;

fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    let truncated: String = s.chars().take(max_len).collect();
    if s.chars().count() > max_len {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

fn parse_cli_flag_extensions(
    extensions: &[String],
    streamable_http_extensions: &[String],
    builtins: &[String],
) -> Vec<(String, ExtensionConfig)> {
    let mut extensions_to_load = Vec::new();

    for (idx, ext_str) in extensions.iter().enumerate() {
        match CliSession::parse_stdio_extension(ext_str) {
            Ok(config) => {
                let hint = truncate_with_ellipsis(ext_str, EXTENSION_HINT_MAX_LEN);
                let label = format!("stdio #{}({})", idx + 1, hint);
                extensions_to_load.push((label, config));
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    style(format!(
                        "Warning: Invalid --extension value '{}' ({}); ignoring",
                        ext_str, e
                    ))
                    .yellow()
                );
            }
        }
    }

    for (idx, ext_str) in streamable_http_extensions.iter().enumerate() {
        let config = CliSession::parse_streamable_http_extension(ext_str);
        let hint = truncate_with_ellipsis(ext_str, EXTENSION_HINT_MAX_LEN);
        let label = format!("http #{}({})", idx + 1, hint);
        extensions_to_load.push((label, config));
    }

    for builtin_str in builtins {
        let configs = CliSession::parse_builtin_extensions(builtin_str);
        for config in configs {
            extensions_to_load.push((config.name(), config));
        }
    }

    extensions_to_load
}

/// Configuration for building a new Biorouter session
///
/// This struct contains all the parameters needed to create a new session,
/// including session identification, extension configuration, and debug settings.
#[derive(Clone, Debug)]
pub struct SessionBuilderConfig {
    /// Session id, optional need to deduce from context
    pub session_id: Option<String>,
    /// Whether to resume an existing session
    pub resume: bool,
    /// Whether to run without a session file
    pub no_session: bool,
    /// List of stdio extension commands to add
    pub extensions: Vec<String>,
    /// List of streamable HTTP extension commands to add
    pub streamable_http_extensions: Vec<String>,
    /// List of builtin extension commands to add
    pub builtins: Vec<String>,
    /// Workflow for the session
    pub workflow: Option<Workflow>,
    /// Any additional system prompt to append to the default
    pub additional_system_prompt: Option<String>,
    /// Provider override from CLI arguments
    pub provider: Option<String>,
    /// Model override from CLI arguments
    pub model: Option<String>,
    /// Enable debug printing
    pub debug: bool,
    /// Maximum number of consecutive identical tool calls allowed
    pub max_tool_repetitions: Option<u32>,
    /// Maximum number of turns (iterations) allowed without user input
    pub max_turns: Option<u32>,
    /// ID of the scheduled job that triggered this session (if any)
    pub scheduled_job_id: Option<String>,
    /// Whether this session will be used interactively (affects debugging prompts)
    pub interactive: bool,
    /// Quiet mode - suppress non-response output
    pub quiet: bool,
    /// Output format (text, json)
    pub output_format: String,
}

/// Manual implementation of Default to ensure proper initialization of output_format
/// This struct requires explicit default value for output_format field
impl Default for SessionBuilderConfig {
    fn default() -> Self {
        SessionBuilderConfig {
            session_id: None,
            resume: false,
            no_session: false,
            extensions: Vec::new(),
            streamable_http_extensions: Vec::new(),
            builtins: Vec::new(),
            workflow: None,
            additional_system_prompt: None,
            provider: None,
            model: None,
            debug: false,
            max_tool_repetitions: None,
            max_turns: None,
            scheduled_job_id: None,
            interactive: false,
            quiet: false,
            output_format: "text".to_string(),
        }
    }
}

/// Offers to help debug an extension failure by creating a minimal debugging session
async fn offer_extension_debugging_help(
    extension_name: &str,
    error_message: &str,
    provider: Arc<dyn biorouter::providers::base::Provider>,
    interactive: bool,
) -> Result<(), anyhow::Error> {
    // Only offer debugging help in interactive mode
    if !interactive {
        return Ok(());
    }

    let help_prompt = format!(
        "Would you like me to help debug the '{}' extension failure?",
        extension_name
    );

    let should_help = match cliclack::confirm(help_prompt)
        .initial_value(false)
        .interact()
    {
        Ok(choice) => choice,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::Interrupted {
                return Ok(());
            } else {
                return Err(e.into());
            }
        }
    };

    if !should_help {
        return Ok(());
    }

    println!("{}", style("Starting debugging session...").cyan());

    // Create a debugging prompt with context about the extension failure
    let debug_prompt = format!(
        "I'm having trouble starting an extension called '{}'. Here's the error I encountered:\n\n{}\n\nCan you help me diagnose what might be wrong and suggest how to fix it? Please consider common issues like:\n- Missing dependencies or tools\n- Configuration problems\n- Network connectivity (for remote extensions)\n- Permission issues\n- Path or environment variable problems",
        extension_name,
        error_message
    );

    // Create a minimal agent for debugging
    let debug_agent = Agent::new();

    let session = debug_agent
        .config
        .session_manager
        .create_session(
            std::env::current_dir()?,
            "CLI Session".to_string(),
            SessionType::Hidden,
        )
        .await?;

    debug_agent.update_provider(provider, &session.id).await?;

    // Add the developer extension if available to help with debugging
    let extensions = get_all_extensions();
    for ext_wrapper in extensions {
        if ext_wrapper.enabled && ext_wrapper.config.name() == "developer" {
            if let Err(e) = debug_agent.add_extension(ext_wrapper.config).await {
                // If we can't add developer extension, continue without it
                eprintln!(
                    "Note: Could not load developer extension for debugging: {}",
                    e
                );
            }
            break;
        }
    }

    let mut debug_session = CliSession::new(
        debug_agent,
        session.id,
        false,
        None,
        None,
        None,
        None,
        "text".to_string(),
    )
    .await;

    // Process the debugging request
    println!("{}", style("Analyzing the extension failure...").yellow());
    match debug_session.headless(debug_prompt).await {
        Ok(_) => {
            println!(
                "{}",
                style("Debugging session completed. Check the suggestions above.").green()
            );
        }
        Err(e) => {
            eprintln!(
                "{}",
                style(format!("Debugging session failed: {}", e)).red()
            );
        }
    }
    Ok(())
}

async fn load_extensions(
    agent: Agent,
    extensions_to_load: Vec<(String, ExtensionConfig)>,
    provider_for_debug: Arc<dyn biorouter::providers::base::Provider>,
    interactive: bool,
) -> Arc<Agent> {
    let mut set = JoinSet::new();
    let agent_ptr = Arc::new(agent);

    let mut waiting_ids: BTreeSet<usize> = (0..extensions_to_load.len()).collect();
    for (id, (_label, extension)) in extensions_to_load.iter().enumerate() {
        let agent_ptr = agent_ptr.clone();
        let cfg = extension.clone();
        set.spawn(async move { (id, agent_ptr.add_extension(cfg).await) });
    }

    let get_message = |waiting_ids: &BTreeSet<usize>| {
        let labels: Vec<String> = waiting_ids
            .iter()
            .map(|id| {
                extensions_to_load
                    .get(*id)
                    .map(|e| e.0.clone())
                    .unwrap_or_default()
            })
            .collect();
        format!(
            "starting {} extensions: {}",
            waiting_ids.len(),
            labels.join(", ")
        )
    };

    let spinner = cliclack::spinner();
    spinner.start(get_message(&waiting_ids));

    let mut offer_debug: Vec<(usize, anyhow::Error)> = Vec::new();
    while let Some(result) = set.join_next().await {
        match result {
            Ok((id, Ok(_))) => {
                waiting_ids.remove(&id);
                spinner.set_message(get_message(&waiting_ids));
            }
            Ok((id, Err(e))) => offer_debug.push((id, e.into())),
            Err(e) => tracing::error!("failed to add extension: {}", e),
        }
    }

    spinner.clear();

    for (id, err) in offer_debug {
        let label = extensions_to_load
            .get(id)
            .map(|e| e.0.clone())
            .unwrap_or_default();
        eprintln!(
            "{}",
            style(format!(
                "Warning: Failed to start extension '{}' ({}), continuing without it",
                label, err
            ))
            .yellow()
        );

        if let Err(debug_err) = offer_extension_debugging_help(
            &label,
            &err.to_string(),
            Arc::clone(&provider_for_debug),
            interactive,
        )
        .await
        {
            eprintln!("Note: Could not start debugging session: {}", debug_err);
        }
    }

    agent_ptr
}

fn check_missing_extensions_or_exit(saved_extensions: &[ExtensionConfig], interactive: bool) {
    let missing: Vec<_> = saved_extensions
        .iter()
        .filter(|ext| get_extension_by_name(&ext.name()).is_none())
        .cloned()
        .collect();

    if !missing.is_empty() {
        let names = missing
            .iter()
            .map(|e| e.name())
            .collect::<Vec<_>>()
            .join(", ");

        if interactive {
            if !cliclack::confirm(format!(
                "Extension(s) {} from previous session are no longer available. Restore for this session?",
                names
            ))
            .initial_value(true)
            .interact()
            .unwrap_or(false)
            {
                println!("{}", style("Resume cancelled.").yellow());
                process::exit(0);
            }
        } else {
            eprintln!(
                "{}",
                style(format!(
                    "Warning: Extension(s) {} from previous session are no longer available, continuing without them.",
                    names
                ))
                .yellow()
            );
        }
    }
}

/// #31: the private, per-run session store backing a `--no-session` run.
///
/// `--no-session` is documented as "run without storing a session file", yet
/// it used to create a hidden session in the SHARED
/// `<data_dir>/sessions/sessions.db` — every message of every headless run
/// landed in (and contended on) the same store the desktop app and daemon
/// write. This builds a `SessionManager` rooted in a fresh temp directory
/// instead, giving each `--no-session` run structural isolation. The
/// returned `TempDir` deletes the store when dropped. `build_session`'s
/// early-exit paths call [`close_ephemeral_store_with_manager`] before
/// `process::exit` (which skips destructors) so the pool is closed and the
/// directory removed; panics unwind and drop it normally.
fn ephemeral_session_store() -> anyhow::Result<(tempfile::TempDir, Arc<SessionManager>)> {
    let dir = tempfile::Builder::new()
        .prefix("biorouter-no-session-")
        .tempdir()?;
    let manager = Arc::new(SessionManager::new(dir.path().to_path_buf()));
    Ok((dir, manager))
}

/// #31: `process::exit` skips destructors, so bailing out of `build_session`
/// after the private `--no-session` store was created would leak a
/// `biorouter-no-session-*` directory under the OS temp root on every early
/// exit. Close the store explicitly — **pool first, then the directory** —
/// and let the caller `process::exit`.
///
/// The ordering matters: by the later early-exit paths the agent's SQLite
/// pool is already open on this directory (WAL + -shm files). Deleting the
/// directory while those handles are open happens to work on Unix, but on
/// platforms where unlinking open files fails (Windows) the removal errors
/// and the directory leaks. Closing the pool first releases the handles, so
/// the deletion is reliable everywhere.
async fn close_ephemeral_store_with_manager(
    session_manager: &SessionManager,
    ephemeral_store_dir: Option<tempfile::TempDir>,
) {
    if ephemeral_store_dir.is_some() {
        session_manager.close().await;
    }
    close_ephemeral_store(ephemeral_store_dir);
}

/// Best-effort removal of the private `--no-session` store directory. Split
/// out so the cleanup itself is unit-testable (`process::exit` is not).
/// Prefer [`close_ephemeral_store_with_manager`], which also closes the
/// SQLite pool first.
fn close_ephemeral_store(ephemeral_store_dir: Option<tempfile::TempDir>) {
    if let Some(dir) = ephemeral_store_dir {
        if let Err(e) = dir.close() {
            tracing::warn!(
                "failed to remove the --no-session session store on exit: {}",
                e
            );
        }
    }
}

/// An [`Agent`] whose session manager is the given (private) store, with every
/// other config knob identical to [`Agent::new`].
fn agent_with_session_manager(session_manager: Arc<SessionManager>) -> Agent {
    Agent::with_config(AgentConfig::new(
        session_manager,
        PermissionManager::instance(),
        None,
        Config::global()
            .get_biorouter_mode()
            .unwrap_or(BioRouterMode::Auto),
    ))
}

#[allow(clippy::too_many_lines)]
pub async fn build_session(session_config: SessionBuilderConfig) -> CliSession {
    let config = Config::global();
    // #31: `--no-session` gets a private per-run store so it can never write
    // to — or contend on — the shared sessions.db.
    let mut ephemeral_store_dir: Option<tempfile::TempDir> = None;
    let agent: Agent = if session_config.no_session {
        match ephemeral_session_store() {
            Ok((dir, manager)) => {
                let agent = agent_with_session_manager(manager);
                ephemeral_store_dir = Some(dir);
                agent
            }
            Err(e) => {
                output::render_error(&format!(
                    "Failed to create the private session store for --no-session: {}",
                    e
                ));
                process::exit(1);
            }
        }
    } else {
        Agent::new()
    };
    let session_manager = agent.config.session_manager.clone();

    let (saved_provider, saved_model_config) = if session_config.resume {
        if let Some(ref session_id) = session_config.session_id {
            match session_manager.get_session(session_id, false).await {
                Ok(session_data) => (session_data.provider_name, session_data.model_config),
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let workflow = session_config.workflow.as_ref();
    let workflow_settings = workflow.and_then(|r| r.settings.as_ref());

    let provider_name = session_config
        .provider
        .or(saved_provider)
        .or_else(|| workflow_settings.and_then(|s| s.biorouter_provider.clone()))
        .or_else(|| config.get_biorouter_provider().ok())
        .expect("No provider configured. Run 'biorouter configure' first");

    let model_name = session_config
        .model
        .or_else(|| saved_model_config.as_ref().map(|mc| mc.model_name.clone()))
        .or_else(|| workflow_settings.and_then(|s| s.biorouter_model.clone()))
        .or_else(|| config.get_biorouter_model().ok())
        .expect("No model configured. Run 'biorouter configure' first");

    let model_config = if session_config.resume
        && saved_model_config
            .as_ref()
            .is_some_and(|mc| mc.model_name == model_name)
    {
        let mut config = saved_model_config.unwrap();
        if let Some(temp) = workflow_settings.and_then(|s| s.temperature) {
            config = config.with_temperature(Some(temp));
        }
        config
    } else {
        let temperature = workflow_settings.and_then(|s| s.temperature);
        match biorouter::model::ModelConfig::new(&model_name) {
            Ok(config) => config.with_temperature(temperature),
            Err(e) => {
                output::render_error(&format!("Failed to create model configuration: {}", e));
                close_ephemeral_store_with_manager(&session_manager, ephemeral_store_dir).await;
                process::exit(1);
            }
        }
    };

    agent
        .apply_workflow_components(
            workflow.and_then(|r| r.sub_workflows.clone()),
            workflow.and_then(|r| r.response.clone()),
            true,
        )
        .await;

    let new_provider = match create(&provider_name, model_config).await {
        Ok(provider) => provider,
        Err(e) => {
            output::render_error(&format!(
                "Error {}.\n\
                Please check your system keychain and run 'biorouter configure' again.\n\
                If your system is unable to use the keyring, please try setting secret key(s) via environment variables.\n\
                For more info, see: https://BaranziniLab.github.io/biorouter/docs/troubleshooting/#keychainkeyring-errors",
                e
            ));
            close_ephemeral_store_with_manager(&session_manager, ephemeral_store_dir).await;
            process::exit(1);
        }
    };
    let provider_for_display = Arc::clone(&new_provider);

    if let Some(lead_worker) = new_provider.as_lead_worker() {
        let (lead_model, worker_model) = lead_worker.get_model_info();
        tracing::info!(
            "Lead/worker mode enabled: lead model (first 3 turns): {}, worker model (turn 4+): {}, auto-fallback on failures: enabled",
            lead_model,
            worker_model
        );
    } else {
        tracing::info!("Using model: {}", model_name);
    }

    let session_id: String = if session_config.no_session {
        let working_dir = std::env::current_dir().expect("Could not get working directory");
        // The hidden session lives in the PRIVATE per-run store built above,
        // never in the shared sessions.db (#31).
        let session = match session_manager
            .create_session(working_dir, "CLI Session".to_string(), SessionType::Hidden)
            .await
        {
            Ok(session) => session,
            Err(e) => {
                let store = ephemeral_store_dir
                    .as_ref()
                    .map(|d| d.path().join("sessions").join("sessions.db"))
                    .unwrap_or_default();
                output::render_error(&format!(
                    "Could not initialize the --no-session session store at {}: {}",
                    store.display(),
                    e
                ));
                close_ephemeral_store_with_manager(&session_manager, ephemeral_store_dir).await;
                process::exit(1);
            }
        };
        session.id
    } else if session_config.resume {
        if let Some(session_id) = session_config.session_id {
            match session_manager.get_session(&session_id, false).await {
                Ok(_) => session_id,
                Err(_) => {
                    output::render_error(&format!(
                        "Cannot resume session {} - no such session exists",
                        style(&session_id).cyan()
                    ));
                    close_ephemeral_store_with_manager(&session_manager, ephemeral_store_dir).await;
                    process::exit(1);
                }
            }
        } else {
            match session_manager.list_sessions().await {
                Ok(sessions) if !sessions.is_empty() => sessions[0].id.clone(),
                _ => {
                    output::render_error("Cannot resume - no previous sessions found");
                    close_ephemeral_store_with_manager(&session_manager, ephemeral_store_dir).await;
                    process::exit(1);
                }
            }
        }
    } else {
        session_config.session_id.unwrap()
    };

    if let Err(e) = agent.update_provider(new_provider, &session_id).await {
        output::render_error(&format!("Failed to initialize agent: {}", e));
        close_ephemeral_store_with_manager(&session_manager, ephemeral_store_dir).await;
        process::exit(1);
    }

    if session_config.resume {
        let session = match agent
            .config
            .session_manager
            .get_session(&session_id, false)
            .await
        {
            Ok(session) => session,
            Err(e) => {
                output::render_error(&format!("Failed to read session metadata: {}", e));
                close_ephemeral_store_with_manager(&session_manager, ephemeral_store_dir).await;
                process::exit(1);
            }
        };

        let current_workdir =
            std::env::current_dir().expect("Failed to get current working directory");
        if current_workdir != session.working_dir {
            if session_config.interactive {
                let change_workdir = cliclack::confirm(format!("{} The original working directory of this session was set to {}. Your current directory is {}. Do you want to switch back to the original working directory?", style("WARNING:").yellow(), style(session.working_dir.display()).cyan(), style(current_workdir.display()).cyan()))
                        .initial_value(true)
                        .interact().expect("Failed to get user input");

                if change_workdir {
                    if !session.working_dir.exists() {
                        output::render_error(&format!(
                            "Cannot switch to original working directory - {} no longer exists",
                            style(session.working_dir.display()).cyan()
                        ));
                    } else if let Err(e) = std::env::set_current_dir(&session.working_dir) {
                        output::render_error(&format!(
                            "Failed to switch to original working directory: {}",
                            e
                        ));
                    }
                }
            } else {
                eprintln!(
                    "{}",
                    style(format!(
                        "Warning: Working directory differs from session (current: {}, session: {}). Staying in current directory.",
                        current_workdir.display(),
                        session.working_dir.display()
                    ))
                    .yellow()
                );
            }
        }
    }

    // Setup extensions for the agent
    // Extensions need to be added after the session is created because we change directory when resuming a session

    for warning in biorouter::config::get_warnings() {
        eprintln!("{}", style(format!("Warning: {}", warning)).yellow());
    }

    let configured_extensions: Vec<ExtensionConfig> = if session_config.resume {
        agent
            .config
            .session_manager
            .get_session(&session_id, false)
            .await
            .ok()
            .and_then(|s| EnabledExtensionsState::from_extension_data(&s.extension_data))
            .map(|state| {
                check_missing_extensions_or_exit(&state.extensions, session_config.interactive);
                state.extensions
            })
            .unwrap_or_else(get_enabled_extensions)
    } else {
        resolve_extensions_for_new_session(workflow.and_then(|r| r.extensions.as_deref()), None)
    };

    let cli_flag_extensions_to_load = parse_cli_flag_extensions(
        &session_config.extensions,
        &session_config.streamable_http_extensions,
        &session_config.builtins,
    );

    let mut extensions_to_load: Vec<(String, ExtensionConfig)> = configured_extensions
        .iter()
        .map(|cfg| (cfg.name(), cfg.clone()))
        .collect();
    extensions_to_load.extend(cli_flag_extensions_to_load);

    let agent_ptr = load_extensions(
        agent,
        extensions_to_load,
        Arc::clone(&provider_for_display),
        session_config.interactive,
    )
    .await;

    // Determine editor mode
    let edit_mode = config
        .get_param::<String>("EDIT_MODE")
        .ok()
        .and_then(|edit_mode| match edit_mode.to_lowercase().as_str() {
            "emacs" => Some(EditMode::Emacs),
            "vi" => Some(EditMode::Vi),
            _ => {
                eprintln!("Invalid EDIT_MODE specified, defaulting to Emacs");
                None
            }
        });

    let debug_mode = session_config.debug || config.get_param("BIOROUTER_DEBUG").unwrap_or(false);

    let mut session = CliSession::new(
        Arc::try_unwrap(agent_ptr).unwrap_or_else(|_| panic!("There should be no more references")),
        session_id.clone(),
        debug_mode,
        session_config.scheduled_job_id.clone(),
        session_config.max_turns,
        edit_mode,
        workflow.and_then(|r| r.retry.clone()),
        session_config.output_format.clone(),
    )
    .await;
    // #31: keep the private --no-session store alive for the whole run; its
    // temp dir is deleted (best-effort) when the session is dropped.
    if let Some(dir) = ephemeral_store_dir {
        session.hold_ephemeral_store_dir(dir);
    }

    if let Err(e) = session
        .agent
        .persist_extension_state(&session_id.clone())
        .await
    {
        tracing::warn!("Failed to save extension state: {}", e);
    }

    // Add CLI-specific system prompt extension
    session
        .agent
        .extend_system_prompt(super::prompt::get_cli_prompt())
        .await;

    if let Some(additional_prompt) = session_config.additional_system_prompt {
        session.agent.extend_system_prompt(additional_prompt).await;
    }

    // Only override system prompt if a system override exists
    let system_prompt_file: Option<String> =
        config.get_param("BIOROUTER_SYSTEM_PROMPT_FILE_PATH").ok();
    if let Some(ref path) = system_prompt_file {
        let override_prompt =
            std::fs::read_to_string(path).expect("Failed to read system prompt file");
        session.agent.override_system_prompt(override_prompt).await;
    }

    // Display session information unless in quiet mode
    if !session_config.quiet {
        output::display_session_info(
            session_config.resume,
            &provider_name,
            &model_name,
            &Some(session_id),
            Some(&provider_for_display),
        );
    }
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #31: the `--no-session` store must be a private per-run directory —
    /// never the shared `<data_dir>/sessions/sessions.db` — and it must be a
    /// fully functional session store (create / write / read back).
    #[tokio::test]
    async fn no_session_store_is_private_and_functional() {
        let shared_root = biorouter::config::paths::Paths::data_dir();
        let shared_db = shared_root.join("sessions").join("sessions.db");
        let shared_mtime_before = std::fs::metadata(&shared_db)
            .and_then(|m| m.modified())
            .ok();

        let (dir, manager) = ephemeral_session_store().expect("ephemeral store");
        assert_ne!(dir.path(), shared_root.as_path());
        assert!(
            !dir.path().starts_with(&shared_root),
            "the ephemeral store must not live under the shared data dir ({})",
            shared_root.display()
        );

        // The exact operations a --no-session run performs against its store.
        let session = manager
            .create_session(
                dir.path().to_path_buf(),
                "CLI Session".to_string(),
                SessionType::Hidden,
            )
            .await
            .expect("create session in the private store");
        manager
            .add_message(
                &session.id,
                &biorouter::conversation::message::Message::user().with_text("hello"),
            )
            .await
            .expect("write a message to the private store");
        let loaded = manager
            .get_session(&session.id, true)
            .await
            .expect("read back from the private store");
        assert_eq!(loaded.conversation.unwrap().len(), 1);

        // The writes landed in the private store...
        assert!(
            dir.path().join("sessions").join("sessions.db").exists(),
            "the private sessions.db must exist under the temp dir"
        );
        // ...and the shared sessions.db was untouched (same mtime, or still
        // absent) throughout the run.
        let shared_mtime_after = std::fs::metadata(&shared_db)
            .and_then(|m| m.modified())
            .ok();
        assert_eq!(
            shared_mtime_before, shared_mtime_after,
            "a --no-session run must not touch the shared session store"
        );
    }

    /// #31: the explicit early-exit cleanup removes the private store (and
    /// tolerates `None`). `process::exit` itself is untestable; this pins the
    /// close half that every early-exit path runs first.
    #[tokio::test]
    async fn close_ephemeral_store_removes_the_directory() {
        let (dir, manager) = ephemeral_session_store().expect("ephemeral store");
        let path = dir.path().to_path_buf();
        manager
            .create_session(path.clone(), "CLI Session".to_string(), SessionType::Hidden)
            .await
            .expect("create session");
        drop(manager);
        assert!(path.exists());
        close_ephemeral_store(Some(dir));
        assert!(
            !path.exists(),
            "an early exit must not leak the biorouter-no-session-* directory"
        );
        // A run without --no-session has no store to close.
        close_ephemeral_store(None);
    }

    /// #31 ordering: the early-exit cleanup closes the SQLite pool BEFORE
    /// deleting the temp directory, releasing the WAL/-shm handles that made
    /// the removal fail (and leak the dir) on platforms where deleting open
    /// files is not allowed. On Unix the deletion succeeds either way, so
    /// the pool-closed assertion is the cross-platform proof of ordering.
    #[tokio::test]
    async fn early_exit_cleanup_closes_the_pool_before_deleting_the_store() {
        let (dir, manager) = ephemeral_session_store().expect("ephemeral store");
        let path = dir.path().to_path_buf();
        // Open the pool with real traffic so WAL files exist.
        manager
            .create_session(path.clone(), "CLI Session".to_string(), SessionType::Hidden)
            .await
            .expect("create session");

        close_ephemeral_store_with_manager(&manager, Some(dir)).await;

        assert!(
            !path.exists(),
            "the biorouter-no-session-* directory must be removed"
        );
        assert!(
            manager
                .create_session(path, "again".to_string(), SessionType::Hidden)
                .await
                .is_err(),
            "the pool must be closed, not still writing into deleted files"
        );

        // A run without --no-session must leave its (shared) store untouched.
        let (dir2, manager2) = ephemeral_session_store().expect("ephemeral store");
        close_ephemeral_store_with_manager(&manager2, None).await;
        manager2
            .create_session(
                dir2.path().to_path_buf(),
                "still open".to_string(),
                SessionType::Hidden,
            )
            .await
            .expect("no dir handed over means the store must stay usable");
    }

    /// #31: dropping the `TempDir` (end of run) removes the private store —
    /// the best-effort cleanup contract.
    #[tokio::test]
    async fn dropping_the_ephemeral_store_removes_it() {
        let (dir, manager) = ephemeral_session_store().expect("ephemeral store");
        let path = dir.path().to_path_buf();
        manager
            .create_session(path.clone(), "CLI Session".to_string(), SessionType::Hidden)
            .await
            .expect("create session");
        assert!(path.exists());
        drop(manager);
        drop(dir);
        assert!(
            !path.exists(),
            "the private store must be cleaned up on drop"
        );
    }

    #[test]
    fn test_session_builder_config_creation() {
        let config = SessionBuilderConfig {
            session_id: None,
            resume: false,
            no_session: false,
            extensions: vec!["echo test".to_string()],
            streamable_http_extensions: vec!["http://localhost:8080/mcp".to_string()],
            builtins: vec!["developer".to_string()],
            workflow: None,
            additional_system_prompt: Some("Test prompt".to_string()),
            provider: None,
            model: None,
            debug: true,
            max_tool_repetitions: Some(5),
            max_turns: None,
            scheduled_job_id: None,
            interactive: true,
            quiet: false,
            output_format: "text".to_string(),
        };

        assert_eq!(config.extensions.len(), 1);
        assert_eq!(config.streamable_http_extensions.len(), 1);
        assert_eq!(config.builtins.len(), 1);
        assert!(config.debug);
        assert_eq!(config.max_tool_repetitions, Some(5));
        assert!(config.max_turns.is_none());
        assert!(config.scheduled_job_id.is_none());
        assert!(config.interactive);
        assert!(!config.quiet);
    }

    #[test]
    fn test_session_builder_config_default() {
        let config = SessionBuilderConfig::default();

        assert!(config.session_id.is_none());
        assert!(!config.resume);
        assert!(!config.no_session);
        assert!(config.extensions.is_empty());
        assert!(config.streamable_http_extensions.is_empty());
        assert!(config.builtins.is_empty());
        assert!(config.workflow.is_none());
        assert!(config.additional_system_prompt.is_none());
        assert!(!config.debug);
        assert!(config.max_tool_repetitions.is_none());
        assert!(config.max_turns.is_none());
        assert!(config.scheduled_job_id.is_none());
        assert!(!config.interactive);
        assert!(!config.quiet);
    }

    #[tokio::test]
    async fn test_offer_extension_debugging_help_function_exists() {
        // This test just verifies the function compiles and can be called
        // We can't easily test the interactive parts without mocking

        // We can't actually test the full function without a real provider and user interaction
        // But we can at least verify it compiles and the function signature is correct
        let extension_name = "test-extension";
        let error_message = "test error";

        // This test mainly serves as a compilation check
        assert_eq!(extension_name, "test-extension");
        assert_eq!(error_message, "test error");
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("abc", 5), "abc");

        assert_eq!(truncate_with_ellipsis("abcde", 5), "abcde");

        assert_eq!(truncate_with_ellipsis("abcdef", 5), "abcde…");
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");

        assert_eq!(truncate_with_ellipsis("", 5), "");
    }
}
