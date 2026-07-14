//! Cross-platform OS-level sandbox for the developer shell tool (BR-69).
//!
//! BR-64 shipped a macOS-only Seatbelt sandbox; its gate was a *silent no-op*
//! on Linux and Windows (`seatbelt::available()` returns `false` there, and the
//! single call site fell open with a `warn!` nobody reads). BR-69 generalizes
//! that one backend behind a mechanism-neutral trait so the shell tool names a
//! **capability**, not a mechanism, and so a user can tell *which* enforcement
//! tier they actually got.
//!
//! The shape is deliberately identical to today's `SeatbeltPolicy::wrap` ->
//! `(program, prefix_args)`: every backend re-expresses "run this program under
//! a sandbox" as "replace the program with a wrapper that re-execs it". Seatbelt
//! is `sandbox-exec … -- <prog>`; bubblewrap is `bwrap … -- <prog>`; the
//! Landlock/seccomp path re-execs *ourselves* as a hidden helper
//! (`<current_exe> __br-sandbox … -- <prog>`) that applies the in-process
//! kernel restrictions to itself before `execve`ing the real program. So the
//! caller ([`crate::shell_sandbox`] users, i.e. the Developer MCP shell) never
//! needs a `pre_exec` escape hatch and `configure_shell_command` needs no
//! restructuring.
//!
//! Three enforcement backends live here, each behind exactly one `#[cfg]` arm
//! inside [`detect`] — the *only* platform ladder in the tree:
//! - macOS  → [`macos::SeatbeltSandbox`] (a thin adapter over the unchanged
//!   [`crate::seatbelt::SeatbeltPolicy`]; argv is byte-for-byte what BR-64 ships).
//! - Linux  → [`linux::LinuxSandbox`] (Landlock write-confinement + a seccomp
//!   `socket(2)` network filter, with a bubblewrap fallback and a **loud**
//!   `tier: None` when neither is usable).
//! - Windows → [`windows::WindowsSandbox`]. There is **no** unprivileged,
//!   general-purpose, kernel-enforced write-confinement sandbox on Windows that
//!   can wrap an arbitrary developer shell command without breaking `git`,
//!   `node`, `python`, `cargo`. Rather than fail open silently, the Windows
//!   backend reports `tier: None` with an explicit degradation string. See
//!   `windows.rs` for the full rationale and the designed W1/W2 upgrade path.
//!
//! Everything below is pure string/argv construction plus a host-capability
//! probe; nothing in this module spawns the target program — the caller does.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

/// What the sandbox should permit. Mechanism-neutral; every backend maps this
/// onto its own primitives. This is BR-64's `SeatbeltPolicy` fields, renamed and
/// de-mechanised — same two fields, same defaults (no writable roots, network
/// denied).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxPolicy {
    /// Directories the sandboxed process may write to (subpaths included).
    /// Everything else on the filesystem is readable but not writable. Zero
    /// roots means "no writes anywhere" — never "all writes" (the BR-64
    /// `zero_roots_emits_no_unrestricted_write_block` invariant, generalized:
    /// each backend must honour it).
    pub writable_roots: Vec<PathBuf>,
    /// When false (the default), outbound network is denied.
    pub allow_network: bool,
}

impl SandboxPolicy {
    /// A policy writable only under `writable_roots`, with network denied.
    pub fn new(writable_roots: Vec<PathBuf>) -> Self {
        Self {
            writable_roots,
            allow_network: false,
        }
    }

    /// Re-allow (or keep denied) outbound network inside the sandbox.
    pub fn with_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }
}

/// Which enforcement mechanism actually took effect. Reported to the user so a
/// "sandbox on" claim can never overstate what the kernel is enforcing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxTier {
    /// Kernel-enforced write confinement AND network deny.
    Full,
    /// Kernel-enforced write confinement; network deny NOT enforced (e.g.
    /// Landlock present but the seccomp filter could not be installed).
    WriteOnly,
    /// Process containment + resource caps only. No write confinement, no
    /// network deny. The honest name for an unprivileged Windows tier.
    ContainmentOnly,
    /// Nothing. The command runs with the user's full authority.
    None,
}

impl SandboxTier {
    /// A short, user-facing description of what this tier enforces.
    pub fn describe(self) -> &'static str {
        match self {
            SandboxTier::Full => "writes confined + network denied",
            SandboxTier::WriteOnly => "writes confined; network NOT denied",
            SandboxTier::ContainmentOnly => "resource caps only; writes NOT confined",
            SandboxTier::None => "no sandbox; command runs with full user authority",
        }
    }
}

