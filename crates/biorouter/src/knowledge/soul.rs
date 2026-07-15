//! The built-in **Soul** knowledge base and its self-maintaining machinery.
//!
//! Soul is a personal, initially-empty knowledge base installed automatically
//! the first time a user runs BioRouter. It accumulates durable facts about the
//! user — how they approach scientific questions, which tools and commands they
//! reach for, the shape of their tool calls and the responses they act on, and
//! personal details they reveal (name, occupation, preferences). A built-in
//! **"Meditation"** workflow, an **update-soul** skill, and a daily 3:00 AM
//! scheduled job ("Daily Meditation") keep it growing from the user's
//! conversation history.
//!
//! [`install`] is idempotent and safe to call on every startup: it only creates
//! what is missing and never overwrites user edits.
//!
//! The skill is named `update-soul` (it was previously `soul-writer`);
//! [`ensure_soul_skill`] removes the stale `soul-writer` folder on startup so
//! users don't see a duplicate after upgrading.

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::paths::Paths;
use crate::scheduler::ScheduledJob;
use crate::scheduler_trait::SchedulerTrait;
use biorouter_mcp::knowledge::service::KnowledgeService;

pub const SOUL_KB_ID: &str = "soul";
pub const SOUL_KB_NAME: &str = "Soul";
pub const MEDITATION_WORKFLOW_FILE: &str = "meditation.yaml";
/// Job ids double as on-disk filenames in the scheduler, so the id stays a
/// slug; the UI renders it as "Daily Meditation".
pub const MEDITATION_SCHEDULE_ID: &str = "daily-meditation";
pub const SOUL_SKILL_DIR: &str = "update-soul";
/// The skill's previous directory name, removed on startup so upgraded users
/// don't see a stale duplicate alongside the renamed `update-soul` skill.
pub const SOUL_SKILL_DIR_LEGACY: &str = "soul-writer";
/// 6-field cron (sec min hour dom mon dow) — every day at 03:00 local time.
pub const MEDITATION_CRON: &str = "0 0 3 * * *";

/// Warm parchment tone distinct from the default KB colour.
pub const SOUL_COLOR: &str = "#9c6b3f";

/// Install every Soul component that is missing. Best-effort: a failure in one
/// component is logged and does not abort the others or block startup.
pub async fn install(scheduler: &Arc<dyn SchedulerTrait>) {
    if let Err(e) = ensure_soul_kb() {
        tracing::warn!("Soul: failed to create knowledge base: {e}");
    }
    if let Err(e) = ensure_soul_skill() {
        tracing::warn!("Soul: failed to install skill: {e}");
    }
    match ensure_meditation_workflow() {
        Ok(path) => {
            if let Err(e) = ensure_meditation_schedule(scheduler, path).await {
                tracing::warn!("Soul: failed to register Daily Meditation schedule: {e}");
            }
        }
        Err(e) => tracing::warn!("Soul: failed to install Meditation workflow: {e}"),
    }
}

/// Install the assets that don't need a running scheduler (KB, skill, workflow
/// file). Used on surfaces that have no scheduler handy (e.g. CLI-only flows).
pub fn install_assets() {
    let _ = ensure_soul_kb();
    let _ = ensure_soul_skill();
    let _ = ensure_meditation_workflow();
}

/// Create the empty Soul KB if it does not already exist.
pub fn ensure_soul_kb() -> anyhow::Result<()> {
    let svc = KnowledgeService::new_default()?;
    if svc.list_bases()?.iter().any(|b| b.id == SOUL_KB_ID) {
        return Ok(());
    }
    svc.create_base(SOUL_KB_ID, SOUL_KB_NAME, Some(SOUL_COLOR))?;
    tracing::info!("Soul: created built-in knowledge base '{SOUL_KB_ID}'");
    Ok(())
}

/// Write the "Meditation" workflow to the global workflow library if it is
/// not already present. Returns the workflow file path either way.
pub fn ensure_meditation_workflow() -> anyhow::Result<PathBuf> {
    let dir = Paths::config_dir().join("workflows");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(MEDITATION_WORKFLOW_FILE);
    if !path.exists() {
        std::fs::write(&path, MEDITATION_WORKFLOW_YAML)?;
        tracing::info!("Soul: installed Meditation workflow at {}", path.display());
    }
    Ok(path)
}

