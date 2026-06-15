//! Background shell jobs for the developer extension.
//!
//! Supervises long-running commands started by `shell` with `background=true`
//! so the agent can launch something (a dev server, build, test suite, training
//! run), keep watching it, and continue once it finishes, without killing the
//! job or guessing whether it is done. The shape mirrors Claude Code / Codex:
//! start in a background process group, get a durable `job_id`, read *only new
//! output since the last check*, and decide done-vs-running from the **OS exit
//! status** (never from log text). `wait` adds a bounded in-turn watch that
//! returns the instant the job exits, or at a timeout with `status: running`
//! so the agent can keep watching while the job stays alive.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{watch, Mutex};

use super::shell::{configure_shell_command, ShellConfig};

/// Cap a single job's captured output so a runaway process cannot exhaust
/// memory. Matches the foreground shell's 400 KB ceiling.
const MAX_OUTPUT_BYTES: usize = 400_000;
/// Default and ceiling for `wait`'s bounded watch.
pub const DEFAULT_WAIT_SECS: u64 = 120;
pub const MAX_WAIT_SECS: u64 = 600;

/// Terminal-or-running state of a job. `Exited` carries the real OS exit code,
/// the only source of truth for done-vs-running.
#[derive(Debug, Clone, PartialEq, Eq)]
enum JobStatus {
    Running,
    Exited(i32),
    /// Process ended without a normal exit code (e.g. killed by a signal) and
    /// was not a `shell_kill`.
    Ended(String),
    Killed,
}

impl JobStatus {
    fn is_terminal(&self) -> bool {
        !matches!(self, JobStatus::Running)
    }

    fn describe(&self) -> String {
        match self {
            JobStatus::Running => "running".to_string(),
            JobStatus::Exited(0) => "exited(0) — success".to_string(),
            JobStatus::Exited(code) => format!("exited({code}) — non-zero"),
            JobStatus::Ended(why) => format!("ended ({why})"),
            JobStatus::Killed => "killed".to_string(),
        }
    }
}

/// Captured output plus a per-job read cursor so reads return only what is new
/// since the previous read.
#[derive(Default)]
struct Output {
    buf: String,
    cursor: usize,
    truncated: bool,
}

struct Job {
    label: String,
    // Read by `list()` (exercised in tests); the listing isn't wired into the
    // tool surface yet, so the lib build sees it as unused.
    #[allow(dead_code)]
    command: String,
    started: Instant,
    /// Process-group leader pid (== child pid because we spawn it in its own
    /// group), used to signal the whole group on kill.
    pid: Option<u32>,
    status: Arc<Mutex<JobStatus>>,
    output: Arc<Mutex<Output>>,
    /// Set before signalling so the supervisor records `Killed` rather than
    /// `Ended` when `wait()` returns.
    killed: Arc<AtomicBool>,
    /// Flips to `true` once the job reaches a terminal state; `wait` parks on
    /// this race-free instead of polling.
    done_rx: watch::Receiver<bool>,
}

/// Registry of background shell jobs, shared (via `Arc`) by the developer
/// server's `shell`, `shell_output`, `shell_wait` and `shell_kill` tools.
#[derive(Default)]
pub struct BackgroundJobs {
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    next_id: AtomicU64,
}

