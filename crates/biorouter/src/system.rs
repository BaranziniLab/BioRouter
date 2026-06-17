//! Shared system-prerequisite checks and CLI installation — the single source
//! of truth used by both the terminal (`biorouter doctor` / `setup-path`) and,
//! via the server route, the desktop app's dependency setup. Define a
//! prerequisite once in [`specs`] and every front-end picks it up.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

/// One external tool Biorouter can use, plus how to detect and install it.
struct Spec {
    name: &'static str,
    display_name: &'static str,
    /// Probe candidates tried in order until one reports a version.
    probes: &'static [(&'static str, &'static [&'static str])],
    /// Required tools block core flows; recommended ones unlock optional ones.
    required: bool,
    doc_url: &'static str,
    purpose: &'static str,
}

/// The catalog of prerequisites. **Edit here** to change what every front-end
/// checks for.
fn specs() -> &'static [Spec] {
    &[
        Spec {
            name: "git",
            display_name: "Git",
            probes: &[("git", &["--version"])],
            required: true,
            doc_url: "https://git-scm.com/downloads",
            purpose: "Clone and version repositories and knowledge bases",
        },
        Spec {
            name: "uv",
            display_name: "uv (Python toolchain)",
            probes: &[("uv", &["--version"])],
            required: true,
            doc_url: "https://docs.astral.sh/uv/",
            purpose: "Run Python MCP extensions and install .brxt bundles",
        },
        Spec {
            name: "python",
            display_name: "Python 3",
            probes: &[("python3", &["--version"]), ("python", &["--version"])],
            required: false,
            doc_url: "https://www.python.org/downloads/",
            purpose: "Back Python-based extensions (uv can manage this for you)",
        },
        Spec {
            name: "node",
            display_name: "Node.js (npx)",
            probes: &[("node", &["--version"])],
            required: false,
            doc_url: "https://nodejs.org/",
            purpose: "Run npx-based MCP servers such as Playwright",
        },
        Spec {
            name: "aws",
            display_name: "AWS CLI",
            probes: &[("aws", &["--version"])],
            required: false,
            doc_url: "https://aws.amazon.com/cli/",
            purpose: "Optional — UCSF Bedrock / Amazon Bedrock credentials",
        },
        Spec {
            name: "llama-server",
            display_name: "llama-server (local models)",
            probes: &[("llama-server", &["--version"])],
            required: false,
            doc_url: "https://github.com/ggml-org/llama.cpp/releases",
            purpose: "Run built-in local models (bundled with the desktop app)",
        },
        // `rustc -vV` is a *health* probe, not just a presence probe: a broken
        // Homebrew rust (whose `rustc` aborts after an `llvm` upgrade) exits
        // non-zero here and is correctly reported as unavailable, steering the
        // user to rustup. Some Python extension deps (e.g. cryptography ≥49,
        // which dropped Intel-Mac wheels) are compiled from source and need it.
        Spec {
            name: "rust",
            display_name: "Rust toolchain (rustc)",
            probes: &[("rustc", &["-vV"])],
            required: false,
            doc_url: "https://rustup.rs",
            purpose: "Compile source-only Python extension deps (e.g. cryptography on Intel Macs)",
        },
    ]
}

/// The detected state of a single prerequisite. This is the shape both
/// `biorouter doctor --json` and the desktop dependency setup consume.
#[derive(Clone, Debug, Serialize)]
pub struct DependencyStatus {
    pub name: String,
    pub display_name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub required: bool,
    pub purpose: String,
    pub doc_url: String,
    /// A shell command that installs it on the current OS, when one is known.
    pub install_command: Option<String>,
    /// True when `install_command` needs elevated privileges (Linux apt/dnf).
    pub requires_sudo: bool,
    /// A download/instructions page when there is no one-line install.
    pub download_url: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum LinuxDistro {
    Deb,
    Rpm,
    Unknown,
}

fn linux_distro() -> LinuxDistro {
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        let c = content.to_lowercase();
        if c.contains("ubuntu") || c.contains("debian") {
            return LinuxDistro::Deb;
        }
        if ["fedora", "rhel", "centos", "rocky", "alma"]
            .iter()
            .any(|d| c.contains(d))
        {
            return LinuxDistro::Rpm;
        }
    }
    if Path::new("/etc/debian_version").exists() {
        return LinuxDistro::Deb;
    }
    if Path::new("/etc/redhat-release").exists() {
        return LinuxDistro::Rpm;
    }
    LinuxDistro::Unknown
}