/// A human-readable, user-facing capability report. `mechanism` is the concrete
/// backend (`"seatbelt"`, `"landlock+seccomp"`, `"bubblewrap"`, `"none"`);
/// `degradations` explains every capability we asked for and did not get, so the
/// tool-result line / `doctor` / startup log never have to *infer* what was lost
/// from the tier enum alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxReport {
    pub tier: SandboxTier,
    pub mechanism: &'static str,
    pub degradations: Vec<String>,
}

impl SandboxReport {
    /// A `tier: None` report with an actionable, non-empty degradation. Used by
    /// [`NullSandbox`] and by any backend that probes to "nothing available".
    pub fn none(mechanism: &'static str, why: impl Into<String>) -> Self {
        Self {
            tier: SandboxTier::None,
            mechanism,
            degradations: vec![why.into()],
        }
    }

    /// A one-line, assistant- and human-facing summary, e.g.
    /// `sandbox: landlock+seccomp — writes confined + network denied`.
    /// Degradations, if any, are appended after `;`.
    pub fn summary(&self) -> String {
        let mut s = format!("sandbox: {} — {}", self.mechanism, self.tier.describe());
        if !self.degradations.is_empty() {
            s.push_str("; ");
            s.push_str(&self.degradations.join("; "));
        }
        s
    }
}

/// The argv rewrite a backend performs. Deliberately identical in shape to
/// BR-64's `SeatbeltPolicy::wrap` -> `(program, prefix_args)`: the caller runs
/// `program` and appends `prefix_args` followed by the target program's own
/// args, exactly as `configure_shell_command` does today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wrapped {
    pub program: String,
    pub prefix_args: Vec<String>,
}

/// Errors a sandbox backend's [`ShellSandbox::wrap`] can return.
#[derive(Debug, thiserror::Error)]
pub enum ShellSandboxError {
    /// The backend cannot build a wrapper on this host (e.g. `current_exe()`
    /// is unreadable, so the Linux helper cannot re-exec itself).
    #[error("shell sandbox unavailable: {0}")]
    Unavailable(String),
    /// A policy the backend cannot represent (kept for future backends).
    #[error("{0}")]
    Other(String),
}

/// One enforcement backend. There is exactly one implementor compiled per
/// target (plus [`NullSandbox`]), selected by [`detect`].
pub trait ShellSandbox: Send + Sync {
    /// Probe this host. MUST be a real capability probe (a syscall / a
    /// binary-on-PATH check / a self-test re-exec), never a version guess — a
    /// version check would confidently report `Full` on a host that enforces
    /// nothing (e.g. Landlock compiled in but disabled in the boot `lsm=` list).
    fn probe(&self) -> SandboxReport;

    /// Rewrite `program` so it runs under `policy`. Only called by the shell
    /// tool when `probe().tier != SandboxTier::None`.
    fn wrap(&self, policy: &SandboxPolicy, program: &str) -> Result<Wrapped, ShellSandboxError>;
}

/// The always-available backend: enforces nothing and says so. Selected on any
/// target without a real backend, and returned by [`detect`] as a last resort.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSandbox;

impl ShellSandbox for NullSandbox {
    fn probe(&self) -> SandboxReport {
        SandboxReport::none(
            "none",
            "no OS-level shell sandbox is available on this platform",
        )
    }

    fn wrap(&self, _policy: &SandboxPolicy, _program: &str) -> Result<Wrapped, ShellSandboxError> {
        Err(ShellSandboxError::Unavailable(
            "NullSandbox cannot wrap a command".to_string(),
        ))
    }
}

/// The one entry point the rest of the codebase uses. Returns the best backend
/// compiled in for this target, or a [`NullSandbox`]. **This is the only
/// `#[cfg]` platform ladder in the sandbox tree.**
pub fn detect() -> Box<dyn ShellSandbox> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::SeatbeltSandbox)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxSandbox::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsSandbox)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Box::new(NullSandbox)
    }
}

/// Sandbox-helper entry point. Call this as the **first line of `main()`** in
/// every binary that may wrap a shell command (`biorouter`, `biorouterd`).
///
/// On Linux, when the process was re-exec'd with the hidden `__br-sandbox`
/// marker, this applies the in-process Landlock + seccomp restrictions and
/// `execve`s the real program — **it never returns** in that case (see
/// [`linux::run_helper_if_invoked`]). On every other platform, and on Linux for
/// a normal invocation, it returns immediately and startup proceeds unchanged.
///
/// It MUST run before the Tokio runtime starts: the helper deliberately re-execs
/// a fresh single-threaded process to dodge the fork-in-a-multithreaded-runtime
/// allocator-deadlock hazard that a `pre_exec` closure would hit.
pub fn run_helper_if_invoked() {
    #[cfg(target_os = "linux")]
    {
        linux::run_helper_if_invoked();
    }
    // macOS (Seatbelt) and Windows use external wrappers / no helper tier, so
    // there is nothing to intercept here — a deliberate no-op.
}

