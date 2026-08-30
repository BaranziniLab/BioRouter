use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_cron_scheduler::{job::JobId, Job, JobScheduler as TokioJobScheduler};
use tokio_util::sync::CancellationToken;

use crate::agents::AgentEvent;
use crate::agents::{Agent, SessionConfig};
use crate::config::paths::Paths;
use crate::config::{resolve_extensions_for_new_session, Config};
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::scheduler_trait::SchedulerTrait;
use crate::session::session_manager::SessionType;
use crate::session::{Session, SessionManager};
use crate::workflow::Workflow;

type RunningTasksMap = HashMap<String, CancellationToken>;
type JobsMap = HashMap<String, (JobId, ScheduledJob)>;

/// Count of in-progress *interactive* (user-facing) agent turns. While > 0 the
/// scheduler defers scheduled jobs so background work doesn't compete with the
/// user for the provider / rate-limit budget (jcode's "pause when active").
static ACTIVE_INTERACTIVE_TURNS: AtomicUsize = AtomicUsize::new(0);

/// RAII guard marking an interactive turn in progress. Hold it for the lifetime
/// of an interactive reply (e.g. the HTTP `/reply` SSE stream).
pub struct InteractiveTurnGuard;

/// Begin an interactive turn; the returned guard decrements the counter on drop.
pub fn interactive_turn_guard() -> InteractiveTurnGuard {
    ACTIVE_INTERACTIVE_TURNS.fetch_add(1, Ordering::SeqCst);
    InteractiveTurnGuard
}

impl Drop for InteractiveTurnGuard {
    fn drop(&mut self) {
        ACTIVE_INTERACTIVE_TURNS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn interactive_active() -> bool {
    ACTIVE_INTERACTIVE_TURNS.load(Ordering::SeqCst) > 0
}

/// Whether to pause scheduled work while a user is interacting. Default on; set
/// `BIOROUTER_SCHEDULER_PAUSE_ON_ACTIVE=0` to disable.
fn pause_on_active() -> bool {
    !matches!(
        std::env::var("BIOROUTER_SCHEDULER_PAUSE_ON_ACTIVE")
            .ok()
            .as_deref(),
        Some("0") | Some("false")
    )
}

pub fn get_default_scheduler_storage_path() -> Result<PathBuf, io::Error> {
    let data_dir = Paths::data_dir();
    fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("schedule.json"))
}

pub fn get_default_scheduled_workflows_dir() -> Result<PathBuf, SchedulerError> {
    let data_dir = Paths::data_dir();
    let workflows_dir = data_dir.join("scheduled_workflows");
    fs::create_dir_all(&workflows_dir).map_err(SchedulerError::StorageError)?;
    Ok(workflows_dir)
}

#[derive(Debug)]
pub enum SchedulerError {
    JobIdExists(String),
    JobNotFound(String),
    StorageError(io::Error),
    WorkflowLoadError(String),
    AgentSetupError(String),
    PersistError(String),
    CronParseError(String),
    SchedulerInternalError(String),
    AnyhowError(anyhow::Error),
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedulerError::JobIdExists(id) => write!(f, "Job ID '{}' already exists.", id),
            SchedulerError::JobNotFound(id) => write!(f, "Job ID '{}' not found.", id),
            SchedulerError::StorageError(e) => write!(f, "Storage error: {}", e),
            SchedulerError::WorkflowLoadError(e) => write!(f, "Workflow load error: {}", e),
            SchedulerError::AgentSetupError(e) => write!(f, "Agent setup error: {}", e),
            SchedulerError::PersistError(e) => write!(f, "Failed to persist schedules: {}", e),
            SchedulerError::CronParseError(e) => write!(f, "Invalid cron string: {}", e),
            SchedulerError::SchedulerInternalError(e) => {
                write!(f, "Scheduler internal error: {}", e)
            }
            SchedulerError::AnyhowError(e) => write!(f, "Scheduler operation failed: {}", e),
        }
    }
}