impl BackgroundJobs {
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Spawn `command` as a background job in its own process group, wire up
    /// output capture and a supervisor that records the terminal status, and
    /// register the job. Returns the new job id.
    pub async fn spawn(&self, command: &str, label: Option<String>) -> Result<String, String> {
        let id = format!("job-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let label = label.unwrap_or_else(|| command.chars().take(40).collect());

        // Reuse the foreground shell's hardened command builder (same shell,
        // sanitized git/editor env, own process group) but override
        // kill_on_drop: a background job must survive the tool call returning.
        let shell_config = ShellConfig::default();
        let mut cmd = configure_shell_command(&shell_config, command, None);
        cmd.kill_on_drop(false);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to start command: {e}"))?;
        let pid = child.id();

        let status = Arc::new(Mutex::new(JobStatus::Running));
        let output = Arc::new(Mutex::new(Output::default()));
        let killed = Arc::new(AtomicBool::new(false));
        let (done_tx, done_rx) = watch::channel(false);

        if let Some(out) = child.stdout.take() {
            spawn_reader(out, output.clone());
        }
        if let Some(err) = child.stderr.take() {
            spawn_reader(err, output.clone());
        }

        // Supervisor: own the child so it is not dropped (and thus not killed)
        // when the tool returns; record the terminal status from the OS.
        let status_for_sup = status.clone();
        let killed_for_sup = killed.clone();
        tokio::spawn(async move {
            let wait_result = child.wait().await;
            let terminal = if killed_for_sup.load(Ordering::SeqCst) {
                JobStatus::Killed
            } else {
                match wait_result {
                    Ok(st) => match st.code() {
                        Some(code) => JobStatus::Exited(code),
                        None => JobStatus::Ended("terminated by signal".to_string()),
                    },
                    Err(e) => JobStatus::Ended(format!("wait failed: {e}")),
                }
            };
            *status_for_sup.lock().await = terminal;
            let _ = done_tx.send(true);
        });

        let job = Arc::new(Job {
            label,
            command: command.to_string(),
            started: Instant::now(),
            pid,
            status,
            output,
            killed,
            done_rx,
        });
        self.jobs.lock().await.insert(id.clone(), job);
        Ok(id)
    }

    async fn job(&self, id: &str) -> Result<Arc<Job>, String> {
        self.jobs
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no such background job: {id}"))
    }

    /// Return the new output since the last read (advancing the cursor) plus a
    /// truncation note if the buffer cap was hit.
    async fn drain_new_output(job: &Job) -> String {
        let mut out = job.output.lock().await;
        let new: String = out.buf.get(out.cursor..).unwrap_or("").to_string();
        out.cursor = out.buf.len();
        let mut s = new;
        if out.truncated {
            s.push_str("\n[output truncated at 400 KB]");
        }
        s
    }

    /// Status + only-new-output snapshot for a job.
    pub async fn snapshot(&self, id: &str) -> Result<String, String> {
        let job = self.job(id).await?;
        let status = job.status.lock().await.describe();
        let new_output = Self::drain_new_output(&job).await;
        let elapsed = job.started.elapsed().as_secs();
        let body = if new_output.trim().is_empty() {
            "(no new output)".to_string()
        } else {
            new_output
        };
        Ok(format!(
            "job {id} [{}] — status: {status} ({elapsed}s elapsed)\nnew output since last check:\n{body}",
            job.label
        ))
    }

    /// Watch a job for up to `dur_secs`, returning early when it exits.
    pub async fn wait(&self, id: &str, dur_secs: u64) -> Result<String, String> {
        let job = self.job(id).await?;
        if !job.status.lock().await.is_terminal() {
            let mut rx = job.done_rx.clone();
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(dur_secs),
                rx.wait_for(|done| *done),
            )
            .await;
        }
        let terminal = job.status.lock().await.is_terminal();
        let snap = self.snapshot(id).await?;
        if terminal {
            Ok(format!("{snap}\n\nThe job has finished."))
        } else {
            Ok(format!(
                "{snap}\n\nStill running after {dur_secs}s. The job was NOT killed — call shell_wait again to keep watching, or do other work and check back."
            ))
        }
    }

    /// Kill a job's whole process group (SIGTERM then SIGKILL).
    pub async fn kill(&self, id: &str) -> Result<String, String> {
        let job = self.job(id).await?;
        if job.status.lock().await.is_terminal() {
            return Ok(format!("job {id} has already finished; nothing to kill"));
        }
        job.killed.store(true, Ordering::SeqCst);
        kill_process_group(job.pid);
        let mut rx = job.done_rx.clone();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), rx.wait_for(|d| *d)).await;
        Ok(format!("sent kill signal to job {id}"))
    }

    /// One-line summary of every job. Covered by tests; not yet surfaced as a
    /// tool, so the non-test build sees it as unused.
    #[allow(dead_code)]
    pub async fn list(&self) -> String {
        let jobs = self.jobs.lock().await;
        if jobs.is_empty() {
            return "No background jobs.".to_string();
        }
        let mut ids: Vec<_> = jobs.keys().cloned().collect();
        ids.sort();
        let mut lines = Vec::new();
        for id in ids {
            let job = &jobs[&id];
            let status = job.status.lock().await.describe();
            lines.push(format!(
                "- {id} [{}]: {status} ({}s) — `{}`",
                job.label,
                job.started.elapsed().as_secs(),
                job.command
            ));
        }
        lines.join("\n")
    }
}