/// How to install a prerequisite on the current OS.
struct InstallInfo {
    command: Option<String>,
    requires_sudo: bool,
    download_url: Option<String>,
}

fn s(x: &str) -> Option<String> {
    Some(x.to_string())
}

fn install_info(name: &str) -> InstallInfo {
    let none = InstallInfo {
        command: None,
        requires_sudo: false,
        download_url: None,
    };
    if cfg!(target_os = "macos") {
        let (cmd, url) = match name {
            "git" => ("xcode-select --install", "https://git-scm.com/download/mac"),
            "python" => ("brew install python3", "https://www.python.org/downloads/"),
            "uv" => (
                "curl -LsSf https://astral.sh/uv/install.sh | sh",
                "https://docs.astral.sh/uv/",
            ),
            "node" => ("brew install node", "https://nodejs.org/en/download"),
            "aws" => ("brew install awscli", "https://aws.amazon.com/cli/"),
            "llama-server" => (
                "brew install llama.cpp",
                "https://github.com/ggml-org/llama.cpp/releases",
            ),
            // rustup, NOT `brew install rust` — Homebrew's rust links libLLVM
            // dynamically and breaks on llvm upgrades; rustup is self-contained.
            // `sh -s -- -y`: the installer is piped (no controlling TTY), so
            // rustup-init aborts with "Unable to run interactively. Run with -y
            // to accept defaults" unless we pass `-y` through to it.
            "rust" => (
                "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
                "https://rustup.rs",
            ),
            _ => return none,
        };
        InstallInfo {
            command: s(cmd),
            requires_sudo: false,
            download_url: s(url),
        }
    } else if cfg!(target_os = "windows") {
        // Run unattended: without these flags winget blocks on the source/package
        // agreement prompt the first time it's used, which fails in the app's
        // non-interactive shell (the same class of failure as rustup needing -y).
        const WG: &str =
            "--accept-package-agreements --accept-source-agreements --disable-interactivity";
        let (cmd, url): (String, &str) = match name {
            "git" => (format!("winget install --id Git.Git -e --source winget {WG}"), "https://git-scm.com/download/win"),
            "python" => (format!("winget install --id Python.Python.3 -e --source winget {WG}"), "https://www.python.org/downloads/"),
            "uv" => (
                "powershell -ExecutionPolicy ByPass -c \"irm https://astral.sh/uv/install.ps1 | iex\"".to_string(),
                "https://docs.astral.sh/uv/",
            ),
            "node" => (format!("winget install --id OpenJS.NodeJS -e --source winget {WG}"), "https://nodejs.org/en/download"),
            "aws" => (format!("winget install Amazon.AWSCLI {WG}"), "https://aws.amazon.com/cli/"),
            "llama-server" => (
                format!("winget install --id ggml.llamacpp -e --source winget {WG}"),
                "https://github.com/ggml-org/llama.cpp/releases",
            ),
            "rust" => (format!("winget install --id Rustlang.Rustup -e --source winget {WG}"), "https://rustup.rs"),
            _ => return none,
        };
        InstallInfo {
            command: Some(cmd),
            requires_sudo: false,
            download_url: s(url),
        }
    } else {
        // Linux
        if name == "uv" {
            return InstallInfo {
                command: s("curl -LsSf https://astral.sh/uv/install.sh | sh"),
                requires_sudo: false,
                download_url: s("https://docs.astral.sh/uv/"),
            };
        }
        if name == "llama-server" {
            // No standard distro package; prebuilt binaries on GitHub releases.
            return InstallInfo {
                command: None,
                requires_sudo: false,
                download_url: s("https://github.com/ggml-org/llama.cpp/releases"),
            };
        }
        if name == "rust" {
            // rustup over the distro `rustc`, which is often too old for modern
            // Rust-backed wheels. `sh -s -- -y` runs it non-interactively (the
            // installer is piped, so it has no TTY and would otherwise abort).
            return InstallInfo {
                command: s("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"),
                requires_sudo: false,
                download_url: s("https://rustup.rs"),
            };
        }
        let pkg = match name {
            "git" => "git",
            "python" => "python3",
            "node" => "nodejs npm",
            "aws" => "awscli",
            _ => return none,
        };
        let url = match name {
            "aws" => Some("https://aws.amazon.com/cli/".to_string()),
            _ => None,
        };
        match linux_distro() {
            LinuxDistro::Deb => InstallInfo {
                command: s(&format!("sudo apt-get install -y {pkg}")),
                requires_sudo: true,
                download_url: url,
            },
            LinuxDistro::Rpm => InstallInfo {
                command: s(&format!("sudo dnf install -y {pkg}")),
                requires_sudo: true,
                download_url: url,
            },
            LinuxDistro::Unknown => InstallInfo {
                command: None,
                requires_sudo: false,
                download_url: url,
            },
        }
    }
}

