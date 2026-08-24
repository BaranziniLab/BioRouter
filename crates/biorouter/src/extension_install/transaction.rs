//! Installing an extension as one transaction that can be parked, cancelled and
//! rolled back (#117).
//!
//! # Why a transaction and not a sequence of steps
//!
//! An extension install is seven things — download, validate, build the
//! environment, collect credentials, write them, register the config entry,
//! attach to the running chat — and the fourth **stops and waits for a person**.
//! Before this existed, each surface did its own subset in its own order and
//! none of them could wait: the CLI printed a warning about a missing required
//! value and registered the extension anyway, producing something that starts,
//! fails to authenticate, and reports success.
//!
//! Every failure path therefore has to undo the ones before it, or the machine
//! is left with a half-registered extension, an orphaned `~/.config/biorouter/
//! extensions/<name>/` tree, and a credential in the keychain for something that
//! is not installed. That undo is the reason these steps are one type.
//!
//! # Cancellation is a decision, not an error
//!
//! [`InstallState::Cancelled`] and [`InstallState::NeedsCredentials`] are
//! outcomes a caller reports, not failures it retries. An unattended run cannot
//! collect a passcode and must say so — with the key names, so the operator can
//! configure them and re-run — rather than installing something broken or
//! inventing a prompt nobody will see.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::agents::extension::{Envs, ExtensionConfig};
use crate::agents::extension_manager::ExtensionManager;
use crate::config::extensions::{name_to_key, remove_extension, set_extension, ExtensionEntry};
use crate::conversation::message::SecretDestination;
use crate::pending_user_action::UserActionOutcome;

use super::brxt::{
    extensions_root, run_uv_sync, secret_already_stored, uv_available, uv_missing_message,
    BrxtBundle, BrxtEnvVar, BrxtManifest, BundledSkill,
};
use super::credentials::{
    request_credentials, revoke, CredentialSpec, DEFAULT_CREDENTIAL_TTL,
};

/// Where the bundle comes from.
#[derive(Debug, Clone)]
pub enum InstallSource {
    /// A `.brxt` already on this machine: a drop, a Finder deep link, or
    /// `biorouter extension install <path>`.
    LocalFile { path: PathBuf },
    /// A BAAM marketplace entry. `registry_id` is issue #56 Task 43 (DR-23)
    /// provenance and is recorded beside the config entry, so the daemon
    /// re-derives the privacy tier from a stable id rather than from a config
    /// name anyone can rename.
    Marketplace { registry_id: String, url: String },
}

/// How this transaction may collect a value it does not have.
pub enum CredentialPolicy {
    /// Publish a credential card to `session_id` and park until a person
    /// answers it. The desktop and any agent-driven install.
    Ask {
        session_id: Option<String>,
        /// Groups parks that must die together — a bridged tool call passes the
        /// bridge nonce so the card dies with the turn.
        owner: Option<String>,
        ttl: Duration,
    },
    /// Ask on this process's terminal, with echo off. Interactive CLI only:
    /// the closure is handed key names and returns what the person typed.
    ///
    /// A closure rather than a direct `rpassword` call so the prompt stays in
    /// the CLI crate, where the terminal is, and this crate keeps no opinion
    /// about how a terminal asks.
    Prompt(Box<dyn FnMut(&[BrxtEnvVar]) -> anyhow::Result<HashMap<String, String>> + Send>),
    /// Never ask. An unattended run stops at
    /// [`InstallState::NeedsCredentials`] and rolls back.
    Refuse,
}

impl std::fmt::Debug for CredentialPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ask { session_id, .. } => write!(f, "Ask({session_id:?})"),
            Self::Prompt(_) => write!(f, "Prompt"),
            Self::Refuse => write!(f, "Refuse"),
        }
    }
}

