//! `biorouter doctor` and `biorouter setup-path` — the terminal equivalent of
//! the desktop dependency-setup screen. Both read the same shared spec in
//! `biorouter::system`, so prerequisites are defined in exactly one place.

use anyhow::Result;
use biorouter::providers::llamacpp_sidecar::{self, SidecarState, SidecarStatus};
use biorouter::system::{self, DependencyStatus};
use console::{style, Color};

/// Brand warm tan-brown accent (xterm-256 137 ≈ #af875f).
const ACCENT: Color = Color::Color256(137);

fn section(title: &str) {
    println!("  {} {}", style("▌").fg(ACCENT), style(title).bold());
}

pub async fn handle_doctor(format: &str, check_update: bool) -> Result<()> {
    let deps = system::check_all();
    let cli_path = system::biorouter_on_path();
    // Snapshot the local-model sidecar. `status()` health-probes the configured
    // port, so this also detects a llama-server started by the desktop app or a
    // standalone `biorouterd` — useful for "the local model works in the app but
    // my CLI says nothing".
    let llama = llamacpp_sidecar::global().status().await;
    let model_cache_dir = llamacpp_sidecar::model_cache_dir().display().to_string();
    // Best-effort, networked (offline → None); skipped for fast callers (GUI).
    let update = if check_update {
        system::check_for_update().await
    } else {
        None
    };

    if format == "json" {
        println!(
            "{}",
            serde_json::json!({
                "dependencies": deps,
                "cli_on_path": cli_path,
                "local_models": {
                    "sidecar": llama,
                    "model_cache_dir": model_cache_dir,
                },
                "update": update,
            })
        );
        return Ok(());
    }

    section("System check");
    let width = deps.iter().map(|d| d.display_name.len()).max().unwrap_or(0);
    for d in &deps {
        print_dep(d, width);
    }

    // The CLI-on-PATH status, mirroring the desktop "install CLI" affordance.
    println!();
    section("Biorouter CLI");
    match &cli_path {
        Some(p) => println!(
            "    {} on PATH {}",
            style("✓").green(),
            style(p.display()).dim()
        ),
        None => {
            println!(
                "    {} not on PATH — run {} to call `biorouter` from any terminal",
                style("○").yellow(),
                style("biorouter setup-path").fg(ACCENT).bold()
            );
        }
    }

    // Local model (Llama Server) status — the terminal equivalent of the
    // desktop onboarding/settings local-model card.
    println!();
    section("Local models (Llama Server)");
    print_llama_status(&llama, &model_cache_dir);

    // Actionable next steps for anything missing.
    let missing_required: Vec<&DependencyStatus> =
        deps.iter().filter(|d| d.required && !d.installed).collect();
    let missing_optional: Vec<&DependencyStatus> = deps
        .iter()
        .filter(|d| !d.required && !d.installed)
        .collect();

    if !missing_required.is_empty() || !missing_optional.is_empty() {
        println!();
        section("To install");
        for d in missing_required.iter().chain(missing_optional.iter()) {
            let tag = if d.required {
                style("required").red()
            } else {
                style("optional").dim()
            };
            println!("    {} {}", style(&d.display_name).bold(), tag);
            match &d.install_command {
                Some(cmd) => println!("      {}", style(cmd).fg(ACCENT)),
                None => println!("      {}", style(&d.doc_url).dim()),
            }
        }
    } else {
        println!();
        println!("  {} all prerequisites present", style("✓").green());
    }

    // Self-update check.
    if !check_update {
        return Ok(());
    }
    println!();
    section("Updates");
    match update {
        Some(u) if u.update_available => {
            println!(
                "    {} update available: {} {}",
                style("↑").fg(ACCENT).bold(),
                style(&u.latest).fg(ACCENT).bold(),
                style(format!("(you have {})", u.current)).dim()
            );
            println!(
                "      install the latest release, then run {}",
                style("biorouter setup-path").fg(ACCENT)
            );
        }
        Some(u) => println!(
            "    {} up to date {}",
            style("✓").green(),
            style(format!("({})", u.current)).dim()
        ),
        None => println!(
            "    {} {}",
            style("·").dim(),
            style("could not check for updates (offline?)").dim()
        ),
    }
    Ok(())
}

fn print_llama_status(status: &SidecarStatus, model_cache_dir: &str) {
    let (marker, label) = match status.state {
        SidecarState::Ready => (style("●").green().to_string(), "ready".to_string()),
        SidecarState::Starting => (
            style("◐").yellow().to_string(),
            "starting (downloading or loading)".to_string(),
        ),
        SidecarState::Stopped => (style("○").dim().to_string(), "stopped".to_string()),
        SidecarState::Error => (style("✗").red().to_string(), "error".to_string()),
        SidecarState::NoBinary => (
            style("○").dim().to_string(),
            "no llama-server binary found".to_string(),
        ),
    };
    println!("    {} server {}", marker, style(label).dim());

    if let Some(model) = &status.model {
        let ctx = status
            .context_size
            .map(|c| format!(" ({} ctx)", c))
            .unwrap_or_default();
        println!(
            "    {} model  {}{}",
            style("·").dim(),
            style(model).bold(),
            style(ctx).dim()
        );
    }
    match &status.binary_path {
        Some(path) => println!("    {} binary {}", style("·").dim(), style(path).dim()),
        None => println!(
            "    {} binary {}",
            style("·").dim(),
            style(
                "not found — set BIOROUTER_LLAMACPP_BIN, or use the desktop app which bundles it"
            )
            .dim()
        ),
    }
    println!(
        "    {} build  {}",
        style("·").dim(),
        style(&status.build).dim()
    );
    println!(
        "    {} cache  {}",
        style("·").dim(),
        style(model_cache_dir).dim()
    );
    if let Some(detail) = &status.detail {
        if !detail.trim().is_empty() {
            println!("    {} {}", style("·").dim(), style(detail).dim());
        }
    }
}

fn print_dep(d: &DependencyStatus, width: usize) {
    let (marker, ver) = if d.installed {
        (
            style("●").green().to_string(),
            d.version.clone().unwrap_or_default(),
        )
    } else if d.required {
        (style("✗").red().to_string(), "missing".to_string())
    } else {
        (style("○").dim().to_string(), "not installed".to_string())
    };
    println!(
        "    {} {:<width$}  {}",
        marker,
        style(&d.display_name).bold(),
        style(ver).dim(),
        width = width
    );
}

pub async fn handle_setup_path() -> Result<()> {
    let exe = std::env::current_exe()
        .map_err(|e| anyhow::anyhow!("Could not locate the running executable: {}", e))?;
    let result = system::install_cli(&exe)?;

    println!(
        "  {} installed the Biorouter CLI {}",
        style("✓").green(),
        style(format!("→ {}", result.link.display())).dim()
    );
    if !result.on_path {
        println!(
            "  {} add {} to your PATH, e.g. add this to your shell profile:",
            style("⚠").yellow(),
            style(result.target_dir.display()).fg(ACCENT)
        );
        println!(
            "      {}",
            style(format!(
                "export PATH=\"{}:$PATH\"",
                result.target_dir.display()
            ))
            .fg(ACCENT)
        );
    } else {
        println!(
            "  {} you can now run {} from any terminal",
            style("·").dim(),
            style("biorouter").fg(ACCENT).bold()
        );
    }
    Ok(())
}
