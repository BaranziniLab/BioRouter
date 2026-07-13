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
use crate::providers::create;
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
}

/// Decide, under one lock, whether a fired cron job should actually run, and
/// stamp its start state if so. Returns `true` when the caller should execute.
///
/// Skips when the job is gone, paused, still running from a previous firing
/// (overlap guard — a slow run never stacks), or has hit its `max_runs` cap
/// (which also auto-pauses it). On a real run it records `last_run`, marks the
/// job running, and bumps `run_count`.
async fn claim_run_slot(jobs: &Arc<Mutex<JobsMap>>, job_id: &str, now: DateTime<Utc>) -> bool {
    let mut jobs_guard = jobs.lock().await;
    match jobs_guard.get_mut(job_id) {
        None => false,
        Some((_, job)) if job.paused => false,
        Some((_, job)) if job.currently_running => {
            tracing::info!("Skipping job '{}': previous run still in progress", job_id);
            false
        }
        Some((_, job)) if job.max_runs.is_some_and(|max| job.run_count >= max) => {
            tracing::info!(
                "Job '{}' reached its run cap ({:?}); auto-pausing",
                job_id,
                job.max_runs
            );
            job.paused = true;
            false
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
            false
        }
        Some(_) if pause_on_active() && interactive_active() => {
            tracing::info!("Deferring scheduled job '{}': user session active", job_id);
            false
        }
        Some((_, job)) => {
            job.last_run = Some(now);
            job.currently_running = true;
            job.process_start_time = Some(now);
            job.run_count = job.run_count.saturating_add(1);
            true
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
            let job_to_execute = job_for_task.clone();
            let running_tasks = running_tasks_arc.clone();

            Box::pin(async move {
                let should_execute =
                    claim_run_slot(&current_jobs_arc, &task_job_id, Utc::now()).await;

                if !should_execute {
                    // Persist the auto-pause (if any) so it survives restart.
                    if let Err(e) = persist_jobs(&local_storage_path, &current_jobs_arc).await {
                        tracing::error!("Failed to persist job status: {}", e);
                    }
                    return;
                }

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
                job.last_run = Some(Utc::now());
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

    let config = Config::global();
    let provider_name = config.get_biorouter_provider()?;
    let model_name = config.get_biorouter_model()?;
    let model_config = crate::model::ModelConfig::new(&model_name)?;

    let agent_provider = create(&provider_name, model_config).await?;

    let extensions = resolve_extensions_for_new_session(workflow.extensions.as_deref(), None);
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

    let mut jobs_guard = jobs.lock().await;
    if let Some((_, job_def)) = jobs_guard.get_mut(job_id.as_str()) {
        job_def.current_session_id = Some(session.id.clone());
    }
    drop(jobs_guard);

    let prompt_text = workflow
        .prompt
        .as_ref()
        .or(workflow.instructions.as_ref())
        .unwrap();

    let user_message = Message::user().with_text(prompt_text);
    let mut conversation = Conversation::new_unvalidated(vec![user_message.clone()]);

    let session_config = SessionConfig {
        id: session.id.clone(),
        schedule_id: Some(job.id.clone()),
        max_turns: None,
        max_tool_calls: None,
        retry_config: None,
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

    while let Some(message_result) = stream.next().await {
        tokio::task::yield_now().await;

        match message_result {
            Ok(AgentEvent::Message(msg)) => {
                conversation.push(msg);
            }
            Ok(AgentEvent::HistoryReplaced(updated)) => {
                conversation = updated;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Error in agent stream: {}", e);
                break;
            }
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

    agent
        .config
        .session_manager
        .update(&session.id)
        .schedule_id(Some(job.id.clone()))
        .workflow(Some(workflow))
        .apply()
        .await?;

    Ok(session.id)
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
    async fn test_job_runs_on_schedule() {
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
        };

        scheduler.add_scheduled_job(job, true).await.unwrap();
        sleep(Duration::from_millis(1500)).await;

        let jobs = scheduler.list_scheduled_jobs().await;
        assert!(jobs[0].last_run.is_some(), "Job should have run");
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
}
