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
use std::path::{Path, PathBuf};
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
/// server's `shell`, `shell_output`, `shell_wait`, `shell_kill` and
/// `shell_list` tools.
#[derive(Default)]
pub struct BackgroundJobs {
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    next_id: AtomicU64,
}

impl BackgroundJobs {
    pub fn new() -> Self {
        // Reap background jobs orphaned by a previously-crashed Biorouter
        // process before we start tracking new ones (see "orphan reaping").
        reap_orphans();
        Self {
            jobs: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Spawn `command` as a background job in its own process group, wire up
    /// output capture and a supervisor that records the terminal status, and
    /// register the job. Returns the new job id.
    pub async fn spawn(
        &self,
        command: &str,
        label: Option<String>,
        working_dir: Option<PathBuf>,
    ) -> Result<String, String> {
        let id = format!("job-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let label = label.unwrap_or_else(|| command.chars().take(40).collect());

        // Reuse the foreground shell's hardened command builder (same shell,
        // sanitized git/editor env, own process group) but override
        // kill_on_drop: a background job must survive the tool call returning.
        let shell_config = ShellConfig::default();
        let mut cmd = configure_shell_command(&shell_config, command, working_dir.as_deref());
        cmd.kill_on_drop(false);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to start command: {e}"))?;
        let pid = child.id();

        // Record this job's process-group-leader pid to the run dir so a future
        // Biorouter process can reap it if we crash before it finishes; the
        // supervisor removes the record once the job exits (see "orphan
        // reaping"). kill_on_drop(false) means nothing else would clean it up.
        if let Some(pid) = pid {
            record_job_pidfile(pid);
        }

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
        let pid_for_sup = pid;
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
            // The job reached a terminal state under our supervision, so it is
            // no longer an orphan candidate — drop its run-dir record.
            if let Some(pid) = pid_for_sup {
                remove_job_pidfile(pid);
            }
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

    /// One-line summary of every background job — id, label, status, runtime,
    /// whether unread output is waiting, and the command — so the agent can
    /// rediscover jobs whose `job_id` it has lost. Backs the `shell_list` tool.
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
            // Peek at the read cursor without draining it, so listing a job
            // doesn't consume the output a later `shell_output` should return.
            let new_output = {
                let out = job.output.lock().await;
                out.cursor < out.buf.len()
            };
            let pending = if new_output {
                " · new output available"
            } else {
                ""
            };
            lines.push(format!(
                "- {id} [{}]: {status} ({}s elapsed){pending} — `{}`",
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

// ── orphan reaping ──────────────────────────────────────────────────────────
// Background jobs are spawned with `kill_on_drop(false)` so they outlive the
// tool call that started them — but that also means a daemon crash orphans the
// whole process group with no supervisor left to reap it. Mirror the llama.cpp
// sidecar's pid-file scheme (`llamacpp_sidecar.rs`): record each job's
// process-group-leader pid under a run dir keyed by *this* process's pid, and
// on the next `BackgroundJobs::new()` in any Biorouter process, kill the process
// groups recorded by processes that no longer exist. Records of still-living
// parents (e.g. a CLI and the desktop app running side by side) are left alone,
// so a live daemon's jobs are never reaped out from under it.
//
// Unlike the sidecar (one child), a Biorouter process can own many background
// jobs, so there is one file per job: `<parent-pid>-<child-pid>.pid`.

/// Directory holding the per-job pid files. Honors `BIOROUTER_DEVELOPER_RUN_DIR`
/// (tests point it at a scratch dir); otherwise `<data>/developer/run`.
fn run_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BIOROUTER_DEVELOPER_RUN_DIR") {
        return PathBuf::from(dir);
    }
    use etcetera::AppStrategy;
    etcetera::choose_app_strategy(crate::APP_STRATEGY.clone())
        .map(|s| s.data_dir().join("developer/run"))
        .unwrap_or_else(|_| std::env::temp_dir().join("biorouter/developer/run"))
}

fn pidfile_name(parent: u32, child: u32) -> String {
    format!("{parent}-{child}.pid")
}

fn record_job_pidfile(child: u32) {
    record_pidfile_in(&run_dir(), std::process::id(), child);
}

fn remove_job_pidfile(child: u32) {
    remove_pidfile_in(&run_dir(), std::process::id(), child);
}

fn record_pidfile_in(dir: &Path, parent: u32, child: u32) {
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(dir.join(pidfile_name(parent, child)), child.to_string());
    }
}

fn remove_pidfile_in(dir: &Path, parent: u32, child: u32) {
    let _ = std::fs::remove_file(dir.join(pidfile_name(parent, child)));
}

/// Kill background-job process groups recorded by Biorouter processes that are
/// no longer alive, then delete their pid files. Best-effort and idempotent.
fn reap_orphans() {
    reap_orphans_in(&run_dir());
}

/// Returns the child pids whose process groups were killed (for tests).
fn reap_orphans_in(dir: &Path) -> Vec<u32> {
    let mut reaped = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return reaped;
    };
    let me = std::process::id();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pid") {
            continue;
        }
        // Filename is `<parent>-<child>.pid`; the child pid is also stored in
        // the file body (source of truth, matching the sidecar), with the
        // filename as a fallback.
        let stem = path.file_stem().and_then(|s| s.to_str());
        let parent: Option<u32> = stem
            .and_then(|s| s.split('-').next())
            .and_then(|s| s.parse().ok());
        let child: Option<u32> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .or_else(|| {
                stem.and_then(|s| s.split('-').nth(1))
                    .and_then(|s| s.parse().ok())
            });
        let (Some(parent), Some(child)) = (parent, child) else {
            let _ = std::fs::remove_file(&path);
            continue;
        };
        // Never touch a record owned by this process or any still-living one.
        if parent == me || pid_alive(parent) {
            continue;
        }
        if pid_alive(child) && is_group_leader(child) {
            tracing::info!(
                "Reaping orphaned background job process group {child} left by exited Biorouter process {parent}"
            );
            kill_orphan_group(child);
            reaped.push(child);
        }
        let _ = std::fs::remove_file(&path);
    }
    reaped
}

/// Whether `pid` currently names a live process.
fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0): 0 => exists; EPERM => exists but not ours; ESRCH => gone.
        if unsafe { libc::kill(pid as i32, 0) } == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
            .unwrap_or(false)
    }
}

