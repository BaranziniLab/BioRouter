use crate::config::base::Config;
use crate::config::extensions::get_enabled_extensions;
use crate::config::paths::Paths;
use crate::providers::utils::LOGS_TO_KEEP;
use crate::session::session_manager::{ModelUsageRow, UsageTotals};
use crate::session::SessionManager;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::io::Write;
use utoipa::ToSchema;
use zip::write::FileOptions;
use zip::ZipWriter;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemInfo {
    pub app_version: String,
    pub os: String,
    pub os_version: String,
    pub architecture: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub enabled_extensions: Vec<String>,
}

impl SystemInfo {
    pub fn collect() -> Self {
        let config = Config::global();
        let provider = config.get_biorouter_provider().ok();
        let model = config.get_biorouter_model().ok();
        let enabled_extensions = get_enabled_extensions()
            .into_iter()
            .map(|ext| ext.name().to_string())
            .collect();

        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            os_version: sys_info::os_release().unwrap_or_else(|_| "unknown".to_string()),
            architecture: std::env::consts::ARCH.to_string(),
            provider,
            model,
            enabled_extensions,
        }
    }

    pub fn to_text(&self) -> String {
        format!(
            "App Version: {}\n\
             OS: {}\n\
             OS Version: {}\n\
             Architecture: {}\n\
             Provider: {}\n\
             Model: {}\n\
             Enabled Extensions: {}\n\
             Timestamp: {}\n",
            self.app_version,
            self.os,
            self.os_version,
            self.architecture,
            self.provider.as_deref().unwrap_or("unknown"),
            self.model.as_deref().unwrap_or("unknown"),
            self.enabled_extensions.join(", "),
            chrono::Utc::now().to_rfc3339()
        )
    }
}

pub fn get_system_info() -> SystemInfo {
    SystemInfo::collect()
}

/// Token accounting for one session plus month-to-date totals, so a future
/// "my dashboard says N but Biorouter shows M" report is self-diagnosing.
///
/// The crux of issue #1: the chat headline used to show the LAST TURN's
/// `total_tokens` (context-window occupancy) while the vendor bills the
/// ACCUMULATED sum across every turn. This bundle records both, per model, plus
/// the cache buckets, so the two numbers can be reconciled without re-running.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UsageDiagnostics {
    pub session_id: String,
    /// Last turn's context-window occupancy (what the old headline showed).
    pub last_turn_total_tokens: Option<i32>,
    pub last_turn_input_tokens: Option<i32>,
    pub last_turn_output_tokens: Option<i32>,
    /// Lifetime accumulated counters. New events are billed totals; legacy
    /// databases can contain context totals, so the ledger summary is authoritative.
    pub accumulated_total_tokens: Option<i64>,
    pub accumulated_input_tokens: Option<i64>,
    pub accumulated_output_tokens: Option<i64>,
    /// Cache tokens summed over this session's turns.
    pub session_cache_read_tokens: Option<i64>,
    pub session_cache_creation_tokens: Option<i64>,
    /// Per-`(model, provider)` breakdown for this session.
    pub per_model: Vec<ModelUsageRow>,
    /// Current local month, `YYYY-MM`.
    pub month: String,
    pub month_to_date: UsageTotals,
    pub all_time: UsageTotals,
}

