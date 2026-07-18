//! macOS Seatbelt (`sandbox-exec`) profile generation for tool execution.
//!
//! Slice 1 of BR-64 (see `docs/agent-loop/designs/macos-seatbelt-sandbox.md`): a
//! kernel-enforced, default-deny profile for the developer shell tool on macOS.
//! It grants **read** of the whole filesystem, confines **writes** to a set of
//! injected writable roots (the session working dir + the temp dir), and
//! **denies outbound network** by default — the same model Codex CLI uses
//! (`sandbox-exec -p` with writable-roots injected, network-outbound denied).
//!
//! This module is pure string/argv construction plus a host-capability check;
//! it never spawns anything itself. The caller ([`crate`] users, e.g. the
//! Developer MCP shell) wraps its own `Command` program with [`SeatbeltPolicy::wrap`].
//!
//! Paths are passed to `sandbox-exec` as `-DWRITABLE_ROOT_n=…` parameters and
//! referenced from the profile via `(param "WRITABLE_ROOT_n")`, so a path can
//! never inject SBPL syntax into the profile text.

use std::path::{Path, PathBuf};

/// Absolute path to the macOS Seatbelt front-end. It ships with the OS.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// The static base policy, modeled on Codex CLI's `seatbelt_base_policy.sbpl`
/// (itself derived from Chromium's `common.sb`). Default-deny; read-only file
/// ops everywhere; children inherit the policy; a bounded `sysctl-read` set that
/// dyld/libSystem need to start a process. The writable-roots `file-write*`
/// block and any network allowance are appended dynamically by [`SeatbeltPolicy::profile`].
const BASE_POLICY: &str = r#"(version 1)

; Biorouter shell sandbox (macOS Seatbelt). Default-deny; full read; writes only
; to injected writable roots; outbound network denied by omission.
(deny default)

; Read-only file operations are permitted anywhere.
(allow file-read*)

; Child processes inherit the policy of their parent.
(allow process-exec)
(allow process-fork)
(allow signal (target self))

; Writing to the null / std streams is always permitted.
(allow file-write-data
  (require-all
    (path "/dev/null")
    (vnode-type CHARACTER-DEVICE)))
(allow file-write-data
  (path "/dev/stdout")
  (path "/dev/stderr")
  (path "/dev/dtracehelper"))

; A conservative set of read-only sysctls that dyld / libSystem need to launch.
(allow sysctl-read
  (sysctl-name "hw.activecpu")
  (sysctl-name "hw.busfrequency_compat")
  (sysctl-name "hw.byteorder")
  (sysctl-name "hw.cacheconfig")
  (sysctl-name "hw.cachelinesize_compat")
  (sysctl-name "hw.cpufamily")
  (sysctl-name "hw.cpufrequency_compat")
  (sysctl-name "hw.cpusubtype")
  (sysctl-name "hw.cputype")
  (sysctl-name "hw.logicalcpu_max")
  (sysctl-name "hw.machine")
  (sysctl-name "hw.memsize")
  (sysctl-name "hw.ncpu")
  (sysctl-name "hw.nperflevels")
  (sysctl-name "hw.pagesize")
  (sysctl-name "hw.pagesize_compat")
  (sysctl-name "hw.physicalcpu_max")
  (sysctl-name "hw.tbfrequency_compat")
  (sysctl-name "hw.vectorunit")
  (sysctl-name "kern.hostname")
  (sysctl-name "kern.maxfilesperproc")
  (sysctl-name "kern.osproductversion")
  (sysctl-name "kern.osrelease")
  (sysctl-name "kern.ostype")
  (sysctl-name "kern.osvariant_status")
  (sysctl-name "kern.osversion")
  (sysctl-name "kern.secure_kernel")
  (sysctl-name "kern.usrstack64")
  (sysctl-name "kern.version")
  (sysctl-name "sysctl.proc_cputype")
  (sysctl-name-prefix "hw.optional.")
  (sysctl-name-prefix "hw.perflevel"))

; Bootstrap / mach services needed to exec a process image.
(allow mach-lookup)
(allow ipc-posix-shm-read-data)"#;

/// A Seatbelt policy: full filesystem read, writes confined to `writable_roots`,
/// outbound network denied unless `allow_network`.
#[derive(Debug, Clone, Default)]
pub struct SeatbeltPolicy {
    /// Directories the sandboxed process may write to (subpaths included).
    pub writable_roots: Vec<PathBuf>,
    /// When true, re-allow outbound network (off by default = Codex's deny).
    pub allow_network: bool,
}

impl SeatbeltPolicy {
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

    /// The full SBPL profile string to pass via `sandbox-exec -p`.
    pub fn profile(&self) -> String {
        let mut p = String::from(BASE_POLICY);

        // Writable roots, referenced as parameters so paths stay data, not SBPL.
        // With zero roots we emit NO write block — an unfiltered `(allow
        // file-write*)` would permit *all* writes, the opposite of the intent.
        if !self.writable_roots.is_empty() {
            p.push_str("\n\n; Writable roots (paths injected as -D parameters).\n");
            p.push_str("(allow file-write*");
            for i in 0..self.writable_roots.len() {
                p.push_str(&format!("\n  (subpath (param \"WRITABLE_ROOT_{i}\"))"));
            }
            p.push(')');
        }

        if self.allow_network {
            p.push_str("\n\n; Network re-allowed (BIOROUTER_SHELL_SANDBOX_NETWORK).\n");
            p.push_str("(allow network*)\n(allow system-socket)");
        }

        p.push('\n');
        p
    }

