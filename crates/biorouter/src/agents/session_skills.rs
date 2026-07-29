//! BR-71 decision (c): per-SESSION skill enablement.
//!
//! `workspace_set_tools { add_skills, remove_skills }` scopes skills to one
//! conversation. It must never write `~/.config/biorouter/skills-config.json`
//! — that file is the machine-wide user preference shared by the GUI toggles
//! and `biorouter skill enable/disable`
//! (`biorouter-cli/src/commands/skill.rs`), and rewriting it from an agent
//! tool would change every other conversation, window and CLI invocation.
//!
//! The override is stored where every other per-session extension state lives:
//! `Session.extension_data` under `("workspace_skills", "v1")` — the
//! `set_extension_state` precedent of `agents/goal.rs` and
//! `guardrails/run_state.rs`. A process-wide cache keyed by session id keeps
//! the read path (called for every `listSkills`/`searchSkills`/`loadSkill`)
//! off the database.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use anyhow::Result;

use crate::session::SessionManager;

pub const STATE_KEY: &str = "workspace_skills";
pub const STATE_VERSION: &str = "v1";

/// One session's deviation from the machine-wide skill set.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SessionSkillOverride {
    /// Enabled for this session even if machine-wide disabled.
    #[serde(default)]
    pub add: Vec<String>,
    /// Disabled for this session even if machine-wide enabled.
    #[serde(default)]
    pub remove: Vec<String>,
}

impl SessionSkillOverride {
    /// The composition rule, in one place: an explicit session `add` wins over
    /// everything, then an explicit session `remove`, then the machine-wide
    /// disabled set.
    pub fn is_disabled(&self, skill_name: &str, machine_disabled: &HashSet<String>) -> bool {
        if self.add.iter().any(|s| s == skill_name) {
            return false;
        }
        if self.remove.iter().any(|s| s == skill_name) {
            return true;
        }
        machine_disabled.contains(skill_name)
    }

    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }
}

static OVERRIDES: LazyLock<Mutex<HashMap<String, SessionSkillOverride>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock() -> std::sync::MutexGuard<'static, HashMap<String, SessionSkillOverride>> {
    OVERRIDES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// This session's override. Cheap and infallible — an unknown session is simply
/// "no deviation", which is the correct answer for every session that has never
/// been touched by `workspace_set_tools`.
pub fn for_session(session_id: &str) -> SessionSkillOverride {
    lock().get(session_id).cloned().unwrap_or_default()
}

/// Merge `add_skills` / `remove_skills` into the session's override and persist
/// it. Idempotent; a name appearing in both lists ends up in `add` only, which
/// matches [`SessionSkillOverride::is_disabled`]'s precedence.
pub async fn apply(
    session_manager: &SessionManager,
    session_id: &str,
    add_skills: &[String],
    remove_skills: &[String],
) -> Result<SessionSkillOverride> {
    let mut current = for_session(session_id);

    for name in remove_skills {
        current.add.retain(|s| s != name);
        if !current.remove.iter().any(|s| s == name) {
            current.remove.push(name.clone());
        }
    }
    for name in add_skills {
        current.remove.retain(|s| s != name);
        if !current.add.iter().any(|s| s == name) {
            current.add.push(name.clone());
        }
    }

    let session = session_manager.get_session(session_id, false).await?;
    let mut extension_data = session.extension_data.clone();
    extension_data.set_extension_state(STATE_KEY, STATE_VERSION, serde_json::to_value(&current)?);
    session_manager
        .update(session_id)
        .extension_data(extension_data)
        .apply()
        .await?;

    lock().insert(session_id.to_string(), current.clone());
    Ok(current)
}

/// Load a session's persisted override into the cache. Called once per session
/// by the skills extension the first time it learns its session id, so the
/// override survives a daemon restart. Best-effort: a read failure leaves the
/// session with no deviation, which is the pre-BR-71 behaviour.
pub async fn hydrate(session_manager: &SessionManager, session_id: &str) {
    if lock().contains_key(session_id) {
        return;
    }
    let loaded = match session_manager.get_session(session_id, false).await {
        Ok(session) => session
            .extension_data
            .get_extension_state(STATE_KEY, STATE_VERSION)
            .cloned()
            .and_then(|v| serde_json::from_value::<SessionSkillOverride>(v).ok())
            .unwrap_or_default(),
        Err(e) => {
            tracing::debug!("session skill override hydrate failed for {session_id}: {e}");
            SessionSkillOverride::default()
        }
    };
    lock().entry(session_id.to_string()).or_insert(loaded);
}