impl UsageDiagnostics {
    pub async fn collect(
        session_manager: &SessionManager,
        session_id: &str,
    ) -> anyhow::Result<Self> {
        let session = session_manager.get_session(session_id, false).await?;
        let per_model = session_manager.get_session_model_usage(session_id).await?;
        let summary = session_manager.get_usage_summary().await?;

        let session_cache_read_tokens = per_model
            .iter()
            .try_fold(0_i64, |sum, row| Some(sum + row.cache_read_tokens?));
        let session_cache_creation_tokens = per_model
            .iter()
            .try_fold(0_i64, |sum, row| Some(sum + row.cache_creation_tokens?));

        Ok(Self {
            session_id: session_id.to_string(),
            last_turn_total_tokens: session.total_tokens,
            last_turn_input_tokens: session.input_tokens,
            last_turn_output_tokens: session.output_tokens,
            accumulated_total_tokens: session.accumulated_total_tokens,
            accumulated_input_tokens: session.accumulated_input_tokens,
            accumulated_output_tokens: session.accumulated_output_tokens,
            session_cache_read_tokens,
            session_cache_creation_tokens,
            per_model,
            month: summary.month,
            month_to_date: summary.month_to_date,
            all_time: summary.all_time,
        })
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Session: {}\n\n", self.session_id));
        out.push_str("Last turn (context-window occupancy, NOT the billed total):\n");
        out.push_str(&format!(
            "  total={}  input={}  output={}\n\n",
            opt_i32(self.last_turn_total_tokens),
            opt_i32(self.last_turn_input_tokens),
            opt_i32(self.last_turn_output_tokens),
        ));
        out.push_str("Accumulated session counters (legacy rows may use context totals):\n");
        out.push_str(&format!(
            "  total={}  input={}  output={}\n",
            opt_i64(self.accumulated_total_tokens),
            opt_i64(self.accumulated_input_tokens),
            opt_i64(self.accumulated_output_tokens),
        ));
        out.push_str(&format!(
            "  cache_read={}  cache_creation={}\n\n",
            opt_i64(self.session_cache_read_tokens),
            opt_i64(self.session_cache_creation_tokens),
        ));

        out.push_str("Per-model (this session):\n");
        for row in &self.per_model {
            out.push_str(&format!(
                "  {} / {}: input={} output={} cache_read={} cache_creation={} total={} turns={}\n",
                row.provider.as_deref().unwrap_or("unknown"),
                row.model_id.as_deref().unwrap_or("unknown"),
                row.input_tokens,
                row.output_tokens,
                opt_i64(row.cache_read_tokens),
                opt_i64(row.cache_creation_tokens),
                opt_i64(row.total_tokens),
                row.turns,
            ));
        }

        out.push_str(&format!("\nMonth-to-date ({}):\n", self.month));
        out.push_str(&totals_text(&self.month_to_date));
        out.push_str("\nAll-time:\n");
        out.push_str(&totals_text(&self.all_time));
        out
    }
}

fn opt_i32(v: Option<i32>) -> String {
    v.map_or_else(|| "-".to_string(), |n| n.to_string())
}

fn opt_i64(v: Option<i64>) -> String {
    v.map_or_else(|| "-".to_string(), |n| n.to_string())
}

fn totals_text(t: &UsageTotals) -> String {
    let cost = t.cost.map_or_else(
        || "unpriced".to_string(),
        |c| {
            let flag = match (t.has_unpriced, t.cost_excludes_cache) {
                (true, true) => " (known subtotal; unpriced usage and cache excluded)",
                (true, false) => " (known subtotal; some usage unpriced)",
                (false, true) => " (known subtotal; cache excluded)",
                (false, false) => "",
            };
            format!("${c:.4}{flag}")
        },
    );
    format!(
        "  input={} output={} cache_read={} cache_creation={} total={} turns={} cost={}\n",
        t.input_tokens,
        t.output_tokens,
        opt_i64(t.cache_read_tokens),
        opt_i64(t.cache_creation_tokens),
        opt_i64(t.total_tokens),
        t.turns,
        cost,
    )
}

/// The session a `llm_request.*.jsonl` names in its header line, if any.
///
/// `RequestLog::start` writes that header first and always (see
/// `crate::providers::utils::RequestLog`). `None` means the file predates that
/// header, is not JSONL, or was opened outside a scoped session — in every one
/// of those cases the file's provenance is unknown.
fn log_session_id(contents: &str) -> Option<String> {
    let header: serde_json::Value = serde_json::from_str(contents.lines().next()?).ok()?;
    header.get("session_id")?.as_str().map(str::to_string)
}

