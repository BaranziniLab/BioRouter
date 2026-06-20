//! On-disk artifact store for Agent Drafter.
//!
//! Each artifact is a directory under the store root containing a
//! `manifest.json` plus the artifact's own files (entry HTML, CSS, JS, …).
//! Layout mirrors the conventions used by `memory`/`knowledge`:
//! `~/.config/biorouter/agent_drafter/<id>/`.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Component, Path, PathBuf};

/// Whether an artifact embeds live agent capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    /// A plain interactive artifact (HTML/CSS/JS), no agent.
    Static,
    /// An artifact wired to a BioRouter agent (ACP / MCP-App bridge).
    Agentic,
}

impl ArtifactKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "static" | "" => Some(Self::Static),
            "agentic" | "agent" => Some(Self::Agentic),
            _ => None,
        }
    }
}

/// Per-artifact agent configuration (present for `Agentic` artifacts).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    /// System prompt that defines the embedded agent's behavior.
    #[serde(default)]
    pub system_prompt: String,
    /// Optional greeting shown when the chat panel mounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greeting: Option<String>,
    /// Tool / MCP-extension names the artifact's agent is allowed to use.
    #[serde(default)]
    pub tools: Vec<String>,
}

/// Metadata describing a single artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub kind: ArtifactKind,
    /// Entry file rendered for previews/exports.
    pub entry: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentConfig>,
    /// Preferred preview width in CSS px (None → fill the panel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Preferred preview height in CSS px (None → a comfortable default that
    /// then auto-grows to fit content).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Turn a title into a filesystem-safe, URL-safe slug.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in title.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        "artifact".to_string()
    } else {
        slug
    }
}

/// Reject paths that escape the artifact directory (absolute, `..`, or rooted).
fn safe_relative(path: &str) -> io::Result<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be relative to the artifact",
        ));
    }
    let mut clean = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::Normal(c) => clean.push(c),
            Component::CurDir => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "path may not contain '..' or root components",
                ))
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty path"));
    }
    Ok(clean)
}

