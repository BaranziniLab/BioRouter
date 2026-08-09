//! `biorouter extension` subcommands — install a `.brxt` bundle, list installed
//! extensions, and remove one. Mirrors the desktop GUI's `.brxt` flow: extract
//! into `~/.config/biorouter/extensions/<name>/`, build the Python venv with
//! `uv sync`, store any secret env vars in the keyring, and register a stdio
//! extension that launches via `uv run --directory <dir> <entry_point>`.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use biorouter::agents::extension::{Envs, ExtensionConfig};
use biorouter::config::extensions::{
    get_all_extensions, name_to_key, remove_extension, set_extension, ExtensionEntry,
};
use biorouter::config::paths::Paths;
use biorouter::config::Config;
use console::{style, Color};
use serde::Deserialize;

const ACCENT: Color = Color::Color256(137);

#[derive(Debug, Deserialize)]
struct BrxtManifest {
    name: String,
    display_name: String,
    description: String,
    #[allow(dead_code)]
    version: String,
    entry_point: String,
    #[allow(dead_code)]
    repository: String,
    #[serde(default)]
    env_vars: Vec<BrxtEnvVar>,
}

#[derive(Debug, Deserialize)]
struct BrxtEnvVar {
    key: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    secret: bool,
}

fn extensions_root() -> PathBuf {
    Paths::config_dir().join("extensions")
}

/// Human-readable transport label for an extension config.
fn kind_str(c: &ExtensionConfig) -> &'static str {
    match c {
        ExtensionConfig::Sse { .. } => "sse",
        ExtensionConfig::StreamableHttp { .. } => "http",
        ExtensionConfig::Stdio { .. } => "stdio",
        ExtensionConfig::Builtin { .. } => "builtin",
        ExtensionConfig::Platform { .. } => "platform",
        ExtensionConfig::Frontend { .. } => "frontend",
        ExtensionConfig::InlinePython { .. } => "inline-python",
    }
}

/// Parse repeated `KEY=VALUE` flags into a map, erroring on malformed entries.
fn parse_kv(pairs: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for pair in pairs {
        let (k, v) = pair
            .split_once('=')
            .ok_or_else(|| anyhow!("Invalid KEY=VALUE pair: '{}'", pair))?;
        map.insert(k.trim().to_string(), v.to_string());
    }
    Ok(map)
}