/// Write the update-soul skill if it is not already present.
pub fn ensure_soul_skill() -> anyhow::Result<()> {
    // Drop the skill's former directory (`soul-writer`) so upgraded users don't
    // see a stale duplicate next to the renamed `update-soul` skill. It is a
    // built-in instruction asset, never user data, so removing it is safe.
    let legacy = Paths::config_dir()
        .join("skills")
        .join(SOUL_SKILL_DIR_LEGACY);
    if legacy.exists() {
        if let Err(e) = std::fs::remove_dir_all(&legacy) {
            tracing::warn!(
                "Soul: failed to remove legacy skill at {}: {e}",
                legacy.display()
            );
        } else {
            tracing::info!("Soul: removed legacy skill at {}", legacy.display());
        }
    }
    let dir = Paths::config_dir().join("skills").join(SOUL_SKILL_DIR);
    let skill_file = dir.join("SKILL.md");
    // Refresh the shipped skill when it is missing or out of date so updates to
    // its description and guidance propagate on the next launch (mirrors the
    // built-in about-biorouter skill). This is a built-in instruction asset, not
    // user data, so rewriting the shipped content is safe.
    let up_to_date = std::fs::read_to_string(&skill_file)
        .map(|existing| existing == SOUL_SKILL_MD)
        .unwrap_or(false);
    if up_to_date {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(&skill_file, SOUL_SKILL_MD)?;
    tracing::info!("Soul: installed/updated skill at {}", skill_file.display());
    Ok(())
}

/// Register the daily 3:00 AM Meditation job if it is not already scheduled.
pub async fn ensure_meditation_schedule(
    scheduler: &Arc<dyn SchedulerTrait>,
    workflow_path: PathBuf,
) -> anyhow::Result<()> {
    let already = scheduler
        .list_scheduled_jobs()
        .await
        .into_iter()
        .any(|j| j.id == MEDITATION_SCHEDULE_ID);
    if already {
        return Ok(());
    }
    let job = ScheduledJob {
        id: MEDITATION_SCHEDULE_ID.to_string(),
        source: workflow_path.to_string_lossy().into_owned(),
        cron: MEDITATION_CRON.to_string(),
        last_run: None,
        currently_running: false,
        paused: false,
        current_session_id: None,
        process_start_time: None,
        run_count: 0,
        max_runs: None,
    };
    scheduler
        .add_scheduled_job(job, true)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    tracing::info!("Soul: registered Daily Meditation 03:00 schedule '{MEDITATION_SCHEDULE_ID}'");
    Ok(())
}

/// The "Meditation" workflow definition. It uses the user's configured
/// default provider/model (no `settings` override), loads the update-soul
/// skill, focuses on the Soul KB, and instructs the agent to digest recent
/// user interactions into durable, personalised knowledge.
pub const MEDITATION_WORKFLOW_YAML: &str = r#"version: 1.0.0
title: Meditation
description: >-
  Review the user's recent Biorouter sessions and save what matters about them
  into the built-in "Soul" knowledge base: how they approach scientific
  questions, the tools and commands they use, the tool responses they rely on,
  and lasting personal details such as name, role, and preferences. Runs daily
  at 3:00 AM by default as "Daily Meditation".
instructions: |-
  You are maintaining the user's personal "Soul" knowledge base (id: soul).
  Follow the `update-soul` skill exactly.

  Goal: turn the user's recent interaction history into durable, high-signal,
  personalised knowledge about THE USER — not a summary of every chat.

  Procedure:
  1. Find the user's recent REAL chat sessions with the `chatrecall` tool
     (search mode, with a recent date range and broad queries). Collect their
     session ids. Skip scheduled-job sessions (names starting with
     "Scheduled job:") — especially this very session.
  2. Call `platform__ingest_conversation` with EXPLICIT `session_ids` for the
     most relevant recent session(s), targeting the `soul` knowledge base.
     Never omit `session_ids`: omitting it defaults to the current scheduled
     session, which contains nothing about the user. Prefer sessions since
     the last Meditation. If chatrecall surfaces no real user sessions, stop
     and make no changes.
  3. Prioritise capturing:
       - the way the user approaches different scientific questions,
       - the tools and extensions they use and how they call them,
       - the commands they run and the responses they rely on,
       - personal information they reveal: name, role/occupation, affiliations,
         stated preferences and working style.
  4. Explicitly DISCARD low-value noise: greetings, chit-chat, one-off
     irrelevant details, and anything that would not help a future assistant
     serve this user better.
  5. Keep the Soul coherent: link related entities/concepts with [[wiki-links]],
     avoid duplicating facts already present, and prefer updating an existing
     page over creating a near-duplicate.

  If there is nothing new worth recording, say so and make no changes.
extensions:
- type: platform
  name: skills
  description: Load and use skills from relevant directories
  bundled: true
  available_tools: []
- type: platform
  name: todo
  description: Enable a todo list for biorouter so it can keep track of what it is doing
  bundled: true
  available_tools: []
- type: platform
  name: chatrecall
  description: Search past conversations and load session summaries for contextual memory
  bundled: true
  available_tools: []
knowledge_bases:
  default: soul
  visible:
  - soul
skills:
- update-soul
activities:
- Update my Soul from recent interactions
- Learn my preferences and working style
- Record the tools and commands I use
parameters: []
"#;

/// The update-soul skill — guidance the agent loads when writing the Soul.
pub const SOUL_SKILL_MD: &str = r#"---
name: update-soul
description: >-
  Update the user's personal "Soul" knowledge base from their conversation
  history. Load this skill when running a Meditation, or whenever asked to learn
  about, remember, or record durable facts about the user. It defines what to
  keep — how the user approaches scientific questions, the tools and commands
  they use, the tool responses they rely on, and personal details such as name,
  role, affiliation, and stated preferences — and what to leave out (greetings,
  small talk, and one-off transient details). It also covers how to write Soul
  pages: which page kinds to use, how to cross-link with [[wiki-links]], and to
  prefer a few durable facts over many shallow ones.
---

# Writing the Soul

"Soul" is the user's personal knowledge base (`soul`). Its purpose is to make
future assistance better by remembering durable, high-signal facts about **the
user** — not to log conversations verbatim.

## What to capture (high value)

- **Approach to scientific questions.** How the user frames problems, the
  assumptions they make, the methods and statistical choices they prefer, the
  trade-offs they weigh.
- **Tools and extensions.** Which tools the user reaches for, in what order, and
  why. Note recurring workflows.
- **Commands and tool calls.** Concrete commands and the shape of tool calls the
  user runs (e.g. specific CLIs, query patterns, file layouts) — generalise them
  into reusable knowledge rather than copying one-off arguments.
- **Tool responses they rely on.** What outputs the user treats as authoritative
  or acts upon.
- **Personal information.** Name, occupation/role, lab or affiliation, domain of
  expertise, and explicitly stated preferences (formatting, verbosity, tone,
  preferred models/providers, working hours).

## What to discard (low value)

- Greetings, sign-offs, and small talk ("hi", "thanks", "ok").
- One-off, irrelevant, or transient details that won't help future sessions.
- Anything already recorded — update the existing page instead of duplicating.

## How to write it

- Create or update pages under the `soul` knowledge base. Use the existing
  page kinds: `entity` (people, labs, tools), `concept` (methods, preferences,
  approaches), and `note` for observations.
- Cross-reference related pages with `[[wiki-links]]` so the graph stays
  connected.
- Prefer a few well-formed, durable facts over many shallow ones.
- Each fact should read as a natural-language statement a future assistant could
  act on, e.g. "Prefers ggplot2 over base R for figures" rather than "used
  ggplot once".
- If a conversation yields nothing durable, record nothing.
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::Workflow;

    #[test]
    fn workflow_yaml_parses_into_a_valid_workflow() {
        let wf: Workflow = serde_yaml::from_str(MEDITATION_WORKFLOW_YAML)
            .expect("Meditation workflow YAML must deserialize");
        assert_eq!(wf.title, "Meditation");
        let kbs = wf.knowledge_bases.expect("knowledge_bases present");
        assert_eq!(kbs.default.as_deref(), Some("soul"));
        assert!(kbs.visible.iter().any(|k| k == "soul"));
        assert!(wf
            .skills
            .unwrap_or_default()
            .iter()
            .any(|s| s == "update-soul"));
    }

    #[test]
    fn skill_md_has_frontmatter_name() {
        assert!(SOUL_SKILL_MD.starts_with("---\n"));
        assert!(SOUL_SKILL_MD.contains("name: update-soul"));
    }

    #[tokio::test]
    async fn ensure_meditation_schedule_is_idempotent() {
        use crate::scheduler::Scheduler;
        use crate::session::session_manager::SessionManager;

        let tmp = tempfile::tempdir().unwrap();
        // A real workflow file the scheduler can copy.
        let wf = tmp.path().join("meditation.yaml");
        std::fs::write(&wf, MEDITATION_WORKFLOW_YAML).unwrap();

        let storage = tmp.path().join("schedule.json");
        let session_manager = Arc::new(SessionManager::new(tmp.path().to_path_buf()));
        let scheduler: Arc<dyn SchedulerTrait> =
            Scheduler::new(storage, session_manager).await.unwrap();

        ensure_meditation_schedule(&scheduler, wf.clone())
            .await
            .unwrap();
        // Second call must not create a duplicate.
        ensure_meditation_schedule(&scheduler, wf).await.unwrap();

        let jobs = scheduler.list_scheduled_jobs().await;
        let meditation_jobs: Vec<_> = jobs
            .iter()
            .filter(|j| j.id == MEDITATION_SCHEDULE_ID)
            .collect();
        assert_eq!(meditation_jobs.len(), 1, "exactly one Meditation job");
        assert_eq!(meditation_jobs[0].cron, MEDITATION_CRON);
    }

    #[test]
    fn cron_is_six_field_3am() {
        assert_eq!(MEDITATION_CRON.split_whitespace().count(), 6);
        // sec=0 min=0 hour=3
        let parts: Vec<&str> = MEDITATION_CRON.split_whitespace().collect();
        assert_eq!((parts[0], parts[1], parts[2]), ("0", "0", "3"));
    }
}