/// Stream a child pipe into the shared buffer, line by line, honoring the cap.
fn spawn_reader<R>(reader: R, output: Arc<Mutex<Output>>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut out = output.lock().await;
            if out.buf.len() >= MAX_OUTPUT_BYTES {
                out.truncated = true;
                continue;
            }
            out.buf.push_str(&line);
            out.buf.push('\n');
        }
    });
}

/// Kill the whole process group led by `pid` (SIGTERM, then SIGKILL shortly
/// after). Mirrors the foreground shell's kill idiom.
fn kill_process_group(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    {
        unsafe { libc::kill(-(pid as i32), libc::SIGTERM) };
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        });
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn wait_terminal(jobs: &BackgroundJobs, id: &str, max_ms: u64) -> JobStatus {
        let job = jobs.job(id).await.unwrap();
        let deadline = Instant::now() + std::time::Duration::from_millis(max_ms);
        loop {
            let st = job.status.lock().await.clone();
            if st.is_terminal() || Instant::now() > deadline {
                return st;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn start_lists_and_completes_with_output() {
        let jobs = BackgroundJobs::new();
        let id = jobs.spawn("echo hello-bg", None).await.unwrap();
        assert!(jobs.list().await.contains(&id));
        assert_eq!(wait_terminal(&jobs, &id, 5000).await, JobStatus::Exited(0));
        let snap = jobs.snapshot(&id).await.unwrap();
        assert!(snap.contains("hello-bg"), "snapshot: {snap}");
    }

    #[tokio::test]
    async fn nonzero_exit_code_is_surfaced() {
        let jobs = BackgroundJobs::new();
        let id = jobs.spawn("exit 3", None).await.unwrap();
        assert_eq!(wait_terminal(&jobs, &id, 5000).await, JobStatus::Exited(3));
    }

    #[tokio::test]
    async fn output_is_incremental() {
        let jobs = BackgroundJobs::new();
        let id = jobs
            .spawn("echo first; sleep 0.3; echo second", None)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let first = BackgroundJobs::drain_new_output(&jobs.job(&id).await.unwrap()).await;
        assert!(first.contains("first"), "first read: {first}");
        assert!(!first.contains("second"), "second leaked early: {first}");
        wait_terminal(&jobs, &id, 5000).await;
        let second = BackgroundJobs::drain_new_output(&jobs.job(&id).await.unwrap()).await;
        assert!(second.contains("second"), "second read: {second}");
        assert!(!second.contains("first"), "first duplicated: {second}");
    }

    #[tokio::test]
    async fn wait_returns_early_on_completion() {
        let jobs = BackgroundJobs::new();
        let id = jobs.spawn("echo done", None).await.unwrap();
        let started = Instant::now();
        let out = jobs.wait(&id, 30).await.unwrap();
        assert!(out.contains("finished"), "wait result: {out}");
        assert!(started.elapsed().as_secs() < 5);
    }

    #[tokio::test]
    async fn wait_times_out_without_killing_then_kill_works() {
        let jobs = BackgroundJobs::new();
        let id = jobs.spawn("sleep 30", None).await.unwrap();
        let out = jobs.wait(&id, 1).await.unwrap();
        assert!(out.contains("Still running"), "wait result: {out}");
        assert_eq!(
            *jobs.job(&id).await.unwrap().status.lock().await,
            JobStatus::Running,
            "bounded wait must NOT kill the job"
        );
        jobs.kill(&id).await.unwrap();
        assert_eq!(wait_terminal(&jobs, &id, 5000).await, JobStatus::Killed);
    }

    #[tokio::test]
    async fn unknown_job_is_clean_error() {
        let jobs = BackgroundJobs::new();
        assert!(jobs.snapshot("job-nope").await.is_err());
        assert!(jobs.wait("job-nope", 1).await.is_err());
        assert!(jobs.kill("job-nope").await.is_err());
    }
}
