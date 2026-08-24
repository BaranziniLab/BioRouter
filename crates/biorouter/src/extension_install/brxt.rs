//! Reading a `.brxt` bundle: validate its shape, parse its manifest, extract it,
//! and build its Python environment.
//!
//! This is the one implementation of "what is in this bundle", shared by
//! [`super::transaction::ExtensionInstallTransaction`] and therefore by the CLI,
//! the daemon and any agent tool. It was previously a private copy inside
//! `biorouter-cli`, with a second copy in the Electron main process; the CLI's
//! copy is gone and this is what replaced it.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::conversation::message::SecretKeyRequest;

/// One environment value an extension declares in its manifest.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct BrxtEnvVar {
    pub key: String,
    #[serde(default)]
    pub required: bool,
    /// Whether a machine-wide value of the same name should be reused.
    #[serde(default)]
    pub auto_propagate: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub description: String,
    /// Whether the value is a credential. **This is the only thing that decides
    /// whether a value may be written to `config.yaml`**: a secret goes to the
    /// OS credential store and only its key name is recorded beside the config.
    #[serde(default)]
    pub secret: bool,
}

impl BrxtEnvVar {
    /// The card field for this variable. Carries the name, a label and help
    /// text — never a value, and never the `default`, because a card that
    /// pre-filled a secret would be showing the user something the surface had
    /// to read back out of the keyring first.
    pub fn as_key_request(&self) -> SecretKeyRequest {
        SecretKeyRequest {
            key: self.key.clone(),
            label: self.key.clone(),
            description: (!self.description.is_empty()).then(|| self.description.clone()),
            required: self.required,
        }
    }
}

/// A bundle's `manifest.json`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrxtManifest {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub entry_point: String,
    pub repository: String,
    #[serde(default)]
    pub tools_count: Option<u32>,
    #[serde(default)]
    pub env_vars: Vec<BrxtEnvVar>,
}

impl BrxtManifest {
    /// The variables the bundle cannot run without.
    pub fn required_vars(&self) -> impl Iterator<Item = &BrxtEnvVar> {
        self.env_vars.iter().filter(|v| v.required)
    }

    /// The declared variables still missing once `supplied` is applied.
    ///
    /// A value already in the credential store counts as supplied — an install
    /// that re-asks for a passcode the machine already holds is an install that
    /// trains the user to paste secrets they did not need to.
    pub fn unmet_requirements(&self, supplied: &HashMap<String, String>) -> Vec<&BrxtEnvVar> {
        self.required_vars()
            .filter(|v| !supplied.contains_key(&v.key) && !secret_already_stored(&v.key))
            .collect()
    }
}

/// Whether the credential store already holds `key`.
///
/// The value is fetched and **immediately dropped**: nothing outside the store
/// ever learns what it is, only that it exists. A read failure reads as absent,
/// which is the direction that asks the user rather than the direction that
/// registers an extension that cannot authenticate.
pub fn secret_already_stored(key: &str) -> bool {
    crate::config::Config::global()
        .get_secret::<String>(key)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/// A bundle a caller may read a manifest out of, extract, or refuse.
pub struct BrxtBundle {
    path: PathBuf,
    manifest: BrxtManifest,
    skills: Vec<BundledSkill>,
}

/// A skill shipped inside a bundle, as `skills/<slug>/SKILL.md`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BundledSkill {
    pub slug: String,
    pub name: String,
    pub description: String,
}