// ──────────────────────────────────────────────────────────────────────────────
// install
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_install(
    path: PathBuf,
    env_flags: Vec<String>,
    secret_flags: Vec<String>,
    no_enable: bool,
) -> Result<()> {
    if !path.exists() {
        bail!("File not found: {}", path.display());
    }

    // 0. Gate on `uv`: installing a .brxt builds a Python venv with `uv sync`,
    // so refuse up front (with an actionable message) rather than failing late.
    if biorouter::system::status_of("uv")
        .map(|d| !d.installed)
        .unwrap_or(true)
    {
        let cmd = biorouter::system::install_command("uv").unwrap_or_default();
        bail!(
            "`uv` is required to install .brxt extensions, but it was not found.\n  \
             Install it:  {}\n  \
             Then re-run, or run `biorouter doctor` to check prerequisites.",
            cmd
        );
    }

    // 1. Open and validate the bundle.
    let file = fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| anyhow!("Not a valid .brxt (zip) bundle: {}", e))?;

    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    let require = |pred: bool, msg: &str| -> Result<()> {
        if pred {
            Ok(())
        } else {
            bail!("{}: not a valid .brxt bundle", msg)
        }
    };
    require(
        names.iter().any(|n| n == "manifest.json"),
        "Missing manifest.json",
    )?;
    require(
        names.iter().any(|n| n.eq_ignore_ascii_case("readme.md")),
        "Missing README.md",
    )?;
    require(
        names.iter().any(|n| n == "pyproject.toml"),
        "Missing pyproject.toml",
    )?;
    require(
        names.iter().any(|n| n.starts_with("src/")),
        "Missing src/ directory",
    )?;

    // 2. Parse the manifest.
    let manifest: BrxtManifest = {
        let mut entry = archive
            .by_name("manifest.json")
            .map_err(|e| anyhow!("Could not read manifest.json: {}", e))?;
        let mut buf = String::new();
        entry.read_to_string(&mut buf)?;
        serde_json::from_str(&buf).map_err(|e| anyhow!("Invalid manifest.json: {}", e))?
    };

    // 3. Extract into ~/.config/biorouter/extensions/<name>/.
    let install_dir = extensions_root().join(&manifest.name);
    fs::create_dir_all(&install_dir)
        .with_context(|| format!("creating {}", install_dir.display()))?;
    extract_zip(&mut archive, &install_dir)?;

    println!(
        "  {} extracted {} {}",
        style("·").dim(),
        style(&manifest.display_name).bold(),
        style(format!("→ {}", install_dir.display())).dim()
    );

    // 4. Build the Python venv (matches the GUI; requires `uv` on PATH).
    run_uv_sync(&install_dir)?;
    println!("  {} built virtual environment (uv sync)", style("·").dim());

    // 5. Resolve env vars: secrets → keyring (+ env_keys), the rest → envs map.
    let provided_env = parse_kv(&env_flags)?;
    let provided_secret = parse_kv(&secret_flags)?;

    let mut envs: HashMap<String, String> = HashMap::new();
    let mut env_keys: Vec<String> = Vec::new();
    let config = Config::global();

    for var in &manifest.env_vars {
        if let Some(val) = provided_secret.get(&var.key).or_else(|| {
            // A declared-secret var passed via --env is still treated as secret.
            if var.secret {
                provided_env.get(&var.key)
            } else {
                None
            }
        }) {
            config
                .set_secret(&var.key, &val)
                .map_err(|e| anyhow!("Failed to store secret '{}': {}", var.key, e))?;
            env_keys.push(var.key.clone());
        } else if let Some(val) = provided_env.get(&var.key) {
            envs.insert(var.key.clone(), val.clone());
        } else if var.required {
            println!(
                "  {} required env var {} not provided. Set it later with `--env {}=...`",
                style("⚠").yellow(),
                style(&var.key).yellow(),
                var.key
            );
        }
    }
    // Allow ad-hoc envs/secrets not declared in the manifest too.
    for (k, v) in &provided_env {
        if !manifest.env_vars.iter().any(|e| &e.key == k) {
            envs.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &provided_secret {
        if !manifest.env_vars.iter().any(|e| &e.key == k) {
            config.set_secret(k, &v).ok();
            env_keys.push(k.clone());
        }
    }

    // 6. Register the stdio extension.
    let config_entry = ExtensionConfig::Stdio {
        name: manifest.name.clone(),
        description: manifest.description.clone(),
        cmd: "uv".to_string(),
        args: vec![
            "run".to_string(),
            "--directory".to_string(),
            install_dir.display().to_string(),
            manifest.entry_point.clone(),
        ],
        envs: Envs::new(envs),
        env_keys,
        timeout: Some(300),
        bundled: None,
        available_tools: Vec::new(),
    };
    // Issue #56 Task 43 (DR-23): no provenance is recorded here, deliberately.
    // This subcommand installs a `.brxt` from a local path, which carries no
    // BAAM registry id — the id exists only where a marketplace install happens,
    // which today is the desktop's Browse Extensions flow. With nothing to key
    // on, `classify_extension` falls back to the config-name join, i.e. exactly
    // the behaviour that shipped before that task. `privacy::provenance::record`
    // is the writer to call if this path ever learns an id.
    set_extension(ExtensionEntry {
        enabled: !no_enable,
        config: config_entry,
    });

    let state = if no_enable {
        "installed"
    } else {
        "installed and enabled"
    };
    println!(
        "  {} {} {}",
        style("✓").green(),
        style(&manifest.display_name).fg(ACCENT).bold(),
        style(state).dim()
    );
    Ok(())
}

/// Extract every file in the archive under `dest`, guarding against zip-slip.
fn extract_zip(archive: &mut zip::ZipArchive<fs::File>, dest: &Path) -> Result<()> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name().map(Path::to_path_buf) else {
            bail!("Unsafe path in bundle: {}", entry.name());
        };
        let out_path = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&out_path)
            .with_context(|| format!("writing {}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

/// Run `uv sync` in `dir`, surfacing a clear, actionable error if `uv` is
/// missing or the sync fails.
fn run_uv_sync(dir: &Path) -> Result<()> {
    let spinner = cliclack::spinner();
    spinner.start("building virtual environment (uv sync)...");
    let output = Command::new("uv").arg("sync").current_dir(dir).output();
    spinner.stop("");

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let detail = String::from_utf8_lossy(&out.stderr);
            // uv puts the root cause and its `help:` dependency-chain line at
            // the END of stderr, so keep the tail, not the head.
            let lines: Vec<&str> = detail.trim().lines().collect();
            let tail = if lines.len() > 15 {
                format!("…\n{}", lines[lines.len() - 15..].join("\n"))
            } else {
                lines.join("\n")
            };
            let hint = uv_sync_hint(&detail)
                .map(|h| format!("\n\nhint: {h}"))
                .unwrap_or_default();
            bail!("uv sync failed:\n{}{}", tail, hint)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "`uv` was not found on your PATH. Install it from https://docs.astral.sh/uv/ \
                 and re-run, or extract the bundle and build the venv manually."
            )
        }
        Err(e) => bail!("Failed to run uv sync: {}", e),
    }
}

