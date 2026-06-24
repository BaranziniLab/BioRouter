//! On-disk artifact store for Agent Drafter.
//!
//! Each artifact is a directory under the store root containing a
//! `manifest.json` plus the artifact's own files (entry HTML, CSS, JS, …).
//! Layout mirrors the conventions used by `memory`/`knowledge`:
//! `~/.config/biorouter/agent_drafter/<id>/`.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Component, Path, PathBuf};

use super::manifest::{Capabilities, GuardrailsConfig, ModelSettings, Orchestration, ReliabilityConfig};

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

/// The provider + model an app's agent should run on. When absent, the app
/// falls back to BioRouter's globally-configured provider/model.
// Note: not `Eq` — `settings` carries `f32` fields (temperature/top_p).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelSelection {
    /// Provider name (e.g. "xiaomi_mimo", "anthropic", "openai").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Model name (e.g. "mimo-v2.5", "claude-opus-4-8").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Provider-agnostic generation settings (temperature, reasoning effort, …).
    /// Consumed by the per-app model surface (Phase 4); `None` → provider defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ModelSettings>,
}

impl ModelSelection {
    pub fn is_set(&self) -> bool {
        self.provider.is_some() || self.model.is_some()
    }
}

/// Per-app agent configuration (present for `Agentic` apps). Captures everything
/// that distinguishes one app's BioRouter backend from another: the system
/// prompt/persona, which model runs it, and which extensions / skills /
/// knowledge base the agent may use.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    /// System prompt that defines the embedded agent's behavior.
    #[serde(default)]
    pub system_prompt: String,
    /// Optional greeting shown when the chat panel mounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greeting: Option<String>,
    /// Legacy free-form tool names (kept for back-compat; prefer `extensions`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Provider + model the app's agent should run on. Defaults to the global
    /// BioRouter model when unset (the GUI/CLI seeds this with the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSelection>,
    /// Builtin / platform extension names the app's agent should load
    /// (e.g. "developer", "computercontroller", "autovisualiser", "knowledge").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Skill ids the app's agent should have available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// Knowledge base id the app's agent should be scoped to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_base: Option<String>,
    /// Bound on the agent's tool-calling loop per user message (a guardrail and
    /// a workflow control, like the knowledge sub-agent's `max_steps`). When
    /// unset, the server applies a safe default cap. Higher values let
    /// workflow-style apps chain more tool calls autonomously.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,

    // ───── BRSDK (all default; absence = denied / off) ─────
    /// Deny-by-default capability grants: files / data / compute / vault /
    /// memory / tracing / lifecycle-events (Phases 3–7).
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Declarative content guardrails + goal harness + HITL approvals (Phase 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrails: Option<GuardrailsConfig>,
    /// Reliability knobs: tool timeouts, stop conditions, error→output, etc.
    /// (Phase 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reliability: Option<ReliabilityConfig>,
    /// Multi-agent orchestration: sub-agents-as-tools, handoffs, workflows,
    /// lazy tools (Phase 6).
    #[serde(default)]
    pub orchestration: Orchestration,
    /// JSON-Schema contract the final answer must validate against (Phase 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<serde_json::Value>,
    /// Durable, resumable per-app sessions (Phase 1). `None` is treated as ON
    /// (the recovery default); set `Some(false)` to restore ephemeral
    /// per-connection sessions. Use [`AgentConfig::durable_session`] to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_session: Option<bool>,
}