    /// Build the `sandbox-exec` invocation that runs `program` under this
    /// policy. Returns `(sandbox_exec_path, wrapper_args)` where `wrapper_args`
    /// is `["-p", <profile>, "-DWRITABLE_ROOT_0=…", …, "--", program]`, ready to
    /// have the program's own arguments appended by the caller.
    pub fn wrap(&self, program: &str) -> (String, Vec<String>) {
        let mut args = Vec::with_capacity(self.writable_roots.len() + 4);
        args.push("-p".to_string());
        args.push(self.profile());
        for (i, root) in self.writable_roots.iter().enumerate() {
            // Seatbelt matches `subpath` against the *canonical* (symlink-
            // resolved) path, so a root like `/tmp` or macOS's `/var/folders/…`
            // temp dir must be resolved to `/private/…` first or an in-root
            // write is spuriously denied. Best-effort: fall back to the literal
            // path when it doesn't yet exist (e.g. unit-test fixtures).
            let resolved = root.canonicalize().unwrap_or_else(|_| root.clone());
            // `-DKEY=VALUE` as one argument (Codex's form); the value is passed
            // as an argv element so no shell quoting is involved.
            args.push(format!("-DWRITABLE_ROOT_{i}={}", resolved.display()));
        }
        args.push("--".to_string());
        args.push(program.to_string());
        (SANDBOX_EXEC.to_string(), args)
    }
}

/// Whether the Seatbelt sandbox is usable on this host: macOS with
/// `/usr/bin/sandbox-exec` present.
pub fn available() -> bool {
    cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_profile_is_default_deny_read_allow() {
        let p = SeatbeltPolicy::new(vec![PathBuf::from("/work")]).profile();
        assert!(p.starts_with("(version 1)"), "profile: {p}");
        assert!(p.contains("(deny default)"));
        assert!(p.contains("(allow file-read*)"));
    }

    #[test]
    fn network_denied_by_default_allowed_on_request() {
        let denied = SeatbeltPolicy::new(vec![PathBuf::from("/work")]).profile();
        assert!(
            !denied.contains("(allow network*)"),
            "network must be denied by default"
        );
        let allowed = SeatbeltPolicy::new(vec![PathBuf::from("/work")])
            .with_network(true)
            .profile();
        assert!(allowed.contains("(allow network*)"));
    }

    #[test]
    fn writable_roots_become_params() {
        let policy = SeatbeltPolicy::new(vec![PathBuf::from("/work"), PathBuf::from("/tmp/x")]);
        let profile = policy.profile();
        assert!(profile.contains("(subpath (param \"WRITABLE_ROOT_0\"))"));
        assert!(profile.contains("(subpath (param \"WRITABLE_ROOT_1\"))"));

        let (prog, args) = policy.wrap("/bin/zsh");
        assert_eq!(prog, SANDBOX_EXEC);
        assert!(args.contains(&"-DWRITABLE_ROOT_0=/work".to_string()));
        assert!(args.contains(&"-DWRITABLE_ROOT_1=/tmp/x".to_string()));
    }

    #[test]
    fn zero_roots_emits_no_unrestricted_write_block() {
        let p = SeatbeltPolicy::new(vec![]).profile();
        // No writable-roots block at all -> no path can be written (except the
        // explicit /dev/null etc.). Crucially, never a bare `(allow file-write*)`.
        assert!(
            !p.contains("(allow file-write*"),
            "zero roots must not allow any subtree write"
        );
    }

    #[test]
    fn wrap_ends_with_separator_and_program() {
        let (_prog, args) = SeatbeltPolicy::new(vec![PathBuf::from("/work")]).wrap("/bin/bash");
        assert_eq!(args[0], "-p");
        let tail = &args[args.len() - 2..];
        assert_eq!(tail, &["--".to_string(), "/bin/bash".to_string()]);
    }

    /// Live proof of kernel enforcement: a write inside the writable root
    /// succeeds while a write outside it is denied by the sandbox. This is the
    /// whole point of BR-64 — it cannot be validated by string assertions.
    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_enforces_write_confinement() {
        use std::process::Command;

        if !available() {
            eprintln!("sandbox-exec unavailable; skipping live enforcement test");
            return;
        }

        let work = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir outside the writable root");
        let policy = SeatbeltPolicy::new(vec![work.path().to_path_buf()]);
        let (prog, mut args) = policy.wrap("/bin/sh");
        args.push("-c".to_string());

        // Inside the writable root: allowed.
        let inside_path = work.path().join("ok.txt");
        let mut inside = args.clone();
        inside.push(format!("echo hi > {}", inside_path.display()));
        let status = Command::new(&prog)
            .args(&inside)
            .status()
            .expect("spawn sandbox-exec");
        assert!(
            status.success() && inside_path.exists(),
            "write inside the writable root should succeed"
        );

        // Outside the writable root: denied by the kernel (non-zero exit).
        let outside_path = outside.path().join("blocked.txt");
        let mut deny = args.clone();
        deny.push(format!("echo nope > {}", outside_path.display()));
        let status = Command::new(&prog)
            .args(&deny)
            .status()
            .expect("spawn sandbox-exec");
        assert!(
            !status.success() && !outside_path.exists(),
            "write outside the writable root must be denied"
        );
    }

    /// A benign read/echo still runs cleanly under the profile (no false break).
    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_allows_benign_command() {
        use std::process::Command;

        if !available() {
            return;
        }
        let work = tempfile::tempdir().expect("tempdir");
        let policy = SeatbeltPolicy::new(vec![work.path().to_path_buf()]);
        let (prog, mut args) = policy.wrap("/bin/sh");
        args.push("-c".to_string());
        args.push("echo sandboxed && cat /etc/hosts > /dev/null".to_string());
        let out = Command::new(&prog).args(&args).output().expect("spawn");
        assert!(
            out.status.success(),
            "benign command failed under sandbox: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