/// Map well-known `uv sync` failure signatures to an actionable hint appended
/// below the raw output. Checks run most-specific first.
fn uv_sync_hint(stderr: &str) -> Option<&'static str> {
    if stderr.contains("Symbol not found") && stderr.contains("librustc_driver") {
        // Homebrew's `rust` dynamically links `libLLVM.dylib`; when `llvm` is
        // upgraded the ABI mismatches and `rustc` aborts. `brew upgrade rust`
        // does NOT reliably fix this (there may be no rebuilt bottle yet), so
        // steer users to the self-contained rustup toolchain and tell them to
        // remove the Homebrew one so it wins on PATH.
        Some(
            "your Homebrew Rust toolchain is broken. `rustc` aborts because Homebrew's \
             `llvm` was upgraded out from under it (a known Homebrew issue). \
             `brew upgrade rust` usually does NOT fix this. Install the self-contained \
             rustup toolchain and remove the Homebrew one so it takes priority:\n    \
             brew uninstall rust\n    \
             curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\n  \
             then fully restart Biorouter and retry.",
        )
    } else if stderr.contains("cryptography") && cryptography_built_from_source(stderr) {
        // cryptography ≥49 (2026-06-12) dropped x86_64 macOS wheels, so Intel
        // Macs must compile it (it is a Rust/maturin project) instead of
        // downloading a wheel.
        Some(
            "`cryptography` ≥49 no longer ships x86_64 (Intel) macOS wheels, so on an \
             Intel Mac it must be compiled from source — which needs a Rust toolchain. \
             Install rustup (https://rustup.rs) and retry, or ask the extension author \
             to cap `cryptography<49` (the last series with Intel-Mac wheels).",
        )
    } else if stderr.contains("maturin") || stderr.contains("rustc") {
        Some(
            "a dependency has no prebuilt package for your platform, so it was compiled \
             from source, which needs a working Rust toolchain. Install one via \
             https://rustup.rs (or repair your existing install) and retry.",
        )
    } else if stderr.contains("Failed to build") {
        Some(
            "a dependency has no prebuilt package for your platform, so uv tried to \
             compile it from source. Make sure a compiler toolchain is installed, or ask \
             the extension author to pin versions that ship prebuilt wheels.",
        )
    } else {
        None
    }
}