impl BrxtBundle {
    /// Open and validate `path`, returning a friendly message if it is not a
    /// bundle. The four structural checks and the required-field list mirror the
    /// desktop validator exactly — a bundle the GUI accepts and the CLI refuses
    /// (or the reverse) is a bug report nobody can reproduce.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            bail!("File not found: {}", path.display());
        }
        let file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| anyhow!("Not a valid .brxt (zip) bundle: {}", e))?;

        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();

        for (present, missing) in [
            (names.iter().any(|n| n == "manifest.json"), "manifest.json"),
            (
                names.iter().any(|n| n.eq_ignore_ascii_case("readme.md")),
                "README.md",
            ),
            (names.iter().any(|n| n == "pyproject.toml"), "pyproject.toml"),
            (names.iter().any(|n| n.starts_with("src/")), "src/ directory"),
        ] {
            if !present {
                bail!("Missing {missing}: not a valid .brxt bundle");
            }
        }

        let manifest: BrxtManifest = {
            let mut entry = archive
                .by_name("manifest.json")
                .map_err(|e| anyhow!("Could not read manifest.json: {}", e))?;
            let mut buf = String::new();
            entry.read_to_string(&mut buf)?;
            serde_json::from_str(&buf).map_err(|e| anyhow!("Invalid manifest.json: {}", e))?
        };
        if manifest.name.trim().is_empty() {
            bail!("manifest.json missing required field: \"name\"");
        }
        if manifest.entry_point.trim().is_empty() {
            bail!("manifest.json missing required field: \"entry_point\"");
        }

        let skills = read_bundled_skills(&mut archive, &names);
        Ok(Self {
            path: path.to_path_buf(),
            manifest,
            skills,
        })
    }

    pub fn manifest(&self) -> &BrxtManifest {
        &self.manifest
    }

    pub fn skills(&self) -> &[BundledSkill] {
        &self.skills
    }

    /// Extract every file under `dest`, refusing any entry whose path escapes it.
    pub fn extract_to(&self, dest: &Path) -> Result<()> {
        let file = fs::File::open(&self.path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            // `enclosed_name` is the zip-slip guard: it returns `None` for any
            // path that would leave `dest`, including `..` and absolute paths.
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
}

fn read_bundled_skills(
    archive: &mut zip::ZipArchive<fs::File>,
    names: &[String],
) -> Vec<BundledSkill> {
    let mut skills = Vec::new();
    for name in names {
        let Some(slug) = name
            .strip_prefix("skills/")
            .and_then(|rest| rest.strip_suffix("/SKILL.md"))
        else {
            continue;
        };
        if slug.is_empty() || slug.contains('/') {
            continue;
        }
        let mut body = String::new();
        if archive
            .by_name(name)
            .ok()
            .and_then(|mut e| e.read_to_string(&mut body).ok())
            .is_none()
        {
            continue;
        }
        if let Some((skill_name, description)) = skill_frontmatter(&body) {
            skills.push(BundledSkill {
                slug: slug.to_string(),
                name: skill_name,
                description,
            });
        }
    }
    skills.sort_by(|a, b| a.slug.cmp(&b.slug));
    skills
}

/// `name:` and `description:` out of a `SKILL.md` YAML frontmatter block.
fn skill_frontmatter(body: &str) -> Option<(String, String)> {
    let rest = body.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let (mut name, mut description) = (None, None);
    for line in rest[..end].lines() {
        if let Some(v) = line.strip_prefix("name:") {
            name = Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if let Some(v) = line.strip_prefix("description:") {
            description = Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }
    Some((name?, description.unwrap_or_default()))
}

/// Whether `uv` — which every `.brxt` needs to build its environment — is here.
pub fn uv_available() -> bool {
    crate::system::status_of("uv")
        .map(|d| d.installed)
        .unwrap_or(false)
}

/// The message to refuse an install with when `uv` is missing.
pub fn uv_missing_message() -> String {
    let cmd = crate::system::install_command("uv").unwrap_or_default();
    format!(
        "`uv` is required to install .brxt extensions, but it was not found.\n  \
         Install it:  {cmd}\n  \
         Then re-run, or run `biorouter doctor` to check prerequisites."
    )
}

/// Build the bundle's Python environment.
pub fn run_uv_sync(dir: &Path) -> Result<()> {
    let output = Command::new("uv").arg("sync").current_dir(dir).output();
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
            bail!("uv sync failed:\n{}", tail)
        }
        Err(e) => bail!("Could not run `uv sync`: {e}"),
    }
}

/// `~/.config/biorouter/extensions/`.
pub fn extensions_root() -> PathBuf {
    crate::config::paths::Paths::config_dir().join("extensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_is_read_out_of_a_skill_file() {
        let body = "---\nname: Genomics\ndescription: Variant calling\n---\n\n# body\n";
        assert_eq!(
            skill_frontmatter(body),
            Some(("Genomics".to_string(), "Variant calling".to_string()))
        );
    }

    #[test]
    fn a_file_without_frontmatter_contributes_no_skill() {
        assert_eq!(skill_frontmatter("# just a heading\n"), None);
    }

    /// A card is the *ask*. Pre-filling it would mean reading the value back out
    /// of the credential store first, which is the one direction this whole
    /// feature exists to prevent.
    #[test]
    fn a_key_request_carries_no_value_not_even_the_default() {
        let var = BrxtEnvVar {
            key: "SPOKEAGENT_PASSCODE".to_string(),
            required: true,
            auto_propagate: false,
            default: Some("hunter2".to_string()),
            description: "From the UCSF wiki".to_string(),
            secret: true,
        };
        let request = var.as_key_request();
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("SPOKEAGENT_PASSCODE"));
        assert!(
            !json.contains("hunter2"),
            "a default must never ride on the card: {json}"
        );
    }
}