/// Where an install got to. Every variant is reportable to a user *and* to a
/// model, and none of them carries a value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum InstallState {
    Downloading,
    Validating,
    /// Stopped because required values are missing and this run may not ask for
    /// them. `keys` are names, so an operator can configure them and re-run.
    NeedsCredentials {
        keys: Vec<String>,
    },
    Installing,
    /// Registered, and live in the session that asked for it.
    Attached,
    /// Registered, but not attached — no session asked, or the hot-load failed
    /// and the extension is still correct for the next chat.
    Installed,
    Cancelled,
    Failed {
        reason: String,
    },
}

impl InstallState {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Attached | Self::Installed)
    }
}

/// The result of a run, safe to hand to a model verbatim.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    /// Stable for the life of the transaction, so a retry can name it.
    pub install_id: String,
    pub state: InstallState,
    pub extension_name: Option<String>,
    pub display_name: Option<String>,
    /// **Names only.** There is nowhere in this struct a value can sit, and that
    /// is deliberate: this is what gets serialised into a tool result.
    pub configured_keys: Vec<String>,
    pub skills: Vec<BundledSkill>,
    pub enabled: bool,
}

impl InstallReport {
    fn failed(install_id: String, reason: impl Into<String>) -> Self {
        Self {
            install_id,
            state: InstallState::Failed {
                reason: reason.into(),
            },
            extension_name: None,
            display_name: None,
            configured_keys: Vec::new(),
            skills: Vec::new(),
            enabled: false,
        }
    }
}

/// Non-secret state kept so a cancelled install can be retried without
/// re-downloading and re-building. Deliberately holds no values.
#[derive(Debug, Clone)]
pub struct ResumableInstall {
    pub install_id: String,
    pub extension_name: String,
    pub display_name: String,
    pub bundle_path: PathBuf,
    pub install_dir: PathBuf,
    /// The variables still to collect.
    pub pending_vars: Vec<BrxtEnvVar>,
}

/// Installs that stopped at the credential step and can be resumed.
#[derive(Default)]
pub struct ResumableInstalls {
    entries: Mutex<HashMap<String, ResumableInstall>>,
}

impl ResumableInstalls {
    pub fn global() -> &'static Arc<Self> {
        static INSTANCE: once_cell::sync::Lazy<Arc<ResumableInstalls>> =
            once_cell::sync::Lazy::new(|| Arc::new(ResumableInstalls::default()));
        &INSTANCE
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, ResumableInstall>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn get(&self, install_id: &str) -> Option<ResumableInstall> {
        self.lock().get(install_id).cloned()
    }

    pub fn list(&self) -> Vec<ResumableInstall> {
        self.lock().values().cloned().collect()
    }

    fn put(&self, entry: ResumableInstall) {
        self.lock().insert(entry.install_id.clone(), entry);
    }

    pub fn forget(&self, install_id: &str) {
        self.lock().remove(install_id);
    }
}

/// One install, from a source to a registered (and possibly attached) extension.
pub struct ExtensionInstallTransaction {
    install_id: String,
    source: InstallSource,
    /// Values the caller already has — `--env` flags, a form the desktop filled
    /// in before the transaction started. Secret-declared keys among these are
    /// still written to the credential store, never to `config.yaml`.
    supplied: HashMap<String, String>,
    enable: bool,
    manager: Option<std::sync::Weak<ExtensionManager>>,
    /// Everything this run created, in the order it created it, so a failure
    /// undoes exactly its own work and nothing else.
    undo: Undo,
}

#[derive(Default)]
struct Undo {
    install_dir: Option<PathBuf>,
    /// Only true when THIS run extracted the tree. An install over an existing
    /// one must not delete the tree it found — a failed upgrade that removes a
    /// working extension is worse than a failed upgrade.
    created_install_dir: bool,
    config_key: Option<String>,
    /// Only keys this run wrote. A key the machine already held is left alone:
    /// deleting it would break every other extension sharing it.
    written_secrets: Vec<String>,
}

impl ExtensionInstallTransaction {
    pub fn new(source: InstallSource) -> Self {
        Self {
            install_id: uuid::Uuid::new_v4().to_string(),
            source,
            supplied: HashMap::new(),
            enable: true,
            manager: None,
            undo: Undo::default(),
        }
    }

