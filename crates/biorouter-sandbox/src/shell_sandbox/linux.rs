//! Linux backend: Landlock write-confinement + a seccomp `socket(2)` network
//! filter, with a bubblewrap fallback and a **loud** `tier: None` when neither
//! is usable (BR-69 Slice 2).
//!
//! ## Why a helper binary, not `pre_exec`
//!
//! Seatbelt (macOS) is an external wrapper program. Landlock and seccomp are
//! **in-process syscalls that must run in the child, after `fork`, before
//! `exec`**. Doing that in a `CommandExt::pre_exec` closure is `unsafe` and must
//! be async-signal-safe: in a child forked from a multi-threaded process (which
//! `biorouterd` is — it is a Tokio runtime) any `malloc` can deadlock on an
//! allocator lock another thread held at fork time, and `rust-landlock`'s
//! ruleset construction allocates. That is a real, intermittent, un-debuggable
//! hang.
//!
//! So we do what OpenAI's `codex-linux-sandbox` does: **re-exec ourselves** as a
//! hidden helper. [`wrap`](LinuxSandbox::wrap) returns
//! `(current_exe(), ["__br-sandbox", "--writable-root", "/w", "--deny-net", "--", "/bin/bash"])`.
//! The helper process is a fresh, single-threaded `main` (before Tokio starts):
//! it applies `PR_SET_NO_NEW_PRIVS` → Landlock ruleset → seccomp filter to
//! *itself*, then `execve`s the real program. No fork-safety hazard, and it
//! collapses to the same `(program, prefix_args)` shape as Seatbelt, so the
//! trait needs no `pre_exec` escape hatch.
//!
//! [`run_helper_if_invoked`] is the helper entry point; it MUST be called as the
//! first line of `main()` in both `biorouter` and `biorouterd`. It keys off a
//! hidden argv marker (`__br-sandbox`), not a clap subcommand, so it never shows
//! in `--help` and cannot collide with a user command.
//!
//! ## Detection: probe the ABI/self-test, never `uname`
//!
//! Landlock being *compiled into* the kernel is not enough — it must also be
//! enabled in the boot-time `lsm=` list, which several enterprise distros
//! (SELinux-first) and older WSL2 kernels do not do. A `kernel >= 5.13` check
//! would confidently report `Full` on a host enforcing nothing. So [`probe`]
//! (a) does a side-effect-free `landlock_create_ruleset` probe in-process, and
//! (b) re-execs `current_exe() __br-sandbox --selftest`, which applies the real
//! restrictions to a throwaway child and reports, via its exit code, exactly
//! which capabilities took effect — this also proves the helper marker is wired
//! into the running binary (a binary without it makes `probe` fall through to
//! the bubblewrap fallback or `None`, never a wrapper that would exec a normal
//! CLI startup).

use std::path::PathBuf;
use std::sync::OnceLock;

use super::{SandboxPolicy, SandboxReport, SandboxTier, ShellSandbox, ShellSandboxError, Wrapped};

/// Hidden argv marker that turns a normal binary invocation into the sandbox
/// helper. Not a clap subcommand — see the module docs.
const HELPER_MARKER: &str = "__br-sandbox";

/// Test/override hook: `BIOROUTER_SHELL_SANDBOX_BACKEND=landlock|bubblewrap`
/// forces a specific backend so the degradation ladder can be exercised on a
/// host that would otherwise pick a different one (used by the CI matrix).
const BACKEND_OVERRIDE_ENV: &str = "BIOROUTER_SHELL_SANDBOX_BACKEND";

// `--selftest` exit codes. probe() maps these back to a tier. Anything *else*
// (e.g. a clap "unknown subcommand" error from a binary that never wired the
// marker in) means "helper not wired" — never a false `Full`.
const SELFTEST_FULL: i32 = 0; // landlock + seccomp both enforced
const SELFTEST_WRITE_ONLY: i32 = 10; // landlock enforced, seccomp refused
const SELFTEST_NO_LANDLOCK: i32 = 20; // landlock not enforced (kernel/lsm=)

/// The Linux backend.
pub struct LinuxSandbox {
    _priv: (),
}

impl LinuxSandbox {
    pub fn new() -> Self {
        Self { _priv: () }
    }
}

impl Default for LinuxSandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Which mechanism the probe settled on. Kept internal; surfaced via
/// [`SandboxReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    LandlockSeccomp,
    LandlockOnly,
    Bubblewrap,
    None,
}

fn backend_override() -> Option<String> {
    std::env::var(BACKEND_OVERRIDE_ENV)
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
}

fn bwrap_on_path() -> bool {
    which_on_path("bwrap")
}

