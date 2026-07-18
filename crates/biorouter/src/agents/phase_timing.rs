//! Opt-in phase timing for the agent turn loop.
//!
//! Stage 0 of the tool-call latency investigation
//! (`docs/investigations/2026-07-18-tool-call-ui-latency.md` §6.0): before any
//! fix can be justified, the individual phases of a turn have to be separately
//! measurable. `Phase::start(name)` returns a guard that emits
//!
//! ```text
//! DEBUG phase: phase=<name> dur_us=<elapsed>
//! ```
//!
//! on drop, under the `phase` tracing target.
//!
//! **Cost when disabled.** The `BIOROUTER_PHASE_TIMING` env var is read exactly
//! once into a `LazyLock<bool>`; `Phase::start` must never call
//! `std::env::var` itself, because these guards sit on the hot path (per tool
//! call, per provider turn) and `var` is a lock + allocation + environ scan.
//! With the flag off, `start` is an atomic load plus storing a `None`, and
//! `drop` is a single branch — no `Instant::now` syscall, no formatting.
//!
//! Enable with `BIOROUTER_PHASE_TIMING=1` (or `=true`) plus a subscriber that
//! passes `phase=debug`, e.g.
//! `BIOROUTER_PHASE_TIMING=1 RUST_LOG=phase=debug biorouter ...`.

use std::sync::LazyLock;
use std::time::Instant;

use tracing::debug;

/// Parse the raw env value. Split out from the `LazyLock` so the accepted
/// spellings can be unit-tested without mutating process environment (which is
/// unsound to do from multi-threaded tests).
fn parse_flag(raw: Option<&str>) -> bool {
    match raw {
        Some(v) => {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        }
        None => false,
    }
}

/// Read once, for the life of the process. See the module note on why this is
/// not `std::env::var` at each call site.
static ENABLED: LazyLock<bool> =
    LazyLock::new(|| parse_flag(std::env::var("BIOROUTER_PHASE_TIMING").ok().as_deref()));

/// Whether phase timing is on. Cheap enough to call on the hot path (one
/// relaxed atomic load after first use).
#[inline]
pub fn enabled() -> bool {
    *ENABLED
}

/// A scoped timer. Times the region between `start` and drop, and logs it
/// under the `phase` target. Does nothing at all when the feature is off.
///
/// ```ignore
/// let _p = Phase::start("agent.integrate_tool_result");
/// // ... work ...
/// // logged here, at end of scope
/// ```
///
/// To time two adjacent regions separately (e.g. lock *wait* vs the call made
/// while holding it — the H6 measurement), drop the first guard explicitly:
///
/// ```ignore
/// let wait = Phase::start("mcp.client_lock_wait");
/// let guard = client.lock().await;
/// drop(wait);
/// let _call = Phase::start("mcp.call_tool");
/// ```
#[derive(Debug)]
pub struct Phase {
    name: &'static str,
    /// `None` when timing is disabled — this is what makes the guard free.
    started: Option<Instant>,
}

impl Phase {
    /// Start timing `name`. `name` is `&'static str` deliberately: it keeps
    /// the disabled path allocation-free.
    #[inline]
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            started: if enabled() {
                Some(Instant::now())
            } else {
                None
            },
        }
    }

    /// Elapsed microseconds so far, or `None` when disabled. Mainly for tests
    /// and for callers that want the number as well as the log line.
    pub fn elapsed_us(&self) -> Option<u64> {
        self.started.map(|t| t.elapsed().as_micros() as u64)
    }

    /// Whether this guard is actually timing anything.
    pub fn is_active(&self) -> bool {
        self.started.is_some()
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            debug!(
                target: "phase",
                phase = %self.name,
                dur_us = started.elapsed().as_micros() as u64,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flag_accepts_documented_truthy_spellings() {
        assert!(parse_flag(Some("1")));
        assert!(parse_flag(Some("true")));
        assert!(parse_flag(Some("TRUE")));
        assert!(parse_flag(Some("yes")));
        assert!(parse_flag(Some(" 1 ")));
    }

    #[test]
    fn parse_flag_rejects_unset_and_falsey() {
        assert!(!parse_flag(None));
        assert!(!parse_flag(Some("")));
        assert!(!parse_flag(Some("0")));
        assert!(!parse_flag(Some("false")));
        // Guard against a naive `is_some()` check treating any value as on.
        assert!(!parse_flag(Some("no")));
        assert!(!parse_flag(Some("off")));
    }

    /// The core zero-overhead claim: with the flag off, the guard captures no
    /// `Instant` at all, so drop has nothing to format or log.
    #[test]
    fn disabled_guard_captures_no_instant() {
        if enabled() {
            // The test process opted in; the inverse property is covered below.
            return;
        }
        let p = Phase::start("test.disabled");
        assert!(!p.is_active());
        assert_eq!(p.elapsed_us(), None);
    }

    #[test]
    fn enabled_guard_measures_elapsed_time() {
        // Exercise the timing path directly, without depending on the
        // process-wide flag (a `LazyLock` cannot be re-read per test).
        let p = Phase {
            name: "test.enabled",
            started: Some(Instant::now()),
        };
        assert!(p.is_active());
        std::thread::sleep(std::time::Duration::from_millis(2));
        let us = p.elapsed_us().expect("active guard reports elapsed time");
        assert!(us >= 1_000, "expected >=1ms, got {us}us");
    }

    /// Proves the `LazyLock` is actually wired to the env var. Holds in both
    /// directions, so it is meaningful whether or not the suite is run with
    /// `BIOROUTER_PHASE_TIMING=1` — run it both ways.
    #[test]
    fn enabled_reflects_the_environment_flag() {
        let expected = parse_flag(std::env::var("BIOROUTER_PHASE_TIMING").ok().as_deref());
        assert_eq!(enabled(), expected);
    }

    #[test]
    fn guard_drop_is_safe_in_both_states() {
        drop(Phase::start("test.drop.flag_state"));
        drop(Phase {
            name: "test.drop.forced_on",
            started: Some(Instant::now()),
        });
    }
}
