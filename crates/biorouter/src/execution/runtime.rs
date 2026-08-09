//! Tokio runtime construction for every process that can host an agent turn.
//!
//! # Why this module exists
//!
//! A `workspace__subagent` spawn polls the **child's** `Agent::reply` inline, on
//! the parent's stack — `execute_subagent` → `run_complete_subagent_task` →
//! `get_agent_messages` → `Agent::reply`. Nothing recurses; two independent
//! agent state machines simply share one worker thread's stack. Measured on a
//! debug `biorouterd` (crash report `biorouterd-2026-08-09-132353.ips`): 146
//! frames, the parent's chain alone accounting for 113 of them, ending in
//! `std::sys::pal::unix::stack_overflow::imp::signal_handler` → `abort` on a
//! `tokio-runtime-worker` thread. Every delegation took the daemon down about
//! 2.3 s in, and Electron does not respawn it, so the app read "Backend
//! disconnected" and never recovered.
//!
//! Tokio leaves [`tokio::runtime::Builder::thread_stack_size`] unset by default,
//! so its workers inherit `std::thread`'s default — 2 MiB, or whatever
//! `RUST_MIN_STACK` says. That is the whole of the bug: the same binary and the
//! same delegation succeed under `RUST_MIN_STACK=67108864`, and a release build
//! (thinner frames) succeeds unmodified.
//!
//! # Why the size is set, not the nesting removed
//!
//! Moving the child onto its own `tokio::spawn` would halve the depth, and it
//! was rejected for three reasons:
//!
//! 1. It does not actually clear the hazard. The parent's chain **on its own**
//!    reached 113 of the 146 frames — roughly three quarters of a 2 MiB stack
//!    before a subagent existed. Anything that deepens a single agent's tool
//!    path (a `render_dashboard` panel calling another figure tool, a knowledge
//!    macro's sub-agent loop) walks back into the same abort with no subagent
//!    involved.
//! 2. `tokio::spawn` does not inherit the caller's task-local scope, which
//!    `subagent_tool` already depends on: `max_pending_subagents()` is read in
//!    the requesting task *precisely because* the detached path cannot see a
//!    per-task override. A blocking spawn moved behind `tokio::spawn` would
//!    silently start reading process defaults.
//! 3. The blocking spawn is load-bearing. The parent's turn is supposed to block
//!    on the child, and cancellation is threaded through it (`run_token`, the
//!    turn lease, `ActiveWorkGuard`). Reproducing that around a `JoinHandle`
//!    means re-deriving abort-on-drop, panic propagation and lease ordering — a
//!    behaviour change, in service of a smaller number.
//!
//! Setting the stack size is one line per process, changes no semantics, and is
//! applied **identically in debug and release** so a test can never pass on a
//! configuration users do not run.

/// Stack size, in bytes, for the worker and blocking threads of any process
/// that can host an agent turn.
///
/// 16 MiB — 8× tokio's inherited default, and the same figure
/// `biorouter-acp`'s test runtimes already use. The measured overflow needed
/// slightly more than 2 MiB for 146 debug frames (~14 KiB/frame), so this
/// carries roughly 1100 frames: about 7× the deepest chain observed, which is
/// the headroom a *debug* build wants, since that is where frames are fattest.
///
/// The cost is address space, not memory: thread stacks are reserved lazily and
/// only the pages actually touched are ever committed.
pub const AGENT_WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

/// Escape hatch for a machine that finds a chain deeper than the default
/// covers. Read once, when the runtime is built.
///
/// ⚠ It can only ever RAISE the value. A lowerable knob is a way to reintroduce
/// the abort this module exists to prevent, and it would buy nothing: the
/// default's cost is reserved address space, which is free until touched.
pub const AGENT_WORKER_STACK_SIZE_ENV: &str = "BIOROUTER_WORKER_STACK_SIZE";

/// The stack size [`build_agent_runtime`] will ask for: the default, or a
/// larger value from [`AGENT_WORKER_STACK_SIZE_ENV`].
pub fn worker_stack_size() -> usize {
    std::env::var(AGENT_WORKER_STACK_SIZE_ENV)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(0)
        .max(AGENT_WORKER_STACK_SIZE)
}

