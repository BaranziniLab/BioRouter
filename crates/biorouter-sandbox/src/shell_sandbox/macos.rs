//! macOS backend: a thin adapter over BR-64's unchanged
//! [`crate::seatbelt::SeatbeltPolicy`].
//!
//! **No macOS behavior change.** `SeatbeltPolicy` — its SBPL text, its
//! `-DWRITABLE_ROOT_n=` params, its `--` separator, its `canonicalize()`
//! fallback — is untouched. This adapter simply forwards a mechanism-neutral
//! [`SandboxPolicy`] onto the exact same `SeatbeltPolicy::new(...).with_network(...).wrap(...)`
//! call. The `golden_argv_matches_seatbelt_policy` test below pins that the argv
//! handed to `tokio::process::Command` is byte-for-byte what BR-64 produces.

use crate::seatbelt::{self, SeatbeltPolicy};

use super::{SandboxPolicy, SandboxReport, SandboxTier, ShellSandbox, ShellSandboxError, Wrapped};

/// The macOS Seatbelt (`sandbox-exec`) backend.
pub struct SeatbeltSandbox;

impl ShellSandbox for SeatbeltSandbox {
    fn probe(&self) -> SandboxReport {
        if seatbelt::available() {
            SandboxReport {
                tier: SandboxTier::Full,
                mechanism: "seatbelt",
                degradations: vec![],
            }
        } else {
            SandboxReport::none(
                "seatbelt",
                "/usr/bin/sandbox-exec not found (not macOS, or a stripped system)",
            )
        }
    }

    fn wrap(&self, policy: &SandboxPolicy, program: &str) -> Result<Wrapped, ShellSandboxError> {
        // SandboxPolicy IS SeatbeltPolicy's two fields. Same constructor, same
        // `wrap()`, same argv — this is what makes macOS byte-for-byte unchanged.
        let p =
            SeatbeltPolicy::new(policy.writable_roots.clone()).with_network(policy.allow_network);
        let (program, prefix_args) = p.wrap(program);
        Ok(Wrapped {
            program,
            prefix_args,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The load-bearing "no macOS behavior change" guarantee: the adapter's
    /// `(program, prefix_args)` equals `SeatbeltPolicy::wrap`'s output exactly.
    /// Pure string construction, so it runs on every platform.
    #[test]
    fn golden_argv_matches_seatbelt_policy() {
        let roots = vec![PathBuf::from("/work"), PathBuf::from("/tmp/x")];
        let policy = SandboxPolicy::new(roots.clone()).with_network(false);

        let (want_prog, want_args) = SeatbeltPolicy::new(roots)
            .with_network(false)
            .wrap("/bin/zsh");
        let got = SeatbeltSandbox.wrap(&policy, "/bin/zsh").unwrap();

        assert_eq!(
            got.program, want_prog,
            "program must match Seatbelt exactly"
        );
        assert_eq!(
            got.prefix_args, want_args,
            "argv must match Seatbelt exactly"
        );
    }

    #[test]
    fn network_flag_threads_through_unchanged() {
        let policy = SandboxPolicy::new(vec![PathBuf::from("/work")]).with_network(true);
        let (_p, want) = SeatbeltPolicy::new(vec![PathBuf::from("/work")])
            .with_network(true)
            .wrap("/bin/sh");
        let got = SeatbeltSandbox.wrap(&policy, "/bin/sh").unwrap();
        assert_eq!(got.prefix_args, want);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn probe_reports_full_when_sandbox_exec_present() {
        let r = SeatbeltSandbox.probe();
        if seatbelt::available() {
            assert_eq!(r.tier, SandboxTier::Full);
            assert_eq!(r.mechanism, "seatbelt");
            assert!(r.degradations.is_empty());
        }
    }
}