impl std::error::Error for SchedulerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SchedulerError::StorageError(e) => Some(e),
            SchedulerError::AnyhowError(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<io::Error> for SchedulerError {
    fn from(err: io::Error) -> Self {
        SchedulerError::StorageError(err)
    }
}

impl From<serde_json::Error> for SchedulerError {
    fn from(err: serde_json::Error) -> Self {
        SchedulerError::PersistError(err.to_string())
    }
}

impl From<anyhow::Error> for SchedulerError {
    fn from(err: anyhow::Error) -> Self {
        SchedulerError::AnyhowError(err)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, utoipa::ToSchema)]
pub struct ScheduledJob {
    pub id: String,
    pub source: String,
    pub cron: String,
    pub last_run: Option<DateTime<Utc>>,
    #[serde(default)]
    pub currently_running: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub current_session_id: Option<String>,
    #[serde(default)]
    pub process_start_time: Option<DateTime<Utc>>,
    /// Number of times this job has fired. Used to enforce `max_runs`.
    #[serde(default)]
    pub run_count: u32,
    /// Optional cap on total firings. When `Some(n)`, the job auto-pauses once
    /// `run_count` reaches `n` — the bound that keeps `/loop` from running
    /// forever. `None` = unbounded (durable `/schedule` jobs).
    #[serde(default)]
    pub max_runs: Option<u32>,
    /// Issue #56 (§9.3 C2). The chat this schedule was created from, when there
    /// was one — `/loop` and `/schedule` always have one; a workflow scheduled
    /// from the CLI or the schedules route does not.
    ///
    /// A scheduled run resolves its provider from THIS session's recorded
    /// `provider_name` before falling back to the global default (see
    /// [`resolve_scheduled_provider`]). Without it a job created from a chat on
    /// a private model silently runs on the user's commercial default — R5 —
    /// and, once the design's §6.3 rule that a `Scheduled` session inherits its
    /// creator's classification lands, Gate A would refuse the bind on every
    /// tick forever.
    #[serde(default)]
    pub creator_session_id: Option<String>,
    /// The last run's failure, kept ON THE JOB so the schedules UI can show it.
    ///
    /// A cron tick that returns `Err` used to leave nothing behind but a log
    /// line, and a scheduled run mints a fresh session each time, so there was
    /// no surface anywhere that a repeating job had been failing since the day
    /// it was created. Cleared by the next successful run.
    #[serde(default)]
    pub last_error: Option<String>,
}

/// Decide, under one lock, whether a fired cron job should actually run, and
/// stamp its start state if so. Returns the run snapshot when the caller should
/// execute. Its `last_run` is the last successful run, suitable as a cursor.
///
/// Skips when the job is gone, paused, still running from a previous firing
/// (overlap guard — a slow run never stacks), or has hit its `max_runs` cap
/// (which also auto-pauses it). On a real run it marks the job running and bumps
/// `run_count`; completion advances `last_run` only after success.
async fn claim_run_slot(
    jobs: &Arc<Mutex<JobsMap>>,
    job_id: &str,
    now: DateTime<Utc>,
) -> Option<ScheduledJob> {
    let mut jobs_guard = jobs.lock().await;
    match jobs_guard.get_mut(job_id) {
        None => None,
        Some((_, job)) if job.paused => None,
        Some((_, job)) if job.currently_running => {
            tracing::info!("Skipping job '{}': previous run still in progress", job_id);
            None
        }
        Some((_, job)) if job.max_runs.is_some_and(|max| job.run_count >= max) => {
            tracing::info!(
                "Job '{}' reached its run cap ({:?}); auto-pausing",
                job_id,
                job.max_runs
            );
            job.paused = true;
            None
        }
        // Resource-aware deferral (jcode "ambient" idea): skip this firing (the
        // cron fires again next interval) when the provider is rate-limited or a
        // user is mid-conversation, so background work never competes with the
        // user. Does NOT bump run_count, so the job isn't consumed.
        Some(_) if crate::providers::retry::is_rate_limited() => {
            tracing::info!(
                "Deferring scheduled job '{}': provider rate-limited, backing off",
                job_id
            );
            None
        }
        Some(_) if pause_on_active() && interactive_active() => {
            tracing::info!("Deferring scheduled job '{}': user session active", job_id);
            None
        }
        Some((_, job)) => {
            job.currently_running = true;
            job.process_start_time = Some(now);
            job.run_count = job.run_count.saturating_add(1);
            Some(job.clone())
        }
    }
}

async fn persist_jobs(
    storage_path: &Path,
    jobs: &Arc<Mutex<JobsMap>>,
) -> Result<(), SchedulerError> {
    let jobs_guard = jobs.lock().await;
    let list: Vec<ScheduledJob> = jobs_guard.values().map(|(_, j)| j.clone()).collect();
    if let Some(parent) = storage_path.parent() {
        // tokio::fs to avoid blocking the runtime — persist_jobs runs from
        // async cron callbacks, several times per job firing.
        tokio::fs::create_dir_all(parent).await?;
    }
    let data = serde_json::to_string_pretty(&list)?;
    tokio::fs::write(storage_path, data).await?;
    Ok(())
}

pub struct Scheduler {
    tokio_scheduler: TokioJobScheduler,
    jobs: Arc<Mutex<JobsMap>>,
    storage_path: PathBuf,
    running_tasks: Arc<Mutex<RunningTasksMap>>,
    session_manager: Arc<SessionManager>,
}

impl Scheduler {
    pub async fn new(
        storage_path: PathBuf,
        session_manager: Arc<SessionManager>,
    ) -> Result<Arc<Self>, SchedulerError> {
        let internal_scheduler = TokioJobScheduler::new()
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        let jobs = Arc::new(Mutex::new(HashMap::new()));
        let running_tasks = Arc::new(Mutex::new(HashMap::new()));

        let arc_self = Arc::new(Self {
            tokio_scheduler: internal_scheduler,
            jobs,
            storage_path,
            running_tasks,
            session_manager,
        });

        arc_self.load_jobs_from_storage().await;
        arc_self
            .tokio_scheduler
            .start()
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        Ok(arc_self)
    }

    fn create_cron_task(&self, job: ScheduledJob) -> Result<Job, SchedulerError> {
        let job_for_task = job.clone();
        let jobs_arc = self.jobs.clone();
        let storage_path = self.storage_path.clone();
        let running_tasks_arc = self.running_tasks.clone();

        let cron_parts: Vec<&str> = job.cron.split_whitespace().collect();
        let cron = match cron_parts.len() {
            5 => {
                tracing::warn!(
                    "Job '{}' has legacy 5-field cron '{}', converting to 6-field",
                    job.id,
                    job.cron
                );
                format!("0 {}", job.cron)
            }
            6 => job.cron.clone(),
            _ => {
                return Err(SchedulerError::CronParseError(format!(
                    "Invalid cron expression '{}': expected 5 or 6 fields, got {}",
                    job.cron,
                    cron_parts.len()
                )))
            }
        };

        let local_tz = Local::now().timezone();

        Job::new_async_tz(&cron, local_tz, move |_uuid, _l| {
            tracing::info!("Cron task triggered for job '{}'", job_for_task.id);
            let task_job_id = job_for_task.id.clone();
            let current_jobs_arc = jobs_arc.clone();
            let local_storage_path = storage_path.clone();
            let running_tasks = running_tasks_arc.clone();

            Box::pin(async move {
                let Some(job_to_execute) =
                    claim_run_slot(&current_jobs_arc, &task_job_id, Utc::now()).await
                else {
                    // Persist the auto-pause (if any) so it survives restart.
                    if let Err(e) = persist_jobs(&local_storage_path, &current_jobs_arc).await {
                        tracing::error!("Failed to persist job status: {}", e);
                    }
                    return;
                };

                if let Err(e) = persist_jobs(&local_storage_path, &current_jobs_arc).await {
                    tracing::error!("Failed to persist job status: {}", e);
                }

                let cancel_token = CancellationToken::new();
                {
                    let mut tasks = running_tasks.lock().await;
                    tasks.insert(task_job_id.clone(), cancel_token.clone());
                }

                let result = execute_job(
                    job_to_execute,
                    current_jobs_arc.clone(),
                    task_job_id.clone(),
                    cancel_token.clone(),
                )
                .await;

                {
                    let mut tasks = running_tasks.lock().await;
                    tasks.remove(&task_job_id);
                }

                {
                    let mut jobs_guard = current_jobs_arc.lock().await;
                    if let Some((_, job)) = jobs_guard.get_mut(&task_job_id) {
                        job.currently_running = false;
                        job.current_session_id = None;
                        job.process_start_time = None;
                        // Issue #56 (§9.3 C2). A failing tick leaves a
                        // job-level error the schedules UI can show, instead of
                        // only a log line nobody reads — a scheduled run mints
                        // a new session each time, so the failure has no other
                        // durable home. Cleared by the next run that succeeds.
                        job.last_error = match &result {
                            Ok(_) => None,
                            Err(e) => Some(format!("{e:#}")),
                        };
                        if result.is_ok() {
                            job.last_run = Some(Utc::now());
                        }
                    }
                }

                if let Err(e) = persist_jobs(&local_storage_path, &current_jobs_arc).await {
                    tracing::error!("Failed to persist job completion: {}", e);
                }

                match result {
                    Ok(_) => tracing::info!("Job '{}' completed", task_job_id),
                    Err(ref e) => {
                        tracing::error!("Job '{}' failed: {}", task_job_id, e);
                    }
                }
            })
        })
        .map_err(|e| SchedulerError::CronParseError(e.to_string()))
    }

    pub async fn add_scheduled_job(
        &self,
        original_job_spec: ScheduledJob,
        make_copy: bool,
    ) -> Result<(), SchedulerError> {
        {
            let jobs_guard = self.jobs.lock().await;
            if jobs_guard.contains_key(&original_job_spec.id) {
                return Err(SchedulerError::JobIdExists(original_job_spec.id.clone()));
            }
        }

        let mut stored_job = original_job_spec;
        if make_copy {
            let original_workflow_path = Path::new(&stored_job.source);
            if !original_workflow_path.is_file() {
                return Err(SchedulerError::WorkflowLoadError(format!(
                    "Workflow file not found: {}",
                    stored_job.source
                )));
            }

            let scheduled_workflows_dir = get_default_scheduled_workflows_dir()?;
            let original_extension = original_workflow_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("yaml");

            let destination_filename = format!("{}.{}", stored_job.id, original_extension);
            let destination_workflow_path = scheduled_workflows_dir.join(destination_filename);

            fs::copy(original_workflow_path, &destination_workflow_path)?;
            stored_job.source = destination_workflow_path.to_string_lossy().into_owned();
            stored_job.current_session_id = None;
            stored_job.process_start_time = None;
        }

        let cron_task = self.create_cron_task(stored_job.clone())?;

        let job_uuid = self
            .tokio_scheduler
            .add(cron_task)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        {
            let mut jobs_guard = self.jobs.lock().await;
            jobs_guard.insert(stored_job.id.clone(), (job_uuid, stored_job));
        }

        persist_jobs(&self.storage_path, &self.jobs).await?;
        Ok(())
    }

    pub async fn schedule_workflow(
        &self,
        workflow_path: PathBuf,
        cron_schedule: Option<String>,
    ) -> Result<(), SchedulerError> {
        let workflow_path_str = workflow_path.to_string_lossy().to_string();

        let existing_job_id = {
            let jobs_guard = self.jobs.lock().await;
            jobs_guard
                .iter()
                .find(|(_, (_, job))| job.source == workflow_path_str)
                .map(|(id, _)| id.clone())
        };

        match cron_schedule {
            Some(cron) => {
                if let Some(job_id) = existing_job_id {
                    self.update_schedule(&job_id, cron).await
                } else {
                    let job_id = self.generate_unique_job_id(&workflow_path).await;
                    let job = ScheduledJob {
                        id: job_id,
                        source: workflow_path_str,
                        cron,
                        last_run: None,
                        currently_running: false,
                        paused: false,
                        current_session_id: None,
                        process_start_time: None,
                        run_count: 0,
                        max_runs: None,
                        // A workflow scheduled by path has no creating chat.
                        creator_session_id: None,
                        last_error: None,
                    };
                    self.add_scheduled_job(job, false).await
                }
            }
            None => {
                if let Some(job_id) = existing_job_id {
                    self.remove_scheduled_job(&job_id, false).await
                } else {
                    Ok(())
                }
            }
        }
    }

    async fn generate_unique_job_id(&self, path: &Path) -> String {
        let base_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let jobs_guard = self.jobs.lock().await;
        let mut id = base_id.clone();
        let mut counter = 1;

        while jobs_guard.contains_key(&id) {
            id = format!("{}_{}", base_id, counter);
            counter += 1;
        }

        id
    }

    async fn load_jobs_from_storage(self: &Arc<Self>) {
        if !self.storage_path.exists() {
            return;
        }
        let data = match fs::read_to_string(&self.storage_path) {
            Ok(data) => data,
            Err(e) => {
                tracing::error!(
                    "Failed to read {}: {}. Starting with empty schedule list.",
                    self.storage_path.display(),
                    e
                );
                return;
            }
        };
        if data.trim().is_empty() {
            return;
        }

        let list: Vec<ScheduledJob> = match serde_json::from_str(&data) {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::error!(
                    "Failed to parse {}: {}. Starting with empty schedule list.",
                    self.storage_path.display(),
                    e
                );
                return;
            }
        };

        for mut job_to_load in list {
            // BR-38: a scheduled run lives only in memory — no process or turn
            // survives a daemon restart. A job persisted as `currently_running`
            // was mid-run when we crashed, so its run is already gone. Reconcile
            // the stale flag on load; otherwise the overlap guard in
            // `claim_run_slot` would treat the ghost run as still in progress and
            // skip this job forever.
            if job_to_load.currently_running
                || job_to_load.current_session_id.is_some()
                || job_to_load.process_start_time.is_some()
            {
                tracing::warn!(
                    "Resetting stale running state for scheduled job '{}' on load (session {:?} did not survive restart)",
                    job_to_load.id,
                    job_to_load.current_session_id
                );
                job_to_load.currently_running = false;
                job_to_load.current_session_id = None;
                job_to_load.process_start_time = None;
            }

            if !Path::new(&job_to_load.source).exists() {
                tracing::warn!(
                    "Workflow file {} not found, skipping job '{}'",
                    job_to_load.source,
                    job_to_load.id
                );
                continue;
            }

            let cron_task = match self.create_cron_task(job_to_load.clone()) {
                Ok(task) => task,
                Err(e) => {
                    tracing::error!(
                        "Failed to create cron task for job '{}': {}. Skipping.",
                        job_to_load.id,
                        e
                    );
                    continue;
                }
            };

            let job_uuid = match self.tokio_scheduler.add(cron_task).await {
                Ok(uuid) => uuid,
                Err(e) => {
                    tracing::error!(
                        "Failed to add job '{}' to scheduler: {}. Skipping.",
                        job_to_load.id,
                        e
                    );
                    continue;
                }
            };

            let mut jobs_guard = self.jobs.lock().await;
            jobs_guard.insert(job_to_load.id.clone(), (job_uuid, job_to_load));
        }
    }

    pub async fn list_scheduled_jobs(&self) -> Vec<ScheduledJob> {
        self.jobs
            .lock()
            .await
            .values()
            .map(|(_, j)| j.clone())
            .collect()
    }

    pub async fn remove_scheduled_job(
        &self,
        id: &str,
        remove_workflow: bool,
    ) -> Result<(), SchedulerError> {
        let (job_uuid, workflow_path) = {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.remove(id) {
                Some((uuid, job)) => (uuid, job.source.clone()),
                None => return Err(SchedulerError::JobNotFound(id.to_string())),
            }
        };

        self.tokio_scheduler
            .remove(&job_uuid)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        if remove_workflow {
            let path = Path::new(&workflow_path);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await?;
        Ok(())
    }

    pub async fn sessions(
        &self,
        sched_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, Session)>, SchedulerError> {
        let all_sessions = self
            .session_manager
            .list_sessions()
            .await
            .map_err(|e| SchedulerError::StorageError(io::Error::other(e)))?;

        let mut schedule_sessions: Vec<(String, Session)> = all_sessions
            .into_iter()
            .filter(|s| s.schedule_id.as_deref() == Some(sched_id))
            .map(|s| (s.id.clone(), s))
            .collect();

        schedule_sessions.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
        schedule_sessions.truncate(limit);

        Ok(schedule_sessions)
    }

    pub async fn run_now(&self, sched_id: &str) -> Result<String, SchedulerError> {
        let job_to_run = {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((_, job)) => {
                    if job.currently_running {
                        return Err(SchedulerError::AnyhowError(anyhow!(
                            "Job '{}' is already running",
                            sched_id
                        )));
                    }
                    job.currently_running = true;
                    job.process_start_time = Some(Utc::now());
                    job.clone()
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        };

        persist_jobs(&self.storage_path, &self.jobs).await?;

        let cancel_token = CancellationToken::new();
        {
            let mut tasks = self.running_tasks.lock().await;
            tasks.insert(sched_id.to_string(), cancel_token.clone());
        }

        let result = execute_job(
            job_to_run,
            self.jobs.clone(),
            sched_id.to_string(),
            cancel_token.clone(),
        )
        .await;

        {
            let mut tasks = self.running_tasks.lock().await;
            tasks.remove(sched_id);
        }

        {
            let mut jobs_guard = self.jobs.lock().await;
            if let Some((_, job)) = jobs_guard.get_mut(sched_id) {
                job.currently_running = false;
                job.current_session_id = None;
                job.process_start_time = None;
                if result.is_ok() {
                    job.last_run = Some(Utc::now());
                }
                // Issue #56 (§9.3 C2), same rule as the cron path: the failure
                // is recorded on the schedule, not only returned to whoever
                // pressed "run now".
                job.last_error = match &result {
                    Ok(_) => None,
                    Err(e) => Some(format!("{e:#}")),
                };
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await?;

        match result {
            Ok(session_id) => Ok(session_id),
            Err(e) => Err(SchedulerError::AnyhowError(anyhow!(
                "Job '{}' failed: {}",
                sched_id,
                e
            ))),
        }
    }

    pub async fn pause_schedule(&self, sched_id: &str) -> Result<(), SchedulerError> {
        {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((_, job)) => {
                    if job.currently_running {
                        return Err(SchedulerError::AnyhowError(anyhow!(
                            "Cannot pause running schedule '{}'",
                            sched_id
                        )));
                    }
                    job.paused = true;
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await
    }

    pub async fn unpause_schedule(&self, sched_id: &str) -> Result<(), SchedulerError> {
        {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((_, job)) => job.paused = false,
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await
    }

    pub async fn update_schedule(
        &self,
        sched_id: &str,
        new_cron: String,
    ) -> Result<(), SchedulerError> {
        let (old_uuid, updated_job) = {
            let mut jobs_guard = self.jobs.lock().await;
            match jobs_guard.get_mut(sched_id) {
                Some((uuid, job)) => {
                    if job.currently_running {
                        return Err(SchedulerError::AnyhowError(anyhow!(
                            "Cannot update running schedule '{}'",
                            sched_id
                        )));
                    }
                    if new_cron == job.cron {
                        return Ok(());
                    }
                    job.cron = new_cron.clone();
                    (*uuid, job.clone())
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
            }
        };

        self.tokio_scheduler
            .remove(&old_uuid)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        let cron_task = self.create_cron_task(updated_job)?;
        let new_uuid = self
            .tokio_scheduler
            .add(cron_task)
            .await
            .map_err(|e| SchedulerError::SchedulerInternalError(e.to_string()))?;

        {
            let mut jobs_guard = self.jobs.lock().await;
            if let Some((uuid, _)) = jobs_guard.get_mut(sched_id) {
                *uuid = new_uuid;
            }
        }

        persist_jobs(&self.storage_path, &self.jobs).await
    }

    pub async fn kill_running_job(&self, sched_id: &str) -> Result<(), SchedulerError> {
        {
            let jobs_guard = self.jobs.lock().await;
            match jobs_guard.get(sched_id) {
                Some((_, job)) if !job.currently_running => {
                    return Err(SchedulerError::AnyhowError(anyhow!(
                        "Schedule '{}' is not running",
                        sched_id
                    )));
                }
                None => return Err(SchedulerError::JobNotFound(sched_id.to_string())),
                _ => {}
            }
        }

        {
            let tasks = self.running_tasks.lock().await;
            if let Some(token) = tasks.get(sched_id) {
                token.cancel();
            }
        }

        Ok(())
    }

    pub async fn get_running_job_info(
        &self,
        sched_id: &str,
    ) -> Result<Option<(String, DateTime<Utc>)>, SchedulerError> {
        let jobs_guard = self.jobs.lock().await;
        match jobs_guard.get(sched_id) {
            Some((_, job)) if job.currently_running => {
                match (&job.current_session_id, &job.process_start_time) {
                    (Some(sid), Some(start)) => Ok(Some((sid.clone(), *start))),
                    _ => Ok(None),
                }
            }
            Some(_) => Ok(None),
            None => Err(SchedulerError::JobNotFound(sched_id.to_string())),
        }
    }
}

/// The `(provider_name, model_config)` a scheduled run binds.
///
/// Issue #56 (§9.3 C2). This used to read `Config::global()` and nothing else,
/// which is wrong twice over:
///
/// * **R5.** A `/loop` or `/schedule` created from a chat running on a private
///   model produced runs on the user's *commercial* default — the schedule
///   quietly does its work somewhere the chat that asked for it would not.
/// * **C2.** Under §6.3 a `Scheduled` session inherits its creator's
///   classification, and Gate A refuses a public provider on a private row. A
///   job created from a private chat would therefore fail on *every* tick,
///   forever, with no repair affordance — a fresh session is minted per run, so
///   there is nothing for the user to switch.
///
/// The creating session is consulted first and the global default is the
/// fallback, so a job with no creator (a workflow scheduled from the CLI or the
/// schedules route) behaves exactly as it did before. A creator row that is
/// gone, or that records no provider, also falls back rather than failing: the
/// chat may legitimately have been deleted long after the schedule was made.
async fn resolve_scheduled_provider(
    job: &ScheduledJob,
    session_manager: &SessionManager,
) -> Result<(String, crate::model::ModelConfig)> {
    if let Some(creator_id) = job.creator_session_id.as_deref() {
        match session_manager.get_session(creator_id, false).await {
            Ok(creator) => match creator.provider_name.clone() {
                Some(provider_name) => {
                    // A row with a provider but no model config is a legacy row;
                    // `Agent::update_provider` has always written both together.
                    // Fall back for the model alone, exactly as
                    // `Agent::rebind_from_row` does, rather than discarding a
                    // perfectly good provider.
                    let model_config = match creator.model_config.clone() {
                        Some(model_config) => model_config,
                        None => {
                            let model_name = Config::global().get_biorouter_model()?;
                            crate::model::ModelConfig::new(&model_name)?
                        }
                    };
                    return Ok((provider_name, model_config));
                }
                None => tracing::warn!(
                    job = %job.id,
                    session = %creator_id,
                    "the chat this schedule was created from records no provider; \
                     using the global default"
                ),
            },
            Err(e) => tracing::warn!(
                job = %job.id,
                session = %creator_id,
                "the chat this schedule was created from could not be read ({e}); \
                 using the global default"
            ),
        }
    }

    let config = Config::global();
    let provider_name = config.get_biorouter_provider()?;
    let model_name = config.get_biorouter_model()?;
    Ok((provider_name, crate::model::ModelConfig::new(&model_name)?))
}

fn scheduled_prompt(job: &ScheduledJob, workflow: &Workflow) -> String {
    let base = workflow
        .prompt
        .as_ref()
        .or(workflow.instructions.as_ref())
        .cloned()
        .unwrap_or_default();
    if job.id != crate::knowledge::soul::MEDITATION_SCHEDULE_ID {
        return base;
    }

    let after = job
        .last_run
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(7));
    format!(
        "{base}\n\nMeditation discovery window: when searching Chat Recall, pass after_date exactly as `{}`. This cursor is the last successful Meditation run; on the first run it covers seven days. Exclude this scheduled session and every result whose name starts with `Scheduled job:`. If no real user session remains, make no knowledge write.",
        after.to_rfc3339()
    )
}

#[allow(clippy::too_many_lines)]
async fn execute_job(
    job: ScheduledJob,
    jobs: Arc<Mutex<JobsMap>>,
    job_id: String,
    cancel_token: CancellationToken,
) -> Result<String> {
    if job.source.is_empty() {
        return Ok(job.id.to_string());
    }

    let workflow_path = Path::new(&job.source);
    let workflow_content = fs::read_to_string(workflow_path)?;

    let workflow: Workflow = {
        let extension = workflow_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("yaml")
            .to_lowercase();

        match extension.as_str() {
            "json" | "jsonl" => serde_json::from_str(&workflow_content)?,
            _ => serde_yaml::from_str(&workflow_content)?,
        }
    };

    let agent = Agent::new();

    // Issue #56 (§9.3 C2 / R5). The creating chat's model first, the global
    // default only as a fallback — see [`resolve_scheduled_provider`].
    let (provider_name, model_config) =
        resolve_scheduled_provider(&job, agent.config.session_manager.as_ref()).await?;

    // ⚠ DELIBERATE BEHAVIOUR CHANGE, and the one place this task makes a job
    // fail that used to run. `resolve_scheduled_provider` falls back carefully —
    // a creator row that is gone, or that records no provider, yields the global
    // default — but a creator that DID name a provider is taken at its word, and
    // if that provider can no longer be constructed (its credential was revoked,
    // its endpoint retired) this `?` ends the run.
    //
    // Falling back to the global default here instead would be the R5 defect
    // wearing a repair's clothing: the job silently moves the private chat's
    // work onto the user's commercial default, which is exactly what the chat
    // chose a private model to avoid. And under C2 it would not even work — a
    // `Scheduled` row inherits its creator's classification, so Gate A refuses
    // the public bind a few lines below regardless.
    //
    // Failing loudly is therefore the correct outcome, and it is no longer
    // invisible: `run_workflow_job` records the error on the job as
    // `last_error`, and both schedule views render it.
    let agent_provider =
        crate::providers::create_from_persisted(&provider_name, model_config).await?;

    let mut extensions = resolve_extensions_for_new_session(workflow.extensions.as_deref(), None);
    crate::workflow::runtime::ensure_required_extensions(&workflow, &mut extensions);
    for ext in extensions {
        agent.add_extension(ext.clone()).await?;
    }

    let session = agent
        .config
        .session_manager
        .create_session(
            std::env::current_dir()?,
            format!("Scheduled job: {}", job.id),
            SessionType::Scheduled,
        )
        .await?;

    agent.update_provider(agent_provider, &session.id).await?;

    let prepared_workflow_prompt = crate::workflow::runtime::prepare_prompt(
        agent.config.session_manager.as_ref(),
        &session.id,
        &workflow,
    )
    .await?;

    // Persist and apply the workflow before the first model call. Scheduled
    // runs used to load only `extensions`; their declared knowledge base,
    // required skills, instructions, and structured-output components were
    // silently ignored until after the turn had already finished.
    agent
        .config
        .session_manager
        .update(&session.id)
        .schedule_id(Some(job.id.clone()))
        .workflow(Some(workflow.clone()))
        .apply()
        .await?;

    let knowledge = biorouter_mcp::knowledge::service::KnowledgeService::new_default()?;
    crate::workflow::runtime::apply_knowledge_selection(&knowledge, &session.id, &workflow)?;
    crate::workflow::runtime::apply_prepared_to_agent(
        &agent,
        &workflow,
        true,
        prepared_workflow_prompt,
    )
    .await;

    let mut jobs_guard = jobs.lock().await;
    if let Some((_, job_def)) = jobs_guard.get_mut(job_id.as_str()) {
        job_def.current_session_id = Some(session.id.clone());
    }
    drop(jobs_guard);

    let prompt_text = scheduled_prompt(&job, &workflow);

    let user_message = Message::user().with_text(prompt_text);
    let mut conversation = Conversation::new_unvalidated(vec![user_message.clone()]);

    let session_config = SessionConfig {
        id: session.id.clone(),
        schedule_id: Some(job.id.clone()),
        max_turns: None,
        max_tool_calls: None,
        budget: None,
        retry_config: None,
        reasoning_effort: None,
    };

    let session_id = session_config.id.clone();
    let stream = crate::session_context::with_session_id(Some(session_id.clone()), async {
        agent
            .reply(user_message, session_config, Some(cancel_token))
            .await
    })
    .await?;

    use futures::StreamExt;
    let mut stream = std::pin::pin!(stream);

    let mut stream_error = None;
    while let Some(message_result) = stream.next().await {
        tokio::task::yield_now().await;

        if let Err(error) = apply_scheduled_stream_item(&mut conversation, message_result) {
            tracing::error!("Error in agent stream: {}", error);
            stream_error = Some(error);
            break;
        }
    }

    // Scheduled run finished: SessionEnd hooks (awaited; failure-open).
    {
        let hooks = agent.hooks_manager();
        // BR-28: shutdown boundary — join any observe-only hook still running
        // from the run's last turn, so it finishes here instead of being killed
        // mid-flight when the runtime tears the job's tasks down.
        hooks
            .join_fired(crate::hooks::FIRE_JOIN_BUDGET_SHUTDOWN)
            .await;
        let mut payload = crate::hooks::HookPayload::new(
            crate::hooks::HookEvent::SessionEnd,
            &session.id,
            session.working_dir.to_string_lossy(),
        );
        payload.source = Some("scheduled_run_complete".to_string());
        hooks
            .dispatch(
                crate::hooks::HookEvent::SessionEnd,
                Some("scheduled_run_complete"),
                &payload,
                &session.working_dir,
            )
            .await;
    }

    if let Some(error) = stream_error {
        return Err(error);
    }
    Ok(session.id)
}

fn apply_scheduled_stream_item(
    conversation: &mut Conversation,
    item: Result<AgentEvent>,
) -> Result<()> {
    match item? {
        AgentEvent::Message(message) => conversation.push(message),
        AgentEvent::HistoryReplaced(updated) => *conversation = updated,
        _ => {}
    }
    Ok(())
}

#[async_trait]
impl SchedulerTrait for Scheduler {
    async fn add_scheduled_job(
        &self,
        job: ScheduledJob,
        make_copy: bool,
    ) -> Result<(), SchedulerError> {
        self.add_scheduled_job(job, make_copy).await
    }

    async fn schedule_workflow(
        &self,
        workflow_path: PathBuf,
        cron_schedule: Option<String>,
    ) -> Result<(), SchedulerError> {
        self.schedule_workflow(workflow_path, cron_schedule).await
    }

    async fn list_scheduled_jobs(&self) -> Vec<ScheduledJob> {
        self.list_scheduled_jobs().await
    }

    async fn remove_scheduled_job(
        &self,
        id: &str,
        remove_workflow: bool,
    ) -> Result<(), SchedulerError> {
        self.remove_scheduled_job(id, remove_workflow).await
    }

    async fn pause_schedule(&self, id: &str) -> Result<(), SchedulerError> {
        self.pause_schedule(id).await
    }

    async fn unpause_schedule(&self, id: &str) -> Result<(), SchedulerError> {
        self.unpause_schedule(id).await
    }

    async fn run_now(&self, id: &str) -> Result<String, SchedulerError> {
        self.run_now(id).await
    }

    async fn sessions(
        &self,
        sched_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, Session)>, SchedulerError> {
        self.sessions(sched_id, limit).await
    }

    async fn update_schedule(
        &self,
        sched_id: &str,
        new_cron: String,
    ) -> Result<(), SchedulerError> {
        self.update_schedule(sched_id, new_cron).await
    }

    async fn kill_running_job(&self, sched_id: &str) -> Result<(), SchedulerError> {
        self.kill_running_job(sched_id).await
    }

    async fn get_running_job_info(
        &self,
        sched_id: &str,
    ) -> Result<Option<(String, DateTime<Utc>)>, SchedulerError> {
        self.get_running_job_info(sched_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::time::{sleep, Duration};

    fn create_test_workflow(dir: &Path, name: &str) -> PathBuf {
        let workflow_path = dir.join(format!("{}.yaml", name));
        fs::write(&workflow_path, "prompt: test\n").unwrap();
        workflow_path
    }

    #[tokio::test]
    async fn test_job_fires_and_records_its_outcome_on_schedule() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("schedule.json");
        let workflow_path = create_test_workflow(temp_dir.path(), "scheduled_job");
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        let job = ScheduledJob {
            id: "scheduled_job".to_string(),
            source: workflow_path.to_string_lossy().to_string(),
            cron: "* * * * * *".to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
            run_count: 0,
            max_runs: None,
            creator_session_id: None,
            last_error: None,
        };

        scheduler.add_scheduled_job(job, true).await.unwrap();
        let mut observed = None;
        for _ in 0..40 {
            let job = scheduler.list_scheduled_jobs().await.remove(0);
            if job.run_count > 0 && !job.currently_running {
                observed = Some(job);
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }

        let job = observed.expect("cron should fire and finish within four seconds");
        assert_eq!(job.run_count, 1, "one cron firing should be claimed");
        assert!(
            job.last_run.is_some() || job.last_error.is_some(),
            "a completed firing must expose either success or failure"
        );
    }

    #[tokio::test]
    async fn test_paused_job_does_not_run() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("schedule.json");
        let workflow_path = create_test_workflow(temp_dir.path(), "paused_job");
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        let job = ScheduledJob {
            id: "paused_job".to_string(),
            source: workflow_path.to_string_lossy().to_string(),
            cron: "* * * * * *".to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
            run_count: 0,
            max_runs: None,
            creator_session_id: None,
            last_error: None,
        };

        scheduler.add_scheduled_job(job, true).await.unwrap();
        scheduler.pause_schedule("paused_job").await.unwrap();
        sleep(Duration::from_millis(1500)).await;

        let jobs = scheduler.list_scheduled_jobs().await;
        assert!(jobs[0].last_run.is_none(), "Paused job should not run");
    }

    // BR-38: a job persisted mid-run (crash while running) must reload with its
    // running state cleared, or `claim_run_slot`'s overlap guard skips it forever.
    #[tokio::test]
    async fn test_stale_running_flag_reconciled_on_load() {
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("schedule.json");
        let workflow_path = create_test_workflow(temp_dir.path(), "stale_job");

        // Simulate a job that was mid-run at crash time. Use a cron that will not
        // fire during the test so the reset can only come from load-time reconcile.
        let stored = vec![ScheduledJob {
            id: "stale_job".to_string(),
            source: workflow_path.to_string_lossy().to_string(),
            cron: "0 0 0 1 1 *".to_string(),
            last_run: None,
            currently_running: true,
            paused: false,
            current_session_id: Some("ghost-session".to_string()),
            process_start_time: Some(Utc::now()),
            run_count: 0,
            max_runs: None,
            creator_session_id: None,
            last_error: None,
        }];
        fs::write(&storage_path, serde_json::to_string(&stored).unwrap()).unwrap();

        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        let jobs = scheduler.list_scheduled_jobs().await;
        assert_eq!(jobs.len(), 1);
        assert!(
            !jobs[0].currently_running,
            "stale currently_running flag should be reset on load"
        );
        assert!(
            jobs[0].current_session_id.is_none(),
            "stale session id should be cleared on load"
        );
        assert!(
            jobs[0].process_start_time.is_none(),
            "stale process_start_time should be cleared on load"
        );
    }

    #[test]
    fn scheduled_workflow_state_is_applied_before_the_first_model_call() {
        let source = include_str!("scheduler.rs");
        let execute = source
            .split("async fn execute_job(")
            .nth(1)
            .and_then(|rest| rest.split("impl SchedulerTrait for Scheduler").next())
            .expect("execute_job production body");
        let persisted = execute
            .find(".workflow(Some(workflow.clone()))")
            .expect("scheduled workflow is persisted");
        let prepared = execute
            .find("runtime::prepare_prompt")
            .expect("workflow skills and prompt are preflighted");
        let knowledge = execute
            .find("apply_knowledge_selection")
            .expect("scheduled knowledge selection is applied");
        let skills_and_components = execute
            .find("runtime::apply_prepared_to_agent")
            .expect("scheduled skills and components are applied");
        let reply = execute.find(".reply(").expect("scheduled model call");

        assert!(
            prepared < persisted,
            "fallible workflow inputs are preflighted first"
        );
        assert!(
            persisted < knowledge,
            "workflow must be stored before selection"
        );
        assert!(
            knowledge < skills_and_components,
            "knowledge precedes prompt assembly"
        );
        assert!(
            skills_and_components < reply,
            "all workflow state must precede reply"
        );
    }

    #[test]
    fn scheduled_stream_errors_remain_failures_after_event_collection() {
        let mut conversation = Conversation::default();
        let error = apply_scheduled_stream_item(
            &mut conversation,
            Err(anyhow::anyhow!("fixture scheduled stream failed")),
        )
        .expect_err("a failed agent stream must fail the scheduled run");
        assert!(error
            .to_string()
            .contains("fixture scheduled stream failed"));
    }

    #[test]
    fn meditation_prompt_uses_the_last_successful_run_as_its_recall_cursor() {
        let cursor = chrono::DateTime::parse_from_rfc3339("2026-08-28T10:11:12Z")
            .unwrap()
            .with_timezone(&Utc);
        let workflow = Workflow::builder()
            .title("Meditation")
            .description("test")
            .instructions("Update Soul")
            .build()
            .unwrap();
        let job = ScheduledJob {
            id: crate::knowledge::soul::MEDITATION_SCHEDULE_ID.to_string(),
            source: String::new(),
            cron: crate::knowledge::soul::MEDITATION_CRON.to_string(),
            last_run: Some(cursor),
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
            run_count: 1,
            max_runs: None,
            creator_session_id: None,
            last_error: None,
        };
        let prompt = scheduled_prompt(&job, &workflow);
        assert!(prompt.contains("2026-08-28T10:11:12+00:00"), "{prompt}");
        assert!(
            prompt.contains("last successful Meditation run"),
            "{prompt}"
        );
        assert!(prompt.contains("Scheduled job:"), "{prompt}");
    }
}

/// Issue #56, Task 24 (§9.3 C2 / R5): a scheduled job created from a private
/// chat.
///
/// ⚠ SCOPE, because the plan's sketch of the first test reads
/// `tick(&job).await` and this one does not. `execute_job` builds its agent with
/// `Agent::new()`, whose session manager is the process-wide
/// `SessionManager::instance()` — the developer's REAL `~/.config/biorouter`
/// store — and then drives a real `Agent::reply` against a real provider built
/// from the registry (`versa_azure` needs a UCSF credential). A unit test cannot
/// tick a job without writing sessions into the user's own history and calling a
/// paid endpoint. What it CAN do, and what C2 is actually about, is the two
/// steps that used to be wrong: which provider a run resolves, and whether
/// binding it to the fresh `Scheduled` row is admitted by Gate A.
#[cfg(test)]
mod privacy_c2_tests {
    use super::*;
    use crate::agents::AgentConfig as BioRouterAgentConfig;
    use crate::config::permission::PermissionManager;
    use crate::config::BioRouterMode;
    use crate::conversation::message::Message as ConversationMessage;
    use crate::model::ModelConfig;
    use crate::privacy::{ProviderTier, SessionClassification};
    use crate::providers::base::{Provider, ProviderMetadata, ProviderUsage, Usage};
    use crate::providers::errors::ProviderError;
    use crate::session::session_manager::SessionType;
    use async_trait::async_trait;
    use rmcp::model::Tool;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// A real `Provider` whose only interesting property is its tier — the same
    /// fixture shape `agents::agent::gate_a_bind_tests` uses, and for the same
    /// reason: the gates read `tier()`, `get_name()` and `get_model_config()`
    /// and nothing here ever completes a turn.
    struct TieredProvider {
        name: &'static str,
        model: &'static str,
        tier: ProviderTier,
    }

    #[async_trait]
    impl Provider for TieredProvider {
        fn metadata() -> ProviderMetadata {
            ProviderMetadata::new("tiered", "Tiered", "", "tiered-model", vec![], "", vec![])
        }

        fn get_name(&self) -> &str {
            self.name
        }

        fn tier(&self) -> ProviderTier {
            self.tier
        }

        async fn complete_with_model(
            &self,
            _model_config: &ModelConfig,
            _system: &str,
            _messages: &[ConversationMessage],
            _tools: &[Tool],
        ) -> Result<(ConversationMessage, ProviderUsage), ProviderError> {
            Ok((
                ConversationMessage::assistant().with_text("ok"),
                ProviderUsage::new(self.model.to_string(), Usage::default()),
            ))
        }

        fn get_model_config(&self) -> ModelConfig {
            ModelConfig::new_or_fail(self.model)
        }
    }

    fn versa_azure() -> Arc<dyn Provider> {
        Arc::new(TieredProvider {
            name: "versa_azure",
            model: "gpt-5.5",
            tier: ProviderTier::Private,
        })
    }

    /// A job with only the fields these tests care about set.
    fn job_named(id: &str, source: &str, cron: &str) -> ScheduledJob {
        ScheduledJob {
            id: id.to_string(),
            source: source.to_string(),
            cron: cron.to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
            run_count: 0,
            max_runs: None,
            creator_session_id: None,
            last_error: None,
        }
    }

    /// An agent over `session_manager`, so a test can bind a provider to a row
    /// the way production does — through Gate A — instead of hand-writing the
    /// columns and asserting against its own fixture.
    fn agent_over(session_manager: Arc<SessionManager>, dir: &Path) -> Agent {
        Agent::with_config(BioRouterAgentConfig::new(
            session_manager,
            Arc::new(PermissionManager::new(dir.to_path_buf())),
            None,
            BioRouterMode::Auto,
        ))
    }

    #[tokio::test]
    async fn a_scheduled_job_from_a_private_session_still_runs() {
        let temp_dir = tempdir().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let agent = agent_over(session_manager.clone(), temp_dir.path());

        // The creating chat: on a private model, and ratcheted private by the
        // turn that ran there.
        let creator = session_manager
            .create_session(PathBuf::from("."), "creator".to_string(), SessionType::User)
            .await
            .unwrap();
        agent
            .update_provider(versa_azure(), &creator.id)
            .await
            .unwrap();
        session_manager
            .update(&creator.id)
            .raise_privacy(SessionClassification::Private, "turn:versa_azure")
            .apply()
            .await
            .unwrap();

        let mut job = job_named("loop-abc", "/does/not/matter.yaml", "0 0 0 1 1 *");
        job.creator_session_id = Some(creator.id.clone());

        // 1. The run resolves the creating chat's model, not the global default.
        let (provider_name, model_config) =
            resolve_scheduled_provider(&job, session_manager.as_ref())
                .await
                .unwrap();
        assert_eq!(provider_name, "versa_azure");
        assert_eq!(model_config.model_name, "gpt-5.5");

        // 2. ...and that name really is a private provider, so the bind below is
        //    not passing for the wrong reason. This is the registry's own
        //    metadata — the same value every UI surface reads.
        let registry_tier = crate::providers::providers()
            .await
            .into_iter()
            .find(|(metadata, _)| metadata.name == provider_name)
            .map(|(metadata, _)| metadata.tier)
            .unwrap_or_default();
        assert_eq!(
            registry_tier,
            ProviderTier::Private,
            "{provider_name} must be a private provider for this test to mean anything"
        );

        // 3. Binding it to the run's fresh `Scheduled` session is admitted. This
        //    is the step that used to fail forever, once the row a scheduled run
        //    is born with carries its creator's classification.
        let run_session = session_manager
            .create_session(
                PathBuf::from("."),
                format!("Scheduled job: {}", job.id),
                SessionType::Scheduled,
            )
            .await
            .unwrap();
        session_manager
            .update(&run_session.id)
            .raise_privacy(SessionClassification::Private, "scheduled:creator")
            .apply()
            .await
            .unwrap();
        let bind = agent.update_provider(versa_azure(), &run_session.id).await;
        assert!(bind.is_ok(), "{bind:?}");

        let row = session_manager
            .get_session(&run_session.id, false)
            .await
            .unwrap();
        assert_eq!(row.provider_name.as_deref(), Some("versa_azure"));
    }

    #[tokio::test]
    async fn a_job_with_no_creating_chat_is_unchanged() {
        // The fallback is the whole of the old behaviour, so a workflow
        // scheduled from the CLI or the schedules route must not start
        // depending on a session that does not exist. Asserted through a
        // creator id that names NO row — the shape a deleted chat leaves — so
        // the assertion does not depend on what this machine's config.yaml says
        // the global provider is.
        let temp_dir = tempdir().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));

        let mut job = job_named("orphan", "/does/not/matter.yaml", "0 0 0 1 1 *");
        job.creator_session_id = Some("no-such-session".to_string());
        let from_missing = resolve_scheduled_provider(&job, session_manager.as_ref()).await;

        job.creator_session_id = None;
        let from_global = resolve_scheduled_provider(&job, session_manager.as_ref()).await;

        match (from_missing, from_global) {
            (Ok((a, _)), Ok((b, _))) => assert_eq!(a, b),
            (Err(_), Err(_)) => {}
            (a, b) => {
                panic!("a missing creator must fall back to the global default: {a:?} vs {b:?}")
            }
        }
    }

    #[tokio::test]
    async fn a_failed_run_leaves_a_job_level_error_on_the_schedule() {
        // The other half of C2: a repeating job that fails every tick used to
        // leave nothing but a log line, and a fresh session per run means there
        // is no chat to carry the error either.
        let temp_dir = tempdir().unwrap();
        let storage_path = temp_dir.path().join("schedule.json");
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let scheduler = Scheduler::new(storage_path, session_manager).await.unwrap();

        // A cron that will not fire during the test, so the only run is the
        // explicit one below, and a source that does not exist so the run fails
        // before it can reach a provider or the real session store.
        let job = job_named(
            "broken",
            &temp_dir.path().join("gone.yaml").to_string_lossy(),
            "0 0 0 1 1 *",
        );
        scheduler.add_scheduled_job(job, false).await.unwrap();

        assert!(scheduler.run_now("broken").await.is_err());

        let jobs = scheduler.list_scheduled_jobs().await;
        assert!(
            jobs[0].last_error.is_some(),
            "a failed run must leave a job-level error the schedules UI can show"
        );
    }
}