/// PID-reuse guard: only reap a recorded child if it is still a process-group
/// leader (pgid == pid), which every background job is (`process_group(0)`).
/// A random reused pid is very unlikely to also lead its own group, so this
/// stands in for the sidecar's process-name check (background commands have no
/// fixed name to match against).
#[cfg(unix)]
fn is_group_leader(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .map(|pgid| pgid == pid)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_group_leader(_pid: u32) -> bool {
    // No cheap pgid equivalent on Windows; rely on liveness + `taskkill /T`.
    true
}

/// Force-kill the whole process group led by `pid` (these are known orphans, so
/// go straight to SIGKILL rather than the graceful SIGTERM-then-SIGKILL a live
/// `shell_kill` uses).
fn kill_orphan_group(pid: u32) {
    #[cfg(unix)]
    {
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Point the pid-file run dir at a per-binary scratch dir (set once) so the
    /// test suite never reaps or records against the real `<data>/developer/run`
    /// — that could kill a developer's actual background jobs during `cargo
    /// test`. Every test that builds a `BackgroundJobs` must go through this.
    fn ensure_test_run_dir() -> &'static Path {
        static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        DIR.get_or_init(|| {
            let d = std::env::temp_dir().join(format!("br-dev-run-tests-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&d);
            std::env::set_var("BIOROUTER_DEVELOPER_RUN_DIR", &d);
            d
        })
    }

    fn new_jobs() -> BackgroundJobs {
        ensure_test_run_dir();
        BackgroundJobs::new()
    }

    /// A fresh, isolated scratch dir for hermetic reaping tests that pass the
    /// dir explicitly and must not share state with other tests.
    fn scratch_run_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let d =
            std::env::temp_dir().join(format!("br-dev-run-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

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
        let jobs = new_jobs();
        let id = jobs.spawn("echo hello-bg", None, None).await.unwrap();
        assert!(jobs.list().await.contains(&id));
        assert_eq!(wait_terminal(&jobs, &id, 5000).await, JobStatus::Exited(0));
        let snap = jobs.snapshot(&id).await.unwrap();
        assert!(snap.contains("hello-bg"), "snapshot: {snap}");
    }

    #[tokio::test]
    async fn list_reports_command_status_and_unread_output() {
        let jobs = new_jobs();
        let id = jobs.spawn("echo listme", None, None).await.unwrap();
        assert_eq!(wait_terminal(&jobs, &id, 5000).await, JobStatus::Exited(0));

        let listing = jobs.list().await;
        assert!(listing.contains(&id), "listing: {listing}");
        assert!(
            listing.contains("echo listme"),
            "command missing: {listing}"
        );
        assert!(
            listing.contains("exited(0)"),
            "terminal status missing: {listing}"
        );
        assert!(listing.contains("elapsed"), "runtime missing: {listing}");
        assert!(
            listing.contains("new output available"),
            "unread output not flagged: {listing}"
        );

        // Listing must not drain the cursor; a real read still sees the output,
        // after which the listing stops flagging it.
        let drained = BackgroundJobs::drain_new_output(&jobs.job(&id).await.unwrap()).await;
        assert!(drained.contains("listme"), "drained: {drained}");
        let listing2 = jobs.list().await;
        assert!(
            !listing2.contains("new output available"),
            "flag should clear after read: {listing2}"
        );
    }

    #[tokio::test]
    async fn list_is_empty_message_with_no_jobs() {
        let jobs = new_jobs();
        assert_eq!(jobs.list().await, "No background jobs.");
    }

    #[tokio::test]
    async fn nonzero_exit_code_is_surfaced() {
        let jobs = new_jobs();
        let id = jobs.spawn("exit 3", None, None).await.unwrap();
        assert_eq!(wait_terminal(&jobs, &id, 5000).await, JobStatus::Exited(3));
    }

    #[tokio::test]
    async fn output_is_incremental() {
        let jobs = new_jobs();
        let id = jobs
            .spawn("echo first; sleep 0.3; echo second", None, None)
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
        let jobs = new_jobs();
        let id = jobs.spawn("echo done", None, None).await.unwrap();
        let started = Instant::now();
        let out = jobs.wait(&id, 30).await.unwrap();
        assert!(out.contains("finished"), "wait result: {out}");
        assert!(started.elapsed().as_secs() < 5);
    }

    #[tokio::test]
    async fn wait_times_out_without_killing_then_kill_works() {
        let jobs = new_jobs();
        let id = jobs.spawn("sleep 30", None, None).await.unwrap();
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
        let jobs = new_jobs();
        assert!(jobs.snapshot("job-nope").await.is_err());
        assert!(jobs.wait("job-nope", 1).await.is_err());
        assert!(jobs.kill("job-nope").await.is_err());
    }

    // ── orphan reaping ──────────────────────────────────────────────────────

    #[cfg(unix)]
    fn spawn_group_leader_sleep() -> std::process::Child {
        use std::os::unix::process::CommandExt;
        // Own process group so pgid == pid, matching how real jobs are spawned.
        // Return the Child so the test can `wait()` it (clearing the zombie a
        // SIGKILL leaves behind, since this process — unlike a dead daemon,
        // whose orphans reparent to init — stays alive to parent it).
        std::process::Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn sleep")
    }

    #[cfg(unix)]
    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let pid = child.id();
        let _ = child.wait(); // reap it, so kill(pid, 0) => ESRCH
        pid
    }

    #[cfg(unix)]
    #[test]
    fn reap_kills_orphan_of_dead_parent_and_spares_live_parent() {
        let dir = scratch_run_dir("reap");

        // Orphan whose recorded parent is gone → must be reaped.
        let mut orphan_child = spawn_group_leader_sleep();
        let orphan = orphan_child.id();
        let dead_parent = dead_pid();
        record_pidfile_in(&dir, dead_parent, orphan);

        // Job whose recorded parent (this process) is alive → must be spared.
        let mut live = spawn_group_leader_sleep();
        let live_child = live.id();
        record_pidfile_in(&dir, std::process::id(), live_child);

        let reaped = reap_orphans_in(&dir);

        assert!(
            reaped.contains(&orphan),
            "orphan of a dead parent should be reaped, got {reaped:?}"
        );
        // The reaper SIGKILLed the group; reap the zombie so the pid is freed.
        let _ = orphan_child.wait();
        assert!(!pid_alive(orphan), "orphan should be dead after reaping");
        assert!(
            !dir.join(pidfile_name(dead_parent, orphan)).exists(),
            "orphan pid file should be removed after reaping"
        );

        assert!(
            !reaped.contains(&live_child) && pid_alive(live_child),
            "job of a still-living parent must not be reaped"
        );
        assert!(
            matches!(live.try_wait(), Ok(None)),
            "live-parent job must still be running"
        );
        assert!(
            dir.join(pidfile_name(std::process::id(), live_child))
                .exists(),
            "live-parent pid file must be kept"
        );

        let _ = live.kill();
        let _ = live.wait();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn reap_removes_malformed_pidfiles() {
        let dir = scratch_run_dir("malformed");
        std::fs::write(dir.join("not-a-pid.pid"), "garbage").unwrap();
        std::fs::write(dir.join("keep.txt"), "123").unwrap(); // non-.pid, ignored
        reap_orphans_in(&dir);
        assert!(
            !dir.join("not-a-pid.pid").exists(),
            "unparseable pid file should be swept"
        );
        assert!(
            dir.join("keep.txt").exists(),
            "non-.pid files must be left alone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_records_pidfile_and_terminal_removes_it() {
        let dir = ensure_test_run_dir().to_path_buf();
        let jobs = new_jobs();
        let id = jobs.spawn("sleep 30", None, None).await.unwrap();
        let pid = jobs.job(&id).await.unwrap().pid.unwrap();

        let pidfile = dir.join(pidfile_name(std::process::id(), pid));
        assert!(pidfile.exists(), "spawn should record a pid file");

        jobs.kill(&id).await.unwrap();
        assert_eq!(wait_terminal(&jobs, &id, 5000).await, JobStatus::Killed);

        // The supervisor removes the record once the job reaches a terminal state.
        let mut gone = false;
        for _ in 0..120 {
            if !pidfile.exists() {
                gone = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert!(gone, "terminal job should remove its pid file");
    }
}