impl AgentConfig {
    /// Whether this app's sessions are durable + resumable (default: yes).
    pub fn durable_session(&self) -> bool {
        self.durable_session.unwrap_or(true)
    }
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
    /// Unix seconds of the last successful esbuild bundle, if any. `None` means
    /// the app has never been built (or its sources changed since).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at: Option<u64>,
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
            built_at: None,
        };
        self.save_manifest(&manifest)?;
        for (path, content) in files {
            self.write_file(&id, path, content)?;
        }
        Ok(manifest)
    }

    /// Create an artifact at an **explicit** id (slugified). If one already
    /// exists at that id it is replaced, so re-authoring the same app is
    /// idempotent (no `-2` duplicates) and apps are stably addressable.
    pub fn create_with_id(
        &self,
        id: &str,
        title: &str,
        description: &str,
        kind: ArtifactKind,
        entry: &str,
        files: &[(String, String)],
    ) -> io::Result<Manifest> {
        let id = slugify(id);
        if self.dir(&id).exists() {
            self.delete(&id)?;
        }
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
            built_at: None,
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
        out.sort_by_key(|manifest| std::cmp::Reverse(manifest.updated_at));
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

    /// Read a file's raw bytes (for serving binary assets like images/fonts).
    pub fn read_bytes(&self, id: &str, path: &str) -> io::Result<Vec<u8>> {
        let rel = safe_relative(path)?;
        std::fs::read(self.dir(id).join(rel))
    }

    /// Absolute path to an artifact's directory (used by the bundler/server).
    pub fn artifact_dir(&self, id: &str) -> PathBuf {
        self.dir(id)
    }

    /// Absolute path to a file within an artifact, path-traversal checked.
    pub fn file_path(&self, id: &str, path: &str) -> io::Result<PathBuf> {
        let rel = safe_relative(path)?;
        Ok(self.dir(id).join(rel))
    }

    /// Whether a file exists within an artifact (path-traversal checked).
    pub fn file_exists(&self, id: &str, path: &str) -> bool {
        match safe_relative(path) {
            Ok(rel) => self.dir(id).join(rel).is_file(),
            Err(_) => false,
        }
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
    fn pre_brsdk_manifest_deserializes_with_defaults() {
        // A manifest written before BRSDK existed (no capabilities/guardrails/
        // reliability/orchestration/output_type/durable_session) must still load.
        let legacy = r#"{
            "id": "legacy-app",
            "title": "Legacy App",
            "description": "made before BRSDK",
            "kind": "agentic",
            "entry": "index.html",
            "created_at": 100,
            "updated_at": 200,
            "agent": {
                "system_prompt": "be helpful",
                "model": { "provider": "anthropic", "model": "claude-opus-4-8" },
                "extensions": ["developer"],
                "max_turns": 24
            }
        }"#;
        let m: Manifest = serde_json::from_str(legacy).expect("legacy manifest must load");
        let agent = m.agent.expect("agent present");
        // New fields fall back to deny-by-default / off.
        assert!(agent.capabilities.files.is_none());
        assert!(agent.capabilities.data.is_none());
        assert!(agent.capabilities.compute.is_none());
        assert_eq!(agent.capabilities.memory.mode, crate::agent_drafter::manifest::MemoryMode::Off);
        assert!(!agent.capabilities.tracing.enabled);
        assert!(agent.capabilities.tracing.redact, "tracing redaction defaults ON");
        assert!(agent.guardrails.is_none());
        assert!(agent.reliability.is_none());
        assert!(agent.output_type.is_none());
        assert!(agent.orchestration.sub_agents.is_empty());
        // durable_session: absent → treated as ON (the recovery default).
        assert!(agent.durable_session(), "durable sessions default ON");
        // Advertised capability list is empty for a legacy app (deny-by-default).
        assert!(agent.capabilities.advertised().is_empty());
    }

    #[test]
    fn full_brsdk_manifest_roundtrips_and_advertises() {
        use crate::agent_drafter::manifest::*;
        let mut caps = Capabilities::default();
        caps.files = Some(FilesCapability::default());
        caps.compute = Some(ComputeCapability {
            sandbox: "docker".into(),
            timeout_s: 120,
            network: "none".into(),
            max_mem: Some("1g".into()),
            cpus: Some(2.0),
            image: None,
        });
        caps.memory = MemoryCapability {
            kb: Some("lab".into()),
            mode: MemoryMode::ReadWrite,
            shared_kb: None,
            distill: true,
        };
        caps.tracing = TracingCapability {
            enabled: true,
            redact: true,
            processor: None,
        };
        caps.events = vec!["tool".into(), "llm".into()];

        let agent = AgentConfig {
            system_prompt: "clinical assistant".into(),
            model: Some(ModelSelection {
                provider: Some("anthropic".into()),
                model: Some("claude-opus-4-8".into()),
                settings: Some(ModelSettings {
                    temperature: Some(0.2),
                    reasoning_effort: Some("high".into()),
                    ..Default::default()
                }),
            }),
            capabilities: caps,
            guardrails: Some(GuardrailsConfig {
                goal: Some("cite the KB".into()),
                pii: PiiMode::Mask,
                needs_approval: vec!["send_email".into()],
                ..Default::default()
            }),
            reliability: Some(ReliabilityConfig {
                tool_timeout_s: Some(30),
                parallel_tools: true,
                ..Default::default()
            }),
            output_type: Some(serde_json::json!({"type":"object"})),
            durable_session: Some(true),
            ..Default::default()
        };

        // Round-trips through JSON without loss.
        let json = serde_json::to_string(&agent).unwrap();
        let back: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.guardrails.as_ref().unwrap().pii, PiiMode::Mask);
        assert_eq!(
            back.model.as_ref().unwrap().settings.as_ref().unwrap().temperature,
            Some(0.2)
        );
        assert_eq!(back.capabilities.compute.as_ref().unwrap().sandbox, "docker");
        assert!(back.reliability.as_ref().unwrap().parallel_tools);

        // Advertises exactly the granted capabilities (deny-by-default elsewhere).
        let adv = back.capabilities.advertised();
        assert!(adv.contains(&"files".to_string()));
        assert!(adv.contains(&"compute".to_string()));
        assert!(adv.contains(&"memory".to_string()));
        assert!(adv.contains(&"tracing".to_string()));
        assert!(adv.contains(&"event:tool".to_string()));
        assert!(!adv.contains(&"data".to_string()), "data not granted → not advertised");
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