#[cfg(test)]
pub(crate) fn forget_for_tests(session_id: &str) {
    lock().remove(session_id);
}

/// Test-only: a session whose id is unique **within this test binary**.
///
/// `SessionStorage::create_session` allocates ids as `YYYYMMDD_N` where `N` is
/// counted *per sessions database*, so every test that opens its own `TempDir`
/// database is handed `…_1`. This module's cache is process-wide and keyed by
/// session id, so two such tests silently share — and clobber — one entry
/// (and one test's `forget_for_tests` erases the other's). Burning `slot`
/// throwaway ids first hands each caller a distinct id, so the tests stay
/// parallel and independent without a global test lock.
#[cfg(test)]
pub(crate) async fn unique_test_session(
    session_manager: &SessionManager,
    working_dir: std::path::PathBuf,
    name: &str,
    slot: usize,
) -> crate::session::Session {
    let mut session = None;
    for i in 0..=slot {
        let label = if i == slot {
            name.to_string()
        } else {
            format!("{name}-id-filler-{i}")
        };
        session = Some(
            session_manager
                .create_session(
                    working_dir.clone(),
                    label,
                    crate::session::SessionType::User,
                )
                .await
                .expect("create test session"),
        );
    }
    session.expect("slot loop always creates at least one session")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_compose_add_over_remove_over_machine_wide() {
        let machine_disabled: std::collections::HashSet<String> =
            ["a".to_string(), "b".to_string()].into_iter().collect();

        let empty = SessionSkillOverride::default();
        assert!(empty.is_disabled("a", &machine_disabled));
        assert!(!empty.is_disabled("c", &machine_disabled));

        // add re-enables a machine-disabled skill FOR THIS SESSION ONLY.
        let added = SessionSkillOverride {
            add: vec!["a".into()],
            remove: vec![],
        };
        assert!(!added.is_disabled("a", &machine_disabled));
        assert!(added.is_disabled("b", &machine_disabled));

        // remove disables a machine-enabled skill for this session.
        let removed = SessionSkillOverride {
            add: vec![],
            remove: vec!["c".into()],
        };
        assert!(removed.is_disabled("c", &machine_disabled));

        // An explicit add wins over an explicit remove: the last write in one
        // call is `add`, and a tool that both adds and removes the same name is
        // asking for it to be present.
        let both = SessionSkillOverride {
            add: vec!["c".into()],
            remove: vec!["c".into()],
        };
        assert!(!both.is_disabled("c", &machine_disabled));
    }

    #[tokio::test]
    async fn apply_persists_to_extension_data_and_never_touches_the_machine_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        // Slot 0 — see `unique_test_session`: the cache is process-wide and
        // keyed by session id, and per-database id allocation would otherwise
        // hand every test in this binary the same `YYYYMMDD_1`.
        let session = unique_test_session(&sm, temp.path().to_path_buf(), "skills", 0).await;

        apply(
            &sm,
            &session.id,
            &["single-cell".to_string()],
            &["ralph".to_string()],
        )
        .await
        .unwrap();

        // Persisted in the session row, under the documented key.
        let reread = sm.get_session(&session.id, false).await.unwrap();
        let stored = reread
            .extension_data
            .get_extension_state(STATE_KEY, STATE_VERSION)
            .expect("override persisted");
        assert_eq!(stored["add"][0], "single-cell");
        assert_eq!(stored["remove"][0], "ralph");

        // And readable through the cache without another DB hit.
        let live = for_session(&session.id);
        assert!(live.add.contains(&"single-cell".to_string()));
        assert!(live.remove.contains(&"ralph".to_string()));
    }

    #[tokio::test]
    async fn hydrate_restores_the_override_after_a_process_restart() {
        let temp = tempfile::TempDir::new().unwrap();
        let sm = std::sync::Arc::new(crate::session::SessionManager::new(
            temp.path().to_path_buf(),
        ));
        // Slot 1 — a distinct id from the test above, so its `forget_for_tests`
        // below cannot erase that test's cache entry.
        let session = unique_test_session(&sm, temp.path().to_path_buf(), "skills-2", 1).await;
        apply(&sm, &session.id, &["proteomics".to_string()], &[])
            .await
            .unwrap();

        // Simulate a cold process: drop the cache entry, then hydrate.
        forget_for_tests(&session.id);
        assert!(for_session(&session.id).add.is_empty());
        hydrate(&sm, &session.id).await;
        assert!(for_session(&session.id)
            .add
            .contains(&"proteomics".to_string()));
    }
}