/// Probe one command; returns its first version line on success.
fn probe(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let pick = |bytes: &[u8]| {
        String::from_utf8_lossy(bytes)
            .lines()
            .next()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
    };
    pick(&output.stdout)
        .or_else(|| pick(&output.stderr))
        .or(Some(String::new()))
}

/// A known per-OS install command for a prerequisite, if any.
pub fn install_command(name: &str) -> Option<String> {
    install_info(name).command
}

/// Check a prerequisite by trying each probe in turn.
fn check_spec(spec: &Spec) -> DependencyStatus {
    let mut version = spec.probes.iter().find_map(|(cmd, args)| probe(cmd, args));
    // llama-server usually isn't on PATH: the desktop app bundles it next to
    // the Biorouter binaries. Fall back to the sidecar's resolution logic.
    if version.is_none() && spec.name == "llama-server" {
        if let Some(bin) = crate::providers::llamacpp_sidecar::find_binary() {
            version = probe(&bin.display().to_string(), &["--version"]);
        }
    }
    let install = install_info(spec.name);
    DependencyStatus {
        name: spec.name.to_string(),
        display_name: spec.display_name.to_string(),
        installed: version.is_some(),
        version,
        required: spec.required,
        purpose: spec.purpose.to_string(),
        doc_url: spec.doc_url.to_string(),
        install_command: install.command,
        requires_sudo: install.requires_sudo,
        download_url: install.download_url,
    }
}

/// Check every prerequisite. This is what `biorouter doctor` and the desktop
/// dependency setup both consume.
pub fn check_all() -> Vec<DependencyStatus> {
    specs().iter().map(check_spec).collect()
}

/// Check a single prerequisite by name (e.g. `"uv"`). Returns `None` if there is
/// no such spec.
pub fn status_of(name: &str) -> Option<DependencyStatus> {
    specs().iter().find(|s| s.name == name).map(check_spec)
}

// ── self-update check ────────────────────────────────────────────────────────

/// Result of comparing the running version against the latest GitHub release.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub url: String,
}

/// True if dotted version `a` is strictly newer than `b` (numeric, segment-wise;
/// pre-release suffixes compared as 0).
fn version_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| {
        s.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    let (va, vb) = (parse(a), parse(b));
    for i in 0..va.len().max(vb.len()) {
        let (x, y) = (
            va.get(i).copied().unwrap_or(0),
            vb.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

/// Best-effort check for a newer release on GitHub. Returns `None` on any
/// network/parse error (offline-friendly); times out after 5s.
pub async fn check_for_update() -> Option<UpdateInfo> {
    const CURRENT: &str = env!("CARGO_PKG_VERSION");
    let client = reqwest::Client::builder()
        .user_agent(concat!("biorouter/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;
    let resp = client
        .get("https://api.github.com/repos/BaranziniLab/BioRouter/releases/latest")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let latest = json
        .get("tag_name")?
        .as_str()?
        .trim_start_matches('v')
        .to_string();
    let url = json
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://github.com/BaranziniLab/BioRouter/releases")
        .to_string();
    Some(UpdateInfo {
        update_available: version_newer(&latest, CURRENT),
        current: CURRENT.to_string(),
        latest,
        url,
    })
}

// ── CLI-on-PATH install ──────────────────────────────────────────────────────

/// Where (if anywhere) a `biorouter` executable currently resolves on PATH.
pub fn biorouter_on_path() -> Option<PathBuf> {
    let exe = if cfg!(target_os = "windows") {
        "biorouter.exe"
    } else {
        "biorouter"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(exe);
            candidate.is_file().then_some(candidate)
        })
    })
}

fn dir_on_path(dir: &Path) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d == dir))
        .unwrap_or(false)
}

/// Outcome of installing the CLI onto PATH.
#[derive(Debug, Serialize)]
pub struct CliInstall {
    pub link: PathBuf,
    pub target_dir: PathBuf,
    pub on_path: bool,
}