/// The multi-threaded runtime a Biorouter host process should run on.
///
/// Replaces `#[tokio::main]`, which offers no way to size worker stacks. Worker
/// threads are themselves spawned through tokio's blocking pool, so the one
/// `thread_stack_size` call covers both pools — the crashing thread in the
/// report above is a `tokio-runtime-worker` launched via
/// `blocking::pool::Spawner::spawn_thread`.
pub fn build_agent_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(worker_stack_size())
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read the *actual* stack size the OS gave the calling thread.
    ///
    /// This is what makes the test below a test rather than a restatement of
    /// the constant: it asks the operating system what the worker thread got,
    /// so a `build_agent_runtime` that forgot to pass the size — or passed it to
    /// a builder whose threads are spawned elsewhere — fails.
    #[cfg(unix)]
    fn current_thread_stack_size() -> Option<usize> {
        #[cfg(target_vendor = "apple")]
        {
            Some(unsafe { libc::pthread_get_stacksize_np(libc::pthread_self()) })
        }
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            unsafe {
                let mut attr: libc::pthread_attr_t = std::mem::zeroed();
                if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
                    return None;
                }
                let mut size: libc::size_t = 0;
                let rc = libc::pthread_attr_getstacksize(&attr, &mut size);
                libc::pthread_attr_destroy(&mut attr);
                if rc == 0 {
                    Some(size)
                } else {
                    None
                }
            }
        }
        #[cfg(not(any(target_vendor = "apple", all(target_os = "linux", target_env = "gnu"))))]
        {
            None
        }
    }

    /// **The regression guard for the subagent SIGABRT.**
    ///
    /// A worker thread of [`build_agent_runtime`] must really have the larger
    /// stack. Asserting the OS-reported size rather than overflowing one is
    /// deliberate: a test that actually overflowed would abort the whole test
    /// binary instead of failing, and would be sized against whatever the
    /// compiler happened to inline that day.
    #[test]
    #[cfg(unix)]
    fn a_worker_thread_really_gets_the_larger_stack() {
        let Some(baseline) = std::thread::spawn(current_thread_stack_size)
            .join()
            .expect("probe thread")
        else {
            eprintln!("stack size is not readable on this target; skipping");
            return;
        };
        // The negative control. Without it, a platform that reported a huge
        // stack for every thread would let the assertion below pass on a
        // runtime built with no `thread_stack_size` at all.
        assert!(
            baseline < AGENT_WORKER_STACK_SIZE,
            "a default thread already has {baseline} bytes, so this test cannot \
             distinguish a configured runtime from an unconfigured one"
        );

        let rt = build_agent_runtime().expect("runtime builds");
        let measured = rt
            .block_on(async { tokio::spawn(async { current_thread_stack_size() }).await })
            .expect("worker task")
            .expect("worker stack size is readable");

        assert!(
            measured >= AGENT_WORKER_STACK_SIZE,
            "a tokio worker got {measured} bytes, not the {AGENT_WORKER_STACK_SIZE} \
             `build_agent_runtime` asks for. Two agent state machines share one \
             worker stack during a subagent spawn; at the inherited {baseline}-byte \
             default a debug build aborts partway through the child's first turn."
        );
    }

    /// The override raises and cannot lower — the direction is the whole point,
    /// so both are asserted.
    #[test]
    fn the_override_can_only_raise_the_stack_size() {
        // Serialised against the whole process via `env-lock`, as every other
        // env-reading test in this crate is.
        let _guard = env_lock::lock_env([(
            AGENT_WORKER_STACK_SIZE_ENV,
            Some(&format!("{}", AGENT_WORKER_STACK_SIZE * 2)),
        )]);
        assert_eq!(worker_stack_size(), AGENT_WORKER_STACK_SIZE * 2);
        drop(_guard);

        let _guard = env_lock::lock_env([(AGENT_WORKER_STACK_SIZE_ENV, Some("65536"))]);
        assert_eq!(
            worker_stack_size(),
            AGENT_WORKER_STACK_SIZE,
            "a smaller override must not shrink the stack back into the abort"
        );
        drop(_guard);

        let _guard = env_lock::lock_env([(AGENT_WORKER_STACK_SIZE_ENV, Some("not a number"))]);
        assert_eq!(worker_stack_size(), AGENT_WORKER_STACK_SIZE);
    }
}