/// Whether a log file may be shipped in the bundle for `session_id`.
///
/// **Fail-closed on purpose.** An unattributable log is excluded rather than
/// included: the bundle is emailed to a third party, so "I could not tell whose
/// conversation this was" has to mean "do not send it". The previous rule —
/// newest ten files, whoever produced them — put another chat's prompts into a
/// bug report about this one.
fn log_belongs_to_session(contents: &str, session_id: &str) -> bool {
    log_session_id(contents).is_some_and(|id| id == session_id)
}

/// Does this key name a credential? Matched case-insensitively on a substring,
/// so `SPOKEAGENT_PASSCODE`, `api_key` and `GITHUB_TOKEN` all hit.
///
/// Deliberately broad: over-redacting a bundle costs a support engineer one
/// follow-up question, under-redacting one mails a working credential to an
/// inbox and, for a GitHub issue, to the public.
fn is_secret_key(key: &str) -> bool {
    const NEEDLES: &[&str] = &[
        "key",
        "token",
        "secret",
        "password",
        "passcode",
        "passwd",
        "credential",
        "auth",
        "cookie",
        "session",
        "signature",
        "private",
    ];
    let key = key.to_ascii_lowercase();
    NEEDLES.iter().any(|needle| key.contains(needle))
}

const REDACTED: &str = "[redacted by diagnostics bundle]";

/// Replace credential-shaped values in `config.yaml` before it is zipped.
///
/// Two rules, because extension entries need both: any value whose key looks
/// like a credential, and **every** value inside an `envs` map (a `.brxt`
/// install writes the extension's environment straight into the config entry,
/// and `CDW_USERNAME` names a person while matching no credential needle).
///
/// A file that does not parse is replaced wholesale rather than passed through:
/// the fallback for "I could not inspect this" is not "send it anyway".
fn redact_config_yaml(raw: &str) -> String {
    let Ok(mut doc) = serde_yaml::from_str::<serde_yaml::Value>(raw) else {
        return "# config.yaml was omitted from this bundle: it could not be parsed as YAML,\n\
                # so its values could not be checked for credentials.\n"
            .to_string();
    };
    redact_yaml_value(&mut doc, false);
    serde_yaml::to_string(&doc).unwrap_or_else(|_| {
        "# config.yaml was omitted from this bundle: it could not be re-serialized.\n".to_string()
    })
}

fn redact_yaml_value(value: &mut serde_yaml::Value, redact_every_leaf: bool) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            let keys: Vec<serde_yaml::Value> = map.keys().cloned().collect();
            for key in keys {
                let key_str = key.as_str().unwrap_or_default().to_string();
                let Some(child) = map.get_mut(&key) else {
                    continue;
                };
                if redact_every_leaf || is_secret_key(&key_str) {
                    *child = serde_yaml::Value::String(REDACTED.to_string());
                    continue;
                }
                // A `.brxt` install writes the extension's whole environment
                // here; treat all of it as credential material.
                redact_yaml_value(child, key_str.eq_ignore_ascii_case("envs"));
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for item in items {
                redact_yaml_value(item, redact_every_leaf);
            }
        }
        _ => {}
    }
}

