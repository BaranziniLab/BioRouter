//! `biorouter skill` subcommands — install a skill (or skill bundle) from a
//! `.zip`, list installed skills, and remove one. Mirrors the desktop GUI's
//! skill-zip flow: a single skill is `SKILL.md` (optionally under one folder);
//! a bundle is `<bundle>/<slug>/SKILL.md`. Text files are written under
//! `~/.config/biorouter/skills/<slug>/`, where they are auto-discovered.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use biorouter::config::paths::Paths;
use console::{style, Color};

const ACCENT: Color = Color::Color256(137);
const TEXT_EXTENSIONS: &[&str] = &["md", "txt", "yaml", "yml", "json", "py", "sh"];

fn skills_root() -> PathBuf {
    Paths::config_dir().join("skills")
}

/// Parse the `name` and `description` out of a `SKILL.md` YAML frontmatter block.
fn parse_frontmatter(content: &str) -> Option<(String, String)> {
    let rest = content.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let front = rest.get(..end)?;
    let mut name = None;
    let mut description = None;
    for line in front.lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().trim_matches(['"', '\'']).to_string());
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().trim_matches(['"', '\'']).to_string());
        }
    }
    match (name, description) {
        (Some(n), Some(d)) if !n.is_empty() => Some((n, d)),
        _ => None,
    }
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
            prev_dash = c == '-';
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn is_text(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| TEXT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn read_entry(archive: &mut zip::ZipArchive<fs::File>, name: &str) -> Result<String> {
    let mut entry = archive.by_name(name)?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf)?;
    Ok(buf)
}