    /// Reuse an id from a stopped run so a resumed install keeps its identity.
    pub fn with_install_id(mut self, id: impl Into<String>) -> Self {
        self.install_id = id.into();
        self
    }

    pub fn with_values(mut self, values: HashMap<String, String>) -> Self {
        self.supplied = values;
        self
    }

    pub fn enabled(mut self, enable: bool) -> Self {
        self.enable = enable;
        self
    }

    /// The running session's extension manager, so a successful install can be
    /// hot-attached instead of waiting for a new chat.
    pub fn attach_to(mut self, manager: std::sync::Weak<ExtensionManager>) -> Self {
        self.manager = Some(manager);
        self
    }

    pub fn install_id(&self) -> &str {
        &self.install_id
    }

    /// Run to completion, or to the first outcome that stops it.
    ///
    /// Never returns `Err`: every stop is an [`InstallState`] a caller can
    /// report. A caller that had to tell "refused" from "the machinery broke"
    /// would need a policy for the difference, and the safe policy is identical.
    pub async fn run(
        mut self,
        mut policy: CredentialPolicy,
        cancel: Option<&CancellationToken>,
    ) -> InstallReport {
        match self.run_inner(&mut policy, cancel).await {
            Ok(report) => report,
            Err(e) => {
                let reason = format!("{e:#}");
                self.rollback();
                InstallReport::failed(self.install_id.clone(), reason)
            }
        }
    }