/// Filesystem-backed artifact store rooted at a single directory.
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn manifest_path(&self, id: &str) -> PathBuf {
        self.dir(id).join("manifest.json")
    }

    pub fn exists(&self, id: &str) -> bool {
        self.manifest_path(id).exists()
    }

    /// Allocate a unique id derived from the title.
    fn unique_id(&self, title: &str) -> String {
        let base = slugify(title);
        if !self.exists(&base) && !self.dir(&base).exists() {
            return base;
        }
        for n in 2..10_000 {
            let candidate = format!("{base}-{n}");
            if !self.exists(&candidate) && !self.dir(&candidate).exists() {
                return candidate;
            }
        }
        format!("{base}-{}", now_secs())
    }

    /// Create a new artifact directory with a manifest and the given files.
    pub fn create(
        &self,
        title: &str,
        description: &str,
        kind: ArtifactKind,
        entry: &str,
        files: &[(String, String)],
    ) -> io::Result<Manifest> {
        let id = self.unique_id(title);
        let dir = self.dir(&id);
        std::fs::create_dir_all(&dir)?;
        let now = now_secs();
        let manifest = Manifest {
            id: id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            kind,
            entry: entry.to_string(),
            created_at: now,
            updated_at: now,
            agent: if kind == ArtifactKind::Agentic {
                Some(AgentConfig::default())
            } else {
                None
            },
            width: None,
            height: None,
        };
        self.save_manifest(&manifest)?;
        for (path, content) in files {
            self.write_file(&id, path, content)?;
        }
        Ok(manifest)
    }

    pub fn save_manifest(&self, manifest: &Manifest) -> io::Result<()> {
        let dir = self.dir(&manifest.id);
        std::fs::create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(manifest)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        std::fs::write(self.manifest_path(&manifest.id), json)
    }

    pub fn load_manifest(&self, id: &str) -> io::Result<Manifest> {
        let raw = std::fs::read_to_string(self.manifest_path(id))?;
        serde_json::from_str(&raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn touch(&self, id: &str) -> io::Result<()> {
        let mut m = self.load_manifest(id)?;
        m.updated_at = now_secs();
        self.save_manifest(&m)
    }

    pub fn list(&self) -> Vec<Manifest> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let id = entry.file_name().to_string_lossy().to_string();
                    if let Ok(m) = self.load_manifest(&id) {
                        out.push(m);
                    }
                }
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }

    pub fn write_file(&self, id: &str, path: &str, content: &str) -> io::Result<()> {
        let rel = safe_relative(path)?;
        let full = self.dir(id).join(&rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, content)
    }

    pub fn read_file(&self, id: &str, path: &str) -> io::Result<String> {
        let rel = safe_relative(path)?;
        std::fs::read_to_string(self.dir(id).join(rel))
    }

    pub fn delete(&self, id: &str) -> io::Result<()> {
        let dir = self.dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn store() -> (tempfile::TempDir, ArtifactStore) {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::new(dir.path().to_path_buf());
        (dir, store)
    }

    #[test]
    fn slugify_handles_messy_titles() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  Trim   Me  "), "trim-me");
        assert_eq!(slugify("***"), "artifact");
        assert_eq!(slugify("Café Méta 2"), "caf-m-ta-2");
    }

    #[test]
    fn create_then_load_roundtrips() {
        let (_d, s) = store();
        let m = s
            .create(
                "My App",
                "does things",
                ArtifactKind::Static,
                "index.html",
                &[],
            )
            .unwrap();
        assert_eq!(m.id, "my-app");
        assert_eq!(m.kind, ArtifactKind::Static);
        assert!(m.agent.is_none());
        let loaded = s.load_manifest("my-app").unwrap();
        assert_eq!(loaded.title, "My App");
        assert_eq!(loaded.description, "does things");
    }

    #[test]
    fn agentic_artifact_gets_agent_config() {
        let (_d, s) = store();
        let m = s
            .create("Bot", "", ArtifactKind::Agentic, "index.html", &[])
            .unwrap();
        assert!(m.agent.is_some());
    }

    #[test]
    fn ids_are_unique() {
        let (_d, s) = store();
        let a = s
            .create("Same", "", ArtifactKind::Static, "index.html", &[])
            .unwrap();
        let b = s
            .create("Same", "", ArtifactKind::Static, "index.html", &[])
            .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(a.id, "same");
        assert_eq!(b.id, "same-2");
    }

    #[test]
    fn write_and_read_files() {
        let (_d, s) = store();
        s.create(
            "Files",
            "",
            ArtifactKind::Static,
            "index.html",
            &[
                ("index.html".to_string(), "<h1>hi</h1>".to_string()),
                ("css/app.css".to_string(), "body{}".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(s.read_file("files", "index.html").unwrap(), "<h1>hi</h1>");
        assert_eq!(s.read_file("files", "css/app.css").unwrap(), "body{}");
    }

    #[test]
    fn rejects_path_traversal() {
        let (_d, s) = store();
        s.create("Safe", "", ArtifactKind::Static, "index.html", &[])
            .unwrap();
        assert!(s.write_file("safe", "../escape.txt", "x").is_err());
        assert!(s.write_file("safe", "/etc/passwd", "x").is_err());
        assert!(s.write_file("safe", "a/../../b.txt", "x").is_err());
        assert!(s.write_file("safe", "", "x").is_err());
    }

    #[test]
    fn list_sorts_by_updated_desc() {
        let (_d, s) = store();
        let a = s
            .create("Alpha", "", ArtifactKind::Static, "index.html", &[])
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = s
            .create("Beta", "", ArtifactKind::Static, "index.html", &[])
            .unwrap();
        let list = s.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id);
        assert_eq!(list[1].id, a.id);
    }

    #[test]
    fn delete_removes_artifact() {
        let (_d, s) = store();
        s.create("Gone", "", ArtifactKind::Static, "index.html", &[])
            .unwrap();
        assert!(s.exists("gone"));
        s.delete("gone").unwrap();
        assert!(!s.exists("gone"));
    }
}