// ──────────────────────────────────────────────────────────────────────────────
// install
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_install(path: PathBuf, force: bool) -> Result<()> {
    if !path.exists() {
        bail!("File not found: {}", path.display());
    }
    let file = fs::File::open(&path).with_context(|| format!("opening {}", path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| anyhow!("Not a valid .zip file: {}", e))?;

    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    // --- Single skill: root SKILL.md, or one folder deep <slug>/SKILL.md ---
    let (skill_md, prefix) = if names.iter().any(|n| n == "SKILL.md") {
        (Some("SKILL.md".to_string()), String::new())
    } else if let Some(single) = names
        .iter()
        .find(|n| n.matches('/').count() == 1 && n.ends_with("/SKILL.md"))
    {
        let prefix = single.trim_end_matches("SKILL.md").to_string();
        (Some(single.clone()), prefix)
    } else {
        (None, String::new())
    };

    if let Some(skill_md) = skill_md {
        let content = read_entry(&mut archive, &skill_md)?;
        let (name, description) = parse_frontmatter(&content)
            .ok_or_else(|| anyhow!("SKILL.md must have frontmatter with name and description"))?;
        let slug = slugify(&name);
        let dest = skills_root().join(&slug);
        ensure_writable(&dest, force)?;

        let written = write_files(&mut archive, &names, &prefix, &dest)?;
        report_single(&name, &description, &slug, &dest, written);
        return Ok(());
    }

    // --- Bundle: <bundle>/<slug>/SKILL.md ---
    let bundle_entries: Vec<&String> = names
        .iter()
        .filter(|n| n.matches('/').count() == 2 && n.ends_with("/SKILL.md"))
        .collect();
    if bundle_entries.is_empty() {
        bail!("No SKILL.md found in the zip.");
    }
    let bundle_folder = bundle_entries[0].split('/').next().unwrap().to_string();
    let bundle_prefix = format!("{}/", bundle_folder);

    let mut sub_skills: Vec<(String, String)> = Vec::new();
    for entry in &bundle_entries {
        if !entry.starts_with(&bundle_prefix) {
            continue;
        }
        let content = read_entry(&mut archive, entry)?;
        if let Some(meta) = parse_frontmatter(&content) {
            sub_skills.push(meta);
        }
    }
    if sub_skills.is_empty() {
        bail!("No valid SKILL.md files found in bundle.");
    }

    let slug = slugify(&bundle_folder);
    let dest = skills_root().join(&slug);
    ensure_writable(&dest, force)?;
    let written = write_files(&mut archive, &names, &bundle_prefix, &dest)?;

    println!(
        "  {} installed skill bundle {} {}",
        style("✓").green(),
        style(&slug).fg(ACCENT).bold(),
        style(format!("({} skills, {} files)", sub_skills.len(), written)).dim()
    );
    for (name, _desc) in &sub_skills {
        println!("    {} {}", style("·").dim(), style(name).dim());
    }
    Ok(())
}

fn ensure_writable(dest: &Path, force: bool) -> Result<()> {
    if dest.exists() {
        if !force {
            bail!(
                "Skill already installed at {}. Re-run with --force to overwrite.",
                dest.display()
            );
        }
        fs::remove_dir_all(dest).ok();
    }
    Ok(())
}

/// Write every text file under `prefix` into `dest`, stripping the prefix.
fn write_files(
    archive: &mut zip::ZipArchive<fs::File>,
    names: &[String],
    prefix: &str,
    dest: &Path,
) -> Result<usize> {
    let mut count = 0;
    for name in names {
        if name.ends_with('/') {
            continue;
        }
        if !prefix.is_empty() && !name.starts_with(prefix) {
            continue;
        }
        let rel = if prefix.is_empty() {
            name.as_str()
        } else {
            name.strip_prefix(prefix).unwrap_or(name)
        };
        if rel.is_empty() || !is_text(rel) {
            continue;
        }
        // Guard against zip-slip via the resolved path staying under `dest`.
        let out_path = dest.join(rel);
        if !out_path.starts_with(dest) {
            bail!("Unsafe path in zip: {}", name);
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = read_entry(archive, name)?;
        fs::write(&out_path, content)?;
        count += 1;
    }
    Ok(count)
}

fn report_single(name: &str, description: &str, slug: &str, dest: &Path, files: usize) {
    println!(
        "  {} installed skill {} {}",
        style("✓").green(),
        style(slug).fg(ACCENT).bold(),
        style(format!("({} files)", files)).dim()
    );
    println!("    {} {}", style("name:").dim(), name);
    if !description.is_empty() {
        println!("    {} {}", style("desc:").dim(), style(description).dim());
    }
    println!(
        "    {} {}",
        style("path:").dim(),
        style(dest.display()).dim()
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// list
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_list() -> Result<()> {
    let root = skills_root();
    println!("  {} {}", style("▌").fg(ACCENT), style("Skills").bold());
    if !root.exists() {
        println!("    {}", style("none installed").dim());
        return Ok(());
    }

    let mut found: Vec<(String, String)> = Vec::new();
    collect_skills(&root, &root, &mut found);
    found.sort();

    if found.is_empty() {
        println!("    {}", style("none installed").dim());
        return Ok(());
    }
    let width = found.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
    for (slug, desc) in &found {
        println!(
            "    {} {:<width$}  {}",
            style("·").dim(),
            style(slug).bold(),
            style(desc).dim(),
            width = width
        );
    }
    Ok(())
}

/// Recursively find `SKILL.md` files and record `(relative-slug, description)`.
fn collect_skills(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_skills(root, &path, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
            let rel = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let desc = fs::read_to_string(&path)
                .ok()
                .and_then(|c| parse_frontmatter(&c))
                .map(|(_, d)| d)
                .unwrap_or_default();
            out.push((rel, desc));
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// remove
// ──────────────────────────────────────────────────────────────────────────────

pub async fn handle_remove(slug: String) -> Result<()> {
    let dest = skills_root().join(&slug);
    if !dest.starts_with(skills_root()) {
        bail!("Invalid skill name.");
    }
    if !dest.exists() {
        bail!(
            "No skill installed at {}. Run `biorouter skill list`.",
            dest.display()
        );
    }
    fs::remove_dir_all(&dest)?;
    println!(
        "  {} removed skill {}",
        style("✓").green(),
        style(&slug).bold()
    );
    Ok(())
}