    async fn run_inner(
        &mut self,
        policy: &mut CredentialPolicy,
        cancel: Option<&CancellationToken>,
    ) -> anyhow::Result<InstallReport> {
        if !uv_available() {
            anyhow::bail!("{}", uv_missing_message());
        }

        // ── download ──────────────────────────────────────────────────────
        let (bundle_path, _temp) = match &self.source {
            InstallSource::LocalFile { path } => (path.clone(), None),
            InstallSource::Marketplace { url, .. } => {
                let (path, guard) = download_bundle(url).await?;
                (path, Some(guard))
            }
        };

        // ── validate ──────────────────────────────────────────────────────
        let bundle = BrxtBundle::open(&bundle_path)?;
        let manifest = bundle.manifest().clone();
        let skills = bundle.skills().to_vec();
        let key = name_to_key(&manifest.name);

        // ── extract + build ───────────────────────────────────────────────
        let install_dir = extensions_root().join(&manifest.name);
        let existed = install_dir.exists();
        bundle.extract_to(&install_dir)?;
        self.undo.install_dir = Some(install_dir.clone());
        self.undo.created_install_dir = !existed;
        run_uv_sync(&install_dir)?;

        // ── credentials ───────────────────────────────────────────────────
        //
        // `supplied` may already carry a declared-secret value (a `--secret`
        // flag, the desktop's own form). Those are written here rather than
        // being asked for again, and they go to the SAME place a card's answer
        // would: the credential store, with only the name recorded.
        let mut envs: HashMap<String, String> = HashMap::new();
        let mut env_keys: Vec<String> = Vec::new();
        self.apply_supplied(&manifest, &mut envs, &mut env_keys)?;

        let unmet: Vec<BrxtEnvVar> = manifest
            .env_vars
            .iter()
            .filter(|v| {
                !envs.contains_key(&v.key)
                    && !env_keys.contains(&v.key)
                    && !secret_already_stored(&v.key)
            })
            .cloned()
            .collect();
        let unmet_required: Vec<String> = unmet
            .iter()
            .filter(|v| v.required)
            .map(|v| v.key.clone())
            .collect();

        if !unmet.is_empty() {
            match self
                .collect_credentials(&manifest, &unmet, policy, cancel)
                .await?
            {
                Collected::Values {
                    configured_keys,
                    settings,
                } => {
                    for (k, v) in settings {
                        envs.insert(k, v);
                    }
                    for k in configured_keys {
                        if manifest
                            .env_vars
                            .iter()
                            .any(|v| v.key == k && v.secret)
                            && !env_keys.contains(&k)
                        {
                            env_keys.push(k.clone());
                            self.undo.written_secrets.push(k);
                        }
                    }
                }
                Collected::Cancelled => {
                    self.park_for_resume(&manifest, &bundle_path, &install_dir, &unmet);
                    self.rollback_config_only();
                    return Ok(self.report(InstallState::Cancelled, &manifest, &skills, &[]));
                }
                Collected::Refused => {
                    if unmet_required.is_empty() {
                        // Only optional values are missing, and nobody can be
                        // asked. That is not a failure: the extension runs.
                        debug!("Installing {} without its optional values", manifest.name);
                    } else {
                        self.park_for_resume(&manifest, &bundle_path, &install_dir, &unmet);
                        self.rollback_config_only();
                        return Ok(self.report(
                            InstallState::NeedsCredentials {
                                keys: unmet_required,
                            },
                            &manifest,
                            &skills,
                            &[],
                        ));
                    }
                }
            }
        }

        // ⚠ The last gate before registration, and the reason this type exists.
        // A required value that is still missing here means the extension cannot
        // authenticate, and registering it anyway is what made an agent-driven
        // shell install report success for something permanently broken.
        let still_missing: Vec<String> = manifest
            .required_vars()
            .filter(|v| {
                !envs.contains_key(&v.key)
                    && !env_keys.contains(&v.key)
                    && !secret_already_stored(&v.key)
            })
            .map(|v| v.key.clone())
            .collect();
        if !still_missing.is_empty() {
            self.park_for_resume(&manifest, &bundle_path, &install_dir, &unmet);
            self.rollback_config_only();
            return Ok(self.report(
                InstallState::NeedsCredentials {
                    keys: still_missing,
                },
                &manifest,
                &skills,
                &[],
            ));
        }

        // ── register ──────────────────────────────────────────────────────
        env_keys.sort();
        env_keys.dedup();
        let config = ExtensionConfig::Stdio {
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
            env_keys: env_keys.clone(),
            timeout: Some(300),
            bundled: None,
            available_tools: Vec::new(),
        };
        set_extension(ExtensionEntry {
            enabled: self.enable,
            config: config.clone(),
        });
        self.undo.config_key = Some(key.clone());
        if let InstallSource::Marketplace { registry_id, url } = &self.source {
            record_provenance(&manifest.name, registry_id, url, &install_dir);
        }

        // ── attach ────────────────────────────────────────────────────────
        let attached = self.attach(config).await;
        ResumableInstalls::global().forget(&self.install_id);
        Ok(self.report(
            if attached {
                InstallState::Attached
            } else {
                InstallState::Installed
            },
            &manifest,
            &skills,
            &env_keys,
        ))
    }

    /// Write the values the caller already had, splitting credentials from
    /// ordinary settings by what the manifest declared.
    fn apply_supplied(
        &mut self,
        manifest: &BrxtManifest,
        envs: &mut HashMap<String, String>,
        env_keys: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        let config = crate::config::Config::global();
        for (key, value) in &self.supplied {
            if value.trim().is_empty() {
                continue;
            }
            let declared_secret = manifest
                .env_vars
                .iter()
                .find(|v| &v.key == key)
                .map(|v| v.secret)
                // A key the manifest never declared is an ad-hoc setting. It is
                // NOT assumed secret: guessing wrong in that direction hides a
                // value the user expected to see in their config.
                .unwrap_or(false);
            if declared_secret {
                config
                    .set_secret(key, value)
                    .map_err(|e| anyhow::anyhow!("Failed to store `{key}`: {e}"))?;
                env_keys.push(key.clone());
                self.undo.written_secrets.push(key.clone());
            } else {
                envs.insert(key.clone(), value.clone());
            }
        }
        Ok(())
    }

