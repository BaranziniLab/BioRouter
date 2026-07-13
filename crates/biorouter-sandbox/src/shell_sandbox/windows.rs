//! Windows backend: an **honest** capability reporter (BR-69 Slice 3).
//!
//! ## Say the hard thing plainly
//!
//! There is **no unprivileged, general-purpose, kernel-enforced write-
//! confinement sandbox on Windows** that can wrap an arbitrary developer shell
//! command without breaking it. The candidates and why each fails as a general
//! shell wrapper:
//!
//! | Mechanism | Write confine | Net deny | Needs admin | Why it fails |
//! |---|---|---|---|---|
//! | Job Object | ✗ | ✗ | no | Caps processes/memory/CPU and gives a real kill-the-tree primitive — not an access-control mechanism at all. |
//! | Restricted token + Low IL | partial | ✗ | no | Low-IL blocks writes to medium-IL objects, but only by ACLing the user's *own* project dir to Low integrity — a hostile, persistent mutation of their files. |
//! | AppContainer / LPAC | ✓ | ✓ | no | Real confinement, but the fresh package SID has access to almost nothing; `git`/`node`/`python`/`cargo`/MSVC routinely break on registry/named-pipe/`%LOCALAPPDATA%` denials unless you ACL the toolchain to the package SID. |
//! | Firewall rules (Codex's model) | n/a | ✓ | **yes** | Dedicated sandbox users + firewall rules require administrator; a UCSF-managed laptop user does not have it. |
//! | Windows Sandbox (WDAG) | ✓ | ✓ | Pro/Enterprise | A full VM: seconds of startup per invocation, no shared FS by default. Unusable for per-command wrapping. |
//!
//! Therefore, rather than **fail open silently** (BR-64's original Windows
//! behaviour — a `warn!` nobody reads, then the command runs with the operator's
//! full authority), this backend reports `tier: None` with an explicit
//! degradation string. Under `BIOROUTER_SHELL_SANDBOX=strict` the shell tool
//! then **refuses to run**, which is exactly the loud failure a fleet admin
//! wants on a box that cannot reach `Full`. Under `auto` it warns once and runs
//! unsandboxed, matching every other platform.
//!
//! ## The designed upgrade path (future work, behind this same trait)
//!
//! BR-69's design specifies two Windows tiers this backend is the seam for:
//!
//! - **W1 — `ContainmentOnly` (Job Object + restricted token + Low IL).** Not a
//!   sandbox — it confines nothing — but it delivers resource caps, no privilege
//!   escalation, and a **real kill-the-whole-tree primitive** (a Job Object with
//!   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) that `taskkill /T` only approximates
//!   (this is also the fix for BR-37's structurally-weaker Windows kill arm).
//!   When implemented it will report `SandboxTier::ContainmentOnly`, mechanism
//!   `"job-object"`, with degradations `["writes are NOT confined", "outbound
//!   network is NOT blocked"]` — and it must **never** report `Full`.
//! - **W2 — `Full` via AppContainer (opt-in, `BIOROUTER_SHELL_SANDBOX=appcontainer`).**
//!   Real write-confinement + real kernel network-deny, at the cost of ACL-
//!   granting the app-container SID onto the writable roots and the user's
//!   toolchain. Correct for a locked-down "run untrusted code" mode; wrong as a
//!   default because it breaks tools.
//!
//! Both require Win32 FFI (`windows`/`windows-sys`: `JobObjects`,
//! `CreateRestrictedToken`, `SetTokenInformation`) and can only be validated on
//! a real Windows runner. They are deliberately **not** implemented here as
//! untested `unsafe` FFI — shipping a Windows "sandbox" that cannot be exercised
//! on the mac dev box would be exactly the security theatre this design warns
//! against. The honest `None` report is the correct, testable Slice-3 floor; the
//! W1/W2 tiers land once a `windows-latest` CI job exists to prove them (design
//! §"Test plan → Windows", §"CI job 3").

use super::{SandboxPolicy, SandboxReport, ShellSandbox, ShellSandboxError, Wrapped};

/// The Windows backend. Reports `tier: None` honestly (see the module docs for
/// why a strong unprivileged sandbox is impractical and for the designed
/// W1/W2 upgrade path this type is the seam for).
pub struct WindowsSandbox;

impl ShellSandbox for WindowsSandbox {
    fn probe(&self) -> SandboxReport {
        SandboxReport::none(
            "none",
            "Windows has no unprivileged, general-purpose write-confinement sandbox that can wrap \
             an arbitrary developer command without breaking it (AppContainer breaks git/node/\
             python; firewall network-deny needs administrator). The command runs with full user \
             authority — use `BIOROUTER_SHELL_SANDBOX=strict` to refuse to run instead, or run \
             untrusted work on Linux/macOS where a kernel sandbox is available",
        )
    }

    fn wrap(&self, _policy: &SandboxPolicy, _program: &str) -> Result<Wrapped, ShellSandboxError> {
        // Never called while probe() reports None; if a future W1/W2 tier is
        // wired in, this is where the Job-Object / AppContainer wrapper goes.
        Err(ShellSandboxError::Unavailable(
            "no Windows shell sandbox tier is implemented (probe reports None)".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_sandbox::SandboxTier;

    /// The guard against someone later "upgrading" the label without upgrading
    /// the mechanism: the Windows backend must never claim `Full`.
    #[test]
    fn windows_never_reports_full() {
        let r = WindowsSandbox.probe();
        assert_ne!(r.tier, SandboxTier::Full);
        assert_eq!(r.tier, SandboxTier::None);
        assert!(!r.degradations.is_empty(), "None must explain why, loudly");
    }
}