pub async fn generate_diagnostics(
    session_manager: &SessionManager,
    session_id: &str,
) -> anyhow::Result<Vec<u8>> {
    let logs_dir = Paths::in_state_dir("logs");
    let config_dir = Paths::config_dir();
    let config_path = config_dir.join("config.yaml");
    let data_dir = Paths::data_dir();

    let system_info = SystemInfo::collect();

    let mut buffer = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut log_files: Vec<_> = match fs::read_dir(&logs_dir) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "jsonl")
                })
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };

        log_files.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        // Newest first, but only this session's logs — see
        // `log_belongs_to_session`. `take(LOGS_TO_KEEP)` on the *unfiltered*
        // list is what shipped other chats' prompts: the ten newest files on
        // this machine have nothing to do with the session being reported.
        let mut shipped = 0usize;
        for entry in log_files.iter().rev() {
            if shipped == LOGS_TO_KEEP {
                break;
            }
            let path = entry.path();
            let bytes = fs::read(&path)?;
            if !log_belongs_to_session(&String::from_utf8_lossy(&bytes), session_id) {
                continue;
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default();
            zip.start_file(format!("logs/{}", name), options)?;
            zip.write_all(&bytes)?;
            shipped += 1;
        }

        let session_data = session_manager.export_session(session_id).await?;
        zip.start_file("session.json", options)?;
        zip.write_all(session_data.as_bytes())?;

        if config_path.exists() {
            let raw = fs::read(&config_path)?;
            zip.start_file("config.yaml", options)?;
            zip.write_all(redact_config_yaml(&String::from_utf8_lossy(&raw)).as_bytes())?;
        }

        zip.start_file("system.txt", options)?;
        zip.write_all(system_info.to_text().as_bytes())?;

        // Token accounting so a usage-mismatch report is self-diagnosing
        // (last-turn vs accumulated vs MTD, per model, incl. cache buckets).
        if let Ok(usage) = UsageDiagnostics::collect(session_manager, session_id).await {
            zip.start_file("usage.txt", options)?;
            zip.write_all(usage.to_text().as_bytes())?;
        }

        let schedule_json = data_dir.join("schedule.json");
        if schedule_json.exists() {
            zip.start_file("schedule.json", options)?;
            zip.write_all(&fs::read(&schedule_json)?)?;
        }

        let scheduled_workflows_dir = data_dir.join("scheduled_workflows");
        if scheduled_workflows_dir.exists() && scheduled_workflows_dir.is_dir() {
            for entry in fs::read_dir(&scheduled_workflows_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy())
                        .unwrap_or_default();
                    zip.start_file(format!("scheduled_workflows/{}", name), options)?;
                    zip.write_all(&fs::read(&path)?)?;
                }
            }
        }

        zip.finish()?;
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::session_manager::{SessionType, UsageLedgerEntry};
    use tempfile::TempDir;

    #[tokio::test(flavor = "current_thread")]
    async fn diagnostics_bundle_succeeds_before_any_logs_exist() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        let _guard = env_lock::lock_env([("BIOROUTER_PATH_ROOT", Some(root.as_str()))]);
        let sm = SessionManager::new(temp.path().join("sessions"));
        let session = sm
            .create_session(
                temp.path().to_path_buf(),
                "diagnostics".into(),
                SessionType::User,
            )
            .await
            .unwrap();

        let bundle = generate_diagnostics(&sm, &session.id).await.unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bundle)).unwrap();

        assert!(archive.by_name("session.json").is_ok());
        assert!(archive.by_name("system.txt").is_ok());
        assert!(archive.by_name("usage.txt").is_ok());
    }

    /// Read every entry of a bundle as `(name, bytes)`.
    fn bundle_entries(bundle: Vec<u8>) -> Vec<(String, Vec<u8>)> {
        use std::io::Read;
        let mut archive = zip::ZipArchive::new(Cursor::new(bundle)).unwrap();
        (0..archive.len())
            .map(|i| {
                let mut file = archive.by_index(i).unwrap();
                let name = file.name().to_string();
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).unwrap();
                (name, bytes)
            })
            .collect()
    }

    /// ⚠ The assertion is on the **zip's bytes**, not on the filter's return
    /// value: a correct filter whose result the bundler ignores is the failure
    /// this test exists to catch.
    #[tokio::test(flavor = "current_thread")]
    async fn the_bundle_ships_only_the_named_session_s_llm_logs() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        let _guard = env_lock::lock_env([("BIOROUTER_PATH_ROOT", Some(root.as_str()))]);
        let sm = SessionManager::new(temp.path().join("sessions"));
        let session = sm
            .create_session(
                temp.path().to_path_buf(),
                "diagnostics".into(),
                SessionType::User,
            )
            .await
            .unwrap();

        let logs_dir = Paths::in_state_dir("logs");
        fs::create_dir_all(&logs_dir).unwrap();

        // Three logs on this machine: one from the session being reported, one
        // from somebody else's chat, one from before headers existed.
        fs::write(
            logs_dir.join("llm_request.0.jsonl"),
            format!(
                "{{\"session_id\":\"{}\",\"model_config\":{{}}}}\n{{\"data\":\"MINE\"}}\n",
                session.id
            ),
        )
        .unwrap();
        fs::write(
            logs_dir.join("llm_request.1.jsonl"),
            "{\"session_id\":\"some-other-chat\"}\n{\"data\":\"THEIR PRIVATE PROMPT\"}\n",
        )
        .unwrap();
        fs::write(
            logs_dir.join("llm_request.2.jsonl"),
            "{\"model_config\":{}}\n{\"data\":\"UNATTRIBUTED PROMPT\"}\n",
        )
        .unwrap();

        let entries = bundle_entries(generate_diagnostics(&sm, &session.id).await.unwrap());

        let log_names: Vec<&str> = entries
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| name.starts_with("logs/"))
            .collect();
        assert_eq!(log_names, vec!["logs/llm_request.0.jsonl"]);

        let all_bytes: Vec<u8> = entries.iter().flat_map(|(_, b)| b.clone()).collect();
        let all = String::from_utf8_lossy(&all_bytes);
        assert!(
            all.contains("MINE"),
            "the reported session's own log is missing"
        );
        assert!(
            !all.contains("THEIR PRIVATE PROMPT"),
            "another chat's transcript is in the bundle"
        );
        assert!(
            !all.contains("UNATTRIBUTED PROMPT"),
            "an unattributable transcript is in the bundle"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_bundled_config_has_its_credentials_redacted() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        let _guard = env_lock::lock_env([("BIOROUTER_PATH_ROOT", Some(root.as_str()))]);
        let sm = SessionManager::new(temp.path().join("sessions"));
        let session = sm
            .create_session(temp.path().to_path_buf(), "diag".into(), SessionType::User)
            .await
            .unwrap();

        let config_dir = Paths::config_dir();
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("config.yaml"),
            "BIOROUTER_PROVIDER: anthropic\n\
             ANTHROPIC_API_KEY: sk-ant-LIVE-CREDENTIAL\n\
             extensions:\n\
             \x20 spoke:\n\
             \x20   name: SPOKEAgent\n\
             \x20   envs:\n\
             \x20     SPOKEAGENT_PASSCODE: hunter2\n\
             \x20     CDW_USERNAME: CAMPUS\\\\jdoe\n",
        )
        .unwrap();

        let entries = bundle_entries(generate_diagnostics(&sm, &session.id).await.unwrap());
        let (_, config) = entries
            .iter()
            .find(|(name, _)| name == "config.yaml")
            .expect("config.yaml is in the bundle");
        let config = String::from_utf8_lossy(config);

        assert!(!config.contains("sk-ant-LIVE-CREDENTIAL"), "{config}");
        assert!(!config.contains("hunter2"), "{config}");
        assert!(!config.contains("jdoe"), "{config}");
        // Still diagnostic: the shape of the config survives.
        assert!(config.contains("BIOROUTER_PROVIDER: anthropic"), "{config}");
        assert!(config.contains("SPOKEAgent"), "{config}");
    }

    #[test]
    fn an_unattributable_log_is_not_this_session_s() {
        assert!(log_belongs_to_session(
            "{\"session_id\":\"abc\"}\n{\"data\":1}",
            "abc"
        ));
        assert!(!log_belongs_to_session("{\"session_id\":\"abc\"}\n", "def"));
        assert!(!log_belongs_to_session("{\"session_id\":null}\n", "abc"));
        assert!(!log_belongs_to_session("{\"model_config\":{}}\n", "abc"));
        assert!(!log_belongs_to_session("not json at all\n", "abc"));
        assert!(!log_belongs_to_session("", "abc"));
    }

    #[test]
    fn redaction_covers_credential_keys_and_every_extension_env() {
        let out = redact_config_yaml(
            "OPENAI_API_KEY: sk-live\n\
             GITHUB_TOKEN: ghp_live\n\
             harmless: keep-me\n\
             extensions:\n\
             \x20 a:\n\
             \x20   envs:\n\
             \x20     ANY_NAME_AT_ALL: value\n",
        );
        assert!(!out.contains("sk-live"), "{out}");
        assert!(!out.contains("ghp_live"), "{out}");
        assert!(!out.contains("value"), "{out}");
        assert!(out.contains("keep-me"), "{out}");
    }

    #[test]
    fn unparseable_config_is_replaced_rather_than_shipped() {
        // A tab in the indentation: YAML forbids it, so this cannot parse.
        let out = redact_config_yaml("root:\n\tANTHROPIC_API_KEY: sk-live\n");
        assert!(!out.contains("sk-live"), "{out}");
        assert!(out.starts_with('#'), "{out}");
    }

    #[tokio::test]
    async fn usage_diagnostics_records_cache_and_distinguishes_last_turn_from_accumulated() {
        let temp = TempDir::new().unwrap();
        let sm = SessionManager::new(temp.path().to_path_buf());
        let session = sm
            .create_session(temp.path().to_path_buf(), "diag".into(), SessionType::User)
            .await
            .unwrap();

        sm.apply_usage_event(UsageLedgerEntry {
            event_key: "diagnostics-provider-call-1".to_string(),
            session_id: session.id.clone(),
            schedule_id: None,
            current_total_tokens: Some(740),
            current_input_tokens: Some(100),
            current_output_tokens: Some(40),
            billed_total_tokens: Some(740),
            input_tokens: Some(100),
            output_tokens: Some(40),
            model_id: Some("claude-sonnet-4-20250514".to_string()),
            provider: Some("anthropic".to_string()),
            cache_read_tokens: Some(500),
            cache_creation_tokens: Some(100),
        })
        .await
        .unwrap();
        sm.apply_usage_event(UsageLedgerEntry {
            event_key: "diagnostics-provider-call-2".to_string(),
            session_id: session.id.clone(),
            schedule_id: None,
            current_total_tokens: Some(215),
            current_input_tokens: Some(10),
            current_output_tokens: Some(5),
            billed_total_tokens: Some(215),
            input_tokens: Some(10),
            output_tokens: Some(5),
            model_id: Some("claude-sonnet-4-20250514".to_string()),
            provider: Some("anthropic".to_string()),
            cache_read_tokens: Some(200),
            cache_creation_tokens: Some(0),
        })
        .await
        .unwrap();

        let diag = UsageDiagnostics::collect(&sm, &session.id).await.unwrap();
        assert_eq!(diag.last_turn_total_tokens, Some(215));
        assert_eq!(diag.last_turn_input_tokens, Some(10));
        assert_eq!(diag.last_turn_output_tokens, Some(5));
        assert_eq!(diag.accumulated_total_tokens, Some(955));
        assert_eq!(diag.accumulated_input_tokens, Some(110));
        assert_eq!(diag.accumulated_output_tokens, Some(45));
        // Session cache totals sum across turns: read 500+200=700, creation 100.
        assert_eq!(diag.session_cache_read_tokens, Some(700));
        assert_eq!(diag.session_cache_creation_tokens, Some(100));
        assert_eq!(diag.per_model.len(), 1);
        assert_eq!(diag.per_model[0].cache_read_tokens, Some(700));
        assert_eq!(diag.per_model[0].turns, 2);
        // MTD picks up both turns; priced because Claude is on the pricing card.
        assert!(diag.month_to_date.cost.is_some());
        assert_eq!(diag.month_to_date.cache_read_tokens, Some(700));

        let text = diag.to_text();
        assert!(text.contains("Last turn"));
        assert!(text.contains("Accumulated session counters"));
        assert!(text.contains("cache_read=700"));
        assert!(text.contains("Month-to-date"));
    }
}
