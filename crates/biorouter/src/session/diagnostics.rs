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
        out.push_str("Last turn (context-window occupancy — NOT the billed total):\n");
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

        for entry in log_files.iter().rev().take(LOGS_TO_KEEP) {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default();
            zip.start_file(format!("logs/{}", name), options)?;
            zip.write_all(&fs::read(&path)?)?;
        }

        let session_data = session_manager.export_session(session_id).await?;
        zip.start_file("session.json", options)?;
        zip.write_all(session_data.as_bytes())?;

        if config_path.exists() {
            zip.start_file("config.yaml", options)?;
            zip.write_all(&fs::read(&config_path)?)?;
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