/// True when stderr indicates `cryptography` was being built from source
/// (rather than failing for some unrelated reason that merely mentions it).
fn cryptography_built_from_source(stderr: &str) -> bool {
    stderr.contains("Failed to build `cryptography")
        || stderr.contains("Building cryptography")
        || (stderr.contains("cryptography") && stderr.contains("maturin"))
}

// ──────────────────────────────────────────────────────────────────────────────
// list
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_list(format: &str) -> Result<()> {
    let entries = get_all_extensions();

    if format == "json" {
        let items: Vec<_> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.config.name(),
                    "enabled": e.enabled,
                    "type": kind_str(&e.config),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    println!("  {} {}", style("▌").fg(ACCENT), style("Extensions").bold());
    if entries.is_empty() {
        println!("    {}", style("none configured").dim());
        return Ok(());
    }
    let width = entries
        .iter()
        .map(|e| e.config.name().len())
        .max()
        .unwrap_or(0);
    for entry in &entries {
        let dot = if entry.enabled {
            style("●").green().to_string()
        } else {
            style("○").dim().to_string()
        };
        println!(
            "    {} {:<width$}  {}",
            dot,
            style(entry.config.name()).bold(),
            style(kind_str(&entry.config)).dim(),
            width = width
        );
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// remove
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_remove(name: String, purge: bool) -> Result<()> {
    let key = name_to_key(&name);
    let existed = get_all_extensions()
        .iter()
        .any(|e| name_to_key(&e.config.name()) == key);
    if !existed {
        bail!(
            "No extension named '{}'. Run `biorouter extension list`.",
            name
        );
    }
    remove_extension(&key);
    println!(
        "  {} removed extension {}",
        style("✓").green(),
        style(&name).bold()
    );

    if purge {
        let dir = extensions_root().join(&name);
        if dir.starts_with(extensions_root()) && dir.exists() {
            fs::remove_dir_all(&dir).ok();
            println!(
                "  {} purged {}",
                style("·").dim(),
                style(dir.display()).dim()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::uv_sync_hint;

    #[test]
    fn hint_broken_homebrew_rust() {
        let stderr = "dyld[28466]: Symbol not found: __ZN4llvm10PGOOptionsC1E...\n\
                      Referenced from: /usr/local/Cellar/rust/1.89.0_3/lib/librustc_driver-bccb51ff.dylib";
        let hint = uv_sync_hint(stderr).unwrap();
        // Must steer to rustup + removing Homebrew rust, since field-testing
        // showed `brew upgrade rust` does not fix this.
        assert!(hint.contains("rustup"));
        assert!(hint.contains("brew uninstall rust"));
        assert!(hint.contains("does NOT fix"));
    }

    #[test]
    fn hint_cryptography_intel_wheel_removed() {
        let stderr = "× Failed to build `cryptography==49.0.0`\n\
                      ├─▶ Call to `maturin.build_wheel` failed";
        let hint = uv_sync_hint(stderr).unwrap();
        assert!(hint.contains("cryptography<49"));
        assert!(hint.contains("Intel"));
    }

    #[test]
    fn hint_rust_toolchain_needed() {
        let stderr = "error: process didn't exit successfully: `rustc -vV`\n💥 maturin failed";
        assert!(uv_sync_hint(stderr).unwrap().contains("Rust toolchain"));
    }

    #[test]
    fn hint_generic_source_build() {
        let stderr = "× Failed to build `pymssql==2.3.13`\n├─▶ The build backend returned an error";
        assert!(uv_sync_hint(stderr)
            .unwrap()
            .contains("compile it from source"));
    }

    #[test]
    fn no_hint_for_unrelated_failure() {
        assert!(uv_sync_hint("No solution found when resolving dependencies").is_none());
    }
}