    async fn collect_credentials(
        &self,
        manifest: &BrxtManifest,
        unmet: &[BrxtEnvVar],
        policy: &mut CredentialPolicy,
        cancel: Option<&CancellationToken>,
    ) -> anyhow::Result<Collected> {
        match policy {
            CredentialPolicy::Refuse => Ok(Collected::Refused),
            CredentialPolicy::Prompt(ask) => {
                let values = ask(unmet)?;
                let mut configured_keys = Vec::new();
                let mut settings = HashMap::new();
                let config = crate::config::Config::global();
                for (key, value) in values {
                    if value.trim().is_empty() {
                        continue;
                    }
                    let secret = unmet.iter().any(|v| v.key == key && v.secret);
                    if secret {
                        config
                            .set_secret(&key, &value)
                            .map_err(|e| anyhow::anyhow!("Failed to store `{key}`: {e}"))?;
                    } else {
                        settings.insert(key.clone(), value);
                    }
                    configured_keys.push(key);
                }
                Ok(Collected::Values {
                    configured_keys,
                    settings,
                })
            }
            CredentialPolicy::Ask {
                session_id,
                owner,
                ttl,
            } => {
                let spec = CredentialSpec {
                    destination: SecretDestination::ExtensionEnv {
                        extension_name: manifest.name.clone(),
                    },
                    vars: unmet.to_vec(),
                };
                let prompt = format!(
                    "{} needs {} value{} before it can run.",
                    manifest.display_name,
                    unmet.len(),
                    if unmet.len() == 1 { "" } else { "s" }
                );
                let (outcome, settings) = request_credentials(
                    session_id.as_deref(),
                    owner.as_deref(),
                    prompt,
                    spec,
                    if ttl.is_zero() {
                        DEFAULT_CREDENTIAL_TTL
                    } else {
                        *ttl
                    },
                    cancel,
                )
                .await;
                match outcome {
                    UserActionOutcome::SecretsConfigured { configured_keys } => {
                        Ok(Collected::Values {
                            configured_keys,
                            settings,
                        })
                    }
                    UserActionOutcome::Cancelled | UserActionOutcome::TimedOut => {
                        Ok(Collected::Cancelled)
                    }
                    UserActionOutcome::Failed { reason } => Err(anyhow::anyhow!(reason)),
                    // The registry refuses a value-bearing answer to a secrets
                    // card, so these are unreachable by construction. Treating
                    // them as a refusal keeps the fail-safe direction if that
                    // ever changes.
                    other => {
                        warn!("Unexpected outcome for a credential card: {other:?}");
                        Ok(Collected::Cancelled)
                    }
                }
            }
        }
    }

    async fn attach(&self, config: ExtensionConfig) -> bool {
        if !self.enable {
            return false;
        }
        let Some(manager) = self.manager.as_ref().and_then(|m| m.upgrade()) else {
            return false;
        };
        match manager.add_extension(config).await {
            Ok(()) => true,
            Err(e) => {
                // Not a rollback. The config entry is correct and the next chat
                // will load it; only *this* chat missed out, and saying so is
                // more useful than undoing a good install.
                warn!("Installed extension could not be attached to the running chat: {e}");
                false
            }
        }
    }

    fn park_for_resume(
        &self,
        manifest: &BrxtManifest,
        bundle_path: &std::path::Path,
        install_dir: &std::path::Path,
        pending: &[BrxtEnvVar],
    ) {
        ResumableInstalls::global().put(ResumableInstall {
            install_id: self.install_id.clone(),
            extension_name: manifest.name.clone(),
            display_name: manifest.display_name.clone(),
            bundle_path: bundle_path.to_path_buf(),
            install_dir: install_dir.to_path_buf(),
            pending_vars: pending.to_vec(),
        });
    }

    /// Undo the registration and the credentials, but keep the extracted tree
    /// and its built environment — that is the expensive half, it contains
    /// nothing sensitive, and a resumed install reuses it.
    fn rollback_config_only(&mut self) {
        if let Some(key) = self.undo.config_key.take() {
            remove_extension(&key);
        }
        revoke(&std::mem::take(&mut self.undo.written_secrets));
    }