/// Candidate install directories, best first.
fn install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(target_os = "windows") {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("BioRouter").join("bin"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/local/bin"));
        if let Ok(home) = etcetera::home_dir() {
            dirs.push(home.join(".local").join("bin"));
            dirs.push(home.join("bin"));
        }
    }
    dirs
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".biorouter-write-test");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Install (symlink on Unix, copy on Windows) `source` onto a PATH directory so
/// `biorouter` is callable from any terminal. `source` is normally the running
/// executable (CLI) or the bundled binary (desktop). Returns where it landed.
pub fn install_cli(source: &Path) -> anyhow::Result<CliInstall> {
    let source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    let exe_name = if cfg!(target_os = "windows") {
        "biorouter.exe"
    } else {
        "biorouter"
    };

    // If a `biorouter` already resolves on PATH, that's the binary the user's
    // shell actually runs — overwrite *it* so an update/reinstall takes effect.
    // Otherwise a stale copy that sits earlier on PATH (e.g. an old 1.x in
    // /usr/local/bin or a user dir) keeps shadowing a fresh symlink we'd drop in
    // ~/.local/bin, and the "update" silently never applies. Only do this when
    // the existing location is writable and isn't the source itself.
    let existing_on_path = biorouter_on_path().and_then(|p| {
        let dir = p.parent()?.to_path_buf();
        let canon = p.canonicalize().unwrap_or(p);
        (dir.is_dir() && is_writable(&dir) && canon != source).then_some(dir)
    });

    // Prefer a writable, already-on-PATH dir; else the first writable dir; else
    // the first candidate (creating it if needed).
    let dirs = install_dirs();
    let target_dir = existing_on_path
        .or_else(|| {
            dirs.iter()
                .find(|d| d.is_dir() && is_writable(d) && dir_on_path(d))
                .or_else(|| dirs.iter().find(|d| d.is_dir() && is_writable(d)))
                .cloned()
        })
        .or_else(|| dirs.first().cloned())
        .ok_or_else(|| anyhow::anyhow!("No suitable install directory found"))?;

    std::fs::create_dir_all(&target_dir)
        .map_err(|e| anyhow::anyhow!("Could not create {}: {}", target_dir.display(), e))?;
    if !is_writable(&target_dir) {
        anyhow::bail!(
            "{} is not writable. Re-run with elevated permissions, or add a writable directory to PATH.",
            target_dir.display()
        );
    }

    let link = target_dir.join(exe_name);
    if link.exists() || link.symlink_metadata().is_ok() {
        let _ = std::fs::remove_file(&link);
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(&source, &link).map_err(|e| {
        anyhow::anyhow!(
            "Failed to link {} -> {}: {}",
            link.display(),
            source.display(),
            e
        )
    })?;
    #[cfg(not(unix))]
    std::fs::copy(&source, &link).map(|_| ()).map_err(|e| {
        anyhow::anyhow!(
            "Failed to copy {} -> {}: {}",
            source.display(),
            link.display(),
            e
        )
    })?;

    Ok(CliInstall {
        on_path: dir_on_path(&target_dir),
        link,
        target_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::{install_command, specs, status_of};

    #[test]
    fn rust_spec_exists_and_is_optional() {
        let spec = specs()
            .iter()
            .find(|s| s.name == "rust")
            .expect("rust spec");
        assert!(
            !spec.required,
            "rust must be optional, not a blocking prereq"
        );
        // Health probe, not a presence probe: -vV exits non-zero on a broken
        // toolchain so it reads as unavailable.
        assert_eq!(spec.probes[0], ("rustc", &["-vV"][..]));
    }

    #[test]
    fn rust_install_command_uses_rustup_not_brew() {
        let cmd = install_command("rust").expect("rust install command on this OS");
        assert!(cmd.contains("rustup") || cmd.contains("Rustup"));
        assert!(!cmd.contains("brew install rust"));
    }

    #[test]
    fn rust_install_command_is_non_interactive() {
        // The app runs installers without a TTY, so the rustup-init pipe must
        // accept defaults (`-y`) and winget must not block on its agreement
        // prompts — otherwise the install aborts with "Unable to run
        // interactively" (Unix) / a hung agreement prompt (Windows).
        let cmd = install_command("rust").expect("rust install command on this OS");
        if cmd.contains("rustup.rs") {
            assert!(
                cmd.contains("-y"),
                "piped rustup install must pass -y: {cmd}"
            );
        } else {
            assert!(
                cmd.contains("--accept-source-agreements"),
                "winget install must accept agreements non-interactively: {cmd}"
            );
        }
    }

    #[test]
    fn status_of_rust_resolves() {
        // Should return a status (installed or not) rather than None.
        assert!(status_of("rust").is_some());
    }
}