/// Minimal PATH lookup (avoids a `which` dependency in this leaf crate).
fn which_on_path(bin: &str) -> bool {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                // Best-effort executable check.
                use std::os::unix::fs::PermissionsExt;
                if let Ok(md) = std::fs::metadata(&candidate) {
                    if md.permissions().mode() & 0o111 != 0 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Run `current_exe() __br-sandbox --selftest` and return its exit code, or
/// `None` if it could not be spawned / did not exit with a signal-free code.
fn run_selftest() -> Option<i32> {
    let exe = std::env::current_exe().ok()?;
    let mut command = std::process::Command::new(exe);
    command
        .arg(HELPER_MARKER)
        .arg("--selftest")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    crate::environment::strip_daemon_private_env_std(&mut command);
    let status = command.status().ok()?;
    status.code()
}

/// Determine the effective backend for this host. Cached — it may spawn a
/// subprocess, and capability does not change within a process's lifetime.
fn effective_backend() -> Backend {
    static CACHE: OnceLock<Backend> = OnceLock::new();
    *CACHE.get_or_init(|| {
        match backend_override().as_deref() {
            Some("bubblewrap") | Some("bwrap") => {
                return if bwrap_on_path() {
                    Backend::Bubblewrap
                } else {
                    Backend::None
                };
            }
            Some("landlock") => {
                // Force the helper path; report what the self-test actually got.
                return match run_selftest() {
                    Some(SELFTEST_FULL) => Backend::LandlockSeccomp,
                    Some(SELFTEST_WRITE_ONLY) => Backend::LandlockOnly,
                    _ => Backend::None,
                };
            }
            _ => {}
        }

        // Auto: Landlock first (no dependency), bubblewrap as fallback.
        match run_selftest() {
            Some(SELFTEST_FULL) => Backend::LandlockSeccomp,
            Some(SELFTEST_WRITE_ONLY) => Backend::LandlockOnly,
            // SELFTEST_NO_LANDLOCK, or the helper marker is not wired into this
            // binary (any other exit code): fall back to bubblewrap, an external
            // wrapper that needs no helper.
            _ => {
                if bwrap_on_path() {
                    Backend::Bubblewrap
                } else {
                    Backend::None
                }
            }
        }
    })
}

impl ShellSandbox for LinuxSandbox {
    fn probe(&self) -> SandboxReport {
        match effective_backend() {
            Backend::LandlockSeccomp => SandboxReport {
                tier: SandboxTier::Full,
                mechanism: "landlock+seccomp",
                degradations: vec![],
            },
            Backend::LandlockOnly => SandboxReport {
                tier: SandboxTier::WriteOnly,
                mechanism: "landlock",
                degradations: vec![
                    "outbound network is NOT blocked (seccomp filter could not be installed)"
                        .to_string(),
                ],
            },
            Backend::Bubblewrap => SandboxReport {
                tier: SandboxTier::Full,
                mechanism: "bubblewrap",
                degradations: vec![],
            },
            Backend::None => SandboxReport::none(
                "none",
                "Landlock is not enabled in this kernel's `lsm=` list (or the sandbox helper is \
                 not wired into this binary), and `bubblewrap` is not installed. Install \
                 `bubblewrap` for a namespace sandbox, or boot with Landlock enabled",
            ),
        }
    }

    fn wrap(&self, policy: &SandboxPolicy, program: &str) -> Result<Wrapped, ShellSandboxError> {
        match effective_backend() {
            // Full tier: honour the policy's network setting (deny unless the
            // user opted into `BIOROUTER_SHELL_SANDBOX_NETWORK`).
            Backend::LandlockSeccomp => wrap_helper(policy, program, !policy.allow_network),
            // WriteOnly tier: seccomp is not installable on this host, so we must
            // NOT ask the helper to deny the network — it would try seccomp,
            // fail, and abort the command. Writes are still confined; the report
            // already tells the user the network is not denied.
            Backend::LandlockOnly => wrap_helper(policy, program, false),
            Backend::Bubblewrap => Ok(wrap_bubblewrap(policy, program)),
            Backend::None => Err(ShellSandboxError::Unavailable(
                "no Linux sandbox backend available on this host".to_string(),
            )),
        }
    }
}

/// `(current_exe, ["__br-sandbox", "--writable-root", R, …, "--deny-net", "--", program])`.
/// `deny_net` is decided by the caller from the effective tier, not just the
/// policy — a WriteOnly host cannot install seccomp, so it must never be asked.
fn wrap_helper(
    policy: &SandboxPolicy,
    program: &str,
    deny_net: bool,
) -> Result<Wrapped, ShellSandboxError> {
    let exe = std::env::current_exe()
        .map_err(|e| ShellSandboxError::Unavailable(format!("current_exe() unreadable: {e}")))?;
    let mut args = vec![HELPER_MARKER.to_string()];
    for root in &policy.writable_roots {
        args.push("--writable-root".to_string());
        args.push(root.display().to_string());
    }
    if deny_net {
        args.push("--deny-net".to_string());
    }
    args.push("--".to_string());
    args.push(program.to_string());
    Ok(Wrapped {
        program: exe.display().to_string(),
        prefix_args: args,
    })
}

/// `bwrap --ro-bind / / --proc /proc --dev /dev [--bind R R]* [--unshare-net] -- program`.
/// Bubblewrap is a pure argv wrapper (no helper), so it drops straight into
/// [`Wrapped`]. Full-filesystem read via `--ro-bind / /`; writable roots punch
/// read-write holes; `--unshare-net` denies network when the policy asks.
fn wrap_bubblewrap(policy: &SandboxPolicy, program: &str) -> Wrapped {
    let mut args: Vec<String> = vec![
        "--unshare-user".to_string(),
        "--unshare-pid".to_string(),
        "--die-with-parent".to_string(),
        "--ro-bind".to_string(),
        "/".to_string(),
        "/".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
    ];
    for root in &policy.writable_roots {
        let p = root.display().to_string();
        args.push("--bind".to_string());
        args.push(p.clone());
        args.push(p);
    }
    if !policy.allow_network {
        args.push("--unshare-net".to_string());
    }
    args.push("--".to_string());
    args.push(program.to_string());
    Wrapped {
        program: "bwrap".to_string(),
        prefix_args: args,
    }
}

// ---------------------------------------------------------------------------
// Helper process (runs in the re-exec'd child, single-threaded, before Tokio).
// ---------------------------------------------------------------------------

/// Call this as the **first line of `main()`** in every binary that may wrap a
/// shell command (`biorouter`, `biorouterd`). If argv[1] is the `__br-sandbox`
/// marker it applies the sandbox to the current process and `execve`s the target
/// program — **it never returns** in that case. Otherwise it returns immediately
/// and normal startup proceeds.
pub fn run_helper_if_invoked() {
    let mut args = std::env::args();
    let _exe = args.next();
    if args.next().as_deref() != Some(HELPER_MARKER) {
        return; // normal startup
    }

    // Parse: [--writable-root PATH]* [--deny-net] [--selftest] -- program args…
    // A tiny hand-rolled state machine (no external arg parser in this leaf
    // crate). `expect_root` remembers that the previous token was
    // `--writable-root` and this one is its value.
    let mut writable_roots: Vec<PathBuf> = Vec::new();
    let mut deny_net = false;
    let mut selftest = false;
    let mut program: Vec<String> = Vec::new();
    let mut after_sep = false;
    let mut expect_root = false;
    for a in args {
        if after_sep {
            program.push(a);
            continue;
        }
        if expect_root {
            writable_roots.push(PathBuf::from(a));
            expect_root = false;
            continue;
        }
        match a.as_str() {
            "--writable-root" => expect_root = true,
            "--deny-net" => deny_net = true,
            "--selftest" => selftest = true,
            "--" => after_sep = true,
            other => {
                eprintln!("br-sandbox: unexpected argument {other:?}");
                std::process::exit(64);
            }
        }
    }
    if expect_root {
        eprintln!("br-sandbox: --writable-root without a value");
        std::process::exit(64);
    }

    if selftest {
        // The self-test always probes both Landlock AND seccomp so probe() can
        // tell Full from WriteOnly, regardless of the `--deny-net` flag on this
        // particular invocation.
        let _ = deny_net;
        std::process::exit(selftest_exit_code());
    }

    // Real enforcement path. Set no-new-privs, then Landlock, then seccomp.
    if let Err(e) = set_no_new_privs() {
        eprintln!("br-sandbox: PR_SET_NO_NEW_PRIVS failed: {e}");
        std::process::exit(65);
    }
    if let Err(e) = apply_landlock(&writable_roots) {
        eprintln!("br-sandbox: landlock enforcement failed: {e}");
        std::process::exit(66);
    }
    if deny_net {
        if let Err(e) = apply_seccomp_net_deny() {
            eprintln!("br-sandbox: seccomp network filter failed: {e}");
            std::process::exit(67);
        }
    }
    if program.is_empty() {
        eprintln!("br-sandbox: no program to execute after `--`");
        std::process::exit(68);
    }
    exec_program(&program);
}

/// Apply the real restrictions to *this* process and report, via the exit code,
/// which took effect. Used by [`run_selftest`] to build the honest degradation
/// ladder without a version guess. Runs in a throwaway child, so restricting
/// self here is harmless.
fn selftest_exit_code() -> i32 {
    if set_no_new_privs().is_err() {
        return SELFTEST_NO_LANDLOCK; // cannot enforce anything unprivileged
    }
    if apply_landlock(&[]).is_err() {
        return SELFTEST_NO_LANDLOCK;
    }
    // Landlock enforced. Probe seccomp too so probe() can distinguish Full
    // (both) from WriteOnly (landlock only).
    if apply_seccomp_net_deny().is_err() {
        return SELFTEST_WRITE_ONLY;
    }
    SELFTEST_FULL
}

fn set_no_new_privs() -> Result<(), String> {
    // prctl(PR_SET_NO_NEW_PRIVS, 1) — required for unprivileged seccomp and for
    // Landlock's restrict_self on some kernels; harmless otherwise.
    let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

/// Confine writes to `writable_roots` (read/exec stay allowed everywhere).
/// Zero roots ⇒ writes denied everywhere — the generalized BR-64 zero-roots
/// invariant. Returns `Err` when Landlock is not actually enforced by the
/// kernel, so the helper exits non-zero rather than `execve`ing unconfined.
fn apply_landlock(writable_roots: &[PathBuf]) -> Result<(), String> {
    use landlock::{
        Access, AccessFs, CompatLevel, Compatible, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr, RulesetStatus, ABI,
    };

    let abi = ABI::V1;
    // Handle only WRITE-side accesses: reads/exec are left unhandled and thus
    // allowed everywhere. Best-effort compat so an ABI-1 kernel still confines
    // writes (later ABIs add refinements we take opportunistically).
    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_write(abi))
        .map_err(|e| format!("handle_access: {e}"))?
        .create()
        .map_err(|e| format!("create: {e}"))?;

    for root in writable_roots {
        match PathFd::new(root) {
            Ok(fd) => {
                ruleset = ruleset
                    .add_rule(PathBeneath::new(fd, AccessFs::from_all(abi)))
                    .map_err(|e| format!("add_rule({}): {e}", root.display()))?;
            }
            // A writable root that does not exist yet cannot be opened; skip it
            // rather than fail the whole sandbox (matches Seatbelt's tolerant
            // canonicalize() fallback).
            Err(_) => continue,
        }
    }

    let status = ruleset
        .restrict_self()
        .map_err(|e| format!("restrict_self: {e}"))?;

    match status.ruleset {
        RulesetStatus::NotEnforced => {
            Err("Landlock is not enforced by this kernel (not in `lsm=`?)".to_string())
        }
        // Fully or partially enforced both give real write confinement.
        RulesetStatus::FullyEnforced | RulesetStatus::PartiallyEnforced => Ok(()),
    }
}

/// Deny outbound network by filtering `socket(2)` on its scalar `domain`
/// argument. seccomp cannot dereference the `sockaddr` a later `connect(2)`
/// passes, so the domain filter on `socket` is the robust primitive. We deny the
/// internet/raw families (`AF_INET`, `AF_INET6`, `AF_PACKET`) with a clean
/// `EPERM` (not `SIGSYS`, so programs print a sane error) and **allow
/// `AF_UNIX`** — without it D-Bus/X11/nscd break and the sandbox looks like a
/// Biorouter bug (the exact failure Codex hit). `AF_NETLINK` is also left
/// allowed: it carries no outbound internet traffic and NSS/`getaddrinfo` use it
/// to enumerate interfaces. Note we deliberately do NOT unconditionally deny
/// `connect`/`bind` — those take a pointer we cannot inspect, so a blanket deny
/// would also kill legitimate `AF_UNIX` connects.
fn apply_seccomp_net_deny() -> Result<(), String> {
    use seccompiler::{
        apply_filter, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition,
        SeccompFilter, SeccompRule, TargetArch,
    };
    use std::collections::BTreeMap;

    let denied_domains: [i32; 3] = [libc::AF_INET, libc::AF_INET6, libc::AF_PACKET];
    let mut socket_rules: Vec<SeccompRule> = Vec::new();
    for domain in denied_domains {
        let cond = SeccompCondition::new(
            0, // socket(2) arg0 = domain
            SeccompCmpArgLen::Dword,
            SeccompCmpOp::Eq,
            domain as u64,
        )
        .map_err(|e| format!("seccomp condition: {e}"))?;
        socket_rules.push(SeccompRule::new(vec![cond]).map_err(|e| format!("seccomp rule: {e}"))?);
    }

    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    rules.insert(libc::SYS_socket, socket_rules);

    let target: TargetArch = TargetArch::try_from(std::env::consts::ARCH)
        .map_err(|e| format!("unsupported seccomp arch {}: {e}", std::env::consts::ARCH))?;

    let filter = SeccompFilter::new(
        rules,
        // Default (no rule matched, or syscall not listed): allow.
        SeccompAction::Allow,
        // A listed rule matched (a denied socket domain): EPERM.
        SeccompAction::Errno(libc::EPERM as u32),
        target,
    )
    .map_err(|e| format!("seccomp filter: {e}"))?;

    let program: seccompiler::BpfProgram = filter
        .try_into()
        .map_err(|e| format!("seccomp compile: {e}"))?;
    apply_filter(&program).map_err(|e| format!("seccomp apply: {e}"))?;
    Ok(())
}

/// `execvp` the target program in place. Only returns on failure.
fn exec_program(program: &[String]) -> ! {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(&program[0])
        .args(&program[1..])
        .exec();
    eprintln!("br-sandbox: exec {:?} failed: {err}", program[0]);
    std::process::exit(69);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_argv_has_marker_roots_and_deny_net() {
        let policy = SandboxPolicy::new(vec![PathBuf::from("/work"), PathBuf::from("/tmp")])
            .with_network(false);
        // wrap_helper resolves current_exe(); the test binary always has one.
        let w = wrap_helper(&policy, "/bin/bash", true).expect("wrap");
        assert_eq!(w.prefix_args[0], HELPER_MARKER);
        assert!(w.prefix_args.iter().any(|a| a == "--deny-net"));
        assert!(w
            .prefix_args
            .windows(2)
            .any(|p| p[0] == "--writable-root" && p[1] == "/work"));
        // Program is the last arg, after the `--` separator.
        assert_eq!(w.prefix_args.last().unwrap(), "/bin/bash");
        let sep = w.prefix_args.iter().position(|a| a == "--").unwrap();
        assert!(sep < w.prefix_args.len() - 1);
    }

    #[test]
    fn helper_argv_omits_deny_net_when_caller_says_so() {
        // The caller (LandlockOnly tier, or a network-allowed policy) passes
        // deny_net = false; the helper then skips the seccomp step entirely.
        let policy = SandboxPolicy::new(vec![PathBuf::from("/work")]).with_network(true);
        let w = wrap_helper(&policy, "/bin/sh", false).expect("wrap");
        assert!(!w.prefix_args.iter().any(|a| a == "--deny-net"));
    }

    #[test]
    fn bubblewrap_argv_binds_roots_and_unshares_net() {
        let policy = SandboxPolicy::new(vec![PathBuf::from("/work")]).with_network(false);
        let w = wrap_bubblewrap(&policy, "/bin/bash");
        assert_eq!(w.program, "bwrap");
        assert!(w
            .prefix_args
            .windows(3)
            .any(|s| s[0] == "--bind" && s[1] == "/work" && s[2] == "/work"));
        assert!(w.prefix_args.iter().any(|a| a == "--unshare-net"));
        assert!(w
            .prefix_args
            .windows(3)
            .any(|s| s[0] == "--ro-bind" && s[1] == "/" && s[2] == "/"));
        assert_eq!(w.prefix_args.last().unwrap(), "/bin/bash");
    }

    #[test]
    fn bubblewrap_argv_keeps_net_when_allowed() {
        let policy = SandboxPolicy::new(vec![]).with_network(true);
        let w = wrap_bubblewrap(&policy, "/bin/sh");
        assert!(!w.prefix_args.iter().any(|a| a == "--unshare-net"));
    }

    /// Live enforcement: only runs when the host actually reaches `Full`
    /// (Landlock in `lsm=` or bubblewrap present) AND the helper marker is wired
    /// into this test binary. The sandbox crate's test binary does NOT wire the
    /// marker, so `run_helper_if_invoked` is a no-op there; the real enforcement
    /// coverage lives in the CI matrix (`ubuntu-24.04` + the Docker degradation
    /// grid) against `biorouter`/`biorouterd`, which do wire it. This test just
    /// asserts `probe()` never panics and is internally consistent.
    #[test]
    fn probe_is_consistent() {
        let report = LinuxSandbox::new().probe();
        assert!(!report.mechanism.is_empty());
        match report.tier {
            SandboxTier::Full => assert!(report.degradations.is_empty()),
            SandboxTier::WriteOnly => assert!(!report.degradations.is_empty()),
            SandboxTier::None => assert!(!report.degradations.is_empty()),
            SandboxTier::ContainmentOnly => {
                panic!("Linux never reports ContainmentOnly")
            }
        }
    }
}