    /// Undo everything this run created.
    fn rollback(&mut self) {
        self.rollback_config_only();
        if self.undo.created_install_dir {
            if let Some(dir) = self.undo.install_dir.take() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        ResumableInstalls::global().forget(&self.install_id);
    }

    fn report(
        &self,
        state: InstallState,
        manifest: &BrxtManifest,
        skills: &[BundledSkill],
        configured_keys: &[String],
    ) -> InstallReport {
        InstallReport {
            install_id: self.install_id.clone(),
            enabled: self.enable && state.is_success(),
            state,
            extension_name: Some(manifest.name.clone()),
            display_name: Some(manifest.display_name.clone()),
            configured_keys: configured_keys.to_vec(),
            skills: skills.to_vec(),
        }
    }
}

enum Collected {
    Values {
        configured_keys: Vec<String>,
        settings: HashMap<String, String>,
    },
    Cancelled,
    Refused,
}

/// Issue #56 Task 43 (DR-23). Record where a marketplace bundle came from, so
/// the privacy tier is re-derived from a stable registry id rather than from a
/// config name the user (or the model) can rename.
///
/// `install_dir` is what survives that rename — the `--directory` argument
/// cannot move without breaking the launch — so it is recorded alongside.
fn record_provenance(name: &str, registry_id: &str, url: &str, install_dir: &std::path::Path) {
    let provenance = crate::privacy::provenance::ExtensionProvenance {
        registry_id: registry_id.to_string(),
        install_dir: Some(install_dir.display().to_string()),
        source_url: Some(url.to_string()),
        bundle_sha256: None,
        recorded_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    if let Err(e) = crate::privacy::provenance::record(name, provenance) {
        // A missing provenance record fails CLOSED at read time — the tier
        // falls back to the config-name join — so this is a degradation, not a
        // reason to undo a good install.
        warn!("Could not record marketplace provenance for {name}: {e}");
    }
}

/// A downloaded bundle, and the temp dir that owns it.
async fn download_bundle(url: &str) -> anyhow::Result<(PathBuf, tempfile::TempDir)> {
    let parsed = url::Url::parse(url).map_err(|_| anyhow::anyhow!("Not a valid URL: {url}"))?;
    if parsed.scheme() != "https" {
        anyhow::bail!("Refusing to download an extension over {}", parsed.scheme());
    }
    let response = reqwest::get(url)
        .await
        .map_err(|e| anyhow::anyhow!("Could not download the bundle: {e}"))?;
    if !response.status().is_success() {
        anyhow::bail!("Could not download the bundle: HTTP {}", response.status());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("Could not read the bundle: {e}"))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("bundle.brxt");
    std::fs::write(&path, &bytes)?;
    Ok((path, dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The report is what a tool result carries back to the model. Nothing in
    /// its shape can hold a value, and this asserts that against a state that
    /// names every key it is still waiting for.
    #[test]
    fn a_report_serialises_key_names_and_never_a_value() {
        let report = InstallReport {
            install_id: "i-1".to_string(),
            state: InstallState::NeedsCredentials {
                keys: vec!["SPOKEAGENT_PASSCODE".to_string()],
            },
            extension_name: Some("spokeagent".to_string()),
            display_name: Some("SPOKE Agent".to_string()),
            configured_keys: vec!["SPOKEAGENT_PASSCODE".to_string()],
            skills: Vec::new(),
            enabled: false,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("SPOKEAGENT_PASSCODE"));
        assert!(json.contains("needsCredentials"));
        assert!(!json.contains("value"));
    }

    #[test]
    fn only_a_registered_or_attached_install_counts_as_success() {
        assert!(InstallState::Attached.is_success());
        assert!(InstallState::Installed.is_success());
        assert!(!InstallState::Cancelled.is_success());
        assert!(!InstallState::NeedsCredentials { keys: vec![] }.is_success());
        assert!(!InstallState::Failed {
            reason: "x".to_string()
        }
        .is_success());
    }

    #[tokio::test]
    async fn an_http_bundle_url_is_refused_before_any_request_is_made() {
        let err = download_bundle("http://example.test/x.brxt")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Refusing to download"));
    }
}