/// The three-valued gate for `BIOROUTER_SHELL_SANDBOX` (BR-69). The default is
/// [`SandboxMode::Off`] — byte-for-byte BR-64's unsandboxed path — so nothing
/// changes unless a user opts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    /// Unset / `0` / `off` / `no` / `false` / anything unknown. Run unsandboxed.
    Off,
    /// `1` / `true` / `on` / `auto` / `seatbelt` (the legacy BR-64 value).
    /// Use the best available tier; if none is available, warn once and run
    /// anyway (today's fail-open, made loud).
    Auto,
    /// `strict`. Require `tier == Full`. If the host cannot provide it, the
    /// shell tool **refuses to run** the command with an error naming the
    /// missing capability — a mode named "strict" that silently degraded to
    /// "unsandboxed" would be a false assurance.
    Strict,
}

impl SandboxMode {
    /// Parse the raw `BIOROUTER_SHELL_SANDBOX` value. Case-insensitive, trimmed.
    /// Unknown values are [`SandboxMode::Off`] — never a silent `Auto`.
    pub fn parse(value: Option<&str>) -> Self {
        match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("1") | Some("true") | Some("on") | Some("auto") | Some("seatbelt") => {
                SandboxMode::Auto
            }
            Some("strict") => SandboxMode::Strict,
            _ => SandboxMode::Off,
        }
    }

    /// Read the mode from the process environment.
    pub fn from_env() -> Self {
        Self::parse(std::env::var("BIOROUTER_SHELL_SANDBOX").ok().as_deref())
    }

    /// Whether the gate is on at all (`Auto` or `Strict`).
    pub fn is_on(self) -> bool {
        !matches!(self, SandboxMode::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_defaults_deny_network_and_have_no_roots() {
        let p = SandboxPolicy::default();
        assert!(!p.allow_network, "network must be denied by default");
        assert!(p.writable_roots.is_empty(), "no writable roots by default");
    }

    #[test]
    fn mode_parse_covers_off_auto_strict_and_legacy() {
        for on in [
            "1", "true", "on", "auto", "seatbelt", "  AUTO ", "Seatbelt", "TRUE",
        ] {
            assert_eq!(SandboxMode::parse(Some(on)), SandboxMode::Auto, "{on:?}");
        }
        assert_eq!(SandboxMode::parse(Some("strict")), SandboxMode::Strict);
        assert_eq!(SandboxMode::parse(Some(" STRICT ")), SandboxMode::Strict);
        // Unset, explicit-off, and unknown all mean Off — never a silent Auto.
        for off in [
            None,
            Some(""),
            Some("0"),
            Some("off"),
            Some("no"),
            Some("false"),
            Some("docker"),
            Some("banana"),
        ] {
            assert_eq!(SandboxMode::parse(off), SandboxMode::Off, "{off:?}");
        }
    }

    #[test]
    fn null_sandbox_reports_none_with_actionable_degradation() {
        let r = NullSandbox.probe();
        assert_eq!(r.tier, SandboxTier::None);
        assert!(!r.degradations.is_empty(), "None must explain why");
        assert!(NullSandbox
            .wrap(&SandboxPolicy::default(), "/bin/sh")
            .is_err());
    }

    #[test]
    fn report_summary_lists_mechanism_tier_and_degradations() {
        let r = SandboxReport {
            tier: SandboxTier::WriteOnly,
            mechanism: "landlock",
            degradations: vec!["outbound network is NOT blocked".to_string()],
        };
        let s = r.summary();
        assert!(s.contains("landlock"), "{s}");
        assert!(s.contains("network"), "{s}");
    }

    /// `detect()` must always return a working backend whose `probe()` yields a
    /// coherent report (never panics) on whatever host runs the test.
    #[test]
    fn detect_returns_a_probeable_backend() {
        let backend = detect();
        let report = backend.probe();
        // On macOS this is Seatbelt (Full when sandbox-exec exists); elsewhere
        // it is whatever the platform backend reports. All we assert here is a
        // non-empty mechanism string and, when the tier is None, a reason.
        assert!(!report.mechanism.is_empty());
        if report.tier == SandboxTier::None {
            assert!(!report.degradations.is_empty());
        }
    }
}
